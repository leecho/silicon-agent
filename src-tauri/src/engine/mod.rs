pub mod builder;
mod control_tools;
pub mod event;
pub mod run_registry;

pub use builder::EngineBuilder;
pub use run_registry::{RunGuard, RunRegistry};

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use crate::app_settings::AppSettingsStore;
use crate::context::prompt::{system_prompt, Persona};
use crate::engine::event::{AgentStreamEvent, StreamEmitter};
use crate::provider::client::{ModelCallRequest, ModelClient, ModelEvent};
use crate::provider::message::{ModelMessage, ModelToolCall, ModelToolChoice, ToolSpecForModel};
use crate::session::permission::{needs_confirmation, resolve_effective_mode, PermissionMode};
use crate::session::{
    new_id, AskQuestion, PendingAsk, PendingPermission, PendingPlan, Session, SessionStore,
};
use crate::tools::add_artifact::ADD_ARTIFACT_TOOL;
use crate::tools::ask_user::ASK_USER_TOOL;
use crate::tools::load_skill::LOAD_SKILL_TOOL;
use crate::tools::propose_plan::PROPOSE_PLAN_TOOL;
use crate::tools::update_todos::UPDATE_TODOS_TOOL;
use crate::tools::ToolRegistry;
use crate::usage::{UsageRecord, UsageStore};

/// 解析 ask_user 工具参数 `{ questions: [...] }` → Vec<AskQuestion>。
/// 缺字段宽容：header 默认 ""、multiSelect 默认 false、options 默认 []。无 questions → 空 Vec（不做旧格式兼容）。
pub fn parse_ask_questions(args: &serde_json::Value) -> Vec<AskQuestion> {
    args.get("questions")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|q| {
                    let question = q.get("question").and_then(|v| v.as_str())?.to_string();
                    let header = q
                        .get("header")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let multi_select = q
                        .get("multiSelect")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let options = q
                        .get("options")
                        .and_then(|v| v.as_array())
                        .map(|o| {
                            o.iter()
                                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                                .collect()
                        })
                        .unwrap_or_default();
                    Some(AskQuestion {
                        header,
                        question,
                        multi_select,
                        options,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// 多轮 ReAct 循环默认最大步数（每步 = 一次模型调用）。
const DEFAULT_MAX_TURNS: usize = 24;

/// 自动压缩默认触发阈值：本次调用真实 prompt 占模型上下文上限的百分比达到此值即压缩。
const DEFAULT_AUTO_COMPACT_THRESHOLD_PCT: u64 = 90;

/// 引擎暂停时返回的待用户交互（泛化自原单一 `PendingPermission`）：
/// `Permission` 为风险工具待授权；`Ask` 为模型调用 `ask_user` 主动提问待回答。
#[derive(Debug)]
pub enum PendingInteraction {
    Permission(PendingPermission),
    Ask(PendingAsk),
    Plan(PendingPlan),
}

/// 一批工具执行的内部结果：`Paused` 命中需暂停的交互（权限确认或 ask_user 提问）；
/// `Continue` 携带更新后的事件序号。
enum ControlFlow {
    Paused(PendingInteraction),
    Continue(u64),
}

pub struct Engine {
    session: SessionStore,
    /// app 全局配置（自动重试上限、自动压缩开关、全局权限默认等）。
    /// 测试可不注入，缺省时各取内置默认（见各调用点与 [`resolve_effective_mode`]）。
    app_settings: Option<AppSettingsStore>,
    client: Arc<dyn ModelClient>,
    emitter: Option<StreamEmitter>,
    registry: ToolRegistry,
    usage: Option<UsageStore>,
    /// 当前会话工作目录（沙箱根）的展示路径，注入 system_prompt 引导模型把产出写入此处。
    workspace: String,
    /// 技能服务（注入则启用「可用技能」注入与 load_skill 读盘）；测试可不注入。
    skills: Option<Arc<crate::skill::SkillService>>,
    /// 本会话解析后的模型选择（provider+model）；None 时调用方退回 Gateway 默认。
    selection: Option<crate::provider::model::ResolvedModel>,
    /// 无人值守（headless）运行：定时任务触发时为 true。工具权限仍由会话 permission_mode 决定；
    /// 仅影响 ask_user —— headless 且 full 模式下自动应答继续，否则照常暂停（needs_attention）。
    headless: bool,
}

impl Engine {
    pub fn new(session: SessionStore, client: Arc<dyn ModelClient>) -> Self {
        Self {
            session,
            app_settings: None,
            client,
            emitter: None,
            registry: ToolRegistry::new(),
            usage: None,
            workspace: String::new(),
            skills: None,
            selection: None,
            headless: false,
        }
    }

    /// 注入 app 全局配置存储（自动重试/自动压缩/全局权限默认）。未注入时取内置默认。
    pub fn with_app_settings(mut self, app_settings: AppSettingsStore) -> Self {
        self.app_settings = Some(app_settings);
        self
    }

    /// 注入技能服务。
    pub fn with_skills(mut self, skills: Arc<crate::skill::SkillService>) -> Self {
        self.skills = Some(skills);
        self
    }

    /// 注入本会话解析后的模型选择。
    pub fn with_selection(
        mut self,
        selection: Option<crate::provider::model::ResolvedModel>,
    ) -> Self {
        self.selection = selection;
        self
    }

    /// 注入会话工作目录路径（供 system_prompt 引导模型把产出写入工作目录）。
    pub fn with_workspace(mut self, workspace: String) -> Self {
        self.workspace = workspace;
        self
    }

    pub fn with_emitter(mut self, emitter: StreamEmitter) -> Self {
        self.emitter = Some(emitter);
        self
    }

    pub fn with_registry(mut self, registry: ToolRegistry) -> Self {
        self.registry = registry;
        self
    }

    /// 注入用量存储；注入后引擎每次模型调用后自动写一行 `token_usage`。
    pub fn with_usage(mut self, usage: UsageStore) -> Self {
        self.usage = Some(usage);
        self
    }

    /// 注入 headless（无人值守）标记（定时任务用）。
    pub fn with_headless(mut self, headless: bool) -> Self {
        self.headless = headless;
        self
    }

    /// headless 运行且会话生效权限模式为 full 时，ask_user 自动应答继续（否则照常暂停）。
    fn headless_auto_answers_ask(&self, session_id: &str) -> Result<bool, String> {
        if !self.headless {
            return Ok(false);
        }
        let mode = PermissionMode::parse(&resolve_effective_mode(
            &self.session,
            self.app_settings.as_ref(),
            session_id,
        )?);
        Ok(matches!(mode, PermissionMode::Full))
    }

    /// 多轮 ReAct：落用户消息 → 重入 `run_loop` 驱动循环。
    ///
    /// 返回 `(SessionDetail, Option<PendingInteraction>)`：当循环因风险工具需确认或模型调用
    /// `ask_user` 主动提问而暂停时，第二项为 `Some`（命令层据此组装 `pending_permission` /
    /// `pending_ask`）；正常收口时为 `None`。
    pub fn submit_user_message(
        &self,
        session_id: &str,
        content: &str,
        cancel: Arc<AtomicBool>,
    ) -> Result<(Session, Option<PendingInteraction>), String> {
        let now = now_string();
        // 1. 落用户消息
        self.session
            .append_message(&new_id("msg"), session_id, "user", content, None, &now)?;
        // 2. 进入可重入循环（此入口无看门狗心跳——调度器自带 catch_unwind 收口；传丢弃句柄）。
        self.run_loop(session_id, cancel, Arc::new(AtomicU64::new(0)))
    }

    /// 续跑：用户的权限决定（授权 / 落拒绝结果）已由命令层处理，直接重入 `run_loop`。
    ///
    /// 续跑无需新用户消息——`run_loop` 首步会先处理「pending 未执行工具」：
    /// 若上一轮的风险工具已被授权则执行并回灌，否则（拒绝路径已落拒绝结果）该轮无 pending，
    /// 直接调模型让其基于新结果改道。
    pub fn resume(
        &self,
        session_id: &str,
        cancel: Arc<AtomicBool>,
    ) -> Result<(Session, Option<PendingInteraction>), String> {
        // 无看门狗心跳的入口（测试/简单调用）：传一个丢弃用心跳句柄。
        self.resume_with_heartbeat(session_id, cancel, Arc::new(AtomicU64::new(0)))
    }

    /// 同 [`Self::resume`]，但额外接受看门狗心跳句柄：run 循环每轮刷新它；挂死时心跳过期 → 看门狗回收收敛。
    pub fn resume_with_heartbeat(
        &self,
        session_id: &str,
        cancel: Arc<AtomicBool>,
        heartbeat: Arc<AtomicU64>,
    ) -> Result<(Session, Option<PendingInteraction>), String> {
        self.run_loop(session_id, cancel, heartbeat)
    }

    /// 可重入的多轮循环：每轮先执行「pending 未执行工具」（遇未授权风险工具→暂停并返回权限请求），
    /// 再调模型。触达设置中的最大迭代次数则落一条「已达最大步数」收口。
    ///
    /// 工具执行统一走 [`Engine::execute_calls_with_permission`]，保证首次提交与续跑路径一致。
    /// 模型本轮新请求的 tool_calls **不在本轮执行**：落库后 `continue`，由下一轮 step1 统一执行
    /// （带权限检查），这样 resume 与首次提交走完全相同的执行入口。
    fn run_loop(
        &self,
        session_id: &str,
        cancel: Arc<AtomicBool>,
        heartbeat: Arc<AtomicU64>,
    ) -> Result<(Session, Option<PendingInteraction>), String> {
        self.run_loop_inner(session_id, cancel, heartbeat)
    }

    fn run_loop_inner(
        &self,
        session_id: &str,
        cancel: Arc<AtomicBool>,
        heartbeat: Arc<AtomicU64>,
    ) -> Result<(Session, Option<PendingInteraction>), String> {
        // 循环外取一次会话工作模式（normal | plan）。读失败 → 默认 normal（不影响普通流程）。
        let mode = self
            .session
            .get_session_mode(session_id)
            .unwrap_or_else(|_| "normal".into());

        // 计划模式仅暴露只读 + 控制工具（requires_confirmation=false）；普通模式保留全部。
        let tool_specs: Vec<ToolSpecForModel> = self
            .registry
            .specs()
            .into_iter()
            .filter(|spec| {
                if mode != "plan" {
                    // 普通模式保留全部工具，**但 propose_plan 仅在计划模式可用**——排除之，
                    // 避免模型在普通执行模式误调用提交计划工具。
                    return spec.name != PROPOSE_PLAN_TOOL;
                }
                !self
                    .registry
                    .get(&spec.name)
                    .map(|t| t.requires_confirmation())
                    .unwrap_or(false)
            })
            .map(|spec| {
                ToolSpecForModel::json_schema(
                    spec.name,
                    spec.description,
                    spec.parameters,
                    "auto",
                    "low",
                )
            })
            .collect();
        let has_tools = !tool_specs.is_empty();

        // 循环外取一次启用技能（名+简介注入 system prompt，渐进式披露）。
        let enabled_skills = self
            .skills
            .as_ref()
            .map(|s| s.list_enabled().unwrap_or_default())
            .unwrap_or_default();
        let mut sequence: u64 = 0;
        // 本次 run 是否已自动压缩过：限一次，避免单轮内重复摘要调用。
        let mut compacted_this_run = false;
        let max_turns = self
            .app_settings
            .as_ref()
            .and_then(|s| s.get_max_iterations().ok())
            .map(|n| n.max(1) as usize)
            .unwrap_or(DEFAULT_MAX_TURNS);

        for _turn in 0..max_turns {
            // 看门狗心跳：每轮开头刷新，锚在「有进展」。单轮(含模型调用)若超阈值不刷 → 被判挂死回收。
            heartbeat.store(epoch_ms(), Ordering::Relaxed);
            // 检查点①：每轮开头。命中取消 → 收口停止（已产出此前已落库），不再调模型。
            if cancel.load(Ordering::Relaxed) {
                return self.finish_stopped(session_id, sequence);
            }

            // 1. 先执行「pending 未执行工具」：处理首次提交的本轮工具 + resume 续跑。
            //    遇未授权风险工具→暂停并返回权限请求；全执行完则结果已落库，落到下面调模型。
            let (producing_msg_id, pending_calls) = self.pending_unexecuted_calls(session_id)?;
            if !pending_calls.is_empty() {
                sequence = match self.execute_calls(
                    session_id,
                    &producing_msg_id,
                    &pending_calls,
                    sequence,
                    &mode,
                    &cancel,
                )? {
                    ControlFlow::Paused(it) => {
                        return Ok((self.detail(session_id)?, Some(it)));
                    }
                    ControlFlow::Continue(seq) => seq,
                };
                // 取消可能在本批工具执行期间被置位（execute_calls 已跳过其后的派发）：
                // 立即收口停止，避免再调模型/再开下一轮。
                if cancel.load(Ordering::Relaxed) {
                    return self.finish_stopped(session_id, sequence);
                }
            }

            // 2. 组装上下文（system + 可选压缩摘要 + 历史，OpenAI 角色形态）。
            //    compact 只影响"喂给模型的上下文"：已 compacted 的旧消息以摘要 system 替代，
            //    遍历时跳过；消息本身仍持久化、feed 显示不变。
            let history = self.session.list_messages(session_id)?;
            // 人设快照：从 app_settings 读身份/灵魂；缺席（未注入 settings）时回退默认人设。
            let persona = self
                .app_settings
                .as_ref()
                .map(|s| Persona {
                    identity: s.get_agent_identity().ok().flatten(),
                    soul: s.get_agent_soul().ok().flatten(),
                })
                .unwrap_or_default();
            let sys = system_prompt(&persona, &enabled_skills, &mode, &self.workspace);
            let mut messages = vec![ModelMessage::system(&sys)];
            if let Some(summary) = self.session.get_compaction_summary(session_id)? {
                messages.push(ModelMessage::system(&format!(
                    "以下是早前对话的摘要(已压缩)：\n{summary}"
                )));
            }
            // 历史 → provider 消息：唯一出口，在此强制 tool_call↔tool_result 配对不变式（详见 assemble_history_messages）。
            messages.extend(assemble_history_messages(&history));

            // 3. 流式调模型：逐 delta/thinking emit；收集 tool_calls。
            //    瞬时错误（且本次未发任何 delta）有界自动重试：退避后重发同一请求。
            let assistant_id = new_id("msg");
            let accumulated = std::sync::Mutex::new(String::new());
            let reasoning_buf = std::sync::Mutex::new(String::new());
            let seq_cell = std::sync::Mutex::new(sequence);
            let auto_retry_max = self
                .app_settings
                .as_ref()
                .and_then(|s| s.get_auto_retry_max().ok())
                .unwrap_or(3);
            let mut retry_attempt: u32 = 0;

            let result = loop {
                let request = ModelCallRequest {
                    messages: messages.clone(),
                    tools: tool_specs.clone(),
                    tool_choice: if has_tools {
                        ModelToolChoice::Auto
                    } else {
                        ModelToolChoice::None
                    },
                    stream: true,
                    model_selection: self.selection.as_ref().map(|r| r.selection()),
                    // 调用日志归因（T76）：单会话主运行。
                    attribution: crate::provider::message::ModelAttribution {
                        session_id: session_id.to_string(),
                        message_id: Some(assistant_id.clone()),
                        usage_type: Some("main_agent".to_string()),
                        ..Default::default()
                    },
                    ..Default::default()
                };

                let attempt_result = self.client.stream_model_with_events(request, &mut |event| {
                    // 检查点②：每个 token event 处理之前。命中取消 → 返回 false 中止 token 流
                    //（provider 默认实现据此返回 Err("model stream cancelled")，下方 Err 分支按 cancel 收口）。
                    if cancel.load(Ordering::Relaxed) {
                        return false;
                    }
                    match &event {
                        ModelEvent::ThinkingDelta { text } => {
                            reasoning_buf.lock().unwrap().push_str(text);
                            let mut seq = seq_cell.lock().unwrap();
                            *seq += 1;
                            self.emit(AgentStreamEvent {
                                kind: "thinking_delta".into(),
                                session_id: session_id.into(),
                                message_id: assistant_id.clone(),
                                sequence: *seq,
                                text: Some(text.clone()),
                                status: Some("streaming".into()),
                                tool_name: None,
                                tool_label: None,
                                tool_call_id: None,
                                todos: None,
                                artifacts: None,
                                parent_session_id: None,
                                parent_tool_call_id: None,
                                expert_name: None,
                                created_at: now_string(),
                            });
                        }
                        ModelEvent::Delta { text } => {
                            accumulated.lock().unwrap().push_str(text);
                            let mut seq = seq_cell.lock().unwrap();
                            *seq += 1;
                            self.emit(AgentStreamEvent {
                                kind: "message_delta".into(),
                                session_id: session_id.into(),
                                message_id: assistant_id.clone(),
                                sequence: *seq,
                                text: Some(text.clone()),
                                status: Some("streaming".into()),
                                tool_name: None,
                                tool_label: None,
                                tool_call_id: None,
                                todos: None,
                                artifacts: None,
                                parent_session_id: None,
                                parent_tool_call_id: None,
                                expert_name: None,
                                created_at: now_string(),
                            });
                        }
                        // 工具调用 live 预览：provider 边生成边累积 arguments 并逐帧 emit ToolCallCreated。
                        // arguments 非空时 emit `tool_call`（running）让前端实时显示工具块与参数生成进度，
                        // 避免大参数（如把整篇报告写进 write_file）生成时界面长时间无反馈。args 为空的早帧跳过。
                        // 注意：**持久化的** tool_calls 仍从最终 call_result.events 取（见下方），live 仅供显示。
                        ModelEvent::ToolCallCreated {
                            id,
                            name,
                            arguments_json,
                        } => {
                            if !arguments_json.is_empty() {
                                let mut seq = seq_cell.lock().unwrap();
                                *seq += 1;
                                self.emit(AgentStreamEvent {
                                    kind: "tool_call".into(),
                                    session_id: session_id.into(),
                                    message_id: id.clone(),
                                    sequence: *seq,
                                    text: Some(truncate_event_text(arguments_json, 2000)),
                                    // 生成期：模型正在流式产出该 tool_call 的参数（如把报告写进 write_file 的 content）。
                                    // 用 generating 区别于实际执行（running）——前端显示「正在生成…」而非「正在写入文件…」。
                                    status: Some("generating".into()),
                                    tool_name: Some(name.clone()),
                                    tool_label: self.label_for(&name),
                                    tool_call_id: Some(id.clone()),
                                    todos: None,
                                    artifacts: None,
                                    parent_session_id: None,
                                    parent_tool_call_id: None,
                                    expert_name: None,
                                    created_at: now_string(),
                                });
                            }
                        }
                        ModelEvent::AssistantMessageCompleted { .. } | ModelEvent::Error { .. } => {
                        }
                    }
                    true
                });
                match attempt_result {
                    Ok(cr) => break Ok(cr),
                    Err(e) => {
                        // 取消优先：交给外层失败分支按取消收口。
                        if cancel.load(Ordering::Relaxed) {
                            break Err(e);
                        }
                        let no_output = accumulated.lock().unwrap().is_empty()
                            && reasoning_buf.lock().unwrap().is_empty();
                        if matches!(
                            e.class,
                            crate::provider::client::ProviderErrorClass::Transient
                        ) && no_output
                            && retry_attempt < auto_retry_max
                        {
                            retry_attempt += 1;
                            let delay = e
                                .retry_after_ms
                                .unwrap_or_else(|| retry_backoff_ms(retry_attempt));
                            self.emit(AgentStreamEvent {
                                kind: "model_retrying".into(),
                                session_id: session_id.into(),
                                message_id: assistant_id.clone(),
                                sequence: 0,
                                text: Some(format!("第 {retry_attempt}/{auto_retry_max} 次重试…")),
                                status: None,
                                tool_name: None,
                                tool_label: None,
                                tool_call_id: None,
                                todos: None,
                                artifacts: None,
                                parent_session_id: None,
                                parent_tool_call_id: None,
                                expert_name: None,
                                created_at: now_string(),
                            });
                            if sleep_cancellable(delay, &cancel) {
                                break Err(e);
                            }
                            continue;
                        }
                        break Err(e);
                    }
                }
            };

            let call_result = match result {
                Ok(call_result) => call_result,
                Err(err) => {
                    // 检查点②的后续：回调返回 false 致 provider 返回 Err。先判取消——若是用户取消，
                    // 不当作错误，而是把已累积的部分文本落库并按 stopped 收口（保留已产出）。
                    if cancel.load(Ordering::Relaxed) {
                        sequence = seq_cell.into_inner().unwrap();
                        let partial = accumulated.into_inner().unwrap();
                        let reasoning = reasoning_buf.into_inner().unwrap();
                        return self.finish_stopped_with_partial(
                            session_id,
                            &assistant_id,
                            &partial,
                            &reasoning,
                            sequence,
                        );
                    }
                    let failed_at = now_string();
                    // 1. 保住已流式产出的 partial（若有）：落一条 assistant 消息，与 stopped 一致。
                    let partial = accumulated.into_inner().unwrap();
                    let reasoning = reasoning_buf.into_inner().unwrap();
                    if !partial.trim().is_empty() {
                        let reasoning_opt: Option<&str> = if reasoning.trim().is_empty() {
                            None
                        } else {
                            Some(&reasoning)
                        };
                        let _ = self.session.append_message(
                            &assistant_id,
                            session_id,
                            "assistant",
                            &partial,
                            reasoning_opt,
                            &failed_at,
                        );
                    }
                    // 2. 落一条持久错误消息：role="error" + compacted=1 —— 仅在 feed 显示、不进
                    //    模型上下文（重试时模型看不到这段报错），使失败信息 reload 后仍在。
                    let error_id = new_id("msg");
                    let _ = self.session.append_message(
                        &error_id,
                        session_id,
                        "error",
                        &err.message,
                        None,
                        &failed_at,
                    );
                    let _ = self.session.mark_compacted(session_id, &[error_id.clone()]);
                    // 3. emit（即时反馈；run_finished 后 feed 会用 DB 重建，换成持久化那条）。
                    self.emit(AgentStreamEvent {
                        kind: "message_failed".into(),
                        session_id: session_id.into(),
                        message_id: error_id,
                        sequence: 0,
                        text: Some(err.message.clone()),
                        status: Some("failed".into()),
                        tool_name: None,
                        tool_label: None,
                        tool_call_id: None,
                        todos: None,
                        artifacts: None,
                        parent_session_id: None,
                        parent_tool_call_id: None,
                        expert_name: None,
                        created_at: failed_at,
                    });
                    return Err(err.message);
                }
            };

            // 用量采集：每次模型调用后写一行（含缓存 token）。写库失败仅吞掉，不影响对话。
            if let (Some(usage_store), Some(model_usage)) = (&self.usage, &call_result.usage) {
                let (provider, model) = match &self.selection {
                    Some(r) => (r.provider_name.clone(), r.model.clone()),
                    None => self
                        .client
                        .active_model_provider()
                        .unwrap_or_else(|| (String::new(), String::new())),
                };
                let record = UsageRecord {
                    session_id: session_id.to_string(),
                    message_id: Some(assistant_id.clone()),
                    provider,
                    model,
                    usage_type: "main_agent".to_string(),
                    created_at: now_string(),
                    usage: model_usage.clone(),
                };
                let _ = usage_store.record(&new_id("usage"), &record);
            }

            sequence = seq_cell.into_inner().unwrap();
            let final_text = {
                let acc = accumulated.into_inner().unwrap();
                if acc.trim().is_empty() {
                    final_assistant_text(&call_result)
                } else {
                    acc
                }
            };
            let reasoning = reasoning_buf.into_inner().unwrap();
            let reasoning_opt: Option<&str> = if reasoning.trim().is_empty() {
                None
            } else {
                Some(reasoning.as_str())
            };

            // 检查点③：流正常结束（Ok 部分结果，如取消恰好落在两 event 之间）后、决定 tool_calls 前。
            // 命中取消 → 把已累积文本落为一条 assistant 消息并按 stopped 收口，不再 continue 到下一轮。
            if cancel.load(Ordering::Relaxed) {
                return self.finish_stopped_with_partial(
                    session_id,
                    &assistant_id,
                    &final_text,
                    &reasoning,
                    sequence,
                );
            }
            // 自动压缩：本次调用真实 prompt 占模型上限 ≥ 阈值 → 压缩较早历史；下一轮上下文组装
            // （跳过 compacted + 注入摘要）自动变小。本轮只压一次；失败不阻断对话（仅记日志）。
            if !compacted_this_run {
                if let Some(usage) = &call_result.usage {
                    let used = usage.input_tokens.unwrap_or(0);
                    let limit = crate::provider::model_context_limit(&self.current_model_name())
                        .max(1) as u64;
                    let compact_threshold_pct = self
                        .app_settings
                        .as_ref()
                        .and_then(|s| s.get_auto_compact_threshold_pct().ok())
                        .unwrap_or(DEFAULT_AUTO_COMPACT_THRESHOLD_PCT as u32)
                        as u64;
                    if self
                        .app_settings
                        .as_ref()
                        .and_then(|s| s.get_auto_compact_enabled().ok())
                        .unwrap_or(true)
                        && used.saturating_mul(100) / limit >= compact_threshold_pct
                    {
                        match self.compact_context(session_id) {
                            Ok(true) => {
                                compacted_this_run = true;
                                sequence += 1;
                                self.emit(AgentStreamEvent {
                                    kind: "context_compacted".into(),
                                    session_id: session_id.into(),
                                    message_id: String::new(),
                                    sequence,
                                    text: Some("已自动压缩较早历史，上下文已精简。".into()),
                                    status: None,
                                    tool_name: None,
                                    tool_label: None,
                                    tool_call_id: None,
                                    todos: None,
                                    artifacts: None,
                                    parent_session_id: None,
                                    parent_tool_call_id: None,
                                    expert_name: None,
                                    created_at: now_string(),
                                });
                            }
                            Ok(false) => {}
                            Err(e) => eprintln!("[auto-compact] 失败 会话={session_id}：{e}"),
                        }
                    }
                }
            }

            // 从最终归一化结果提取 tool_calls（含完整累积 arguments），而非 live 闭包（args 可能为空）。
            let calls: Vec<ModelToolCall> = call_result
                .events
                .iter()
                .filter_map(|event| match event {
                    ModelEvent::ToolCallCreated {
                        id,
                        name,
                        arguments_json,
                    } => Some(ModelToolCall {
                        id: id.clone(),
                        name: name.clone(),
                        arguments_json: arguments_json.clone(),
                    }),
                    _ => None,
                })
                .collect();

            if !calls.is_empty() {
                // 模型请求工具：落 assistant(content + tool_calls) 消息并 emit。
                // **不在此执行工具**——落库后 continue，由下一轮 step1 的
                // execute_calls_with_permission 统一执行（带权限检查），保证 resume 与首次提交一致。
                let tool_calls_json =
                    serde_json::to_string(&calls).unwrap_or_else(|_| "[]".to_string());
                let assistant_at = now_string();
                self.session.append_assistant_tool_call(
                    &assistant_id,
                    session_id,
                    &final_text,
                    reasoning_opt,
                    &tool_calls_json,
                    &assistant_at,
                )?;
                sequence += 1;
                self.emit(AgentStreamEvent {
                    kind: "message_delta".into(),
                    session_id: session_id.into(),
                    message_id: assistant_id.clone(),
                    sequence,
                    text: Some(final_text.clone()),
                    status: Some("streaming".into()),
                    tool_name: None,
                    tool_label: None,
                    tool_call_id: None,
                    todos: None,
                    artifacts: None,
                    parent_session_id: None,
                    parent_tool_call_id: None,
                    expert_name: None,
                    created_at: assistant_at,
                });
                // 续轮：下一轮 step1 统一执行本轮工具（带权限检查），再把结果回灌给模型。
                continue;
            }

            // 5. 最终答案：落 assistant 消息并 emit message_completed。
            let done_at = now_string();
            self.session.append_message(
                &assistant_id,
                session_id,
                "assistant",
                &final_text,
                reasoning_opt,
                &done_at,
            )?;
            sequence += 1;
            self.emit(AgentStreamEvent {
                kind: "message_completed".into(),
                session_id: session_id.into(),
                message_id: assistant_id,
                sequence,
                text: Some(final_text),
                status: Some("done".into()),
                tool_name: None,
                tool_label: None,
                tool_call_id: None,
                todos: None,
                artifacts: None,
                parent_session_id: None,
                parent_tool_call_id: None,
                expert_name: None,
                created_at: done_at,
            });
            return Ok((self.detail(session_id)?, None));
        }

        // 触界：落一条收口 assistant 消息并 emit completed。
        let exhausted_id = new_id("msg");
        let done_at = now_string();
        let text = "已达最大步数，停止。";
        self.session.append_message(
            &exhausted_id,
            session_id,
            "assistant",
            text,
            None,
            &done_at,
        )?;
        sequence += 1;
        self.emit(AgentStreamEvent {
            kind: "message_completed".into(),
            session_id: session_id.into(),
            message_id: exhausted_id,
            sequence,
            text: Some(text.into()),
            status: Some("done".into()),
            tool_name: None,
            tool_label: None,
            tool_call_id: None,
            todos: None,
            artifacts: None,
            parent_session_id: None,
            parent_tool_call_id: None,
            expert_name: None,
            created_at: done_at,
        });
        Ok((self.detail(session_id)?, None))
    }

    /// 查找「pending 未执行工具」：末条带 tool_calls 的 assistant 消息里，尚无对应 tool 结果的调用。
    ///
    /// 判定稳健性：A 必须是**最后一条** role=assistant 且 tool_calls_json 非空的消息，
    /// 且其后**没有更靠后的 assistant 消息**（无论是否带 tool_calls）——否则说明该轮已收口或已进入新轮，
    /// 历史里早先轮的 tool_calls 不应被误当 pending。收集 A 之后 role=tool 的 tool_call_id 集合 done，
    /// 返回 A.tool_calls 里 id ∉ done 的调用（保序）。无此 A 或都已执行 → 空 Vec。
    fn pending_unexecuted_calls(
        &self,
        session_id: &str,
    ) -> Result<(String, Vec<ModelToolCall>), String> {
        let messages = self.session.list_messages(session_id)?;
        // 末条带 tool_calls 的 assistant 的下标。
        let anchor_idx = messages.iter().enumerate().rev().find_map(|(i, m)| {
            if m.role == "assistant"
                && m.tool_calls_json
                    .as_deref()
                    .map(|s| !s.trim().is_empty())
                    .unwrap_or(false)
            {
                Some(i)
            } else {
                None
            }
        });
        let anchor_idx = match anchor_idx {
            Some(i) => i,
            None => return Ok((String::new(), Vec::new())),
        };
        // A 之后若存在任何 assistant 消息 → 该轮已收口/进入新轮，无 pending。
        if messages[anchor_idx + 1..]
            .iter()
            .any(|m| m.role == "assistant")
        {
            return Ok((String::new(), Vec::new()));
        }
        let anchor = &messages[anchor_idx];
        let calls: Vec<ModelToolCall> = match anchor.tool_calls_json.as_deref() {
            Some(json) => serde_json::from_str(json).unwrap_or_default(),
            None => Vec::new(),
        };
        // A 之后已落 tool 结果的 tool_call_id 集合。
        let done: std::collections::HashSet<String> = messages[anchor_idx + 1..]
            .iter()
            .filter(|m| m.role == "tool")
            .filter_map(|m| m.tool_call_id.clone())
            .collect();
        let anchor_id = anchor.id.clone();
        Ok((
            anchor_id,
            calls
                .into_iter()
                .filter(|c| !done.contains(&c.id))
                .collect(),
        ))
    }

    /// 从持久化消息重建"当前应展示的待交互"（reload 后恢复权限卡 / Ask 卡）。
    ///
    /// pending 是运行期临时态、不持久化；重开 app / 刷新会话时唯一的恢复依据是持久化里的
    /// 悬空 tool_call（[`Self::pending_unexecuted_calls`]）。对暂停点（首个悬空调用——live 路径
    /// 正是在它处暂停、其前的工具都已落结果）按与 [`Self::execute_calls`] 一致的闸门规则还原：
    /// `ask_user` → [`PendingAsk`]；`requires_confirmation()` 且会话未授权 → [`PendingPermission`]；
    /// 否则 `None`（普通/已授权工具应由续跑执行，不属于"等用户"态）。
    pub fn pending_interaction(
        &self,
        session_id: &str,
    ) -> Result<Option<PendingInteraction>, String> {
        let (_, pending) = self.pending_unexecuted_calls(session_id)?;
        let Some(call) = pending.first() else {
            return Ok(None);
        };
        if call.name == ASK_USER_TOOL {
            let args = serde_json::from_str::<serde_json::Value>(&call.arguments_json)
                .unwrap_or(serde_json::Value::Null);
            return Ok(Some(PendingInteraction::Ask(PendingAsk {
                session_id: session_id.to_string(),
                tool_call_id: call.id.clone(),
                questions: parse_ask_questions(&args),
            })));
        }
        if call.name == PROPOSE_PLAN_TOOL {
            let args = serde_json::from_str::<serde_json::Value>(&call.arguments_json)
                .unwrap_or(serde_json::Value::Null);
            let title = args
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let summary = args
                .get("summary")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let plan_markdown = args
                .get("plan_markdown")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let risk_level = args
                .get("risk_level")
                .and_then(|v| v.as_str())
                .unwrap_or("medium")
                .to_string();
            return Ok(Some(PendingInteraction::Plan(PendingPlan {
                session_id: session_id.to_string(),
                tool_call_id: call.id.clone(),
                title,
                summary,
                plan_markdown,
                risk_level,
            })));
        }
        let risk = self
            .registry
            .get(&call.name)
            .map(|t| t.risk_level())
            .unwrap_or(crate::tools::RiskLevel::Safe);
        let pmode = PermissionMode::parse(&resolve_effective_mode(
            &self.session,
            self.app_settings.as_ref(),
            session_id,
        )?);
        let granted = self.session.is_tool_granted(session_id, &call.name)?;
        if needs_confirmation(risk, pmode, granted) {
            return Ok(Some(PendingInteraction::Permission(PendingPermission {
                session_id: session_id.to_string(),
                tool_call_id: call.id.clone(),
                tool_name: call.name.clone(),
                input: call.arguments_json.clone(),
            })));
        }
        Ok(None)
    }

    /// 执行一批未执行的工具调用，带交互闸：返回 `Paused` 表示暂停（ask_user 提问或未授权风险工具）。
    ///
    /// 对每个 call：先 emit `tool_call` 事件（发起，带完整输入参数 JSON），让前端看到调用发起；
    /// 若工具名为 `ask_user` → 解析 question/options，emit `ask_required` 并返回
    /// `Paused(PendingInteraction::Ask)`（**不执行、不落结果**，答案由命令层落为 tool 结果后续跑）；
    /// 若工具 `requires_confirmation()` 且会话未授权 → emit `permission_required` 并返回
    /// `Paused(PendingInteraction::Permission)`（**不执行、不落结果**，保留天然 pending 状态待续跑）；
    /// 否则 `registry.execute`（Err→结果文本）+ 落 tool 结果 + emit `tool_result`。
    /// 返回 `Continue(sequence)` 携带更新后的事件序号。
    fn execute_calls(
        &self,
        session_id: &str,
        producing_message_id: &str,
        calls: &[ModelToolCall],
        mut sequence: u64,
        mode: &str,
        cancel: &std::sync::atomic::AtomicBool,
    ) -> Result<ControlFlow, String> {
        for call in calls {
            // 运行已被取消：不再执行本批后续工具。
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            // tool_call 事件先 emit（发起，完整 args，截断至 2000 字符防事件过大）。
            sequence += 1;
            self.emit(AgentStreamEvent {
                kind: "tool_call".into(),
                session_id: session_id.into(),
                message_id: call.id.clone(),
                sequence,
                text: Some(truncate_event_text(&call.arguments_json, 2000)),
                status: Some("running".into()),
                tool_name: Some(call.name.clone()),
                tool_label: self.label_for(&call.name),
                tool_call_id: Some(call.id.clone()),
                todos: None,
                artifacts: None,
                parent_session_id: None,
                parent_tool_call_id: None,
                expert_name: None,
                created_at: now_string(),
            });

            // 控制工具按名拦截分发：不走 registry 真执行，处理体见 engine/control_tools.rs。
            if call.name == ASK_USER_TOOL {
                match self.handle_ask_user(session_id, call, &mut sequence)? {
                    Some(cf) => return Ok(cf),
                    None => continue,
                }
            }
            if call.name == PROPOSE_PLAN_TOOL {
                return self.handle_propose_plan(session_id, call, &mut sequence);
            }
            if call.name == LOAD_SKILL_TOOL {
                self.handle_load_skill(session_id, call, &mut sequence)?;
                continue;
            }
            if call.name == crate::tools::read_skill_file::READ_SKILL_FILE_TOOL {
                self.handle_read_skill_file(session_id, call, &mut sequence)?;
                continue;
            }
            if call.name == UPDATE_TODOS_TOOL {
                self.handle_update_todos(session_id, call, &mut sequence)?;
                continue;
            }
            if call.name == ADD_ARTIFACT_TOOL {
                self.handle_add_artifact(session_id, producing_message_id, call, &mut sequence)?;
                continue;
            }

            // 计划模式只读约束（安全网）：即使 spec 已过滤掉写工具，运行期再兜底——
            // plan 模式下任何 requires_confirmation 的工具（写/编辑/命令类）一律不执行、不暂停，
            // 落一条提示性 tool 结果回灌给模型，引导其改用只读工具调研 + propose_plan。
            if mode == "plan"
                && self
                    .registry
                    .get(&call.name)
                    .map(|t| t.requires_confirmation())
                    .unwrap_or(false)
            {
                let msg = format!(
                    "计划模式下不可执行「{}」(写/命令类)。请先用只读工具调研、再用 propose_plan 提交计划。",
                    call.name
                );
                let result_at = now_string();
                self.session.append_tool_result(
                    &new_id("msg"),
                    session_id,
                    &call.id,
                    &call.name,
                    &msg,
                    "done",
                    &result_at,
                )?;
                sequence += 1;
                self.emit(AgentStreamEvent {
                    kind: "tool_result".into(),
                    session_id: session_id.into(),
                    message_id: call.id.clone(),
                    sequence,
                    text: Some(truncate_event_text(&msg, 2000)),
                    status: Some("done".into()),
                    tool_name: Some(call.name.clone()),
                    tool_label: self.label_for(&call.name),
                    tool_call_id: Some(call.id.clone()),
                    todos: None,
                    artifacts: None,
                    parent_session_id: None,
                    parent_tool_call_id: None,
                    expert_name: None,
                    created_at: result_at,
                });
                continue;
            }

            // 权限闸：按生效权限模式 + 工具风险级别判定是否需确认。
            // 定时任务通过设置会话 permission_mode（如 full）来自动放行，无需额外特判。
            let risk = self
                .registry
                .get(&call.name)
                .map(|t| t.risk_level())
                .unwrap_or(crate::tools::RiskLevel::Safe);
            let pmode = PermissionMode::parse(&resolve_effective_mode(
                &self.session,
                self.app_settings.as_ref(),
                session_id,
            )?);
            let granted = self.session.is_tool_granted(session_id, &call.name)?;
            if needs_confirmation(risk, pmode, granted) {
                sequence += 1;
                self.emit(AgentStreamEvent {
                    kind: "permission_required".into(),
                    session_id: session_id.into(),
                    message_id: call.id.clone(),
                    sequence,
                    text: Some(truncate_event_text(&call.arguments_json, 2000)),
                    status: Some("paused".into()),
                    tool_name: Some(call.name.clone()),
                    tool_label: self.label_for(&call.name),
                    tool_call_id: Some(call.id.clone()),
                    todos: None,
                    artifacts: None,
                    parent_session_id: None,
                    parent_tool_call_id: None,
                    expert_name: None,
                    created_at: now_string(),
                });
                return Ok(ControlFlow::Paused(PendingInteraction::Permission(
                    PendingPermission {
                        session_id: session_id.to_string(),
                        tool_call_id: call.id.clone(),
                        tool_name: call.name.clone(),
                        input: call.arguments_json.clone(),
                    },
                )));
            }

            // 安全或已授权 → 执行，区分成功/失败落 tool 结果并 emit tool_result。
            let args = serde_json::from_str::<serde_json::Value>(&call.arguments_json)
                .unwrap_or(serde_json::Value::Null);

            let (result_text, tool_status) = match self.registry.execute(&call.name, &args) {
                Ok(t) => (t, "done"),
                Err(e) => (e, "failed"),
            };
            let result_at = now_string();
            self.session.append_tool_result(
                &new_id("msg"),
                session_id,
                &call.id,
                &call.name,
                &result_text,
                tool_status,
                &result_at,
            )?;
            sequence += 1;
            self.emit(AgentStreamEvent {
                kind: "tool_result".into(),
                session_id: session_id.into(),
                message_id: call.id.clone(),
                sequence,
                text: Some(truncate_event_text(&result_text, 2000)),
                status: Some(tool_status.into()),
                tool_name: Some(call.name.clone()),
                tool_label: self.label_for(&call.name),
                tool_call_id: Some(call.id.clone()),
                todos: None,
                artifacts: None,
                parent_session_id: None,
                parent_tool_call_id: None,
                expert_name: None,
                created_at: result_at,
            });
        }
        Ok(ControlFlow::Continue(sequence))
    }

    /// 落一条「已手动停止」标记消息：role="stopped" + compacted=1 —— 仿照 error 消息，
    /// 仅在 feed 显示、不进模型上下文（续跑时模型看不到它），使 reload 后仍能看到本轮被手动停止。
    fn append_stopped_marker(&self, session_id: &str, at: &str) {
        self.session.append_stopped_marker(session_id, at);
    }

    /// 取消收口（无部分文本）：检查点①命中（每轮开头取消，未进入本轮 stream），
    /// 已产出的此前轮次消息均已落库，此处补一条 stopped 标记消息（供 reload 显示），
    /// 再 emit 一个 stopped 完成事件并返回当前详情。
    fn finish_stopped(
        &self,
        session_id: &str,
        sequence: u64,
    ) -> Result<(Session, Option<PendingInteraction>), String> {
        let stopped_at = now_string();
        self.append_stopped_marker(session_id, &stopped_at);
        self.emit(AgentStreamEvent {
            kind: "message_completed".into(),
            session_id: session_id.into(),
            message_id: new_id("msg"),
            sequence: sequence + 1,
            text: Some(String::new()),
            status: Some("stopped".into()),
            tool_name: None,
            tool_label: None,
            tool_call_id: None,
            todos: None,
            artifacts: None,
            parent_session_id: None,
            parent_tool_call_id: None,
            expert_name: None,
            created_at: stopped_at,
        });
        Ok((self.detail(session_id)?, None))
    }

    /// 取消收口（带部分文本）：检查点②/③命中（token 流被中止），把已累积的部分文本 + reasoning
    /// 落为一条 assistant 消息保留，再 emit stopped 完成事件并返回。空文本也落一条空内容 assistant，
    /// 与正常路径一致（保留该轮 reasoning）。
    fn finish_stopped_with_partial(
        &self,
        session_id: &str,
        assistant_id: &str,
        partial_text: &str,
        reasoning: &str,
        sequence: u64,
    ) -> Result<(Session, Option<PendingInteraction>), String> {
        let reasoning_opt: Option<&str> = if reasoning.trim().is_empty() {
            None
        } else {
            Some(reasoning)
        };
        let stopped_at = now_string();
        self.session.append_message(
            assistant_id,
            session_id,
            "assistant",
            partial_text,
            reasoning_opt,
            &stopped_at,
        )?;
        self.append_stopped_marker(session_id, &stopped_at);
        self.emit(AgentStreamEvent {
            kind: "message_completed".into(),
            session_id: session_id.into(),
            message_id: assistant_id.into(),
            sequence: sequence + 1,
            text: Some(partial_text.to_string()),
            status: Some("stopped".into()),
            tool_name: None,
            tool_label: None,
            tool_call_id: None,
            todos: None,
            artifacts: None,
            parent_session_id: None,
            parent_tool_call_id: None,
            expert_name: None,
            created_at: stopped_at,
        });
        Ok((self.detail(session_id)?, None))
    }

    /// 取会话详情，缺失则报错。
    fn detail(&self, session_id: &str) -> Result<Session, String> {
        self.session
            .get_session_detail(session_id)?
            .ok_or_else(|| "session not found".into())
    }

    fn emit(&self, event: AgentStreamEvent) {
        if let Some(emitter) = &self.emitter {
            emitter(event);
        }
    }

    /// 取工具的面向用户标签（来自 `Tool::label()`），随 tool_call / tool_result 事件带出，
    /// 让远程 IM / 前端展示「执行命令」而非内部名 `run_command`。未注册的名返回 None。
    fn label_for(&self, name: &str) -> Option<String> {
        self.registry.get(name).map(|t| t.label().to_string())
    }

    /// 当前生效模型名（供 context 上限查表）：优先会话选择，否则取 client 报告的 active 模型。
    fn current_model_name(&self) -> String {
        match &self.selection {
            Some(r) => r.model.clone(),
            None => self
                .client
                .active_model_provider()
                .map(|(_, m)| m)
                .unwrap_or_default(),
        }
    }

    /// 上下文压缩：薄委托给 `context::compaction::compact`。手动 `/compact` 与自动压缩共用。
    /// 返回是否真的压缩（候选不足则跳过返回 false）。自身不发事件，可见性由调用方决定。
    pub fn compact_context(&self, session_id: &str) -> Result<bool, String> {
        crate::context::compaction::compact(
            &self.session,
            self.client.as_ref(),
            self.selection.as_ref(),
            session_id,
        )
    }
}

/// 从归一化结果取最终 assistant 文本（流式累积为空时的回退）。
fn final_assistant_text(result: &crate::provider::client::ModelCallResult) -> String {
    for event in result.events.iter().rev() {
        if let ModelEvent::AssistantMessageCompleted { content } = event {
            return content.clone();
        }
    }
    String::new()
}

/// 重试退避（毫秒）：500 * 2^(attempt-1)，attempt 从 1 计。封顶移位避免溢出。
pub(crate) fn retry_backoff_ms(attempt: u32) -> u64 {
    let shift = attempt.saturating_sub(1).min(10);
    500u64.saturating_mul(1u64 << shift)
}

/// 可取消地等待 total_ms：按 ≤100ms 分片轮询 cancel。命中取消返回 true（提前结束），否则睡满返回 false。
/// 当前 epoch 毫秒（看门狗心跳用）。
fn epoch_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub(crate) fn sleep_cancellable(total_ms: u64, cancel: &std::sync::atomic::AtomicBool) -> bool {
    use std::sync::atomic::Ordering;
    let mut remaining = total_ms;
    while remaining > 0 {
        if cancel.load(Ordering::Relaxed) {
            return true;
        }
        let slice = remaining.min(100);
        std::thread::sleep(std::time::Duration::from_millis(slice));
        remaining -= slice;
    }
    cancel.load(Ordering::Relaxed)
}

/// 剥掉 Composer chip 的显示标记 ⟦⟧（仅前端用于把技能/文件还原成 chip 样式）。
/// 模型只需看到内部纯文本（如「技能：名」「@相对路径」），故发模型前去掉这对括号。
fn strip_chip_markers(content: &str) -> String {
    content.replace(['⟦', '⟧'], "")
}

/// 历史消息 → provider 消息序列（唯一出口，强制 OpenAI-compatible 的 tool_call↔tool_result 配对不变式）。
///
/// 每条 assistant 的 `tool_calls` **紧跟**其每个调用的结果，按 `tool_call_id` 配对——**不依赖历史中的位置邻接**。
/// 缺失结果补一条占位（上一次运行被中断时常见）；独立 tool 消息在此跳过（其结果已在所属 assistant 处按 id 输出）。
/// 这样即便历史因中断/迟到回填/「继续」后补结果而乱序（assistant→user→tool），发给 provider 的请求仍合法——
/// 这是「进程被打断后会话仍可续跑」的根本保证，而非靠各恢复路径逐个收口悬空 tool_call（天生不完整、脆弱）。
pub(crate) fn assemble_history_messages(history: &[crate::session::Message]) -> Vec<ModelMessage> {
    use std::collections::HashMap;
    const ORPHAN: &str = "（该工具调用未完成：上一次运行被中断。如仍需要，请重新调用。）";
    // id → 结果正文（仅纳入上下文的非 compacted tool 消息；同 id 取最后写入）。
    let mut tool_results: HashMap<&str, &str> = HashMap::new();
    for m in history {
        if !m.compacted && m.role == "tool" {
            if let Some(id) = m.tool_call_id.as_deref() {
                tool_results.insert(id, m.content.as_str());
            }
        }
    }
    let mut out = Vec::new();
    for m in history {
        if m.compacted {
            continue;
        }
        match m.role.as_str() {
            "assistant" => match m.tool_calls_json.as_deref() {
                Some(json) if !json.trim().is_empty() => {
                    match serde_json::from_str::<Vec<ModelToolCall>>(json) {
                        Ok(calls) => {
                            let ids: Vec<String> = calls.iter().map(|c| c.id.clone()).collect();
                            out.push(ModelMessage::assistant_tool_calls(calls));
                            for id in ids {
                                let content =
                                    tool_results.get(id.as_str()).copied().unwrap_or(ORPHAN);
                                out.push(ModelMessage::tool(id, content));
                            }
                        }
                        Err(_) => out.push(ModelMessage::assistant(&m.content)),
                    }
                }
                _ => out.push(ModelMessage::assistant(&m.content)),
            },
            // 独立 tool 消息：结果已在所属 assistant 处按 id 紧跟输出 → 跳过，避免重复/错位。
            "tool" => {}
            // user：剥掉 Composer chip 的 ⟦⟧ 标记；error/stopped/compaction 等已被 compacted 跳过，余者按 user。
            _ => out.push(ModelMessage::user(&strip_chip_markers(&m.content))),
        }
    }
    out
}

#[cfg(test)]
mod assemble_tests {
    use super::*;
    use crate::provider::message::ModelMessageRole;
    use crate::session::Message;

    fn msg(role: &str, content: &str) -> Message {
        Message {
            id: format!("m-{role}-{content}"),
            session_id: "s".into(),
            role: role.into(),
            content: content.into(),
            reasoning: None,
            tool_calls_json: None,
            tool_call_id: None,
            tool_name: None,
            tool_status: None,
            compacted: false,
            created_at: "1".into(),
        }
    }
    fn assistant_tc(id: &str) -> Message {
        let mut m = msg("assistant", "");
        m.tool_calls_json = Some(format!(
            r#"[{{"id":"{id}","name":"t","argumentsJson":"{{}}"}}]"#
        ));
        m
    }
    fn tool_res(id: &str, content: &str) -> Message {
        let mut m = msg("tool", content);
        m.tool_call_id = Some(id.into());
        m
    }

    /// 复现真实 bug：assistant(tool_calls) → user("继续") → tool(结果迟到写在 user 之后)。
    /// 旧的「按历史位置配对」会组装出 assistant,user,tool（非法）；新实现必须 assistant,tool,user。
    #[test]
    fn tool_result_pairs_with_assistant_despite_intervening_user() {
        let history = vec![
            assistant_tc("call_A"),
            msg("user", "继续"),
            tool_res("call_A", "结果A"),
        ];
        let out = assemble_history_messages(&history);
        // 期望顺序：assistant(tool_calls) → tool(call_A) → user。
        assert!(out[0].tool_calls.is_some(), "首条应为 assistant tool_calls");
        assert!(matches!(out[1].role, ModelMessageRole::Tool));
        assert_eq!(out[1].tool_call_id.as_deref(), Some("call_A"));
        assert_eq!(out[1].content, "结果A");
        assert!(matches!(out[2].role, ModelMessageRole::User));
        assert_eq!(out.len(), 3, "独立 tool 不重复输出");
    }

    /// 悬空（结果完全缺失）→ 补占位，序列仍合法。
    #[test]
    fn missing_tool_result_gets_placeholder() {
        let history = vec![assistant_tc("call_X"), msg("user", "继续")];
        let out = assemble_history_messages(&history);
        assert!(out[0].tool_calls.is_some());
        assert!(matches!(out[1].role, ModelMessageRole::Tool));
        assert_eq!(out[1].tool_call_id.as_deref(), Some("call_X"));
        assert!(out[1].content.contains("未完成"));
        assert!(matches!(out[2].role, ModelMessageRole::User));
    }

    /// compacted 消息（error/stopped/compaction 标记）跳过，不破坏配对。
    #[test]
    fn compacted_markers_are_skipped() {
        let mut marker = msg("stopped", "上一轮因进程退出未完成");
        marker.compacted = true;
        let history = vec![assistant_tc("call_A"), marker, tool_res("call_A", "r")];
        let out = assemble_history_messages(&history);
        assert_eq!(out.len(), 2);
        assert!(out[0].tool_calls.is_some());
        assert!(matches!(out[1].role, ModelMessageRole::Tool));
    }
}

/// 截断长文本（流式事件 text 用），防止事件过大；超过 limit 字符时在末尾加省略号。
fn truncate_event_text(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let head: String = text.chars().take(limit).collect();
    format!("{head}…")
}

pub fn now_string() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod headless_tests {
    use super::*;

    fn test_engine() -> Engine {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("sw-eng-test-{nanos}.sqlite3"));
        let db = std::sync::Arc::new(crate::storage::AppDatabase::open(path.clone()).unwrap());
        let session = crate::session::SessionStore::open(db.clone()).unwrap();
        let store = std::sync::Arc::new(
            crate::provider::ProviderStore::open(db.clone(), path.with_extension("secret"))
                .unwrap(),
        );
        let gateway = std::sync::Arc::new(crate::provider::ProviderGateway::new(store));
        Engine::new(session, gateway)
    }

    #[test]
    fn retry_backoff_is_exponential_from_500ms() {
        assert_eq!(super::retry_backoff_ms(1), 500);
        assert_eq!(super::retry_backoff_ms(2), 1000);
        assert_eq!(super::retry_backoff_ms(3), 2000);
    }

    #[test]
    fn sleep_cancellable_returns_true_immediately_when_cancelled() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;
        let cancel = Arc::new(AtomicBool::new(true));
        let start = std::time::Instant::now();
        assert!(super::sleep_cancellable(5_000, &cancel));
        assert!(start.elapsed().as_millis() < 200, "应尽快返回，不睡满");
        cancel.store(false, Ordering::Relaxed);
        assert!(!super::sleep_cancellable(20, &cancel));
    }

    #[test]
    fn strip_chip_markers_removes_brackets_keeps_inner() {
        assert_eq!(
            strip_chip_markers("看 ⟦@attachments/a.md⟧ 并用 ⟦技能：x⟧"),
            "看 @attachments/a.md 并用 技能：x"
        );
        // 无标记原样返回。
        assert_eq!(
            strip_chip_markers("普通文本 @x 技能：y"),
            "普通文本 @x 技能：y"
        );
    }

    #[test]
    fn engine_defaults_to_attended() {
        // 默认 headless=false（有人值守，交互）。
        let engine = test_engine();
        assert!(!engine.headless);
    }

    #[test]
    fn with_headless_sets_flag() {
        let engine = test_engine().with_headless(true);
        assert!(engine.headless);
    }

    #[test]
    fn headless_auto_answers_ask_only_in_full_mode() {
        let now = now_string();
        // headless + full → 自动应答 ask_user。
        let engine = test_engine().with_headless(true);
        engine
            .session
            .create_session("s-full", "t", &now, false)
            .unwrap();
        engine
            .session
            .set_session_permission_mode("s-full", Some("full"), &now)
            .unwrap();
        assert!(engine.headless_auto_answers_ask("s-full").unwrap());

        // headless + manual → 仍暂停（needs_attention）。
        engine
            .session
            .create_session("s-manual", "t", &now, false)
            .unwrap();
        engine
            .session
            .set_session_permission_mode("s-manual", Some("manual"), &now)
            .unwrap();
        assert!(!engine.headless_auto_answers_ask("s-manual").unwrap());

        // 非 headless + full → 仍暂停（交互行为不受影响）。
        let attended = test_engine();
        attended
            .session
            .create_session("s-full2", "t", &now, false)
            .unwrap();
        attended
            .session
            .set_session_permission_mode("s-full2", Some("full"), &now)
            .unwrap();
        assert!(!attended.headless_auto_answers_ask("s-full2").unwrap());
    }
}

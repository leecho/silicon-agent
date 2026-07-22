pub mod builder;
mod control_tools;
pub mod event;
pub mod run_registry;

pub use builder::EngineBuilder;
pub use run_registry::{RunGuard, RunRegistry};

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use crate::app_settings::AppSettingsStore;
use crate::context::prompt::system_prompt;
use crate::engine::event::{AgentStreamEvent, StreamEmitter};
use crate::provider::client::{ModelCallRequest, ModelClient, ModelEvent};
use crate::provider::message::{
    ModelImage, ModelMessage, ModelToolCall, ModelToolChoice, ToolSpecForModel,
};
use crate::session::permission::{needs_confirmation, resolve_effective_mode, PermissionMode};
use crate::session::{
    new_id, AskQuestion, PendingAsk, PendingPermission, PendingPlan, Session, SessionStore,
};
use crate::tools::add_artifact::ADD_ARTIFACT_TOOL;
use crate::tools::ask_user::ASK_USER_TOOL;
use crate::tools::collect_agents::COLLECT_AGENTS_TOOL;
use crate::tools::dispatch_agent::DISPATCH_AGENT_TOOL;
use crate::tools::load_skill::LOAD_SKILL_TOOL;
use crate::tools::propose_plan::PROPOSE_PLAN_TOOL;
use crate::tools::propose_soul_update::PROPOSE_SOUL_UPDATE_TOOL;
use crate::tools::remember::REMEMBER_TOOL;
use crate::tools::update_tasks::UPDATE_TASKS_TOOL;
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
/// 检索式召回每轮注入的 Tier2 记忆上限（Tier1 画像/置顶不计入，始终注入）。
const MEMORY_RECALL_LIMIT: usize = 12;
/// 每轮注入的相关情景（会话历史摘要）上限。
const MEMORY_EPISODE_LIMIT: usize = 3;

/// 自动压缩默认触发阈值：本次调用真实 prompt 占模型上下文上限的百分比达到此值即压缩。
const DEFAULT_AUTO_COMPACT_THRESHOLD_PCT: u64 = 90;

/// 引擎暂停时返回的待用户交互（泛化自原单一 `PendingPermission`）：
/// `Permission` 为风险工具待授权；`Ask` 为模型调用 `ask_user` 主动提问待回答。
#[derive(Debug)]
pub enum PendingInteraction {
    Permission(PendingPermission),
    Ask(PendingAsk),
    Plan(PendingPlan),
    /// 内部信号（非用户交互）：父 run 本轮派发了**一批** child 子运行（并行），需由 spawn 编排层
    /// 为每个 child 启动 child run。父会话此刻已停泊（`awaiting_subagent` 已落库、各 dispatch tool
    /// result 未写），待整批 child 全部回填后再续跑父。
    Subagent {
        child_session_ids: Vec<String>,
    },
}

/// 一批工具执行的内部结果：`Paused` 命中需暂停的交互（权限确认或 ask_user 提问）；
/// `Continue` 携带更新后的事件序号。
enum ControlFlow {
    Paused(PendingInteraction),
    Continue(u64),
}

/// 项目 PM 编排 SOP：注入 lead 系统提示，指导其在群聊中直接答复 / 路由给成员 / 拆解派发任务。
const PROJECT_PM_SOP: &str = "你是这个项目群聊的主持人（项目经理 PM）。群里有若干成员专家，名册见下方「可调度成员」。\
面对用户消息，你自行决定如何推进：能直接答复的（闲聊/澄清/汇总）直接回答；需要某成员专业回应的，\
用 dispatch_agent 前台派给该成员让其回一条；需要实际产出或多步骤干活的，先用 update_tasks 把本轮计划**一次列全**：goal 写本轮主任务基调，\
tasks 既含委派给成员的子任务（标 assignee=成员 name），也含你自己要做的步骤（assignee 留空=自办，\
例如最后一条「汇总各成员产出、产出最终报告并回复用户」）。然后用 dispatch_agent(task_id=该子任务 id) \
把委派子任务派给对应成员——**具体派发方式（前台逐个 / 后台批量+collect）见下方「团队」段的「派发方式」说明，按当前执行模式来**。\
委派子任务状态随运行自动更新，不必手动改、也不要反复重列整张表。\
派活要把任务说清、指明产出；成员在共享工作目录里干活并用 add_artifact 登记交付物。\
只调度名册内成员，用中文与用户交流。";

pub struct Engine {
    session: SessionStore,
    /// 长期记忆 store（注入则启用记忆注入与 remember 落库）；测试可不注入，缺省时记忆为空。
    memory: Option<crate::memory::MemoryStore>,
    /// 知识库 store（注入则启用 search_knowledge 检索）；测试可不注入，缺省时检索为空。
    knowledge: Option<std::sync::Arc<crate::knowledge::KnowledgeStore>>,
    /// 知识库向量化器（注入则 search_knowledge 可走混合检索）；None/未启用时纯 BM25。
    embedder: Option<std::sync::Arc<dyn crate::knowledge::embed::Embedder>>,
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
    /// 插件服务（注入则按 plugin.enabled 级联隐藏其下 skill）；测试可不注入。
    plugins: Option<Arc<crate::plugin::PluginService>>,
    /// 团队服务（注入则按会话角色槽解析 team 的 SOP/roster/私有 skill）；测试可不注入。
    teams: Option<Arc<crate::team::TeamService>>,
    /// expert 服务（注入则把启用角色作为「团队」清单注入 system prompt）；测试可不注入。
    experts: Option<Arc<crate::expert::ExpertService>>,
    /// 伴随体（agent 实例）服务：会话 role-kind="agent" 时注入其 instructions + 引用源 expert 技能 + 私有记忆作用域。
    agents: Option<Arc<crate::agent::AgentService>>,
    /// 项目服务（注入则支持 project_id 上下文：按 project_members 解析 lead/roster）；测试可不注入。
    projects: Option<Arc<crate::project::ProjectService>>,
    /// T66：plugin hooks 注册表（注入则在工具/会话生命周期点触发其 command hooks）；None 时全程短路。
    hooks: Option<Arc<crate::hook::HookService>>,
    /// system prompt 覆盖（child 子运行用 agent 角色的 system_prompt）；None 时走默认装配。
    /// 注入时**不**再注入「团队」清单——child 是叶子，不再下派（§6.6）。
    system_prompt_override: Option<String>,
    /// child 子运行的 expert name：用于追加该 expert 的私有 skill。
    private_skills_expert: Option<String>,
    /// 额外注入的 team 私有 skill scope（项目成员快照子运行：继承其源团队的私有 skill）。
    private_skills_team_ids: Vec<String>,
    /// 本会话解析后的模型选择（provider+model）；None 时调用方退回 Gateway 默认。
    selection: Option<crate::provider::model::ResolvedModel>,
    /// 本会话所选模型是否支持图像输入（已解析：每模型覆盖 ∨ 内置查表）；缺省 false。
    /// 决定附件图片是读图发送还是降级为文本占位（见 `assemble_history_messages`）。
    supports_vision: bool,
    /// 无人值守（headless）运行：定时任务触发时为 true。工具权限仍由会话 permission_mode 决定；
    /// 仅影响 ask_user —— headless 且 full 模式下自动应答继续，否则照常暂停（needs_attention）。
    headless: bool,
    /// T57：后台子代理启动回调（注入则 dispatch(background=true) 即时启动 child run）；
    /// 由 app_state 注入（捕获 AppHandle → spawn_child_run）。仅顶层父引擎需要（child 不递归派发）。
    child_spawner: Option<Arc<dyn Fn(&str) + Send + Sync>>,
}

impl Engine {
    pub fn new(session: SessionStore, client: Arc<dyn ModelClient>) -> Self {
        Self {
            session,
            memory: None,
            knowledge: None,
            embedder: None,
            app_settings: None,
            client,
            emitter: None,
            registry: ToolRegistry::new(),
            usage: None,
            workspace: String::new(),
            skills: None,
            plugins: None,
            teams: None,
            experts: None,
            agents: None,
            projects: None,
            hooks: None,
            system_prompt_override: None,
            private_skills_expert: None,
            private_skills_team_ids: Vec::new(),
            selection: None,
            supports_vision: false,
            headless: false,
            child_spawner: None,
        }
    }

    /// T57：注入后台子代理启动回调。
    pub fn with_child_spawner(mut self, spawner: Arc<dyn Fn(&str) + Send + Sync>) -> Self {
        self.child_spawner = Some(spawner);
        self
    }

    /// T57：即时启动一个后台 child run（无回调注入时静默忽略——如测试/child 引擎）。
    pub(super) fn spawn_child(&self, child_id: &str) {
        if let Some(spawner) = &self.child_spawner {
            spawner(child_id);
        }
    }

    /// 注入长期记忆 store。注入则启用「已知记忆」注入与 remember 落库；未注入时记忆为空。
    pub fn with_memory(mut self, memory: crate::memory::MemoryStore) -> Self {
        self.memory = Some(memory);
        self
    }

    /// 注入知识库 store。注入则启用 search_knowledge 检索；未注入时返回「未启用」提示。
    pub fn with_knowledge(mut self, knowledge: std::sync::Arc<crate::knowledge::KnowledgeStore>) -> Self {
        self.knowledge = Some(knowledge);
        self
    }

    /// 注入向量化器。注意：embedder 持有的 embedding 模型 id 在**引擎构造时快照**，
    /// 会话存续期间用户改模型不影响当前会话（新会话生效）；而向量开关是每次检索现读。
    /// 注入则 search_knowledge 可走向量/混合检索；未注入时纯 BM25。
    pub fn with_embedder(mut self, embedder: std::sync::Arc<dyn crate::knowledge::embed::Embedder>) -> Self {
        self.embedder = Some(embedder);
        self
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

    /// 注入子代理服务（用于把启用角色注入「团队」清单）。
    pub fn with_experts(mut self, experts: Arc<crate::expert::ExpertService>) -> Self {
        self.experts = Some(experts);
        self
    }

    /// 注入伴随体（agent 实例）服务（会话 role-kind="agent" 时按 id 解析并注入）。
    pub fn with_agents(mut self, agents: Arc<crate::agent::AgentService>) -> Self {
        self.agents = Some(agents);
        self
    }

    /// 注入 system prompt 覆盖（child 子运行用 agent 角色 prompt；注入时不再注入「团队」清单）。
    /// child 子运行注入该 agent 的私有 skill（owner=agent name）。
    pub fn with_private_skills_expert(mut self, agent: String) -> Self {
        self.private_skills_expert = Some(agent);
        self
    }

    /// 追加 team 私有 skill scope（项目成员快照子运行：继承其源团队私有 skill）。空项忽略。
    pub fn with_private_skills_team_ids(mut self, team_ids: Vec<String>) -> Self {
        self.private_skills_team_ids = team_ids
            .into_iter()
            .filter(|id| !id.trim().is_empty())
            .collect();
        self
    }

    pub fn with_system_prompt_override(mut self, prompt: String) -> Self {
        self.system_prompt_override = Some(prompt);
        self
    }

    /// 注入插件服务（用于按 plugin.enabled 级联过滤其下 skill 的可见性）。
    pub fn with_plugins(mut self, plugins: Arc<crate::plugin::PluginService>) -> Self {
        self.plugins = Some(plugins);
        self
    }

    /// 注入团队服务（用于按会话角色槽解析 team 的 SOP/roster/私有 skill）。
    pub fn with_teams(mut self, teams: Arc<crate::team::TeamService>) -> Self {
        self.teams = Some(teams);
        self
    }

    /// 注入项目服务（启用 project_id 上下文的角色解析与 dispatch 成员校验）。
    pub fn with_projects(mut self, projects: Arc<crate::project::ProjectService>) -> Self {
        self.projects = Some(projects);
        self
    }

    /// T66：注入 plugin hooks 注册表。注入则在工具/会话生命周期点触发；None 时全程短路。
    pub fn with_hooks(mut self, hooks: Arc<crate::hook::HookService>) -> Self {
        self.hooks = Some(hooks);
        self
    }

    /// 项目角色解析：读 project_members → coordinator 正文 + PM SOP 作 lead；其余成员作 roster。
    /// 返回 (roster, project_label, lead_sop)。无 projects/agents 注入时返回空。
    pub fn project_run_roster(
        &self,
        project_id: &str,
    ) -> (
        Vec<crate::expert::ExpertSummary>,
        Option<String>,
        Option<String>,
    ) {
        let Some(projects) = self.projects.as_ref() else {
            return (Vec::new(), None, None);
        };
        let project = projects.get(project_id).ok().flatten();
        let members = projects.list_members(project_id).unwrap_or_default();
        // lead = PM 编排 SOP + 项目指令(章程)为主 + 可选协调者成员人设(叠加)。
        let instructions = project
            .as_ref()
            .map(|p| p.instructions.clone())
            .unwrap_or_default();
        // 成员可能是项目私有快照（从团队导入）或散装引用：统一经 ProjectService 解析。
        let coordinator_sop = members
            .iter()
            .find(|m| m.is_coordinator)
            .and_then(|m| projects.member_spec(m))
            .map(|s| s.system_prompt)
            .unwrap_or_default();
        let roster = members
            .iter()
            .filter(|m| !m.is_coordinator)
            .filter_map(|m| projects.member_summary(m))
            .collect();
        let label = project.as_ref().map(|p| p.name.clone());
        let mut parts = vec![PROJECT_PM_SOP.to_string()];
        if !instructions.trim().is_empty() {
            parts.push(instructions);
        }
        if !coordinator_sop.trim().is_empty() {
            parts.push(coordinator_sop);
        }
        let sop = Some(parts.join("\n\n"));
        (roster, label, sop)
    }

    /// 该 name 是否为某项目的成员（dispatch 校验用）。
    pub fn is_project_member(&self, project_id: &str, name: &str) -> bool {
        self.projects
            .as_ref()
            .and_then(|p| p.list_members(project_id).ok())
            .map(|ms| ms.iter().any(|m| m.expert_name == name))
            .unwrap_or(false)
    }

    /// 解析被派发成员的展示名（供子会话标题）：team→成员展示名；project→成员展示名；
    /// 否则散装/全局按名。解析不到（含 ad-hoc）返回 None，由调用方回退原始 name。
    pub(crate) fn dispatch_display_name(
        &self,
        role_kind: &str,
        role_id: &str,
        name: &str,
    ) -> Option<String> {
        match role_kind {
            "team" if !role_id.is_empty() => self
                .teams
                .as_ref()
                .and_then(|t| t.detail(role_id).ok())
                .and_then(|d| d.members.into_iter().find(|m| m.name == name))
                .and_then(|m| m.display_name),
            "project" if !role_id.is_empty() => self
                .projects
                .as_ref()
                .and_then(|p| p.get_member_by_name(role_id, name).ok().flatten())
                .and_then(|m| m.display_name),
            _ => self.experts.as_ref().and_then(|a| {
                a.summary_by_owner("", "", name)
                    .or_else(|| a.summary_by_name(name))
                    .and_then(|s| s.display_name)
            }),
        }
    }

    /// 注入本会话解析后的模型选择。
    pub fn with_selection(
        mut self,
        selection: Option<crate::provider::model::ResolvedModel>,
    ) -> Self {
        self.selection = selection;
        self
    }

    /// 注入本会话所选模型的 vision 能力（已解析）。
    pub fn with_supports_vision(mut self, supports_vision: bool) -> Self {
        self.supports_vision = supports_vision;
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
    /// run_loop 外壳：首轮前触发 SessionStart、收口触发 Stop（覆盖所有退出点，含暂停/错误）。
    /// hooks=None 时两处均短路。SessionStart/Stop 为观察性，不影响循环结果。
    ///
    /// 注：暂停（等权限/提问/子代理）也会触发 Stop——v1 取「本次 run_loop 调用结束」语义；
    /// 续跑（resume）重入时再触发一次 SessionStart，保持事件成对、实现最小化。
    fn run_loop(
        &self,
        session_id: &str,
        cancel: Arc<AtomicBool>,
        heartbeat: Arc<AtomicU64>,
    ) -> Result<(Session, Option<PendingInteraction>), String> {
        self.run_session_hooks("SessionStart", session_id);
        let outcome = self.run_loop_inner(session_id, cancel, heartbeat);
        self.run_session_hooks("Stop", session_id);
        outcome
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

        // 循环外算一次：主会话（非子代理）下，「浏览器操作」「桌面操作」总开关开则自动激活对应工具，
        // 用户无需在面板手动「开启」；子代理仍按白名单经 find_tools 激活，避免每个专家平白多带一份工具。
        let is_main_session = self
            .session
            .get_session_detail(session_id)
            .ok()
            .flatten()
            .map_or(true, |s| s.session.origin != "subagent");
        let auto_activate_browser = is_main_session
            && self
                .app_settings
                .as_ref()
                .map_or(false, |s| s.get_browser_use_enabled().unwrap_or(false));
        let auto_activate_computer = is_main_session
            && self
                .app_settings
                .as_ref()
                .map_or(false, |s| s.get_computer_use_enabled().unwrap_or(false));

        // 渐进式披露（T83）：Deferred 工具目录（name+desc），注入 system prompt 引导 find_tools。
        // 全集稳定（不随激活收缩）以保 prompt cache；plan 模式过滤写工具，与只读闸门一致。
        let deferred_catalog: Vec<(String, String)> = self
            .registry
            .specs()
            .into_iter()
            .filter(|s| s.disclosure == crate::tools::Disclosure::Deferred)
            .filter(|s| {
                if mode == "plan" {
                    !self
                        .registry
                        .get(&s.name)
                        .map(|t| t.requires_confirmation())
                        .unwrap_or(false)
                } else {
                    true
                }
            })
            .map(|s| (s.name, s.description))
            .collect();

        // 循环外取一次启用技能（名+简介注入 system prompt，渐进式披露）。
        // 级联：所属插件被禁用的 skill 不注入（plugin.enabled=false → 其下 skill 不可见）。
        let disabled_plugins: std::collections::HashSet<String> = self
            .plugins
            .as_ref()
            .map(|p| p.disabled_plugin_ids().unwrap_or_default())
            .unwrap_or_default();
        let mut enabled_skills = self
            .skills
            .as_ref()
            .map(|s| s.list_enabled().unwrap_or_default())
            .unwrap_or_default()
            .into_iter()
            .filter(|s| {
                s.plugin_id
                    .as_ref()
                    .map(|pid| !disabled_plugins.contains(pid))
                    .unwrap_or(true)
            })
            .collect::<Vec<_>>();
        // 会话实体归属 + 运行角色（提前读取，供记忆作用域 + 下面的角色解析共用）。
        let (role_kind, role_id, mem_project_id, mem_agent_id) = self
            .session
            .get_session(session_id)
            .ok()
            .flatten()
            .map(|s| {
                let project_id = s.project_id.unwrap_or_default();
                let agent_id = s.agent_id.unwrap_or_default();
                let explicit_role_kind = s.role_kind.unwrap_or_default();
                let explicit_role_id = s.role_id.unwrap_or_default();
                let (run_kind, run_id) = if !project_id.is_empty() {
                    ("project".to_string(), project_id.clone())
                } else if !explicit_role_kind.is_empty() {
                    (explicit_role_kind, explicit_role_id)
                } else if !agent_id.is_empty() {
                    ("agent".to_string(), agent_id.clone())
                } else {
                    (String::new(), String::new())
                };
                (run_kind, run_id, project_id, agent_id)
            })
            .unwrap_or_default();
        // 记忆作用域：project_id 优先 → agent_id 私有 → 全局（读写共用）。
        let mem_scope = crate::memory::MemoryScope::from_session(&mem_project_id, &mem_agent_id);
        // 循环外取一次长期记忆：检索式召回——以本会话最近一条用户消息为 query，
        // 注入 Tier1（画像/置顶，始终）+ Tier2（fact 的 FTS5 top-K）。query 为本次 run 固定值。
        let recall_query = self
            .session
            .list_messages(session_id)
            .ok()
            .and_then(|msgs| {
                msgs.iter()
                    .rev()
                    .find(|m| m.role == "user")
                    .map(|m| m.content.clone())
            })
            .unwrap_or_default();
        // 渲染记忆段：用户画像（Tier1 常驻）+ 相关记忆（检索 top-K）。由 memory::prompt 预渲染。
        let memory_block = self
            .memory
            .as_ref()
            .map(|m| {
                let profile = m.get_profile().ok().flatten();
                let facts = m
                    .recall(&recall_query, MEMORY_RECALL_LIMIT, mem_scope)
                    .unwrap_or_default();
                let episodes = m
                    .recall_episodes(&recall_query, MEMORY_EPISODE_LIMIT, mem_scope)
                    .unwrap_or_default();
                crate::memory::prompt::render(profile.as_deref(), &facts, &episodes)
            })
            .unwrap_or_default();
        // 运行上下文：project → 项目 PM；team → roster+lead SOP；expert → 专家人设；
        // agent → 持久智能体实例；无激活 → 自由模式。
        let (enabled_experts, team_label, persona, team_sop): (
            Vec<crate::expert::ExpertSummary>,
            Option<String>,
            Option<String>,
            Option<String>,
        ) = match (role_kind.as_str(), &self.teams, &self.experts) {
            ("team", Some(teams), _) if !role_id.is_empty() => {
                // 团队：roster = 成员（解析后含展示身份）；lead 正文作团队协作 SOP；team 私有 skill 入池。
                let detail = teams.detail(&role_id).ok();
                let (lead_spec, _roster_specs) = teams
                    .resolve_for_run(&role_id)
                    .unwrap_or((None, Vec::new()));
                let label = detail.as_ref().map(|d| d.team.display_name.clone());
                let roster = detail.map(|d| d.members).unwrap_or_default();
                let sop = lead_spec.map(|s| s.system_prompt);
                // 追加该 team 的私有可见 skill（仅本会话）。
                if let Some(skills) = self.skills.as_ref() {
                    if let Ok(priv_skills) = skills.list_enabled_by_team(&role_id) {
                        enabled_skills.extend(priv_skills);
                    }
                }
                (roster, label, None, sop)
            }
            ("expert", _, Some(agents)) if !role_id.is_empty() => {
                // 散装 expert 作主对话人设：role_id 存稳定 expert id，不按 name 兜底。
                let persona = agents
                    .load_spec_by_id(&role_id)
                    .ok()
                    .flatten()
                    .map(|spec| {
                        let who = if spec.name.is_empty() {
                            role_id.clone()
                        } else {
                            spec.name.clone()
                        };
                        format!(
                            "你现在以「{who}」的身份与用户对话。以下人设与行事准则贯穿整个对话，优先于默认助手设定：\n\n{}",
                            spec.system_prompt
                        )
                    });
                // 私有 skill 当前 owner 仍是 expert name（导入/项目/团队成员快照沿用 name 货币）。
                if let (Some(skills), Some(expert)) =
                    (self.skills.as_ref(), agents.summary_by_id(&role_id))
                {
                    if let Ok(priv_skills) = skills.list_enabled_by_expert(&expert.name) {
                        enabled_skills.extend(priv_skills);
                    }
                }
                (Vec::new(), None, persona, None)
            }
            ("agent", _, _) if !role_id.is_empty() => {
                // 伴随体实例（T69/T74）：人设 = IDENTITY(锚) ⧺ SOUL(instructions)；技能 = 全局 ∪ 引用源 expert 私有 skill。
                let companion = self
                    .agents
                    .as_ref()
                    .and_then(|a| a.get_by_id(&role_id).ok().flatten());
                let persona = companion.as_ref().map(|c| {
                    let who = c
                        .display_name
                        .clone()
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| c.name.clone());
                    // T74：注入正文 = IDENTITY(锚) ⧺ SOUL(instructions)；identity 空时与拆分前逐字等价。
                    let body = crate::agent::model::compose_persona(&c.identity, &c.instructions);
                    format!(
                        "你现在以「{who}」的身份与用户对话。以下人设与行事准则贯穿整个对话，优先于默认助手设定：\n\n{}",
                        body
                    )
                });
                // 引用源 expert 的私有 skill（source_expert_id=expert 名）；源 expert 已删则跳过、仅全局池（降级不崩）。
                if let (Some(c), Some(skills)) = (companion.as_ref(), self.skills.as_ref()) {
                    if let Some(src) = c.source_expert_id.as_deref().filter(|s| !s.is_empty()) {
                        if let Ok(priv_skills) = skills.list_enabled_by_expert(src) {
                            enabled_skills.extend(priv_skills);
                        }
                    }
                }
                (Vec::new(), None, persona, None)
            }
            ("project", _, Some(_)) if !role_id.is_empty() => {
                // 项目：coordinator 成员正文 + PM 编排 SOP 作 lead；其余成员作 roster（可被 dispatch）。
                let (roster, label, sop) = self.project_run_roster(&role_id);
                // 项目成员若来自团队快照，PM/lead 运行继承各源团队的私有 skill（软引用，让被复制进
                // 项目的团队成员仍能用到其团队技能）。
                if let (Some(projects), Some(skills)) =
                    (self.projects.as_ref(), self.skills.as_ref())
                {
                    if let Ok(project_skills) =
                        crate::project::runtime_skills::list_project_runtime_skills(
                            projects, skills, &role_id,
                        )
                    {
                        enabled_skills.extend(project_skills);
                    }
                }
                (roster, label, None, sop)
            }
            // 自由模式（无激活团队/专家/项目）：**不注入任何专家名册**。
            //
            // 专家只在被**显式选用**时才进上下文——激活团队（roster）或选中专家（persona）。
            // 曾经这里注入 `list_enabled()` 全量专家，有两个毛病：
            // ① 上下文随装包数线性膨胀（装 N 个团队包 = N×成员数 行常驻 system prompt）；
            // ② 更糟的是它**是错的**——名册来自 `list_enabled()`（不带 owner 过滤），而
            //    dispatch_agent 在自由模式下只解析 owner 为空的散装专家
            //    （`load_spec_by_owner("", "", name)`）。于是插件带来的专家会被列进名册、
            //    却根本派不动，模型照着派就吃「没有现成专家」。
            // 想让多个现成专家协作，请组建团队——编排本就是 team 的职责。
            _ => (Vec::new(), None, None, None),
        };

        // child 子运行：追加其 agent 的私有 skill（实体 agent 路径已在 "agent" 分支处理）。
        if let (Some(a), Some(skills)) = (self.private_skills_expert.as_ref(), self.skills.as_ref())
        {
            if let Ok(priv_skills) = skills.list_enabled_by_expert(a) {
                enabled_skills.extend(priv_skills);
            }
        }
        // child 子运行：项目成员快照继承其源团队的私有 skill（owner=team_id）。
        if let Some(skills) = self.skills.as_ref() {
            for team_id in &self.private_skills_team_ids {
                if let Ok(priv_skills) = skills.list_enabled_by_team(team_id) {
                    enabled_skills.extend(priv_skills);
                }
            }
        }

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

            // 渐进式披露（T83）：每轮按「核心集 ∪ 会话已激活集」重算 tools[]——激活发生在循环中
            //（模型本轮调 find_tools，下一轮须见到新工具）。未注册的名字不会出现在 specs，
            // 自然被排除（MCP 断连失效项即此剔除）。
            let mut activated: std::collections::HashSet<String> = self
                .session
                .list_activated_tools(session_id)?
                .into_iter()
                .collect();
            // 总开关开的主会话：自动把 browser / computer 计入已激活集（跟随开关、无需持久化的逐会话激活记录）。
            if auto_activate_browser {
                activated.insert(crate::tools::browser::BROWSER_TOOL.to_string());
            }
            if auto_activate_computer {
                activated.insert(crate::tools::computer::COMPUTER_TOOL.to_string());
            }
            let tool_specs: Vec<ToolSpecForModel> = self
                .registry
                .specs()
                .into_iter()
                .filter(|spec| {
                    let requires_confirmation = self
                        .registry
                        .get(&spec.name)
                        .map(|t| t.requires_confirmation())
                        .unwrap_or(false);
                    crate::tools::include_in_tools(
                        spec.disclosure,
                        activated.contains(&spec.name),
                        requires_confirmation,
                        &mode,
                        &spec.name,
                    )
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

            // 2. 组装上下文（system + 可选压缩摘要 + 历史，OpenAI 角色形态）。
            //    compact 只影响"喂给模型的上下文"：已 compacted 的旧消息以摘要 system 替代，
            //    遍历时跳过；消息本身仍持久化、feed 显示不变。
            let history = self.session.list_messages(session_id)?;
            // 子代理执行方式（串/并）：注入派发段措辞，让统筹者按实际并发语义规划任务先后。
            // 运行时强制由 RunCoordinator 调度；此处仅影响提示词。缺省（无 app_settings）按并行。
            let subagent_serial = self
                .app_settings
                .as_ref()
                .and_then(|s| s.get_subagent_execution_mode().ok())
                .map(|m| m == "serial")
                .unwrap_or(false);
            let sys = match &self.system_prompt_override {
                // child 子运行：agent 正文作「当前身份」，并保留基底脚手架（工作目录/技能/时间/记忆），
                // 但去掉「团队/派发」段（叶子，不再下派；registry 也已剔除 dispatch_agent）。
                Some(p) => system_prompt(
                    &enabled_skills,
                    &[],
                    &memory_block,
                    &mode,
                    &self.workspace,
                    None,
                    Some(p),
                    None,
                    false,
                    false,
                    &deferred_catalog,
                ),
                None => system_prompt(
                    &enabled_skills,
                    &enabled_experts,
                    &memory_block,
                    &mode,
                    &self.workspace,
                    team_label.as_deref(),
                    persona.as_deref(),
                    team_sop.as_deref(),
                    true,
                    subagent_serial,
                    &deferred_catalog,
                ),
            };
            let mut messages = vec![ModelMessage::system(&sys)];
            if let Some(summary) = self.session.get_compaction_summary(session_id)? {
                messages.push(ModelMessage::system(&format!(
                    "以下是早前对话的摘要(已压缩)：\n{summary}"
                )));
            }
            // 历史 → provider 消息：唯一出口，在此强制 tool_call↔tool_result 配对不变式（详见 assemble_history_messages）。
            // 附件图片展开/降级按本会话所选模型的 vision 能力（self.supports_vision）。
            messages.extend(assemble_history_messages(
                &history,
                std::path::Path::new(&self.workspace),
                self.supports_vision,
            ));

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
                    // 调用日志归因（T76）：private_skills_expert 仅在 child 子运行注入，
                    // 据此区分 sub_agent / main_agent，并带上专家名。
                    attribution: crate::provider::message::ModelAttribution {
                        session_id: session_id.to_string(),
                        message_id: Some(assistant_id.clone()),
                        usage_type: Some(
                            if self.private_skills_expert.is_some() {
                                "sub_agent"
                            } else {
                                "main_agent"
                            }
                            .to_string(),
                        ),
                        expert_name: self.private_skills_expert.clone(),
                        ..Default::default()
                    },
                    ..Default::default()
                };

                let attempt_result = self.client.stream_model_with_events(request, &cancel, &mut |event| {
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
                            // 带上瞬时错误原因（限流/超时/连接等），便于用户与排查看到「为什么在重试」。
                            let reason = e.message.trim();
                            let retry_text = if reason.is_empty() {
                                format!("第 {retry_attempt}/{auto_retry_max} 次重试…")
                            } else {
                                let short: String = reason.chars().take(60).collect();
                                format!("第 {retry_attempt}/{auto_retry_max} 次重试：{short}")
                            };
                            self.emit(AgentStreamEvent {
                                kind: "model_retrying".into(),
                                session_id: session_id.into(),
                                message_id: assistant_id.clone(),
                                sequence: 0,
                                text: Some(retry_text),
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
                                // 情景层：把本次压缩摘要投递为一条 episode（FTS 可召回的会话历史）。
                                if let Some(memory) = self.memory.as_ref() {
                                    if let Ok(Some(summary)) =
                                        self.session.get_compaction_summary(session_id)
                                    {
                                        let _ = memory.add_episode(
                                            session_id,
                                            &summary,
                                            &now_string(),
                                            mem_scope,
                                        );
                                    }
                                }
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
        let risk_args = serde_json::from_str::<serde_json::Value>(&call.arguments_json)
            .unwrap_or(serde_json::Value::Null);
        let risk = self
            .registry
            .get(&call.name)
            .map(|t| t.risk_for(&risk_args))
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
        cancel: &std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) -> Result<ControlFlow, String> {
        // 本批并行派发的 child 会话 id（dispatch_agent 不即停泊，整批攒齐后统一停泊一次）。
        let mut dispatched_children: Vec<String> = Vec::new();
        let mut i = 0;
        while i < calls.len() {
            // 运行已被取消：不再执行本批后续工具（尤其 dispatch_agent，避免停止后同一轮还派发新成员）。
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            // 探测从 i 起的极大连续可并行段。
            let mut j = i;
            while j < calls.len() && self.is_parallel_eligible(&calls[j]) {
                j += 1;
            }
            if j - i >= 2 {
                self.execute_parallel_group(session_id, &calls[i..j], &mut sequence, cancel)?;
                i = j;
                continue;
            }
            // 单调用（段=1 或非并行）走原内联串行路径。
            let call = &calls[i];
            i += 1;
            // tool_call 事件先 emit（发起，完整 args，截断至 2000 字符防事件过大）。
            self.emit_tool_call_running(session_id, call, &mut sequence);

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
            if call.name == DISPATCH_AGENT_TOOL {
                // 不即返回停泊：建 child（或落 failed）后 continue，让同批其余 dispatch 也启动（并行）。
                // background=true 的派发由 prepare 内部即发即返(写占位结果+即时启动)，返回 None,不进 park 集。
                if let Some(child_id) =
                    self.prepare_dispatch_agent(session_id, call, &mut sequence)?
                {
                    dispatched_children.push(child_id);
                }
                continue;
            }
            if call.name == COLLECT_AGENTS_TOOL {
                match self.handle_collect_agents(session_id, call, &mut sequence)? {
                    Some(cf) => return Ok(cf), // 有未完成且 wait → 父停泊在 collect
                    None => continue,          // 立即写结果，继续本批
                }
            }
            if call.name == REMEMBER_TOOL {
                self.handle_remember(session_id, call, &mut sequence)?;
                continue;
            }
            if call.name == crate::tools::search_knowledge::SEARCH_KNOWLEDGE_TOOL {
                self.handle_search_knowledge(session_id, call, &mut sequence)?;
                continue;
            }
            if call.name == PROPOSE_SOUL_UPDATE_TOOL {
                self.handle_propose_soul_update(session_id, call, &mut sequence)?;
                continue;
            }
            if call.name == LOAD_SKILL_TOOL {
                self.handle_load_skill(session_id, call, &mut sequence)?;
                continue;
            }
            if call.name == crate::tools::find_tools::FIND_TOOLS_TOOL {
                self.handle_find_tools(session_id, mode, call, &mut sequence)?;
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
            if call.name == UPDATE_TASKS_TOOL {
                self.handle_update_tasks(session_id, call, &mut sequence)?;
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
                self.finalize_tool_result(session_id, call, &msg, "done", &mut sequence)?;
                continue;
            }

            // 权限闸：按生效权限模式 + 工具风险级别判定是否需确认。
            // 定时任务通过设置会话 permission_mode（如 full）来自动放行，无需额外特判。
            // 风险按本次参数动态判定（T90 risk_for）：默认回落静态 risk_level。
            let risk_args = serde_json::from_str::<serde_json::Value>(&call.arguments_json)
                .unwrap_or(serde_json::Value::Null);
            let risk = self
                .registry
                .get(&call.name)
                .map(|t| t.risk_for(&risk_args))
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

            // T66：PreToolUse hooks（仅对 registry 真实工具，控制工具已在上方 continue/return）。
            // 任一 block → 不执行该工具、落 blocked 结果并 emit、回灌给模型，继续本批。
            // PreToolUse 只能拦不能放——不阻止即照常执行（hooks=None/无匹配时 None）。
            if let Some(reason) = self.run_pre_tool_hooks(session_id, &call.name, &args) {
                let msg = format!("插件 hook 阻止了工具「{}」：{reason}", call.name);
                self.finalize_tool_result(session_id, call, &msg, "blocked", &mut sequence)?;
                continue;
            }

            let effective_secs = self.resolve_tool_timeout(&call.name)?;
            let (result_text, tool_status) =
                self.execute_with_timeout(&call.name, &args, effective_secs, cancel);
            // T66：PostToolUse hooks（观察性，不阻断）。
            self.run_post_tool_hooks(session_id, &call.name, &args, &result_text, tool_status);
            self.finalize_tool_result(session_id, call, &result_text, tool_status, &mut sequence)?;
        }
        // 本批有 dispatch：父停泊一次（awaiting_subagent 作 busy 标记记首个 child），返回整批 child
        // id 让编排层并行启动；待整批全部回填后再续跑父（见 finish_child_into_parent）。
        if !dispatched_children.is_empty() {
            let now = now_string();
            self.session
                .set_awaiting_subagent(session_id, &dispatched_children[0], &now)?;
            return Ok(ControlFlow::Paused(PendingInteraction::Subagent {
                child_session_ids: dispatched_children,
            }));
        }
        Ok(ControlFlow::Continue(sequence))
    }

    /// emit 一条 "tool_call" running 事件（发起，args 截断 2000）。sequence 自增。
    fn emit_tool_call_running(&self, session_id: &str, call: &ModelToolCall, sequence: &mut u64) {
        *sequence += 1;
        self.emit(AgentStreamEvent {
            kind: "tool_call".into(),
            session_id: session_id.into(),
            message_id: call.id.clone(),
            sequence: *sequence,
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
    }

    /// 落一条 tool 结果并 emit "tool_result"（status 任意：done/failed/blocked/timeout...）。sequence 自增。
    /// 不含 Pre/PostToolUse hook —— 调用方按需在调用前后处理。
    fn finalize_tool_result(
        &self,
        session_id: &str,
        call: &ModelToolCall,
        result_text: &str,
        status: &str,
        sequence: &mut u64,
    ) -> Result<(), String> {
        let result_at = now_string();
        self.session.append_tool_result(
            &new_id("msg"),
            session_id,
            &call.id,
            &call.name,
            result_text,
            status,
            &result_at,
        )?;
        *sequence += 1;
        self.emit(AgentStreamEvent {
            kind: "tool_result".into(),
            session_id: session_id.into(),
            message_id: call.id.clone(),
            sequence: *sequence,
            text: Some(truncate_event_text(result_text, 2000)),
            status: Some(status.to_string()),
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
        Ok(())
    }

    /// 解析某工具的生效超时秒数：工具级 timeout_secs() 覆盖优先，否则全局默认。
    fn resolve_tool_timeout(&self, name: &str) -> Result<u64, String> {
        if let Some(tool) = self.registry.get(name) {
            if let Some(secs) = tool.timeout_secs() {
                return Ok(secs);
            }
        }
        self.app_settings
            .as_ref()
            .map(|s| s.get_tool_timeout_secs())
            .unwrap_or(Ok(crate::app_settings::DEFAULT_TOOL_TIMEOUT_SECS))
    }

    /// 带超时执行单个 registry 工具：clone registry（Arc 复用，廉价）搬入 worker 线程，
    /// 复用 registry.execute 的 8KB 截断与未知工具处理。返回 (结果文本, 持久化 status)。
    fn execute_with_timeout(
        &self,
        name: &str,
        args: &serde_json::Value,
        effective_secs: u64,
        cancel: &std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) -> (String, &'static str) {
        let reg = self.registry.clone();
        let name_owned = name.to_string();
        let args_owned = args.clone();
        // clone owned Arc 进 'static worker 闭包，令进程类工具（run_command）在取消时 kill 子进程。
        let cancel_worker = cancel.clone();
        run_with_timeout(
            move || reg.execute_cancellable(&name_owned, &args_owned, &cancel_worker),
            effective_secs,
            cancel,
            name,
        )
    }

    /// 生效并行上限（app_settings 缺省 8；无 settings 回退 8）。
    fn resolve_tool_parallelism(&self) -> Result<usize, String> {
        let n = self
            .app_settings
            .as_ref()
            .map(|s| s.get_tool_parallelism())
            .unwrap_or(Ok(crate::app_settings::DEFAULT_TOOL_PARALLELISM))?;
        Ok(n as usize)
    }

    /// 该调用能否进并行段：非控制工具 且 registry 工具 concurrency_safe（只读）。
    fn is_parallel_eligible(&self, call: &ModelToolCall) -> bool {
        !is_control_tool(&call.name)
            && self
                .registry
                .get(&call.name)
                .map(|t| t.concurrency_safe())
                .unwrap_or(false)
    }

    /// 并行执行一段连续的、可并行资格的 registry 工具（段长 >= 2）。
    /// 三相：相1 串行 emit running + parse + PreHook；相2 并行 execute_with_timeout（按上限分块）；
    /// 相3 串行 finalize。段内全为只读 Safe 工具：恒不触发计划模式/权限闸，故此处不重复这两闸。
    /// cancel 由各分支的 execute_with_timeout 内部观测（置位时分支 ~250ms 内返回），无需块间显式门控。
    fn execute_parallel_group(
        &self,
        session_id: &str,
        group: &[ModelToolCall],
        sequence: &mut u64,
        cancel: &std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) -> Result<(), String> {
        enum Prep {
            Blocked(String),
            Ready {
                args: serde_json::Value,
                secs: u64,
            },
        }
        // 相 1：串行、按序 —— emit running + parse args + PreToolUse hook。
        let mut prepared: Vec<Prep> = Vec::with_capacity(group.len());
        for call in group {
            self.emit_tool_call_running(session_id, call, sequence);
            let args = serde_json::from_str::<serde_json::Value>(&call.arguments_json)
                .unwrap_or(serde_json::Value::Null);
            if let Some(reason) = self.run_pre_tool_hooks(session_id, &call.name, &args) {
                prepared.push(Prep::Blocked(format!(
                    "插件 hook 阻止了工具「{}」：{reason}",
                    call.name
                )));
            } else {
                let secs = self.resolve_tool_timeout(&call.name)?;
                prepared.push(Prep::Ready { args, secs });
            }
        }
        // 相 2：并行（按上限分块）—— 仅对 Ready 项跑 execute_with_timeout。
        let cap = self.resolve_tool_parallelism()?;
        let ready_idx: Vec<usize> = prepared
            .iter()
            .enumerate()
            .filter_map(|(k, p)| matches!(p, Prep::Ready { .. }).then_some(k))
            .collect();
        let outcomes: Vec<(usize, (String, &'static str))> =
            parallel_execute(&ready_idx, cap, |_, &k| {
                let (args, secs) = match &prepared[k] {
                    Prep::Ready { args, secs } => (args, *secs),
                    Prep::Blocked(_) => unreachable!("ready_idx 只含 Ready"),
                };
                (k, self.execute_with_timeout(&group[k].name, args, secs, cancel))
            });
        let mut results: Vec<Option<(String, &'static str)>> =
            (0..group.len()).map(|_| None).collect();
        for (k, r) in outcomes {
            results[k] = Some(r);
        }
        // 相 3：串行、按序 —— PostHook（仅执行过的）+ finalize。
        for (k, call) in group.iter().enumerate() {
            match &prepared[k] {
                Prep::Blocked(msg) => {
                    self.finalize_tool_result(session_id, call, msg, "blocked", sequence)?;
                }
                Prep::Ready { args, .. } => {
                    let (text, status) = results[k].take().expect("ready 结果应已就绪");
                    self.run_post_tool_hooks(session_id, &call.name, args, &text, status);
                    self.finalize_tool_result(session_id, call, &text, status, sequence)?;
                }
            }
        }
        Ok(())
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

    // ── T66：plugin hooks 触发（hooks=None 全程短路，无插件/测试零影响）──────────

    /// 会话工作目录（沙箱根）；hook 子进程在此 cwd 起。
    fn hook_cwd(&self) -> std::path::PathBuf {
        std::path::PathBuf::from(&self.workspace)
    }

    /// 触发会话级 hooks（SessionStart/Stop）。观察性副作用，不阻断；错误非致命。
    fn run_session_hooks(&self, event: &str, session_id: &str) {
        let Some(hooks) = &self.hooks else {
            return;
        };
        let rules = hooks.rules_for(event, &None);
        if rules.is_empty() {
            return;
        }
        let payload = serde_json::json!({ "session_id": session_id });
        let cwd = self.hook_cwd();
        for rule in &rules {
            let _ = crate::hook::run_command_hook(rule, &payload, &cwd);
        }
    }

    /// 触发 PreToolUse hooks。任一返回 block → 返回 `Some(reason)`（引擎据此拦截该工具）。
    /// hooks=None 或无匹配规则 → None（不拦截）。
    fn run_pre_tool_hooks(
        &self,
        session_id: &str,
        tool: &str,
        input: &serde_json::Value,
    ) -> Option<String> {
        let hooks = self.hooks.as_ref()?;
        let rules = hooks.rules_for("PreToolUse", &Some(tool.to_string()));
        if rules.is_empty() {
            return None;
        }
        let payload = serde_json::json!({
            "session_id": session_id,
            "tool": tool,
            "input": input,
        });
        let cwd = self.hook_cwd();
        for rule in &rules {
            let outcome = crate::hook::run_command_hook(rule, &payload, &cwd);
            if outcome.block {
                return Some(
                    outcome
                        .reason
                        .unwrap_or_else(|| format!("插件 hook 阻止了工具「{tool}」")),
                );
            }
        }
        None
    }

    /// 触发 PostToolUse hooks（观察性，不阻断）。
    fn run_post_tool_hooks(
        &self,
        session_id: &str,
        tool: &str,
        input: &serde_json::Value,
        result: &str,
        status: &str,
    ) {
        let Some(hooks) = &self.hooks else {
            return;
        };
        let rules = hooks.rules_for("PostToolUse", &Some(tool.to_string()));
        if rules.is_empty() {
            return;
        }
        let payload = serde_json::json!({
            "session_id": session_id,
            "tool": tool,
            "input": input,
            "result": result,
            "status": status,
        });
        let cwd = self.hook_cwd();
        for rule in &rules {
            let _ = crate::hook::run_command_hook(rule, &payload, &cwd);
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

const IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "bmp", "avif", "ico"];
const MAX_IMAGE_BYTES: u64 = 5 * 1024 * 1024;
const MAX_IMAGES_PER_MESSAGE: usize = 8;

fn image_media_type(ext: &str) -> &'static str {
    match ext {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "avif" => "image/avif",
        "ico" => "image/x-icon",
        _ => "application/octet-stream",
    }
}

fn base_name(rel: &str) -> &str {
    rel.rsplit(['/', '\\']).next().unwrap_or(rel)
}

fn ext_of(rel: &str) -> String {
    base_name(rel)
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default()
}

/// 渲染单个 `⟦@rel⟧` 附件：非图片→`@rel`；图片+非vision→占位；图片+vision→读图入 images + `[图片: name]`。
/// 护栏：单图 >5MB / 超 8 张 / 读失败 / 越界 → 占位兜底。
fn render_attachment(
    rel: &str,
    workspace: &std::path::Path,
    supports_vision: bool,
    images: &mut Vec<ModelImage>,
) -> String {
    let name = base_name(rel).to_string();
    let ext = ext_of(rel);
    if !IMAGE_EXTS.contains(&ext.as_str()) {
        return format!("@{rel}"); // 非图片：维持现状
    }
    if !supports_vision {
        return format!("[图片附件: {name}，当前模型不支持图像识别，无法查看内容]");
    }
    if images.len() >= MAX_IMAGES_PER_MESSAGE {
        return format!("[图片过多未发送: {name}]");
    }
    // 路径安全：拼 workspace 后规范化，必须仍在 workspace 内。
    let full = workspace.join(rel);
    let inside = match (workspace.canonicalize().ok(), full.canonicalize().ok()) {
        (Some(w), Some(f)) => f.starts_with(&w),
        _ => false,
    };
    if !inside {
        return format!("[图片读取失败: {name}]");
    }
    match std::fs::metadata(&full) {
        Ok(meta) if meta.len() > MAX_IMAGE_BYTES => return format!("[图片过大未发送: {name}]"),
        Ok(_) => {}
        Err(_) => return format!("[图片读取失败: {name}]"),
    }
    match std::fs::read(&full) {
        Ok(bytes) => {
            use base64::Engine as _;
            let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
            images.push(ModelImage {
                media_type: image_media_type(&ext).to_string(),
                base64_data: b64,
            });
            format!("[图片: {name}]")
        }
        Err(_) => format!("[图片读取失败: {name}]"),
    }
}

/// 解析一条 user 消息中的 `⟦@relPath⟧` 附件标记，返回 (展开后纯文本, 图片列表)。
/// 手写扫描器（codebase 不依赖 regex）：`⟦@rel⟧` 为附件标记，其余 `⟦x⟧` 去括号留内文
/// （收敛 strip_chip_markers 行为）。`supports_vision=false` 时图片走文本占位（降级），不读文件。
fn expand_user_attachments(
    content: &str,
    workspace: &std::path::Path,
    supports_vision: bool,
) -> (String, Vec<ModelImage>) {
    let mut out = String::with_capacity(content.len());
    let mut images: Vec<ModelImage> = Vec::new();
    let mut rest = content;
    while let Some(open) = rest.find('⟦') {
        out.push_str(&rest[..open]);
        let after_open = &rest[open + '⟦'.len_utf8()..];
        let Some(close) = after_open.find('⟧') else {
            // 不配对的 ⟦：丢括号、继续扫剩余。
            rest = after_open;
            continue;
        };
        let inner = &after_open[..close];
        let tail = &after_open[close + '⟧'.len_utf8()..];
        match inner.strip_prefix('@') {
            Some(rel) => {
                out.push_str(&render_attachment(rel, workspace, supports_vision, &mut images))
            }
            None => out.push_str(inner), // 非附件 chip：去括号留内文
        }
        rest = tail;
    }
    out.push_str(rest);
    // 兜底清掉任何残留的孤立括号（与旧 strip_chip_markers 等价）。
    (out.replace(['⟦', '⟧'], ""), images)
}

/// 历史消息 → provider 消息序列（唯一出口，强制 OpenAI-compatible 的 tool_call↔tool_result 配对不变式）。
///
/// 每条 assistant 的 `tool_calls` **紧跟**其每个调用的结果，按 `tool_call_id` 配对——**不依赖历史中的位置邻接**。
/// 缺失结果补一条占位（上一次运行被中断时常见）；独立 tool 消息在此跳过（其结果已在所属 assistant 处按 id 输出）。
/// 这样即便历史因中断/迟到回填/「继续」后补结果而乱序（assistant→user→tool），发给 provider 的请求仍合法——
/// 这是「进程被打断后会话仍可续跑」的根本保证，而非靠各恢复路径逐个收口悬空 tool_call（天生不完整、脆弱）。
pub(crate) fn assemble_history_messages(
    history: &[crate::session::Message],
    workspace: &std::path::Path,
    supports_vision: bool,
) -> Vec<ModelMessage> {
    use std::collections::HashMap;
    const ORPHAN: &str = "（该工具调用未完成：上一次运行被中断。如仍需要，请重新调用。）";
    // 仅最新一条 user 消息读图（base64）；历史 user 的图片一律走文本占位，避免重复 base64 爆 token。
    let last_user_idx = history
        .iter()
        .enumerate()
        .filter(|(_, m)| !m.compacted && m.role != "assistant" && m.role != "tool")
        .map(|(i, _)| i)
        .last();
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
    for (idx, m) in history.iter().enumerate() {
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
            // user：展开 Composer 附件标记 ⟦@…⟧——仅最新 user 且模型支持 vision 时读图，
            // 否则图片走文本占位（降级）；其余 chip 去括号。error/stopped/compaction 已被 compacted 跳过。
            _ => {
                let allow_images = supports_vision && Some(idx) == last_user_idx;
                let (text, images) = expand_user_attachments(&m.content, workspace, allow_images);
                let mut msg = ModelMessage::user(text);
                msg.images = images;
                out.push(msg);
            }
        }
    }
    out
}

/// 在独立 worker 线程跑 `f`，主线程带超时+可取消地等待。
/// 返回 `(结果文本, 持久化 status)`：status ∈ "done" | "failed"。
/// 超时/停止/worker 异常断连一律映射为 "failed"（spec §5：前端 tool status 闭合联合，
/// 不引入新枚举），但文本分别注明「超时」「停止」「异常中止」。
/// timeout_secs == 0：退化为同步直调，零额外开销（逃生舱）。
/// 已知限制：超时后 worker 线程 detach 继续跑到自然结束（Rust 不能抢占同步函数）。
fn run_with_timeout<F>(
    f: F,
    timeout_secs: u64,
    cancel: &std::sync::atomic::AtomicBool,
    name: &str,
) -> (String, &'static str)
where
    F: FnOnce() -> Result<String, String> + Send + 'static,
{
    if timeout_secs == 0 {
        return match f() {
            Ok(t) => (t, "done"),
            Err(e) => (e, "failed"),
        };
    }
    let (tx, rx) = std::sync::mpsc::channel::<Result<String, String>>();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    let deadline = std::time::Duration::from_secs(timeout_secs);
    let start = std::time::Instant::now();
    loop {
        match rx.recv_timeout(std::time::Duration::from_millis(250)) {
            Ok(Ok(t)) => return (t, "done"),
            Ok(Err(e)) => return (e, "failed"),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                return (format!("「{name}」执行异常中止（无结果）。"), "failed");
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                    return (format!("「{name}」执行被停止。"), "failed");
                }
                if start.elapsed() >= deadline {
                    return (
                        format!(
                            "「{name}」执行超时（>{timeout_secs}s），已中止等待；底层操作可能仍在后台运行。"
                        ),
                        "failed",
                    );
                }
            }
        }
    }
}

/// 是否为「控制工具」——在 execute_calls 中按名特殊处理、不走 registry.execute。
/// 这些工具即便声明 concurrency_safe 也绝不能进并行段（会绕过其 handle_* 处理）。
fn is_control_tool(name: &str) -> bool {
    name == ASK_USER_TOOL
        || name == PROPOSE_PLAN_TOOL
        || name == DISPATCH_AGENT_TOOL
        || name == COLLECT_AGENTS_TOOL
        || name == REMEMBER_TOOL
        || name == crate::tools::search_knowledge::SEARCH_KNOWLEDGE_TOOL
        || name == PROPOSE_SOUL_UPDATE_TOOL
        || name == LOAD_SKILL_TOOL
        || name == crate::tools::find_tools::FIND_TOOLS_TOOL
        || name == crate::tools::read_skill_file::READ_SKILL_FILE_TOOL
        || name == UPDATE_TODOS_TOOL
        || name == UPDATE_TASKS_TOOL
        || name == ADD_ARTIFACT_TOOL
}

/// 把 `items` 按 `cap` 分块，块内用 `thread::scope` 并发跑 `f(idx, &item)`、块间顺序，
/// 返回与输入同序的结果 `Vec<R>`。cap 至少为 1（0 当作 1）。
/// `f` 必须不 panic（worker panic 会令 join 失败并向上 panic）；本仓调用方传 execute_with_timeout，
/// 它自身吞掉一切错误返回 (text,status)，不会 panic。
fn parallel_execute<T, R, F>(items: &[T], cap: usize, f: F) -> Vec<R>
where
    T: Sync,
    R: Send,
    F: Fn(usize, &T) -> R + Sync,
{
    let cap = cap.max(1);
    let mut out: Vec<Option<R>> = (0..items.len()).map(|_| None).collect();
    let mut start = 0;
    while start < items.len() {
        let end = (start + cap).min(items.len());
        std::thread::scope(|s| {
            let handles: Vec<_> = (start..end)
                .map(|idx| {
                    let f = &f;
                    s.spawn(move || (idx, f(idx, &items[idx])))
                })
                .collect();
            for h in handles {
                let (idx, r) = h.join().expect("parallel_execute: worker thread panicked");
                out[idx] = Some(r);
            }
        });
        start = end;
    }
    out.into_iter().map(|o| o.expect("all slots filled")).collect()
}

#[cfg(test)]
mod attachment_tests {
    use super::expand_user_attachments;
    use std::fs;
    use std::path::PathBuf;

    fn tmp_ws(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("t78-{name}"));
        let _ = fs::create_dir_all(dir.join("attachments"));
        dir
    }

    #[test]
    fn non_vision_image_becomes_placeholder_no_images() {
        let ws = tmp_ws("nv");
        fs::write(ws.join("attachments/a.png"), [0u8; 8]).unwrap();
        let (text, images) = expand_user_attachments("⟦@attachments/a.png⟧ 看图", &ws, false);
        assert!(text.contains("不支持图像识别"));
        assert!(text.contains("a.png"));
        assert!(images.is_empty());
    }

    #[test]
    fn vision_image_reads_base64_and_labels() {
        let ws = tmp_ws("v");
        fs::write(ws.join("attachments/b.png"), [1u8, 2, 3, 4]).unwrap();
        let (text, images) = expand_user_attachments("⟦@attachments/b.png⟧ 看图", &ws, true);
        assert!(text.contains("[图片: b.png]"));
        assert_eq!(images.len(), 1);
        assert_eq!(images[0].media_type, "image/png");
        assert!(!images[0].base64_data.is_empty());
    }

    #[test]
    fn non_image_file_keeps_path_text() {
        let ws = tmp_ws("doc");
        let (text, images) = expand_user_attachments("⟦@notes/report.pdf⟧ 总结", &ws, true);
        assert!(text.contains("@notes/report.pdf"));
        assert!(images.is_empty());
    }

    #[test]
    fn missing_image_file_falls_back() {
        let ws = tmp_ws("miss");
        let (text, images) = expand_user_attachments("⟦@attachments/none.png⟧", &ws, true);
        assert!(text.contains("图片读取失败"));
        assert!(images.is_empty());
    }
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
        let out = assemble_history_messages(&history, std::path::Path::new("/tmp"), false);
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
        let out = assemble_history_messages(&history, std::path::Path::new("/tmp"), false);
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
        let out = assemble_history_messages(&history, std::path::Path::new("/tmp"), false);
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
    fn expand_strips_brackets_and_keeps_inner() {
        let ws = std::path::Path::new("/tmp");
        // 非图片附件 → @相对路径；技能 chip → 去括号留内文。
        let (text, images) =
            expand_user_attachments("看 ⟦@attachments/a.md⟧ 并用 ⟦技能：x⟧", ws, false);
        assert_eq!(text, "看 @attachments/a.md 并用 技能：x");
        assert!(images.is_empty());
        // 无标记原样返回。
        let (plain, _) = expand_user_attachments("普通文本 @x 技能：y", ws, false);
        assert_eq!(plain, "普通文本 @x 技能：y");
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

#[cfg(test)]
mod timeout_tests {
    use super::*;

    #[test]
    fn run_with_timeout_returns_done_on_fast_ok() {
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let (text, status) = run_with_timeout(|| Ok("ok-result".to_string()), 5, &cancel, "工具X");
        assert_eq!(status, "done");
        assert_eq!(text, "ok-result");
    }

    #[test]
    fn run_with_timeout_returns_failed_on_err() {
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let (text, status) = run_with_timeout(|| Err("boom".to_string()), 5, &cancel, "工具X");
        assert_eq!(status, "failed");
        assert_eq!(text, "boom");
    }

    #[test]
    fn run_with_timeout_marks_timeout_and_keeps_running() {
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let (text, status) = run_with_timeout(
            || {
                std::thread::sleep(std::time::Duration::from_secs(3));
                Ok("late".to_string())
            },
            1,
            &cancel,
            "慢工具",
        );
        assert_eq!(status, "failed"); // 持久化用 failed（spec §5），文本含「超时」
        assert!(text.contains("超时"), "text was: {text}");
        assert!(text.contains("慢工具"), "text was: {text}");
    }

    #[test]
    fn run_with_timeout_returns_stopped_when_cancelled() {
        let cancel = std::sync::atomic::AtomicBool::new(true); // 预置取消
        let (text, status) = run_with_timeout(
            || {
                std::thread::sleep(std::time::Duration::from_secs(3));
                Ok("late".to_string())
            },
            10,
            &cancel,
            "工具Y",
        );
        assert_eq!(status, "failed");
        assert!(text.contains("停止"), "text was: {text}");
    }

    #[test]
    fn run_with_timeout_zero_is_synchronous() {
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let (text, status) = run_with_timeout(|| Ok("sync-ok".to_string()), 0, &cancel, "工具Z");
        assert_eq!(status, "done");
        assert_eq!(text, "sync-ok");
        let (etext, estatus) = run_with_timeout(|| Err("sync-err".to_string()), 0, &cancel, "工具Z");
        assert_eq!(estatus, "failed");
        assert_eq!(etext, "sync-err");
    }
}

#[cfg(test)]
mod parallel_tests {
    use super::*;

    #[test]
    fn parallel_execute_preserves_input_order() {
        let items = vec![10usize, 20, 30, 40];
        let out = parallel_execute(&items, 8, |_, &v| v * 2);
        assert_eq!(out, vec![20, 40, 60, 80]);
    }

    #[test]
    fn parallel_execute_runs_concurrently_within_cap() {
        // 4 个各 sleep 300ms 的任务，cap=4 → 一块并发 → 总耗时 ≈ 300ms 而非 1200ms。
        let items = vec![0usize, 1, 2, 3];
        let start = std::time::Instant::now();
        let out = parallel_execute(&items, 4, |_, &v| {
            std::thread::sleep(std::time::Duration::from_millis(300));
            v
        });
        let elapsed = start.elapsed();
        assert_eq!(out, vec![0, 1, 2, 3]);
        assert!(elapsed < std::time::Duration::from_millis(900), "elapsed={elapsed:?}");
    }

    #[test]
    fn parallel_execute_cap_one_is_serial() {
        // cap=1 → 串行：3 个各 200ms → 总耗时 ≈ 600ms。
        let items = vec![0usize, 1, 2];
        let start = std::time::Instant::now();
        let out = parallel_execute(&items, 1, |_, &v| {
            std::thread::sleep(std::time::Duration::from_millis(200));
            v
        });
        assert_eq!(out, vec![0, 1, 2]);
        assert!(start.elapsed() >= std::time::Duration::from_millis(550));
    }

    #[test]
    fn parallel_execute_empty_is_empty() {
        let items: Vec<usize> = vec![];
        let out = parallel_execute(&items, 8, |_, &v| v);
        assert_eq!(out, Vec::<usize>::new());
    }

    #[test]
    fn is_control_tool_excludes_search_knowledge_from_parallel() {
        assert!(is_control_tool(crate::tools::search_knowledge::SEARCH_KNOWLEDGE_TOOL));
        assert!(is_control_tool(ASK_USER_TOOL));
        assert!(is_control_tool(DISPATCH_AGENT_TOOL));
        assert!(!is_control_tool("read_file"));
        assert!(!is_control_tool("web_search"));
    }
}

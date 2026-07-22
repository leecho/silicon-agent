//! 控制工具拦截处理（`impl Engine`，从 `execute_calls` 拆出）。
//!
//! 这些工具由引擎按名拦截、**不走 registry 真执行**：它们改变 run 状态（发事件、读写会话、
//! 暂停/继续）。因此属 engine 概念而非 `tools/**`（移到 tools 会迫使其反向依赖 ControlFlow/emit）。
//! 每个方法用 `&mut sequence` 续用事件序号，保持与 `execute_calls` 的序号连续。

use super::{ControlFlow, Engine, PendingInteraction};
use crate::engine::event::AgentStreamEvent;
use crate::provider::message::ModelToolCall;
use crate::session::{new_id, PendingAsk, PendingPlan, TodoItem};
use crate::tools::ask_user::ASK_USER_TOOL;
use crate::tools::collect_agents::COLLECT_AGENTS_TOOL;
use crate::tools::dispatch_agent::DISPATCH_AGENT_TOOL;

use super::{now_string, parse_ask_questions, truncate_event_text};

impl Engine {
    /// ask_user 拦截：控制工具，引擎按名拦截、不真执行——解析 questions，emit ask_required
    /// 并暂停；答案由命令层落为该调用的 tool 结果后续跑。
    /// 返回 `Some(Paused)` 暂停；`None` 表示 headless+full 已自动应答、调用方应 continue。
    pub(super) fn handle_ask_user(
        &self,
        session_id: &str,
        call: &ModelToolCall,
        sequence: &mut u64,
    ) -> Result<Option<ControlFlow>, String> {
        // headless + full 模式：不暂停提问，注入「自行判断」结果后继续；
        // 非 full 的 headless 运行则照常暂停（needs_attention，等用户下次处理）。
        if self.headless_auto_answers_ask(session_id)? {
            let note = "本次为定时任务无人值守运行，请基于已有信息自行判断并继续；如信息不足请在最终回复中说明。";
            let result_at = now_string();
            self.session.append_tool_result(
                &new_id("msg"),
                session_id,
                &call.id,
                ASK_USER_TOOL,
                note,
                "done",
                &result_at,
            )?;
            *sequence += 1;
            self.emit(AgentStreamEvent {
                kind: "tool_result".into(),
                session_id: session_id.into(),
                message_id: call.id.clone(),
                sequence: *sequence,
                text: Some(note.into()),
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
            return Ok(None);
        }
        let args = serde_json::from_str::<serde_json::Value>(&call.arguments_json)
            .unwrap_or(serde_json::Value::Null);
        let questions = parse_ask_questions(&args);
        let first_q = questions
            .first()
            .map(|q| q.question.clone())
            .unwrap_or_default();
        *sequence += 1;
        self.emit(AgentStreamEvent {
            kind: "ask_required".into(),
            session_id: session_id.into(),
            message_id: call.id.clone(),
            sequence: *sequence,
            text: Some(first_q),
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
        Ok(Some(ControlFlow::Paused(PendingInteraction::Ask(
            PendingAsk {
                session_id: session_id.to_string(),
                tool_call_id: call.id.clone(),
                questions,
            },
        ))))
    }

    /// propose_plan 拦截：控制工具，引擎按名拦截、不真执行——解析 title/summary/plan_markdown/
    /// risk_level，emit plan_required 并暂停；批准/评论由命令层落为该调用的 tool 结果后续跑。
    pub(super) fn handle_propose_plan(
        &self,
        session_id: &str,
        call: &ModelToolCall,
        sequence: &mut u64,
    ) -> Result<ControlFlow, String> {
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
        *sequence += 1;
        self.emit(AgentStreamEvent {
            kind: "plan_required".into(),
            session_id: session_id.into(),
            message_id: call.id.clone(),
            sequence: *sequence,
            text: Some(plan_markdown.clone()),
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
        Ok(ControlFlow::Paused(PendingInteraction::Plan(PendingPlan {
            session_id: session_id.to_string(),
            tool_call_id: call.id.clone(),
            title,
            summary,
            plan_markdown,
            risk_level,
        })))
    }

    /// dispatch_agent 拦截：控制工具，引擎按名拦截、不真执行——校验/解析后建 child 会话
    /// （写父子链 + origin="subagent" + 可选 ad-hoc spec），返回新建 child 的会话 id 供调用方（execute_calls）
    /// 攒入本批并行清单、整批停泊一次。未知专家（无 spec 无 inline）则落 failed 结果并返回 `None`（父改派）。
    /// **不**在此处设停泊/返回 Paused——停泊与 `Subagent` 信号由 execute_calls 在整批处理完后统一发出。
    pub(super) fn prepare_dispatch_agent(
        &self,
        session_id: &str,
        call: &ModelToolCall,
        sequence: &mut u64,
    ) -> Result<Option<String>, String> {
        let args = serde_json::from_str::<serde_json::Value>(&call.arguments_json)
            .unwrap_or(serde_json::Value::Null);
        let name = args
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let task = args
            .get("task")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        // 上游产物引用：共享工作目录内的相对路径等，拼进交给 child 的首条任务消息，提示其先读取。
        let inputs: Vec<String> = args
            .get("inputs")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str())
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .collect()
            })
            .unwrap_or_default();
        let task_full = if inputs.is_empty() {
            task.clone()
        } else {
            let refs = inputs
                .iter()
                .map(|p| format!("- {p}"))
                .collect::<Vec<_>>()
                .join("\n");
            format!(
                "{task}\n\n## 可参考的上游产物（已在你的工作目录内）\n{refs}\n\n请先读取上述文件作为输入，再开展你的任务。"
            )
        };
        // ad-hoc（动态生成）专家：模型现场给 system_prompt + tools。
        let system_prompt = args
            .get("system_prompt")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let tools_joined = args
            .get("tools")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|t| t.as_str())
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(",")
            })
            .unwrap_or_default();
        let ad_hoc = !system_prompt.is_empty();
        // 声明式专家存在性：按会话上下文解析——项目/团队名册优先；否则散装专家。
        let (role_kind, role_id) = self
            .session
            .get_session(session_id)
            .ok()
            .flatten()
            .map(|s| {
                if let Some(project_id) = s.project_id {
                    ("project".to_string(), project_id)
                } else {
                    (
                        s.role_kind.unwrap_or_default(),
                        s.role_id.unwrap_or_default(),
                    )
                }
            })
            .unwrap_or_default();
        let declared = match (role_kind.as_str(), &self.teams, &self.experts) {
            ("team", Some(teams), _) if !role_id.is_empty() => teams
                .detail(&role_id)
                .map(|d| d.members.iter().any(|m| m.name == name))
                .unwrap_or(false),
            ("project", _, _) if !role_id.is_empty() => self.is_project_member(&role_id, &name),
            // 自由模式：**公开**专家（散装 + plugin 提供）皆可按名指派。
            // 标准里 plugin 的 agent 与用户自己的 agent 同级、全局可调用；此前这里只认散装
            // （`load_spec_by_owner("", "", name)`），导致插件专家一律被判「没有现成专家」。
            // 加载器（builder.rs）已同步收紧为同一口径，team 私有专家仍派不动。
            (_, _, Some(agents)) => agents
                .resolve_public_spec_by_name(&name)
                .map(|o| o.is_some())
                .unwrap_or(false),
            _ => false,
        };
        // 名册模式（团队/项目）：只能指派名册内成员，禁止临场新建临时专家（ad-hoc）。
        let roster_mode = !role_id.is_empty() && (role_kind == "team" || role_kind == "project");
        let ad_hoc_effective = ad_hoc && !roster_mode;

        if name.is_empty() || task.is_empty() || (!ad_hoc_effective && !declared) {
            let now = now_string();
            let msg = if name.is_empty() || task.is_empty() {
                "指派失败：name 与 task 均不能为空".to_string()
            } else if roster_mode {
                format!("指派失败：当前是{}模式，只能指派名册内的成员，且不能临时新建专家。请从「可调度成员」名册里选一个已有成员（按其 name 指派），或先把需要的成员加入项目/团队。", if role_kind == "project" { "项目" } else { "团队" })
            } else {
                format!("指派失败：没有现成专家「{name}」。请用 system_prompt + tools 现场定义这个临时专家（system_prompt 写清角色/约束/回禀格式，tools 选最小必要工具），或指派一个已有专家。")
            };
            self.session.append_tool_result(
                &new_id("msg"),
                session_id,
                &call.id,
                DISPATCH_AGENT_TOOL,
                &msg,
                "failed",
                &now,
            )?;
            *sequence += 1;
            self.emit(AgentStreamEvent {
                kind: "tool_result".into(),
                session_id: session_id.into(),
                message_id: call.id.clone(),
                sequence: *sequence,
                text: Some(msg),
                status: Some("failed".into()),
                tool_name: Some(DISPATCH_AGENT_TOOL.into()),
                tool_label: self.label_for(&call.name),
                tool_call_id: Some(call.id.clone()),
                todos: None,
                artifacts: None,
                parent_session_id: None,
                parent_tool_call_id: None,
                expert_name: None,
                created_at: now,
            });
            return Ok(None);
        }

        // 建 child 会话 + 首条任务消息（父停泊由 execute_calls 整批统一处理）。
        // ad-hoc：把 inline spec 存到 child 行，运行时直接用；声明式：留空、运行时查 ExpertService。
        let (sp, tl): (Option<&str>, Option<&str>) = if ad_hoc_effective {
            (Some(system_prompt.as_str()), Some(tools_joined.as_str()))
        } else {
            (None, None)
        };
        // T57：后台派发——即发即返，父不停泊，结论由 collect_agents 取回。
        let background = args
            .get("background")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let now = now_string();
        let child_id = new_id("session");
        // 子会话标题用成员展示名（名册/散装解析）；ad-hoc 或解析不到则回退原始 name。
        let display = self.dispatch_display_name(&role_kind, &role_id, &name);
        self.session.create_child_session(
            &child_id,
            session_id,
            &call.id,
            &name,
            &task,
            sp,
            tl,
            background,
            &now,
            display.as_deref(),
        )?;
        self.session
            .append_message(&new_id("msg"), &child_id, "user", &task_full, None, &now)?;
        // T61：编排模式且带 task_id → 把本次运行关联到任务台账（状态转 in_progress、回填 assignee）。
        if let Some(projects) = self.projects.as_ref() {
            if let Some(task_id) = args
                .get("task_id")
                .and_then(|v| v.as_str())
                .filter(|s| !s.trim().is_empty())
            {
                let _ = projects.set_task_run(task_id.trim(), &child_id, &name);
            }
        }
        *sequence += 1;
        // 父 feed：dispatch 卡的 input 用**精简 JSON**（name+task），保证前端始终能解析出专家名
        // （叙事文案与「查看」入口都靠它）；不要塞自然语言或被截断的完整 args（ad-hoc 的 system_prompt 很长）。
        let card_input = serde_json::json!({ "name": name, "task": task }).to_string();
        self.emit(AgentStreamEvent {
            kind: "tool_call".into(),
            session_id: session_id.into(),
            message_id: call.id.clone(),
            sequence: *sequence,
            text: Some(card_input),
            status: Some("running".into()),
            tool_name: Some(DISPATCH_AGENT_TOOL.into()),
            tool_label: self.label_for(&call.name),
            tool_call_id: Some(call.id.clone()),
            todos: None,
            artifacts: None,
            parent_session_id: None,
            parent_tool_call_id: None,
            expert_name: None,
            created_at: now,
        });
        if background {
            // 即发即返：立刻给该 dispatch tool_call 一个占位 result（父不停泊、协议满足），
            // 并即时启动 child run（经 child_spawner 回调，交给编排层并发跑）。结论后续 collect_agents 取。
            let placeholder = format!(
                "已派发子代理〈{name}〉(handle={})，后台运行中。需要其结论时调用 collect_agents(handles=[\"{}\"])；或省略 handles 收取全部后台子代理。",
                call.id, call.id
            );
            let at = now_string();
            self.session.append_tool_result(
                &new_id("msg"),
                session_id,
                &call.id,
                DISPATCH_AGENT_TOOL,
                &placeholder,
                "done",
                &at,
            )?;
            *sequence += 1;
            self.emit(AgentStreamEvent {
                kind: "tool_result".into(),
                session_id: session_id.into(),
                message_id: call.id.clone(),
                sequence: *sequence,
                text: Some(placeholder),
                status: Some("done".into()),
                tool_name: Some(DISPATCH_AGENT_TOOL.into()),
                tool_label: self.label_for(&call.name),
                tool_call_id: Some(call.id.clone()),
                todos: None,
                artifacts: None,
                parent_session_id: None,
                parent_tool_call_id: None,
                expert_name: None,
                created_at: at,
            });
            self.spawn_child(&child_id); // 即时启动后台 child run。
            return Ok(None); // 不进 dispatched_children → 父不停泊。
        }
        Ok(Some(child_id))
    }

    /// T57 collect_agents 拦截：收取后台子代理结论。立即可收→写结果继续(返回 None)；
    /// 有未完成且 wait→设父 pending_collect 停泊(返回 Paused)。
    pub(super) fn handle_collect_agents(
        &self,
        session_id: &str,
        call: &ModelToolCall,
        sequence: &mut u64,
    ) -> Result<Option<ControlFlow>, String> {
        let args = serde_json::from_str::<serde_json::Value>(&call.arguments_json)
            .unwrap_or(serde_json::Value::Null);
        let wait = args.get("wait").and_then(|v| v.as_bool()).unwrap_or(true);
        let mut handles: Vec<String> = args
            .get("handles")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str())
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        if handles.is_empty() {
            handles = self.session.background_handles_uncollected(session_id)?;
        }
        let emit_result = |this: &Self, seq: &mut u64, text: String| {
            let at = now_string();
            this.session.append_tool_result(
                &new_id("msg"),
                session_id,
                &call.id,
                COLLECT_AGENTS_TOOL,
                &text,
                "done",
                &at,
            )?;
            *seq += 1;
            this.emit(AgentStreamEvent {
                kind: "tool_result".into(),
                session_id: session_id.into(),
                message_id: call.id.clone(),
                sequence: *seq,
                text: Some(text),
                status: Some("done".into()),
                tool_name: Some(COLLECT_AGENTS_TOOL.into()),
                tool_label: this.label_for(&call.name),
                tool_call_id: Some(call.id.clone()),
                todos: None,
                artifacts: None,
                parent_session_id: None,
                parent_tool_call_id: None,
                expert_name: None,
                created_at: at,
            });
            Ok::<(), String>(())
        };
        if handles.is_empty() {
            emit_result(self, sequence, "没有待收取的后台子代理。".into())?;
            return Ok(None);
        }
        let (text, terminal, running) = self.session.collect_summary(session_id, &handles)?;
        if running.is_empty() || !wait {
            for cid in &terminal {
                self.session.mark_collected(cid)?;
            }
            emit_result(self, sequence, text)?;
            return Ok(None);
        }
        // 有未完成且 wait：父停泊在 collect，等其完成由 advance_pending_collect 收口续跑。
        let pc = serde_json::json!({ "collectCallId": call.id, "handles": handles }).to_string();
        let now = now_string();
        self.session
            .set_pending_collect(session_id, Some(&pc), &now)?;
        self.session
            .set_awaiting_subagent(session_id, &running[0], &now)?;
        Ok(Some(ControlFlow::Paused(PendingInteraction::Subagent {
            child_session_ids: running,
        })))
    }

    /// remember 拦截：控制工具，引擎按名拦截、不真执行——解析 content，空则落「记忆内容为空」
    /// 结果；否则即时写入全局长期记忆、落 tool 结果、emit tool_result（不暂停、不需权限）。
    pub(super) fn handle_remember(
        &self,
        session_id: &str,
        call: &ModelToolCall,
        sequence: &mut u64,
    ) -> Result<(), String> {
        let args = serde_json::from_str::<serde_json::Value>(&call.arguments_json)
            .unwrap_or(serde_json::Value::Null);
        let content = args
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let result_text = if content.is_empty() {
            "记忆内容为空".to_string()
        } else {
            if let Some(memory) = self.memory.as_ref() {
                // 记忆作用域：project_id 优先 → 独体 agent 私有 → 全局（spec §3.1，与召回一致）。
                let (pid, aid) = self
                    .session
                    .get_session(session_id)
                    .ok()
                    .flatten()
                    .map(|s| {
                        (
                            s.project_id.unwrap_or_default(),
                            s.agent_id.unwrap_or_default(),
                        )
                    })
                    .unwrap_or_default();
                let scope = crate::memory::MemoryScope::from_session(&pid, &aid);
                memory.add_memory(&content, &now_string(), scope)?;
            }
            format!("已记入长期记忆：{content}")
        };
        let result_at = now_string();
        self.session.append_tool_result(
            &new_id("msg"),
            session_id,
            &call.id,
            "remember",
            &result_text,
            "done",
            &result_at,
        )?;
        *sequence += 1;
        self.emit(AgentStreamEvent {
            kind: "tool_result".into(),
            session_id: session_id.into(),
            message_id: call.id.clone(),
            sequence: *sequence,
            text: Some(truncate_event_text(&result_text, 2000)),
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
        Ok(())
    }

    /// propose_soul_update 拦截（T73）：反思运行里伴随体提交人格改写提案。引擎按名拦截、不真执行——
    /// 写一条 `pending` SOUL 版本（待用户批准），不改活跃人格、不碰 IDENTITY。即时落工具结果、继续。
    pub(super) fn handle_propose_soul_update(
        &self,
        session_id: &str,
        call: &ModelToolCall,
        sequence: &mut u64,
    ) -> Result<(), String> {
        let args = serde_json::from_str::<serde_json::Value>(&call.arguments_json)
            .unwrap_or(serde_json::Value::Null);
        let new_soul = args
            .get("new_soul")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let summary = args
            .get("summary")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        // 当前会话所属伴随体（反思会话为 agent 绑定）。
        let agent_id = self
            .session
            .get_session(session_id)
            .ok()
            .flatten()
            .and_then(|s| s.agent_id)
            .unwrap_or_default();
        let result_text = if agent_id.is_empty() {
            "仅伴随体会话可提议人格更新".to_string()
        } else if new_soul.is_empty() {
            "人格提案内容为空".to_string()
        } else if let Some(agents) = self.agents.as_ref() {
            match agents.propose_soul(&agent_id, &new_soul, &summary) {
                Ok(_) => "已提交人格更新提案，等待用户批准".to_string(),
                Err(e) => format!("提交人格提案失败：{e}"),
            }
        } else {
            "伴随体服务不可用".to_string()
        };
        let result_at = now_string();
        self.session.append_tool_result(
            &new_id("msg"),
            session_id,
            &call.id,
            "propose_soul_update",
            &result_text,
            "done",
            &result_at,
        )?;
        *sequence += 1;
        self.emit(AgentStreamEvent {
            kind: "tool_result".into(),
            session_id: session_id.into(),
            message_id: call.id.clone(),
            sequence: *sequence,
            text: Some(truncate_event_text(&result_text, 2000)),
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
        Ok(())
    }

    /// load_skill 拦截：控制工具，引擎按名拦截、不真执行——即时从 DB 取该技能 content、
    /// 落为该调用的 tool 结果、emit tool_result（不暂停、不需权限）。
    pub(super) fn handle_load_skill(
        &self,
        session_id: &str,
        call: &ModelToolCall,
        sequence: &mut u64,
    ) -> Result<(), String> {
        let args = serde_json::from_str::<serde_json::Value>(&call.arguments_json)
            .unwrap_or(serde_json::Value::Null);
        let name = args.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let mut content = self
            .skills
            .as_ref()
            .and_then(|s| s.load_body(name).ok().flatten())
            .unwrap_or_else(|| format!("未找到技能：{name}"));
        // 渐进披露第三级：列出该技能附带的参考/脚本文件，引导模型按需 read_skill_file。
        // 仅当确有附带文件时追加（只含 SKILL.md 的技能不变）。
        if let Some(skills) = self.skills.as_ref() {
            let files = skills.list_reference_files(name).unwrap_or_default();
            if !files.is_empty() {
                content.push_str(
                    "\n\n---\n本技能附带以下文件，需要其内容时用 read_skill_file(name, path) 读取：\n",
                );
                for f in &files {
                    content.push_str(&format!("- {f}\n"));
                }
            }
        }
        let result_at = now_string();
        self.session.append_tool_result(
            &new_id("msg"),
            session_id,
            &call.id,
            "load_skill",
            &content,
            "done",
            &result_at,
        )?;
        *sequence += 1;
        self.emit(AgentStreamEvent {
            kind: "tool_result".into(),
            session_id: session_id.into(),
            message_id: call.id.clone(),
            sequence: *sequence,
            text: Some(truncate_event_text(&content, 2000)),
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
        Ok(())
    }

    /// find_tools 拦截（T83）：控制工具，引擎按名拦截、不真执行——按 query/select 在
    /// 「当前会话可见的 Deferred 工具」里匹配，写入会话已激活集，回灌精简确认（不回灌 schema）。
    pub(super) fn handle_find_tools(
        &self,
        session_id: &str,
        mode: &str,
        call: &ModelToolCall,
        sequence: &mut u64,
    ) -> Result<(), String> {
        use crate::tools::find_tools::match_deferred_tools;
        let args = serde_json::from_str::<serde_json::Value>(&call.arguments_json)
            .unwrap_or(serde_json::Value::Null);
        let query = args.get("query").and_then(|v| v.as_str());
        let select: Option<Vec<String>> = args.get("select").and_then(|v| v.as_array()).map(|a| {
            a.iter()
                .filter_map(|x| x.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        });

        // 候选目录 = registry 全部 Deferred 规格，且在 plan 模式下过滤掉写工具（只读闸门一致）。
        let candidate: Vec<crate::tools::ToolSpec> = self
            .registry
            .specs()
            .into_iter()
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
            .collect();
        let hits = match_deferred_tools(&candidate, query, select.as_deref());

        let result_text = if hits.is_empty() {
            "未匹配到可加载的工具，请调整关键词或检查「可用工具目录」里的精确名。".to_string()
        } else {
            self.session.activate_tools(session_id, &hits)?;
            format!(
                "已加载 {} 个工具：{}。现在可直接调用它们。",
                hits.len(),
                hits.join("、")
            )
        };

        let result_at = now_string();
        self.session.append_tool_result(
            &new_id("msg"),
            session_id,
            &call.id,
            crate::tools::find_tools::FIND_TOOLS_TOOL,
            &result_text,
            "done",
            &result_at,
        )?;
        *sequence += 1;
        self.emit(AgentStreamEvent {
            kind: "tool_result".into(),
            session_id: session_id.into(),
            message_id: call.id.clone(),
            sequence: *sequence,
            text: Some(truncate_event_text(&result_text, 2000)),
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
        Ok(())
    }

    /// read_skill_file 拦截：读某技能目录内的附带文件（渐进披露第三级）。路径限定在技能目录内。
    pub(super) fn handle_read_skill_file(
        &self,
        session_id: &str,
        call: &ModelToolCall,
        sequence: &mut u64,
    ) -> Result<(), String> {
        let args = serde_json::from_str::<serde_json::Value>(&call.arguments_json)
            .unwrap_or(serde_json::Value::Null);
        let name = args.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let content = match self.skills.as_ref() {
            Some(s) => match s.read_reference_file(name, path) {
                Ok(Some(c)) => c,
                Ok(None) => format!("未找到技能文件：{name}/{path}"),
                Err(e) => format!("读取技能文件失败：{e}"),
            },
            None => format!("未找到技能文件：{name}/{path}"),
        };
        let result_at = now_string();
        self.session.append_tool_result(
            &new_id("msg"),
            session_id,
            &call.id,
            crate::tools::read_skill_file::READ_SKILL_FILE_TOOL,
            &content,
            "done",
            &result_at,
        )?;
        *sequence += 1;
        self.emit(AgentStreamEvent {
            kind: "tool_result".into(),
            session_id: session_id.into(),
            message_id: call.id.clone(),
            sequence: *sequence,
            text: Some(truncate_event_text(&content, 2000)),
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
        Ok(())
    }

    /// update_todos 拦截：控制工具，引擎按名拦截、不真执行——清洗+校验整组 todos，
    /// 持久化覆写到 session、emit todos_updated（带整组）、落工具结果汇总（不暂停、不需权限）。
    /// 校验失败（>1 in_progress）→落错误结果+emit tool_result，不覆写既有 todos。
    pub(super) fn handle_update_todos(
        &self,
        session_id: &str,
        call: &ModelToolCall,
        sequence: &mut u64,
    ) -> Result<(), String> {
        let args = serde_json::from_str::<serde_json::Value>(&call.arguments_json)
            .unwrap_or(serde_json::Value::Null);
        // 清洗：丢空 content；status 非法→pending；id 按 1 基序号重排。
        let mut items: Vec<TodoItem> = Vec::new();
        if let Some(raw) = args.get("todos").and_then(|v| v.as_array()) {
            for entry in raw {
                let content = entry
                    .get("content")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if content.is_empty() {
                    continue;
                }
                let status = match entry.get("status").and_then(|v| v.as_str()) {
                    Some("pending") | Some("in_progress") | Some("completed") => {
                        entry["status"].as_str().unwrap().to_string()
                    }
                    _ => "pending".to_string(),
                };
                items.push(TodoItem {
                    id: (items.len() as u32) + 1,
                    content,
                    status,
                });
            }
        }

        let in_prog = items.iter().filter(|t| t.status == "in_progress").count();
        if in_prog > 1 {
            let msg = "错误：同一时刻至多一项 in_progress，请重新规划后再调用 update_todos。";
            let result_at = now_string();
            self.session.append_tool_result(
                &new_id("msg"),
                session_id,
                &call.id,
                "update_todos",
                msg,
                "done",
                &result_at,
            )?;
            *sequence += 1;
            self.emit(AgentStreamEvent {
                kind: "tool_result".into(),
                session_id: session_id.into(),
                message_id: call.id.clone(),
                sequence: *sequence,
                text: Some(msg.into()),
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
            return Ok(());
        }

        // 校验通过：整组覆写持久化。
        self.session
            .set_session_todos(session_id, &items, &now_string())?;

        // emit todos_updated（带整组 todos，供前端实时刷新面板）。
        *sequence += 1;
        self.emit(AgentStreamEvent {
            kind: "todos_updated".into(),
            session_id: session_id.into(),
            message_id: call.id.clone(),
            sequence: *sequence,
            text: None,
            status: Some("done".into()),
            tool_name: Some(call.name.clone()),
            tool_label: self.label_for(&call.name),
            tool_call_id: Some(call.id.clone()),
            todos: Some(items.clone()),
            artifacts: None,
            parent_session_id: None,
            parent_tool_call_id: None,
            expert_name: None,
            created_at: now_string(),
        });

        // 落工具结果汇总（回灌给模型，让其知道当前清单状态）。
        let total = items.len();
        let done = items.iter().filter(|t| t.status == "completed").count();
        let prog = in_prog;
        let pend = items.iter().filter(|t| t.status == "pending").count();
        let summary = format!("已更新待办：{total} 项（完成 {done}/进行 {prog}/待办 {pend}）");
        let result_at = now_string();
        self.session.append_tool_result(
            &new_id("msg"),
            session_id,
            &call.id,
            "update_todos",
            &summary,
            "done",
            &result_at,
        )?;
        *sequence += 1;
        self.emit(AgentStreamEvent {
            kind: "tool_result".into(),
            session_id: session_id.into(),
            message_id: call.id.clone(),
            sequence: *sequence,
            text: Some(summary),
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
        Ok(())
    }

    /// 落一条简单的工具结果（汇总文本）+ emit，供 update_tasks 的成功/失败回灌。
    fn emit_task_tool_result(
        &self,
        session_id: &str,
        call: &ModelToolCall,
        sequence: &mut u64,
        text: &str,
        status: &str,
    ) -> Result<(), String> {
        let at = now_string();
        self.session.append_tool_result(
            &new_id("msg"),
            session_id,
            &call.id,
            &call.name,
            text,
            status,
            &at,
        )?;
        *sequence += 1;
        self.emit(AgentStreamEvent {
            kind: "tool_result".into(),
            session_id: session_id.into(),
            message_id: call.id.clone(),
            sequence: *sequence,
            text: Some(text.into()),
            status: Some(status.into()),
            tool_name: Some(call.name.clone()),
            tool_label: self.label_for(&call.name),
            tool_call_id: Some(call.id.clone()),
            todos: None,
            artifacts: None,
            parent_session_id: None,
            parent_tool_call_id: None,
            expert_name: None,
            created_at: at,
        });
        Ok(())
    }

    /// update_tasks 拦截（仅项目/团队线程）：全量覆写本线程任务台账、emit tasks_updated、
    /// 回传各任务 id（供 dispatch_agent 引用）。assignee 必须为名册成员，否则整次拒绝。
    pub(super) fn handle_update_tasks(
        &self,
        session_id: &str,
        call: &ModelToolCall,
        sequence: &mut u64,
    ) -> Result<(), String> {
        let session = self.session.get_session(session_id).ok().flatten();
        let project_id = session.as_ref().and_then(|s| s.project_id.clone());
        let role_kind = session
            .as_ref()
            .and_then(|s| s.role_kind.clone())
            .unwrap_or_default();
        let role_id = session
            .as_ref()
            .and_then(|s| s.role_id.clone())
            .unwrap_or_default();
        let orchestration = project_id.is_some() || role_kind == "team";

        let Some(projects) = self.projects.as_ref() else {
            return self.emit_task_tool_result(
                session_id,
                call,
                sequence,
                "任务台账不可用（未注入项目服务）",
                "failed",
            );
        };
        if !orchestration {
            return self.emit_task_tool_result(
                session_id,
                call,
                sequence,
                "update_tasks 仅用于项目/团队线程的任务台账；普通会话请用 update_todos。",
                "failed",
            );
        }

        let args = serde_json::from_str::<serde_json::Value>(&call.arguments_json)
            .unwrap_or(serde_json::Value::Null);
        let is_member = |name: &str| -> bool {
            match (project_id.as_deref(), role_kind.as_str()) {
                (Some(pid), _) => self.is_project_member(pid, name),
                (None, "team") => self
                    .teams
                    .as_ref()
                    .and_then(|t| t.detail(&role_id).ok())
                    .map(|d| d.members.iter().any(|m| m.name == name))
                    .unwrap_or(false),
                _ => false,
            }
        };

        let mut items: Vec<crate::project::TaskInput> = Vec::new();
        if let Some(raw) = args.get("tasks").and_then(|v| v.as_array()) {
            for entry in raw {
                let title = entry
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if title.is_empty() {
                    continue;
                }
                let id = entry
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
                    .filter(|s| !s.is_empty());
                let assignee = entry
                    .get("assignee")
                    .and_then(|v| v.as_str())
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty());
                if let Some(a) = &assignee {
                    if !is_member(a) {
                        let msg = format!(
                            "指派失败：「{a}」不是本{}的名册成员，只能委派给名册内成员。",
                            if role_kind == "team" {
                                "团队"
                            } else {
                                "项目"
                            }
                        );
                        return self
                            .emit_task_tool_result(session_id, call, sequence, &msg, "failed");
                    }
                }
                let status = entry
                    .get("status")
                    .and_then(|v| v.as_str())
                    .filter(|s| matches!(*s, "pending" | "in_progress" | "done"))
                    .map(str::to_string);
                items.push(crate::project::TaskInput {
                    id,
                    title,
                    assignee,
                    status,
                });
            }
        }

        // 本轮锚点：线程内最新一条 user 消息 id（同一请求内多次 update_tasks 归属同一主任务）。
        let round_msg_id = self
            .session
            .list_messages(session_id)
            .ok()
            .and_then(|ms| {
                ms.into_iter()
                    .rev()
                    .find(|m| m.role == "user")
                    .map(|m| m.id)
            })
            .unwrap_or_else(|| session_id.to_string());
        let goal = args.get("goal").and_then(|v| v.as_str()).unwrap_or("");
        let tasks = match projects.upsert_tasks(
            session_id,
            project_id.as_deref(),
            &round_msg_id,
            goal,
            &items,
        ) {
            Ok(t) => t,
            Err(e) => {
                return self.emit_task_tool_result(
                    session_id,
                    call,
                    sequence,
                    &format!("更新任务失败：{e}"),
                    "failed",
                )
            }
        };

        // emit tasks_updated（无 payload，前端按 thread 重取列表刷新）。
        *sequence += 1;
        self.emit(AgentStreamEvent {
            kind: "tasks_updated".into(),
            session_id: session_id.into(),
            message_id: call.id.clone(),
            sequence: *sequence,
            text: None,
            status: Some("done".into()),
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

        // 回传各任务 id + 状态，供 PM 后续 dispatch_agent(task_id=…)。
        let lines: Vec<String> = tasks
            .iter()
            .map(|t| {
                let who = t.assignee.as_deref().unwrap_or("(自办)");
                format!("- id={} [{}] {} → {}", t.id, t.status, t.title, who)
            })
            .collect();
        let summary = format!(
            "已更新任务台账（{} 项）。委派任务用 dispatch_agent(task_id=…) 派给对应成员：\n{}",
            tasks.len(),
            lines.join("\n")
        );
        self.emit_task_tool_result(session_id, call, sequence, &summary, "done")
    }

    /// search_knowledge 拦截：控制工具，引擎按名拦截、不真执行——解析会话挂载的知识库 → FTS 检索 → 带来源片段回灌为 tool_result。
    pub(super) fn handle_search_knowledge(
        &self,
        session_id: &str,
        call: &ModelToolCall,
        sequence: &mut u64,
    ) -> Result<(), String> {
        let args = serde_json::from_str::<serde_json::Value>(&call.arguments_json)
            .unwrap_or(serde_json::Value::Null);
        let query = args
            .get("query")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let top_k = args
            .get("top_k")
            .and_then(|v| v.as_u64())
            .unwrap_or(5)
            .clamp(1, 20) as usize;

        let result_text = match &self.knowledge {
            None => "知识库未启用。".to_string(),
            Some(store) => {
                // 并集解析：会话自身 ∪ 所属智能体 ∪ 所属项目挂载的资料库。
                let sess = self.session.get_session(session_id).ok().flatten();
                let mut scopes: Vec<(&str, String)> = vec![("session", session_id.to_string())];
                if let Some(s) = &sess {
                    if let Some(aid) = s.agent_id.clone().filter(|x| !x.is_empty()) {
                        scopes.push(("agent", aid));
                    }
                    if let Some(pid) = s.project_id.clone().filter(|x| !x.is_empty()) {
                        scopes.push(("project", pid));
                    }
                }
                let mut kb_ids: Vec<String> = Vec::new();
                for (st, sid) in &scopes {
                    if let Ok(ids) = store.resolve_mounted_kb_ids(st, sid) {
                        for id in ids {
                            if !kb_ids.contains(&id) {
                                kb_ids.push(id);
                            }
                        }
                    }
                }
                if kb_ids.is_empty() {
                    "当前对话未挂载任何资料库。可在对话设置里挂载资料库后再查阅。".to_string()
                } else if query.is_empty() {
                    "未提供检索词。".to_string()
                } else {
                    let enabled = self
                        .app_settings
                        .as_ref()
                        .and_then(|s| s.get_knowledge_vector_enabled().ok())
                        .unwrap_or(false);
                    let hits = match &self.embedder {
                        Some(emb) => crate::knowledge::retrieve::retrieve_knowledge(
                            store, emb.as_ref(), enabled, &query, &kb_ids, top_k,
                        ).unwrap_or_default(),
                        None => {
                            use crate::knowledge::retrieve::Retriever as _;
                            let retr = crate::knowledge::retrieve::Fts5Retriever { db: store.db.clone() };
                            retr.retrieve(&crate::knowledge::retrieve::RetrieveQuery { text: &query, kb_ids: &kb_ids, top_k }).unwrap_or_default()
                        }
                    };
                    format_hits(&hits)
                }
            }
        };

        let result_at = now_string();
        self.session.append_tool_result(
            &new_id("msg"),
            session_id,
            &call.id,
            "search_knowledge",
            &result_text,
            "done",
            &result_at,
        )?;
        *sequence += 1;
        self.emit(AgentStreamEvent {
            kind: "tool_result".into(),
            session_id: session_id.into(),
            message_id: call.id.clone(),
            sequence: *sequence,
            text: Some(truncate_event_text(&result_text, 2000)),
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
        Ok(())
    }

    /// add_artifact 拦截：控制工具，引擎按名拦截、不真执行——登记产物（绑定产生它的消息）、
    /// emit artifacts_updated（带整组）、落工具结果（不暂停、不需权限）。
    pub(super) fn handle_add_artifact(
        &self,
        session_id: &str,
        producing_message_id: &str,
        call: &ModelToolCall,
        sequence: &mut u64,
    ) -> Result<(), String> {
        let args = serde_json::from_str::<serde_json::Value>(&call.arguments_json)
            .unwrap_or(serde_json::Value::Null);
        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let result_at = now_string();
        if path.is_empty() {
            self.session.append_tool_result(
                &new_id("msg"),
                session_id,
                &call.id,
                "add_artifact",
                "错误：path 不能为空。",
                "done",
                &result_at,
            )?;
            *sequence += 1;
            self.emit(AgentStreamEvent {
                kind: "tool_result".into(),
                session_id: session_id.into(),
                message_id: call.id.clone(),
                sequence: *sequence,
                text: Some("错误：path 不能为空。".into()),
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
            return Ok(());
        }
        // title 缺省取 basename。
        let title = args
            .get("title")
            .and_then(|v| v.as_str())
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .unwrap_or_else(|| path.rsplit(['/', '\\']).next().unwrap_or(&path).to_string());
        // kind 缺省 final；仅接受 working 作为另一取值，其它一律归 final。
        let kind = match args.get("kind").and_then(|v| v.as_str()) {
            Some("working") => "working",
            _ => "final",
        };
        self.session.add_artifact(
            session_id,
            &path,
            &title,
            kind,
            Some(producing_message_id),
            Some(&call.id),
            &result_at,
        )?;
        let artifacts = self.session.list_artifacts(session_id)?;
        *sequence += 1;
        self.emit(AgentStreamEvent {
            kind: "artifacts_updated".into(),
            session_id: session_id.into(),
            message_id: call.id.clone(),
            sequence: *sequence,
            text: None,
            status: Some("done".into()),
            tool_name: Some(call.name.clone()),
            tool_label: self.label_for(&call.name),
            tool_call_id: Some(call.id.clone()),
            todos: None,
            artifacts: Some(artifacts),
            parent_session_id: None,
            parent_tool_call_id: None,
            expert_name: None,
            created_at: result_at.clone(),
        });
        let summary = format!("已登记产物：{title}");
        self.session.append_tool_result(
            &new_id("msg"),
            session_id,
            &call.id,
            "add_artifact",
            &summary,
            "done",
            &result_at,
        )?;
        *sequence += 1;
        self.emit(AgentStreamEvent {
            kind: "tool_result".into(),
            session_id: session_id.into(),
            message_id: call.id.clone(),
            sequence: *sequence,
            text: Some(summary),
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
        Ok(())
    }
}

/// 把命中片段格式化为带来源标注的文本：【文档 › 标题路径】正文。
pub(crate) fn format_hits(hits: &[crate::knowledge::types::RetrievedChunk]) -> String {
    if hits.is_empty() {
        return "未在已挂载的资料库中找到相关内容。".to_string();
    }
    let mut out = String::new();
    for (i, h) in hits.iter().enumerate() {
        let source = if h.heading_path.is_empty() {
            h.doc_title.clone()
        } else {
            format!("{} › {}", h.doc_title, h.heading_path)
        };
        out.push_str(&format!("[{}] 【{}】\n{}\n\n", i + 1, source, h.content));
    }
    out.trim_end().to_string()
}

#[cfg(test)]
mod search_knowledge_tests {
    use super::format_hits;
    use crate::knowledge::types::RetrievedChunk;

    #[test]
    fn empty_hits_message() {
        assert!(format_hits(&[]).contains("未在已挂载"));
    }

    #[test]
    fn formats_source_with_heading() {
        let hits = vec![RetrievedChunk {
            chunk_id: "c1".into(),
            doc_id: "d1".into(),
            doc_title: "研报".into(),
            heading_path: "趋势".into(),
            content: "光伏增长".into(),
            score: 0.0,
        }];
        let out = format_hits(&hits);
        assert!(out.contains("【研报 › 趋势】"));
        assert!(out.contains("光伏增长"));
    }
}

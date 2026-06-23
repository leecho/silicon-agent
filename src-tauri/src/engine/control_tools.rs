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

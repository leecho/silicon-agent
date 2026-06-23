//! SessionStore：消息读写、上下文压缩标记、会话级工具授权、快捷建议、pending 工具名。
use super::{message_from_row, SessionStore};
use crate::session::types::Message;

impl SessionStore {
    pub fn append_message(
        &self,
        id: &str,
        session_id: &str,
        role: &str,
        content: &str,
        reasoning: Option<&str>,
        now: &str,
    ) -> Result<Message, String> {
        self.db
            .with_connection(|c| {
                c.execute(
                    "insert into messages (id, session_id, role, content, reasoning, created_at) values (?1, ?2, ?3, ?4, ?5, ?6)",
                    rusqlite::params![id, session_id, role, content, reasoning, now],
                )?;
                c.execute(
                    "update sessions set updated_at = ?1 where id = ?2",
                    rusqlite::params![now, session_id],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())?;
        Ok(Message {
            id: id.into(),
            session_id: session_id.into(),
            role: role.into(),
            content: content.into(),
            reasoning: reasoning.map(str::to_string),
            tool_calls_json: None,
            tool_call_id: None,
            tool_name: None,
            tool_status: None,
            compacted: false,
            created_at: now.into(),
        })
    }

    /// 落一条携带工具调用的 assistant 消息（role=assistant，带 tool_calls_json）。
    pub fn append_assistant_tool_call(
        &self,
        id: &str,
        session_id: &str,
        content: &str,
        reasoning: Option<&str>,
        tool_calls_json: &str,
        now: &str,
    ) -> Result<Message, String> {
        self.db
            .with_connection(|c| {
                c.execute(
                    "insert into messages (id, session_id, role, content, reasoning, tool_calls_json, created_at) values (?1, ?2, 'assistant', ?3, ?4, ?5, ?6)",
                    rusqlite::params![id, session_id, content, reasoning, tool_calls_json, now],
                )?;
                c.execute(
                    "update sessions set updated_at = ?1 where id = ?2",
                    rusqlite::params![now, session_id],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())?;
        Ok(Message {
            id: id.into(),
            session_id: session_id.into(),
            role: "assistant".into(),
            content: content.into(),
            reasoning: reasoning.map(str::to_string),
            tool_calls_json: Some(tool_calls_json.into()),
            tool_call_id: None,
            tool_name: None,
            tool_status: None,
            compacted: false,
            created_at: now.into(),
        })
    }

    /// 落一条工具结果消息（role=tool，带 tool_call_id + tool_name + tool_status）。
    pub fn append_tool_result(
        &self,
        id: &str,
        session_id: &str,
        tool_call_id: &str,
        tool_name: &str,
        content: &str,
        status: &str,
        now: &str,
    ) -> Result<Message, String> {
        self.db
            .with_connection(|c| {
                c.execute(
                    "insert into messages (id, session_id, role, content, tool_call_id, tool_name, tool_status, created_at) values (?1, ?2, 'tool', ?3, ?4, ?5, ?6, ?7)",
                    rusqlite::params![id, session_id, content, tool_call_id, tool_name, status, now],
                )?;
                c.execute(
                    "update sessions set updated_at = ?1 where id = ?2",
                    rusqlite::params![now, session_id],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())?;
        Ok(Message {
            id: id.into(),
            session_id: session_id.into(),
            role: "tool".into(),
            content: content.into(),
            reasoning: None,
            tool_calls_json: None,
            tool_call_id: Some(tool_call_id.into()),
            tool_name: Some(tool_name.into()),
            tool_status: Some(status.into()),
            compacted: false,
            created_at: now.into(),
        })
    }

    pub fn list_messages(&self, session_id: &str) -> Result<Vec<Message>, String> {
        self.db
            .with_connection(|c| {
                let mut stmt = c.prepare(
                    "select id, session_id, role, content, reasoning, tool_calls_json, tool_call_id, tool_name, tool_status, compacted, created_at from messages where session_id = ?1 order by created_at, id",
                )?;
                let rows = stmt.query_map([session_id], message_from_row)?;
                let mut out = Vec::new();
                for row in rows {
                    out.push(row?);
                }
                Ok(out)
            })
            .map_err(|e| e.to_string())
    }

    /// 取某会话内某 tool_call 的结果状态（"done"/"failed"）；无对应 tool 结果消息则 None。
    pub fn tool_result_status(
        &self,
        session_id: &str,
        tool_call_id: &str,
    ) -> Result<Option<String>, String> {
        self.db
            .with_connection(|c| {
                let mut stmt = c.prepare(
                    "select tool_status from messages where session_id = ?1 and role = 'tool' and tool_call_id = ?2 order by created_at desc, id desc limit 1",
                )?;
                let mut rows = stmt.query_map(
                    rusqlite::params![session_id, tool_call_id],
                    |r| r.get::<_, Option<String>>(0),
                )?;
                Ok(match rows.next() {
                    Some(r) => r?,
                    None => None,
                })
            })
            .map_err(|e| e.to_string())
    }

    /// 取某会话**最后一条非空 assistant 文本**（供 child 子运行回禀摘要）。无则 None。
    pub fn last_assistant_text(&self, session_id: &str) -> Result<Option<String>, String> {
        self.db
            .with_connection(|c| {
                let mut stmt = c.prepare(
                    "select content from messages where session_id = ?1 and role = 'assistant' and trim(content) <> '' order by created_at desc, id desc limit 1",
                )?;
                let mut rows = stmt.query_map([session_id], |r| r.get::<_, String>(0))?;
                Ok(match rows.next() {
                    Some(r) => Some(r?),
                    None => None,
                })
            })
            .map_err(|e| e.to_string())
    }

    // ── Compact 上下文压缩 ───────────────────────────────────────────────────────

    /// 写入/覆写某会话最新的对话摘要（compact 后存这一段，引擎组装上下文时注入）。
    pub fn set_compaction_summary(
        &self,
        session_id: &str,
        summary: &str,
        now: &str,
    ) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                c.execute(
                    "update sessions set compaction_summary = ?1, updated_at = ?2 where id = ?3",
                    rusqlite::params![summary, now, session_id],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// 读取某会话的对话摘要。NULL / 空字符串 → None。
    pub fn get_compaction_summary(&self, session_id: &str) -> Result<Option<String>, String> {
        let value: Option<String> = self
            .db
            .with_connection(|c| {
                let mut stmt =
                    c.prepare("select compaction_summary from sessions where id = ?1")?;
                let mut rows = stmt.query_map([session_id], |r| r.get::<_, Option<String>>(0))?;
                Ok(match rows.next() {
                    Some(row) => row?,
                    None => None,
                })
            })
            .map_err(|e| e.to_string())?;
        Ok(value.filter(|s| !s.trim().is_empty()))
    }

    /// 返回需压缩的旧消息：除最近 `keep_recent` 条外、且尚未 compacted 的消息（保序）。
    /// compact 命令据此拼摘要素材并标记。
    pub fn messages_to_compact(
        &self,
        session_id: &str,
        keep_recent: usize,
    ) -> Result<Vec<Message>, String> {
        let all = self.list_messages(session_id)?;
        let cutoff = all.len().saturating_sub(keep_recent);
        Ok(all
            .into_iter()
            .take(cutoff)
            .filter(|m| !m.compacted)
            .collect())
    }

    /// 把给定 id 的消息标记为 compacted=1（被摘要吸收，引擎组装上下文时跳过）。
    pub fn mark_compacted(&self, session_id: &str, ids: &[String]) -> Result<(), String> {
        if ids.is_empty() {
            return Ok(());
        }
        self.db
            .with_transaction(|tx| {
                for id in ids {
                    tx.execute(
                        "update messages set compacted = 1 where id = ?1 and session_id = ?2",
                        rusqlite::params![id, session_id],
                    )?;
                }
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// 会话级授权某工具（insert or ignore，幂等）。
    pub fn grant_tool(&self, session_id: &str, tool_name: &str, now: &str) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                c.execute(
                    "insert or ignore into permission_grants (session_id, tool_name, created_at) values (?1, ?2, ?3)",
                    rusqlite::params![session_id, tool_name, now],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// 检查某工具在指定会话中是否已被授权。
    pub fn is_tool_granted(&self, session_id: &str, tool_name: &str) -> Result<bool, String> {
        self.db
            .with_connection(|c| {
                let n: i64 = c.query_row(
                    "select count(*) from permission_grants where session_id = ?1 and tool_name = ?2",
                    rusqlite::params![session_id, tool_name],
                    |r| r.get(0),
                )?;
                Ok(n > 0)
            })
            .map_err(|e| e.to_string())
    }

    /// 写一轮结束后的快捷建议（JSON 数组）。传空切片即清空。
    pub fn set_last_suggestions(
        &self,
        session_id: &str,
        suggestions: &[String],
    ) -> Result<(), String> {
        let json = serde_json::to_string(suggestions).unwrap_or_else(|_| "[]".into());
        self.db
            .with_connection(|c| {
                c.execute(
                    "update sessions set last_suggestions = ?1 where id = ?2",
                    rusqlite::params![json, session_id],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// 从末条带 tool_calls 的 assistant 消息中，查找 tool_call_id 对应的工具名。
    /// 用于 `submit_permission_decision` 命令按 id 定位 tool_name。
    pub fn find_pending_tool_name(
        &self,
        session_id: &str,
        tool_call_id: &str,
    ) -> Result<Option<String>, String> {
        let messages = self.list_messages(session_id)?;
        // 找末条 role=assistant 且 tool_calls_json 非空的消息。
        let anchor = messages.iter().rev().find(|m| {
            m.role == "assistant"
                && m.tool_calls_json
                    .as_deref()
                    .map(|s| !s.trim().is_empty())
                    .unwrap_or(false)
        });
        let anchor = match anchor {
            Some(a) => a,
            None => return Ok(None),
        };
        let tool_calls_json = match anchor.tool_calls_json.as_deref() {
            Some(j) => j,
            None => return Ok(None),
        };
        // 解析 Vec<{id, name, arguments_json}> 找匹配 id 的 name。
        let calls: Vec<serde_json::Value> =
            serde_json::from_str(tool_calls_json).unwrap_or_default();
        for call in &calls {
            let id = call.get("id").and_then(|v| v.as_str()).unwrap_or("");
            if id == tool_call_id {
                let name = call
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                return Ok(Some(name));
            }
        }
        Ok(None)
    }

    /// 找会话当前**第一个悬空的 tool_call**（末条 assistant 的 tool_calls 里、尚无 tool_result 的那个）
    /// 返回 `(tool_call_id, tool_name)`。这是「暂停态」的结构化判别源——权限/ask/plan/dispatch 暂停
    /// 都表现为一条悬空 tool_call。无悬空则 None。供停止收口用。
    pub fn first_dangling_tool_call(
        &self,
        session_id: &str,
    ) -> Result<Option<(String, String)>, String> {
        let messages = self.list_messages(session_id)?;
        let anchor = messages.iter().rev().find(|m| {
            m.role == "assistant"
                && m.tool_calls_json
                    .as_deref()
                    .map(|s| !s.trim().is_empty())
                    .unwrap_or(false)
        });
        let Some(anchor) = anchor else {
            return Ok(None);
        };
        let Some(json) = anchor.tool_calls_json.as_deref() else {
            return Ok(None);
        };
        let calls: Vec<serde_json::Value> = serde_json::from_str(json).unwrap_or_default();
        for call in &calls {
            let id = call.get("id").and_then(|v| v.as_str()).unwrap_or("");
            if id.is_empty() {
                continue;
            }
            if self.tool_result_status(session_id, id)?.is_some() {
                continue;
            }
            let name = call
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            return Ok(Some((id.to_string(), name)));
        }
        Ok(None)
    }

    /// 收口一条悬空 tool_call：若其尚无结果，写一条 `status="cancelled"` 的工具结果解析掉 pending
    /// （前端据该状态渲染分隔线；正文是模型可读的自然语言）。已有结果则跳过（幂等）。返回是否真正写入。
    /// 停止时用于「让暂停态会话不再 pending、决策不复活、reload 不复现卡片」。
    pub fn settle_pending_tool_call(
        &self,
        session_id: &str,
        tool_call_id: &str,
        status_text: &str,
        now: &str,
    ) -> Result<bool, String> {
        if self.tool_result_status(session_id, tool_call_id)?.is_some() {
            return Ok(false);
        }
        let Some(name) = self.find_pending_tool_name(session_id, tool_call_id)? else {
            return Ok(false);
        };
        self.append_tool_result(
            &crate::session::new_id("msg"),
            session_id,
            tool_call_id,
            &name,
            status_text,
            "cancelled",
            now,
        )?;
        Ok(true)
    }

    /// 落一条「已手动停止」标记消息：role="stopped" + compacted=1——仅在 feed 渲染成分隔线、不进模型
    /// 上下文（续跑时模型看不到它），使 reload 后仍能看到本轮被手动停止。引擎与运行编排层共用此实现。
    pub fn append_stopped_marker(&self, session_id: &str, at: &str) {
        self.append_divider_marker(session_id, "已手动停止", at);
    }

    /// 落一条「上一轮因进程退出未完成」标记：进程被 kill 后启动恢复时补，告知用户该轮被中断（非正常完成）。
    /// 同 `append_stopped_marker`，role="stopped" 渲染成分隔线、不进模型上下文。
    pub fn append_interrupted_marker(&self, session_id: &str, at: &str) {
        self.append_divider_marker(session_id, "上一轮因进程退出未完成", at);
    }

    /// 落一条分隔标记消息（role="stopped" + compacted=1）：仅 feed 渲染成分隔线、不进模型上下文。
    fn append_divider_marker(&self, session_id: &str, text: &str, at: &str) {
        let marker_id = crate::session::new_id("msg");
        let _ = self.append_message(&marker_id, session_id, "stopped", text, None, at);
        let _ = self.mark_compacted(session_id, &[marker_id]);
    }

    /// 给定一个 dispatch tool_call_id，找出**产出它的那条 assistant 消息 id**（= 该专家所属「轮次」键）。
    /// 同一轮 fan-out 的多个专家共享同一条 assistant 消息，故可据此把专家按轮次分组。无则 None。
    pub fn find_dispatch_round(
        &self,
        session_id: &str,
        tool_call_id: &str,
    ) -> Result<Option<String>, String> {
        let messages = self.list_messages(session_id)?;
        for m in &messages {
            if m.role != "assistant" {
                continue;
            }
            let Some(json) = m.tool_calls_json.as_deref() else {
                continue;
            };
            if json.trim().is_empty() {
                continue;
            }
            let calls: Vec<serde_json::Value> = serde_json::from_str(json).unwrap_or_default();
            if calls
                .iter()
                .any(|c| c.get("id").and_then(|v| v.as_str()) == Some(tool_call_id))
            {
                return Ok(Some(m.id.clone()));
            }
        }
        Ok(None)
    }
}

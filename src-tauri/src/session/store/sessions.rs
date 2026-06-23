//! SessionStore：会话行生命周期、会话级列（mode/working_dir/model/permission 覆盖）、详情。
use super::{new_id, session_from_row, SessionStore};
use crate::session::types::{Artifact, SessionInfo};

impl SessionStore {
    pub fn create_session(
        &self,
        id: &str,
        title: &str,
        now: &str,
        is_draft: bool,
    ) -> Result<SessionInfo, String> {
        self.db
            .with_connection(|c| {
                c.execute(
                    "insert into sessions (id, title, created_at, updated_at, is_draft) values (?1, ?2, ?3, ?3, ?4)",
                    rusqlite::params![id, title, now, is_draft as i64],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())?;
        Ok(SessionInfo {
            id: id.into(),
            title: title.into(),
            created_at: now.into(),
            updated_at: now.into(),
            pinned: false,
            group_id: None,
            mode: "normal".into(),
            working_dir: None,
            permission_mode: None,
            selected_model_id: None,
            origin: "user".into(),
            is_draft,
            draft_content: String::new(),
            last_suggestions: Vec::new(),
            is_running: false,
            run_started_at: None,
            parent_session_id: None,
            parent_tool_call_id: None,
            expert_name: None,
            agent_task: None,
            awaiting_subagent: None,
            expert_system_prompt: None,
            expert_tools: None,
            agent_id: None,
            role_kind: None,
            role_id: None,
            is_background: false,
            run_outcome: None,
            pending_collect: None,
            project_id: None,
            pending_tasks: None,
        })
    }

    /// T70：读会话任务队列原始 JSON（None=空闲）。
    pub fn get_pending_tasks(&self, session_id: &str) -> Result<Option<String>, String> {
        self.db
            .with_connection(|c| {
                let mut stmt = c.prepare("select pending_tasks from sessions where id = ?1")?;
                let mut rows = stmt.query_map([session_id], |r| r.get::<_, Option<String>>(0))?;
                Ok(match rows.next() {
                    Some(v) => v?,
                    None => None,
                })
            })
            .map_err(|e| e.to_string())
    }

    /// T70：写会话任务队列原始 JSON（None=清空，置列为 null）。
    pub fn set_pending_tasks(
        &self,
        session_id: &str,
        json: Option<&str>,
        now: &str,
    ) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                c.execute(
                    "update sessions set pending_tasks = ?1, updated_at = ?2 where id = ?3",
                    rusqlite::params![json, now, session_id],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// 设/清会话所属持久智能体。智能体是会话归属实体，不再写入运行角色字段。
    pub fn set_agent_id(
        &self,
        session_id: &str,
        agent_id: Option<&str>,
        now: &str,
    ) -> Result<(), String> {
        let agent_id = agent_id.and_then(|id| {
            let trimmed = id.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        });
        self.db
            .with_connection(|c| {
                c.execute(
                    "update sessions set agent_id = ?1, updated_at = ?2 where id = ?3",
                    rusqlite::params![agent_id, now, session_id],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// 设/清会话运行角色。仅 expert/team 这类定义进入 role_*；项目/智能体归属走实体字段。
    pub fn set_role(
        &self,
        session_id: &str,
        kind: Option<&str>,
        id: Option<&str>,
        now: &str,
    ) -> Result<(), String> {
        let normalized = kind
            .and_then(|k| {
                let k = k.trim();
                if k.is_empty() {
                    None
                } else {
                    Some(k)
                }
            })
            .zip(id.and_then(|i| {
                let i = i.trim();
                if i.is_empty() {
                    None
                } else {
                    Some(i)
                }
            }));
        let (kind, id) = match normalized {
            Some((kind @ ("expert" | "team"), id)) => (Some(kind), Some(id)),
            Some((other, _)) => return Err(format!("非法角色类型：{other}")),
            None => (None, None),
        };
        self.db
            .with_connection(|c| {
                c.execute(
                    "update sessions set role_kind = ?1, role_id = ?2, updated_at = ?3 where id = ?4",
                    rusqlite::params![kind, id, now, session_id],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// T59：设会话所属项目 id（项目线程及其 child）。
    pub fn set_project_id(
        &self,
        session_id: &str,
        project_id: &str,
        now: &str,
    ) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                c.execute(
                    "update sessions set project_id = ?1, updated_at = ?2 where id = ?3",
                    rusqlite::params![project_id, now, session_id],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// T59：列项目顶层线程（project_id 命中且无父会话），按 updated_at 倒序。
    pub fn list_project_threads(&self, project_id: &str) -> Result<Vec<SessionInfo>, String> {
        self.db
            .with_connection(|c| {
                let mut stmt = c.prepare(
                    "select id, title, created_at, updated_at, pinned, group_id, mode, working_dir, permission_mode, selected_model_id, origin, is_draft, draft_content, last_suggestions, parent_session_id, parent_tool_call_id, expert_name, agent_task, awaiting_subagent, expert_system_prompt, expert_tools, agent_id, role_kind, role_id, is_background, run_outcome, pending_collect, project_id, pending_tasks from sessions where project_id = ?1 and parent_session_id is null order by updated_at desc, id desc",
                )?;
                let rows = stmt.query_map([project_id], session_from_row)?;
                let mut out = Vec::new();
                for r in rows {
                    out.push(r?);
                }
                Ok(out)
            })
            .map_err(|e| e.to_string())
    }

    /// 建一个子运行会话：一次性写入父子链 + agent 归属 + origin="subagent"。
    /// `system_prompt`/`tools` 非 None = ad-hoc（动态生成）专家；None = 声明式（运行时查 spec）。
    pub fn create_child_session(
        &self,
        id: &str,
        parent_session_id: &str,
        parent_tool_call_id: &str,
        expert_name: &str,
        agent_task: &str,
        system_prompt: Option<&str>,
        tools: Option<&str>,
        is_background: bool,
        now: &str,
        display_name: Option<&str>,
    ) -> Result<SessionInfo, String> {
        // 标题用展示名（如「逆向投资人」）；缺省回退原始 expert_name。
        let label = display_name
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(expert_name);
        self.db
            .with_connection(|c| {
                c.execute(
                    "insert into sessions (id, title, created_at, updated_at, origin, parent_session_id, parent_tool_call_id, expert_name, agent_task, expert_system_prompt, expert_tools, is_background) \
                     values (?1, ?2, ?3, ?3, 'subagent', ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    rusqlite::params![
                        id,
                        format!("专家·{label}"),
                        now,
                        parent_session_id,
                        parent_tool_call_id,
                        expert_name,
                        agent_task,
                        system_prompt,
                        tools,
                        is_background as i64
                    ],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())?;
        self.get_session(id)?
            .ok_or_else(|| "建子会话后读取失败".into())
    }

    /// T57：记子运行终态（done|failed|cancelled）。
    pub fn set_run_outcome(&self, id: &str, outcome: &str, now: &str) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                c.execute(
                    "update sessions set run_outcome = ?1, updated_at = ?2 where id = ?3",
                    rusqlite::params![outcome, now, id],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// T57：设/清父会话 collect 停泊态（None=清）。
    pub fn set_pending_collect(
        &self,
        parent_id: &str,
        json: Option<&str>,
        now: &str,
    ) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                c.execute(
                    "update sessions set pending_collect = ?1, updated_at = ?2 where id = ?3",
                    rusqlite::params![json, now, parent_id],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// T57：父名下「后台且未收集」的 dispatch 句柄(=parent_tool_call_id)去重列表，按创建序。
    pub fn background_handles_uncollected(&self, parent_id: &str) -> Result<Vec<String>, String> {
        self.db
            .with_connection(|c| {
                let mut stmt = c.prepare(
                    "select parent_tool_call_id from sessions \
                     where parent_session_id = ?1 and is_background = 1 and collected = 0 \
                       and parent_tool_call_id is not null \
                     group by parent_tool_call_id order by min(created_at)",
                )?;
                let rows = stmt.query_map([parent_id], |r| r.get::<_, String>(0))?;
                let mut out = Vec::new();
                for r in rows {
                    out.push(r?);
                }
                Ok(out)
            })
            .map_err(|e| e.to_string())
    }

    /// T57：按句柄集汇总后台子代理结论。返回 (汇总文本, 已终态child集, 仍运行child集)。
    /// 每个 handle 取其最新 child（兼容 #4 重试换 child）。
    pub fn collect_summary(
        &self,
        parent_id: &str,
        handles: &[String],
    ) -> Result<(String, Vec<String>, Vec<String>), String> {
        let mut blocks = Vec::new();
        let mut terminal = Vec::new();
        let mut running = Vec::new();
        for h in handles {
            let child = self.find_child_by_dispatch(parent_id, h)?;
            let Some(cid) = child else {
                blocks.push(format!("【handle={h}】未找到对应子代理。"));
                continue;
            };
            let info = self.get_session(&cid)?;
            let name = info
                .as_ref()
                .and_then(|s| s.expert_name.clone())
                .unwrap_or_else(|| "专家".into());
            let outcome = info.as_ref().and_then(|s| s.run_outcome.clone());
            match outcome.as_deref() {
                Some(state) => {
                    let summary = self
                        .last_assistant_text(&cid)?
                        .unwrap_or_else(|| "（无文本产出）".into());
                    let label = match state {
                        "failed" => "失败",
                        "cancelled" => "已取消",
                        _ => "完成",
                    };
                    blocks.push(format!("【{name}】({label})\n{summary}"));
                    terminal.push(cid);
                }
                None => {
                    blocks.push(format!("【{name}】仍在运行（handle={h}）。"));
                    running.push(cid);
                }
            }
        }
        Ok((blocks.join("\n\n---\n\n"), terminal, running))
    }

    /// T57：标记某 child 已被 collect 收集。
    pub fn mark_collected(&self, child_id: &str) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                c.execute(
                    "update sessions set collected = 1 where id = ?1",
                    rusqlite::params![child_id],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// 列某父会话的全部 child（专家）会话，按创建时间升序。
    pub fn list_children(&self, parent_session_id: &str) -> Result<Vec<SessionInfo>, String> {
        self.db
            .with_connection(|c| {
                let mut stmt = c.prepare(
                    "select id, title, created_at, updated_at, pinned, group_id, mode, working_dir, permission_mode, selected_model_id, origin, is_draft, draft_content, last_suggestions, parent_session_id, parent_tool_call_id, expert_name, agent_task, awaiting_subagent, expert_system_prompt, expert_tools, agent_id, role_kind, role_id, is_background, run_outcome, pending_collect, project_id, pending_tasks from sessions where parent_session_id = ?1 order by created_at, id",
                )?;
                let rows = stmt.query_map([parent_session_id], super::session_from_row)?;
                let mut out = Vec::new();
                for r in rows {
                    out.push(r?);
                }
                Ok(out)
            })
            .map_err(|e| e.to_string())
    }

    /// 按 (父会话, dispatch tool_call id) 找 child 会话 id（供前端「打开专家」在无 live 事件时定位）。
    pub fn find_child_by_dispatch(
        &self,
        parent_session_id: &str,
        parent_tool_call_id: &str,
    ) -> Result<Option<String>, String> {
        self.db
            .with_connection(|c| {
                let mut stmt = c.prepare(
                    "select id from sessions where parent_session_id = ?1 and parent_tool_call_id = ?2 order by created_at desc, id desc limit 1",
                )?;
                let mut rows = stmt.query_map(
                    rusqlite::params![parent_session_id, parent_tool_call_id],
                    |r| r.get::<_, String>(0),
                )?;
                Ok(match rows.next() {
                    Some(r) => Some(r?),
                    None => None,
                })
            })
            .map_err(|e| e.to_string())
    }

    /// 设父 run 停泊态：等待 child_session_id 完成。
    pub fn set_awaiting_subagent(
        &self,
        parent_id: &str,
        child_id: &str,
        now: &str,
    ) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                c.execute(
                    "update sessions set awaiting_subagent = ?1, updated_at = ?2 where id = ?3",
                    rusqlite::params![child_id, now, parent_id],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// 清父 run 停泊态（child 完成后）。
    pub fn clear_awaiting_subagent(&self, parent_id: &str, now: &str) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                c.execute(
                    "update sessions set awaiting_subagent = null, updated_at = ?1 where id = ?2",
                    rusqlite::params![now, parent_id],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// 停止一个停泊在子代理（dispatch）上的父会话时解冻它：把所有**悬空**的 dispatch tool_call
    /// （父已发起、但子运行被取消而无 tool_result）回填为「已停止」结果，并清 `awaiting_subagent`，
    /// 使父从「停泊·Composer 禁用·STOP 永转」恢复为可交互的已停止态。返回回填的
    /// `(tool_call_id, summary)`，供上层 emit 重建 feed / 翻卡。不续跑父（用户已明确停止）。
    /// 串行回归：子被 `stop_children` 取消后其运行线程到检查点直接退出、不回填父，此处补这一步。
    pub fn cancel_dangling_dispatches(
        &self,
        parent_id: &str,
        dispatch_tool_name: &str,
        now: &str,
    ) -> Result<Vec<(String, String)>, String> {
        let parent = match self.get_session(parent_id)? {
            Some(p) => p,
            None => return Ok(Vec::new()),
        };
        // 仅 dispatch 停泊态需回填；非停泊（已恢复 / 从未停泊）直接返回（幂等）。
        if parent.awaiting_subagent.is_none() {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for c in self.list_children(parent_id)? {
            if c.origin != "subagent" {
                continue;
            }
            let Some(tc) = c.parent_tool_call_id.clone() else {
                continue;
            };
            // 已有结果（子运行抢先回填 / 重复调用）→ 跳过，避免重复 tool 结果。
            if self.tool_result_status(parent_id, &tc)?.is_some() {
                continue;
            }
            let partial = self.last_assistant_text(&c.id)?.unwrap_or_default();
            let summary = if partial.trim().is_empty() {
                "（用户已停止，子代理无产出）".to_string()
            } else {
                format!("（用户已停止该子代理）此前进展：\n{partial}")
            };
            self.append_tool_result(
                &crate::session::new_id("msg"),
                parent_id,
                &tc,
                dispatch_tool_name,
                &summary,
                "failed",
                now,
            )?;
            out.push((tc, summary));
        }
        self.clear_awaiting_subagent(parent_id, now)?;
        Ok(out)
    }

    /// 父会话当前仍未回填结果的子运行数（其 dispatch tool 结果缺失）。
    /// 0 = 本批并行 child 已全部回禀，可续跑父。用于并行派发的「全部完成」判定。
    pub fn pending_child_count(&self, parent_id: &str) -> Result<usize, String> {
        let children = self.list_children(parent_id)?;
        let mut n = 0;
        for c in children {
            if let Some(tc) = c.parent_tool_call_id.as_deref() {
                if self.tool_result_status(parent_id, tc)?.is_none() {
                    n += 1;
                }
            }
        }
        Ok(n)
    }

    /// 列出所有「停泊等待专家」的父会话 `(parent_id, awaiting_child_id)`，供启动恢复扫描。
    /// 客户端关闭后 child 线程随进程消亡，这些父会永久停泊，需解冻。
    pub fn list_awaiting_parents(&self) -> Result<Vec<(String, String)>, String> {
        self.db
            .with_connection(|c| {
                let mut stmt = c.prepare(
                    "select id, awaiting_subagent from sessions where awaiting_subagent is not null and trim(awaiting_subagent) <> ''",
                )?;
                let rows = stmt.query_map([], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                })?;
                let mut out = Vec::new();
                for row in rows {
                    out.push(row?);
                }
                Ok(out)
            })
            .map_err(|e| e.to_string())
    }

    /// MVP 单会话：取最近会话，无则建一个默认会话。
    pub fn get_or_create_default(&self, now: &str) -> Result<SessionInfo, String> {
        if let Some(session) = self.latest_session()? {
            return Ok(session);
        }
        self.create_session(&new_id("session"), "新会话", now, false)
    }

    fn latest_session(&self) -> Result<Option<SessionInfo>, String> {
        self.db
            .with_connection(|c| {
                let mut stmt = c.prepare(
                    "select id, title, created_at, updated_at, pinned, group_id, mode, working_dir, permission_mode, selected_model_id, origin, is_draft, draft_content, last_suggestions, parent_session_id, parent_tool_call_id, expert_name, agent_task, awaiting_subagent, expert_system_prompt, expert_tools, agent_id, role_kind, role_id, is_background, run_outcome, pending_collect, project_id, pending_tasks from sessions order by updated_at desc, id desc limit 1",
                )?;
                let mut rows = stmt.query_map([], session_from_row)?;
                Ok(match rows.next() {
                    Some(row) => Some(row?),
                    None => None,
                })
            })
            .map_err(|e| e.to_string())
    }

    pub fn list_sessions(&self) -> Result<Vec<SessionInfo>, String> {
        self.db
            .with_connection(|c| {
                let mut stmt = c.prepare(
                    "select id, title, created_at, updated_at, pinned, group_id, mode, working_dir, permission_mode, selected_model_id, origin, is_draft, draft_content, last_suggestions, parent_session_id, parent_tool_call_id, expert_name, agent_task, awaiting_subagent, expert_system_prompt, expert_tools, agent_id, role_kind, role_id, is_background, run_outcome, pending_collect, project_id, pending_tasks from sessions order by updated_at desc, id desc",
                )?;
                let rows = stmt.query_map([], session_from_row)?;
                let mut out = Vec::new();
                for row in rows {
                    out.push(row?);
                }
                Ok(out)
            })
            .map_err(|e| e.to_string())
    }

    /// 列出某个持久智能体直接激活的顶层会话。项目线程和 child 子会话不属于该视图。
    pub fn list_agent_threads(&self, agent_id: &str) -> Result<Vec<SessionInfo>, String> {
        self.db
            .with_connection(|c| {
                let mut stmt = c.prepare(
                    "select id, title, created_at, updated_at, pinned, group_id, mode, working_dir, permission_mode, selected_model_id, origin, is_draft, draft_content, last_suggestions, parent_session_id, parent_tool_call_id, expert_name, agent_task, awaiting_subagent, expert_system_prompt, expert_tools, agent_id, role_kind, role_id, is_background, run_outcome, pending_collect, project_id, pending_tasks from sessions where agent_id = ?1 and parent_session_id is null order by updated_at desc, id desc",
                )?;
                let rows = stmt.query_map([agent_id], session_from_row)?;
                let mut out = Vec::new();
                for row in rows {
                    out.push(row?);
                }
                Ok(out)
            })
            .map_err(|e| e.to_string())
    }

    pub fn get_session(&self, session_id: &str) -> Result<Option<SessionInfo>, String> {
        self.db
            .with_connection(|c| {
                let mut stmt = c.prepare(
                    "select id, title, created_at, updated_at, pinned, group_id, mode, working_dir, permission_mode, selected_model_id, origin, is_draft, draft_content, last_suggestions, parent_session_id, parent_tool_call_id, expert_name, agent_task, awaiting_subagent, expert_system_prompt, expert_tools, agent_id, role_kind, role_id, is_background, run_outcome, pending_collect, project_id, pending_tasks from sessions where id = ?1",
                )?;
                let mut rows = stmt.query_map([session_id], session_from_row)?;
                Ok(match rows.next() {
                    Some(row) => Some(row?),
                    None => None,
                })
            })
            .map_err(|e| e.to_string())
    }

    pub fn update_session_title(
        &self,
        session_id: &str,
        title: &str,
        now: &str,
    ) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                c.execute(
                    "update sessions set title = ?1, updated_at = ?2 where id = ?3",
                    rusqlite::params![title, now, session_id],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    pub fn delete_session(&self, session_id: &str) -> Result<(), String> {
        self.db
            .with_transaction(|tx| {
                tx.execute(
                    "delete from messages where session_id = ?1",
                    rusqlite::params![session_id],
                )?;
                tx.execute(
                    "delete from permission_grants where session_id = ?1",
                    rusqlite::params![session_id],
                )?;
                tx.execute(
                    "delete from sessions where id = ?1",
                    rusqlite::params![session_id],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// 仅当 title='新会话' 时更新标题，返回是否实际更新（changes()>0）。
    pub fn set_title_if_default(
        &self,
        session_id: &str,
        title: &str,
        now: &str,
    ) -> Result<bool, String> {
        let changed = self
            .db
            .with_connection(|c| {
                let n = c.execute(
                    "update sessions set title = ?1, updated_at = ?2 where id = ?3 and title = '新会话'",
                    rusqlite::params![title, now, session_id],
                )?;
                Ok(n > 0)
            })
            .map_err(|e| e.to_string())?;
        Ok(changed)
    }

    // ── 置顶 / 分组 ──────────────────────────────────────────────────────────────

    /// 置顶 / 取消置顶某会话。
    pub fn set_session_pinned(
        &self,
        session_id: &str,
        pinned: bool,
        now: &str,
    ) -> Result<(), String> {
        let flag: i64 = if pinned { 1 } else { 0 };
        self.db
            .with_connection(|c| {
                c.execute(
                    "update sessions set pinned = ?1, updated_at = ?2 where id = ?3",
                    rusqlite::params![flag, now, session_id],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// 把会话归入某分组（`Some(group_id)`）或移出分组（`None` → NULL）。
    pub fn set_session_group(
        &self,
        session_id: &str,
        group_id: Option<&str>,
        now: &str,
    ) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                c.execute(
                    "update sessions set group_id = ?1, updated_at = ?2 where id = ?3",
                    rusqlite::params![group_id, now, session_id],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// 保存草稿内容（同时刷新 updated_at 以便排序/持久）。
    pub fn set_draft_content(
        &self,
        session_id: &str,
        content: &str,
        now: &str,
    ) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                c.execute(
                    "update sessions set draft_content = ?1, updated_at = ?2 where id = ?3",
                    rusqlite::params![content, now, session_id],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// 草稿升级为正式会话：清除草稿标记与暂存内容（幂等：非草稿调用无副作用）。
    pub fn promote_draft(&self, session_id: &str) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                c.execute(
                    "update sessions set is_draft = 0, draft_content = '' where id = ?1",
                    rusqlite::params![session_id],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// 清理空草稿：is_draft 且 draft_content 为空且无任何消息。返回删除条数。
    pub fn cleanup_empty_drafts(&self) -> Result<usize, String> {
        self.db
            .with_connection(|c| {
                let n = c.execute(
                    "delete from sessions where is_draft = 1 and draft_content = '' \
                     and id not in (select distinct session_id from messages)",
                    [],
                )?;
                Ok(n)
            })
            .map_err(|e| e.to_string())
    }

    /// 设置会话来源（user | scheduled | im ...）。定时任务在新建会话后标记为 scheduled。
    pub fn set_session_origin(&self, session_id: &str, origin: &str) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                c.execute(
                    "update sessions set origin = ?1 where id = ?2",
                    rusqlite::params![origin, session_id],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    // ── 计划模式（会话级 mode）─────────────────────────────────────────────────

    /// 读取某会话的工作模式（normal | plan）。会话缺失 / 列为空 → "normal"。
    pub fn get_session_mode(&self, session_id: &str) -> Result<String, String> {
        let value: Option<String> = self
            .db
            .with_connection(|c| {
                let mut stmt = c.prepare("select mode from sessions where id = ?1")?;
                let mut rows = stmt.query_map([session_id], |r| r.get::<_, Option<String>>(0))?;
                Ok(match rows.next() {
                    Some(row) => row?,
                    None => None,
                })
            })
            .map_err(|e| e.to_string())?;
        Ok(value
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "normal".into()))
    }

    /// 设置某会话的工作模式。仅接受 "normal" | "plan"，其余 → Err。
    pub fn set_session_mode(&self, session_id: &str, mode: &str, now: &str) -> Result<(), String> {
        if mode != "normal" && mode != "plan" {
            return Err(format!("非法会话模式：{mode}（仅支持 normal | plan）"));
        }
        self.db
            .with_connection(|c| {
                c.execute(
                    "update sessions set mode = ?1, updated_at = ?2 where id = ?3",
                    rusqlite::params![mode, now, session_id],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// 读取某会话的工作目录（未设为 None）。
    pub fn get_working_dir(&self, session_id: &str) -> Result<Option<String>, String> {
        self.db
            .with_connection(|c| {
                let mut stmt = c.prepare("select working_dir from sessions where id = ?1")?;
                let mut rows = stmt.query_map([session_id], |r| r.get::<_, Option<String>>(0))?;
                Ok(match rows.next() {
                    Some(v) => v?,
                    None => None,
                })
            })
            .map_err(|e| e.to_string())
    }

    /// 写入某会话的工作目录。
    pub fn set_working_dir(&self, session_id: &str, dir: &str, now: &str) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                c.execute(
                    "update sessions set working_dir = ?1, updated_at = ?2 where id = ?3",
                    rusqlite::params![dir, now, session_id],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// 读取会话选中的模型 id（未选为 None）。
    pub fn get_selected_model_id(&self, session_id: &str) -> Result<Option<String>, String> {
        self.db
            .with_connection(|c| {
                let mut stmt = c.prepare("select selected_model_id from sessions where id = ?1")?;
                let mut rows = stmt.query_map([session_id], |r| r.get::<_, Option<String>>(0))?;
                Ok(match rows.next() {
                    Some(v) => v?,
                    None => None,
                })
            })
            .map_err(|e| e.to_string())
    }

    /// 写入会话选中的模型 id（None 置空）。
    pub fn set_selected_model_id(
        &self,
        session_id: &str,
        model_id: Option<&str>,
        now: &str,
    ) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                c.execute(
                    "update sessions set selected_model_id = ?1, updated_at = ?2 where id = ?3",
                    rusqlite::params![model_id, now, session_id],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// 读会话级权限模式覆盖；NULL 返回 None（继承全局）。
    pub fn get_session_permission_mode(&self, session_id: &str) -> Result<Option<String>, String> {
        self.db
            .with_connection(|c| {
                let mut stmt = c.prepare("select permission_mode from sessions where id = ?1")?;
                let mut rows = stmt.query_map([session_id], |r| r.get::<_, Option<String>>(0))?;
                Ok(match rows.next() {
                    Some(v) => v?,
                    None => None,
                })
            })
            .map_err(|e| e.to_string())
    }

    /// 设/清会话级权限模式覆盖。`Some(m)` 设覆盖，`None` 清除（回归继承全局）。
    pub fn set_session_permission_mode(
        &self,
        session_id: &str,
        mode: Option<&str>,
        now: &str,
    ) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                c.execute(
                    "update sessions set permission_mode = ?1, updated_at = ?2 where id = ?3",
                    rusqlite::params![mode, now, session_id],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// 记录一个最近使用的工作目录（去重 upsert，刷新 used_at）。
    pub fn add_recent_workspace(&self, path: &str, now: &str) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                c.execute(
                    "insert into recent_workspaces (path, used_at) values (?1, ?2) \
                     on conflict(path) do update set used_at = excluded.used_at",
                    rusqlite::params![path, now],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// 列出最近使用的工作目录（按 used_at 倒序，最多 limit 个）。
    pub fn list_recent_workspaces(&self, limit: u32) -> Result<Vec<String>, String> {
        self.db
            .with_connection(|c| {
                let mut stmt = c.prepare(
                    "select path from recent_workspaces order by used_at desc, path asc limit ?1",
                )?;
                let rows = stmt.query_map([limit], |r| r.get::<_, String>(0))?;
                let mut out = Vec::new();
                for row in rows {
                    out.push(row?);
                }
                Ok(out)
            })
            .map_err(|e| e.to_string())
    }

    /// 登记一个产物（upsert：重复 path 更新 title/message_id/tool_call_id/kind，保留首次 created_at）。
    /// kind 取 final（最终交付文件）| working（脚本/中间文件）。
    pub fn add_artifact(
        &self,
        session_id: &str,
        path: &str,
        title: &str,
        kind: &str,
        message_id: Option<&str>,
        tool_call_id: Option<&str>,
        now: &str,
    ) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                c.execute(
                    "insert into session_artifacts \
                     (session_id, path, title, message_id, tool_call_id, created_at, kind) \
                     values (?1, ?2, ?3, ?4, ?5, ?6, ?7) \
                     on conflict(session_id, path) do update set \
                     title = excluded.title, \
                     message_id = excluded.message_id, \
                     tool_call_id = excluded.tool_call_id, \
                     kind = excluded.kind",
                    rusqlite::params![session_id, path, title, message_id, tool_call_id, now, kind],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// 列出某会话的产物（按 created_at 升序，再按 path）。
    pub fn list_artifacts(&self, session_id: &str) -> Result<Vec<Artifact>, String> {
        self.db
            .with_connection(|c| {
                let mut stmt = c.prepare(
                    "select path, title, message_id, tool_call_id, created_at, kind \
                     from session_artifacts where session_id = ?1 \
                     order by created_at asc, path asc",
                )?;
                let rows = stmt.query_map([session_id], |r| {
                    Ok(Artifact {
                        path: r.get(0)?,
                        title: r.get(1)?,
                        message_id: r.get(2)?,
                        tool_call_id: r.get(3)?,
                        created_at: r.get(4)?,
                        kind: r.get(5)?,
                    })
                })?;
                let mut out = Vec::new();
                for row in rows {
                    out.push(row?);
                }
                Ok(out)
            })
            .map_err(|e| e.to_string())
    }

    /// 该会话是否已有任意消息（用于「首次发送后锁定工作目录」判定）。
    pub fn session_has_messages(&self, session_id: &str) -> Result<bool, String> {
        self.db
            .with_connection(|c| {
                let n: i64 = c.query_row(
                    "select count(*) from messages where session_id = ?1",
                    [session_id],
                    |r| r.get(0),
                )?;
                Ok(n > 0)
            })
            .map_err(|e| e.to_string())
    }
}

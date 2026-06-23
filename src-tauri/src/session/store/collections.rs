//! SessionStore：最近工作目录、产物、分组、待办，以及会话详情聚合。
use super::{new_id, session_from_row, session_group_from_row, SessionStore};
use crate::session::types::{Session, SessionGroup, TodoItem};

impl SessionStore {
    /// 新建分组（id = new_id("group")），返回创建的 `SessionGroup`。
    /// 用户新建组：built_in=0，sort_order=1000（排在内建分组之后）。
    pub fn create_session_group(
        &self,
        label: &str,
        color_key: &str,
        now: &str,
    ) -> Result<SessionGroup, String> {
        let id = new_id("group");
        self.db
            .with_connection(|c| {
                c.execute(
                    "insert into session_groups (id, label, color_key, built_in, sort_order, created_at) values (?1, ?2, ?3, 0, 1000, ?4)",
                    rusqlite::params![id, label, color_key, now],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())?;
        Ok(SessionGroup {
            id,
            label: label.into(),
            color_key: color_key.into(),
            created_at: now.into(),
            built_in: false,
            sort_order: 1000,
        })
    }

    /// 编辑分组名称与颜色（内建分组不可编辑）。返回更新后的分组。
    pub fn update_session_group(
        &self,
        id: &str,
        label: &str,
        color_key: &str,
    ) -> Result<SessionGroup, String> {
        if let Some(group) = self.get_session_group(id)? {
            if group.built_in {
                return Err("内建分组不可编辑".into());
            }
        }
        self.db
            .with_connection(|c| {
                c.execute(
                    "update session_groups set label = ?1, color_key = ?2 where id = ?3",
                    rusqlite::params![label, color_key, id],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())?;
        self.get_session_group(id)?
            .ok_or_else(|| "分组不存在".to_string())
    }

    /// 列出全部分组（按 sort_order, created_at 升序）。
    pub fn list_session_groups(&self) -> Result<Vec<SessionGroup>, String> {
        self.db
            .with_connection(|c| {
                let mut stmt = c.prepare(
                    "select id, label, color_key, created_at, built_in, sort_order from session_groups order by sort_order, created_at",
                )?;
                let rows = stmt.query_map([], session_group_from_row)?;
                let mut out = Vec::new();
                for row in rows {
                    out.push(row?);
                }
                Ok(out)
            })
            .map_err(|e| e.to_string())
    }

    /// 取单个分组。
    pub fn get_session_group(&self, id: &str) -> Result<Option<SessionGroup>, String> {
        self.db
            .with_connection(|c| {
                let mut stmt = c.prepare(
                    "select id, label, color_key, created_at, built_in, sort_order from session_groups where id = ?1",
                )?;
                let mut rows = stmt.query_map([id], session_group_from_row)?;
                Ok(match rows.next() {
                    Some(row) => Some(row?),
                    None => None,
                })
            })
            .map_err(|e| e.to_string())
    }

    /// 删除分组：内建分组不可删；事务内先把专家会话 group_id 置空（归「最近」），再删分组本身。
    pub fn delete_session_group(&self, id: &str) -> Result<(), String> {
        // 内建分组防护。
        if let Some(group) = self.get_session_group(id)? {
            if group.built_in {
                return Err("内建分组不可删除".into());
            }
        }
        self.db
            .with_transaction(|tx| {
                tx.execute(
                    "update sessions set group_id = null where group_id = ?1",
                    rusqlite::params![id],
                )?;
                tx.execute(
                    "delete from session_groups where id = ?1",
                    rusqlite::params![id],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// 整组覆写某会话的待办清单（serde 序列化为 JSON，写入 sessions.todos_json）。
    pub fn set_session_todos(
        &self,
        session_id: &str,
        todos: &[TodoItem],
        now: &str,
    ) -> Result<(), String> {
        let json = serde_json::to_string(todos).map_err(|e| e.to_string())?;
        self.db
            .with_connection(|c| {
                c.execute(
                    "update sessions set todos_json = ?1, updated_at = ?2 where id = ?3",
                    rusqlite::params![json, now, session_id],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// 读取某会话的待办清单。todos_json 为 NULL / 解析失败 → 空 Vec。
    pub fn get_session_todos(&self, session_id: &str) -> Result<Vec<TodoItem>, String> {
        let json: Option<String> = self
            .db
            .with_connection(|c| {
                let mut stmt = c.prepare("select todos_json from sessions where id = ?1")?;
                let mut rows = stmt.query_map([session_id], |r| r.get::<_, Option<String>>(0))?;
                Ok(match rows.next() {
                    Some(row) => row?,
                    None => None,
                })
            })
            .map_err(|e| e.to_string())?;
        Ok(json
            .and_then(|s| serde_json::from_str::<Vec<TodoItem>>(&s).ok())
            .unwrap_or_default())
    }

    pub fn get_session_detail(&self, session_id: &str) -> Result<Option<Session>, String> {
        let session = self
            .db
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
            .map_err(|e| e.to_string())?;
        let Some(session) = session else {
            return Ok(None);
        };
        let messages = self.list_messages(session_id)?;
        let todos = self.get_session_todos(session_id)?;
        let artifacts = self.list_artifacts(session_id)?;
        Ok(Some(Session {
            session,
            messages,
            pending_permission: None,
            pending_ask: None,
            pending_plan: None,
            todos,
            resolved_working_dir: String::new(),
            artifacts,
            is_running: false,
        }))
    }
}

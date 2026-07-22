use std::sync::Arc;

use crate::session::types::{Message, SessionGroup, SessionInfo};
use crate::storage::AppDatabase;

// 按表/关注点拆分的 `impl SessionStore`（同模块子文件，共享私有 db 字段与行映射器）。
mod collections;
mod messages;
mod sessions;
#[cfg(test)]
mod workspace_tests;

pub struct SessionStore {
    db: Arc<AppDatabase>,
}

impl SessionStore {
    pub fn open(db: Arc<AppDatabase>) -> Result<Self, String> {
        let store = Self { db };
        store.ensure_schema()?;
        Ok(store)
    }

    fn ensure_schema(&self) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                c.execute_batch(
                    "
                    create table if not exists sessions (
                        id text primary key,
                        title text not null,
                        created_at text not null,
                        updated_at text not null,
                        todos_json text,
                        pinned integer not null default 0,
                        group_id text,
                        mode text not null default 'normal'
                    );
                    create table if not exists session_groups (
                        id text primary key,
                        label text not null,
                        color_key text not null,
                        created_at text not null,
                        built_in integer not null default 0,
                        sort_order integer not null default 0
                    );
                    create table if not exists messages (
                        id text primary key,
                        session_id text not null,
                        role text not null,
                        content text not null,
                        reasoning text,
                        compacted integer not null default 0,
                        created_at text not null
                    );
                    create index if not exists idx_messages_session_created
                        on messages(session_id, created_at, id);
                    create table if not exists permission_grants (
                        session_id text not null,
                        tool_name text not null,
                        created_at text not null,
                        primary key (session_id, tool_name)
                    );
                    create table if not exists recent_workspaces (
                        path text primary key,
                        used_at text not null
                    );
                    create table if not exists session_artifacts (
                        session_id text not null,
                        path text not null,
                        title text not null,
                        message_id text,
                        tool_call_id text,
                        created_at text not null,
                        kind text not null default 'final',
                        primary key (session_id, path)
                    );
                    create table if not exists session_activated_tools (
                        session_id text not null,
                        tool_name text not null,
                        primary key (session_id, tool_name)
                    );
                    ",
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())?;
        // 既有库幂等补列（reasoning + 工具列）。
        for column in [
            "reasoning",
            "tool_calls_json",
            "tool_call_id",
            "tool_name",
            "tool_status",
        ] {
            self.ensure_message_column(column)?;
        }
        // 既有库幂等补 messages.compacted（compact 上下文压缩：标记被摘要吸收的旧消息）。
        self.ensure_message_column_decl("compacted", "integer not null default 0")?;
        // 既有库幂等补 sessions.todos_json / pinned / group_id / compaction_summary。
        self.ensure_session_column("todos_json", "text")?;
        self.ensure_session_column("pinned", "integer not null default 0")?;
        self.ensure_session_column("group_id", "text")?;
        // compact 上下文压缩：sessions 存最新一段对话摘要。
        self.ensure_session_column("compaction_summary", "text")?;
        // 计划模式：sessions 存会话工作模式（normal | plan）。
        self.ensure_session_column("mode", "text not null default 'normal'")?;
        // 工作目录：sessions 存用户显式选择的工作目录（沙箱根），NULL 表示用默认。
        self.ensure_session_column("working_dir", "text")?;
        // 权限模式：sessions 存会话级覆盖（manual|auto|full），NULL 表示继承全局默认。
        self.ensure_session_column("permission_mode", "text")?;
        // 多模型：sessions 存会话选中的模型 id，NULL 表示用全局默认。
        self.ensure_session_column("selected_model_id", "text")?;
        // 会话来源：user（默认）| scheduled | （预留 im 等）。侧边栏据此白名单过滤。
        self.ensure_session_column("origin", "text not null default 'user'")?;
        // 草稿：is_draft 标记 + draft_content 暂存 Composer 序列化内容。
        self.ensure_session_column("is_draft", "integer not null default 0")?;
        self.ensure_session_column("draft_content", "text not null default ''")?;
        // 一轮结束后的快捷建议（JSON 字符串数组），持久化供 reload/切会话回显；发新消息即清空。
        self.ensure_session_column("last_suggestions", "text not null default ''")?;
        // 子运行（agent 委派）：父会话链 + 子运行归属的 agent 名/任务。均可空，顶层会话为 NULL。
        self.ensure_session_column("parent_session_id", "text")?;
        self.ensure_session_column("parent_tool_call_id", "text")?;
        self.ensure_session_column("expert_name", "text")?;
        self.ensure_session_column("agent_task", "text")?;
        // 父 run 停泊态：等待哪个 child_session_id 完成。
        self.ensure_session_column("awaiting_subagent", "text")?;
        // T57 非阻塞派发：后台子代理标记 + 终态(done/failed/cancelled) + 是否已被 collect 收集。
        self.ensure_session_column("is_background", "integer not null default 0")?;
        self.ensure_session_column("run_outcome", "text")?;
        self.ensure_session_column("collected", "integer not null default 0")?;
        // 父 collect 停泊态：JSON {collectCallId, remaining:[childId...]}；None=未在 collect 等待。
        self.ensure_session_column("pending_collect", "text")?;
        // ad-hoc（动态生成）专家的 system prompt + 工具白名单（声明式专家为 NULL，运行时查 spec）。
        self.ensure_session_column("expert_system_prompt", "text")?;
        self.ensure_session_column("expert_tools", "text")?;
        // 会话归属实体 + 运行角色。项目用 project_id；持久智能体用 agent_id；
        // 专家/团队这类定义才进入 role_kind/role_id。
        self.ensure_session_column("project_id", "text")?;
        self.ensure_session_column("agent_id", "text")?;
        self.ensure_session_column("role_kind", "text")?;
        self.ensure_session_column("role_id", "text")?;
        // T70：会话任务队列（FIFO 邮箱）。text 存 JSON 数组，可空，向后兼容。
        self.ensure_session_column("pending_tasks", "text")?;
        // 产物分类：final（最终交付文件）| working（脚本/中间文件）。老数据默认 final。
        self.ensure_artifact_column("kind", "text not null default 'final'")?;
        // 旧数据迁移：曾归入内置「定时任务」分组的会话改标 origin='scheduled' 并脱离分组；
        // 该内置分组不再使用，一并删除（idempotent：迁移后无匹配行）。
        self.db
            .with_connection(|c| {
                c.execute(
                    "update sessions set origin = 'scheduled', group_id = null where group_id = 'group-scheduled'",
                    [],
                )?;
                c.execute("delete from session_groups where id = 'group-scheduled'", [])?;
                Ok(())
            })
            .map_err(|e| e.to_string())?;
        // 既有库幂等补 session_groups.built_in / sort_order。
        self.ensure_session_group_column("built_in", "integer not null default 0")?;
        self.ensure_session_group_column("sort_order", "integer not null default 0")?;
        // 种入 6 个内建彩色分组（insert or ignore 幂等）。
        self.seed_session_groups()?;
        Ok(())
    }

    /// 幂等地为 sessions 表补一个列（pragma 检测后 alter，列定义由 `decl` 给出）。
    fn ensure_session_column(&self, column: &str, decl: &str) -> Result<(), String> {
        let exists: bool = self
            .db
            .with_connection(|c| {
                let n: i64 = c.query_row(
                    "select count(*) from pragma_table_info('sessions') where name = ?1",
                    [column],
                    |r| r.get(0),
                )?;
                Ok(n > 0)
            })
            .map_err(|e| e.to_string())?;
        if !exists {
            self.db
                .with_connection(|c| {
                    c.execute(
                        &format!("alter table sessions add column {column} {decl}"),
                        [],
                    )?;
                    Ok(())
                })
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// 幂等地为 session_groups 表补一个列（pragma 检测后 alter，列定义由 `decl` 给出）。
    fn ensure_session_group_column(&self, column: &str, decl: &str) -> Result<(), String> {
        let exists: bool = self
            .db
            .with_connection(|c| {
                let n: i64 = c.query_row(
                    "select count(*) from pragma_table_info('session_groups') where name = ?1",
                    [column],
                    |r| r.get(0),
                )?;
                Ok(n > 0)
            })
            .map_err(|e| e.to_string())?;
        if !exists {
            self.db
                .with_connection(|c| {
                    c.execute(
                        &format!("alter table session_groups add column {column} {decl}"),
                        [],
                    )?;
                    Ok(())
                })
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// 幂等地为 session_artifacts 表补一个列（pragma 检测后 alter，列定义由 `decl` 给出）。
    fn ensure_artifact_column(&self, column: &str, decl: &str) -> Result<(), String> {
        let exists: bool = self
            .db
            .with_connection(|c| {
                let n: i64 = c.query_row(
                    "select count(*) from pragma_table_info('session_artifacts') where name = ?1",
                    [column],
                    |r| r.get(0),
                )?;
                Ok(n > 0)
            })
            .map_err(|e| e.to_string())?;
        if !exists {
            self.db
                .with_connection(|c| {
                    c.execute(
                        &format!("alter table session_artifacts add column {column} {decl}"),
                        [],
                    )?;
                    Ok(())
                })
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// 种入 6 个内建彩色分组 + 内置「定时任务」分组（insert or ignore，幂等）。
    fn seed_session_groups(&self) -> Result<(), String> {
        let builtin = [
            ("red", "红色", 10i64),
            ("orange", "橙色", 20),
            ("yellow", "黄色", 30),
            ("green", "绿色", 40),
            ("blue", "蓝色", 50),
            ("purple", "紫色", 60),
        ];
        self.db
            .with_connection(|c| {
                for (key, label, order) in builtin {
                    c.execute(
                        "insert or ignore into session_groups (id, label, color_key, built_in, sort_order, created_at) values (?1, ?2, ?1, 1, ?3, '0')",
                        rusqlite::params![key, label, order],
                    )?;
                }
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// 幂等地为 messages 表补一个 text 列（pragma 检测后 alter）。
    fn ensure_message_column(&self, column: &str) -> Result<(), String> {
        let exists: bool = self
            .db
            .with_connection(|c| {
                let n: i64 = c.query_row(
                    "select count(*) from pragma_table_info('messages') where name = ?1",
                    [column],
                    |r| r.get(0),
                )?;
                Ok(n > 0)
            })
            .map_err(|e| e.to_string())?;
        if !exists {
            self.db
                .with_connection(|c| {
                    c.execute(
                        &format!("alter table messages add column {column} text"),
                        [],
                    )?;
                    Ok(())
                })
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// 幂等地为 messages 表补一个列（pragma 检测后 alter，列定义由 `decl` 给出）。
    /// 与 [`Self::ensure_message_column`] 区别：可指定非 text 列定义（如 integer 默认值）。
    fn ensure_message_column_decl(&self, column: &str, decl: &str) -> Result<(), String> {
        let exists: bool = self
            .db
            .with_connection(|c| {
                let n: i64 = c.query_row(
                    "select count(*) from pragma_table_info('messages') where name = ?1",
                    [column],
                    |r| r.get(0),
                )?;
                Ok(n > 0)
            })
            .map_err(|e| e.to_string())?;
        if !exists {
            self.db
                .with_connection(|c| {
                    c.execute(
                        &format!("alter table messages add column {column} {decl}"),
                        [],
                    )?;
                    Ok(())
                })
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

}

pub(super) fn session_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SessionInfo> {
    let pinned: i64 = row.get(4)?;
    let group_id: Option<String> = row.get(5)?;
    let is_draft: i64 = row.get(11)?;
    let last_suggestions_raw: String = row.get(13)?;
    // T57/T70：尾部追加列，旧 SELECT 不含时回退默认（兼容并发旧查询）。
    let is_background: i64 = row.get(24).unwrap_or(0);
    let run_outcome: Option<String> = row.get(25).unwrap_or(None);
    let pending_collect: Option<String> = row.get(26).unwrap_or(None);
    let project_id: Option<String> = row.get(27).unwrap_or(None);
    // T70：尾部追加列，旧 SELECT 不含时回退 None（兼容并发旧查询）。
    let pending_tasks: Option<String> = row.get(28).unwrap_or(None);
    Ok(SessionInfo {
        id: row.get(0)?,
        title: row.get(1)?,
        created_at: row.get(2)?,
        updated_at: row.get(3)?,
        pinned: pinned != 0,
        group_id,
        mode: row.get(6)?,
        working_dir: row.get(7)?,
        permission_mode: row.get(8)?,
        selected_model_id: row.get(9)?,
        origin: row.get(10)?,
        is_draft: is_draft != 0,
        draft_content: row.get(12)?,
        last_suggestions: serde_json::from_str(&last_suggestions_raw).unwrap_or_default(),
        is_running: false,
        run_started_at: None,
        parent_session_id: row.get(14)?,
        parent_tool_call_id: row.get(15)?,
        expert_name: row.get(16)?,
        agent_task: row.get(17)?,
        awaiting_subagent: row.get(18)?,
        expert_system_prompt: row.get(19)?,
        expert_tools: row.get(20)?,
        agent_id: row.get(21)?,
        role_kind: row.get(22)?,
        role_id: row.get(23)?,
        is_background: is_background != 0,
        run_outcome,
        pending_collect,
        project_id,
        pending_tasks,
    })
}

pub(super) fn session_group_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SessionGroup> {
    let built_in: i64 = row.get(4)?;
    let sort_order: i64 = row.get(5)?;
    Ok(SessionGroup {
        id: row.get(0)?,
        label: row.get(1)?,
        color_key: row.get(2)?,
        created_at: row.get(3)?,
        built_in: built_in != 0,
        sort_order,
    })
}

pub(super) fn message_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Message> {
    let compacted: i64 = row.get(9)?;
    Ok(Message {
        id: row.get(0)?,
        session_id: row.get(1)?,
        role: row.get(2)?,
        content: row.get(3)?,
        reasoning: row.get(4)?,
        tool_calls_json: row.get(5)?,
        tool_call_id: row.get(6)?,
        tool_name: row.get(7)?,
        tool_status: row.get(8)?,
        compacted: compacted != 0,
        created_at: row.get(10)?,
    })
}

pub fn new_id(prefix: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    format!("{prefix}-{nanos}")
}

//! MemoryStore 的 CRUD（迁自 session/store/collections.rs）+ 写时按内容去重。

use crate::memory::types::{Memory, MemoryScope};
use crate::memory::MemoryStore;
use crate::session::new_id;

/// rusqlite 行 → Memory（仅取对外 3 列；分层列后续切片按需扩展）。
fn memory_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Memory> {
    Ok(Memory {
        id: r.get(0)?,
        content: r.get(1)?,
        created_at: r.get(2)?,
    })
}

/// content 规范化（trim + 折叠内部空白）后做 FNV-1a 64 位哈希，供写时去重。
/// 用稳定的内联哈希避免新增 crate 依赖（遵守 AGENTS.md 低依赖原则）。
fn dedup_hash(content: &str) -> String {
    let norm = content.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut h: u64 = 0xcbf29ce484222325;
    for b in norm.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

impl MemoryStore {
    /// 写入一条 fact 记忆，作用域由 `scope` 决定（全局 / 项目 / 伴随体私有）。
    /// 去重在**作用域内**进行：同内容 + 同作用域才算重复（不同作用域可并存同句）。
    pub fn add_memory(
        &self,
        content: &str,
        now: &str,
        scope: MemoryScope<'_>,
    ) -> Result<Memory, String> {
        let hash = dedup_hash(content);
        if let Some(existing) = self.find_by_hash(&hash, scope)? {
            return Ok(existing);
        }
        let id = new_id("mem");
        let pid = scope.project_id().to_string();
        let aid = scope.agent_id().to_string();
        self.db
            .with_connection(|c| {
                c.execute(
                    "insert into memories (id, content, created_at, kind, tier, pinned, source, dedup_hash, updated_at, agent_id, project_id)
                     values (?1, ?2, ?3, 'fact', 2, 0, 'remember', ?4, ?3, ?5, ?6)",
                    rusqlite::params![id, content, now, hash, aid, pid],
                )?;
                c.execute(
                    "insert into memories_fts (id, content) values (?1, ?2)",
                    rusqlite::params![id, content],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())?;
        Ok(Memory {
            id,
            content: content.into(),
            created_at: now.into(),
        })
    }

    /// 按 dedup_hash + 作用域查既有条目（作用域内去重用）。
    fn find_by_hash(&self, hash: &str, scope: MemoryScope<'_>) -> Result<Option<Memory>, String> {
        let pid = scope.project_id().to_string();
        let aid = scope.agent_id().to_string();
        self.db
            .with_connection(|c| {
                let mut stmt = c.prepare(
                    "select id, content, created_at from memories
                     where dedup_hash = ?1 and project_id = ?2 and agent_id = ?3 limit 1",
                )?;
                let mut rows =
                    stmt.query_map(rusqlite::params![hash, pid, aid], memory_from_row)?;
                Ok(match rows.next() {
                    Some(r) => Some(r?),
                    None => None,
                })
            })
            .map_err(|e| e.to_string())
    }

    /// 列出用户可管理的**全局** fact（按 created_at 升序）。设置页只管理全局层；
    /// 项目层/伴随体私有由各自界面管理。画像/情景不在此列出。
    pub fn list_memories(&self) -> Result<Vec<Memory>, String> {
        self.db
            .with_connection(|c| {
                let mut stmt = c.prepare(
                    "select id, content, created_at from memories
                     where kind = 'fact' and project_id = '' and agent_id = ''
                     order by created_at, id",
                )?;
                let rows = stmt.query_map([], memory_from_row)?;
                let mut out = Vec::new();
                for row in rows {
                    out.push(row?);
                }
                Ok(out)
            })
            .map_err(|e| e.to_string())
    }

    /// 列出**精确作用域**内的 fact（管理界面用：项目页只列本项目、智能体页只列本伴随体；
    /// 不并入全局——与召回 recall 的「全局∪本层」不同）。按 created_at 升序。
    pub fn list_scoped_facts(&self, scope: MemoryScope<'_>) -> Result<Vec<Memory>, String> {
        let pid = scope.project_id().to_string();
        let aid = scope.agent_id().to_string();
        self.db
            .with_connection(|c| {
                let mut stmt = c.prepare(
                    "select id, content, created_at from memories
                     where kind = 'fact' and project_id = ?1 and agent_id = ?2
                     order by created_at, id",
                )?;
                let rows = stmt.query_map(rusqlite::params![pid, aid], memory_from_row)?;
                let mut out = Vec::new();
                for row in rows {
                    out.push(row?);
                }
                Ok(out)
            })
            .map_err(|e| e.to_string())
    }

    /// 统计**精确作用域**内的 fact 条数（首页记忆卡片用）。
    /// T73：统计某伴随体私有记忆中、创建时刻晚于 `since_secs`（epoch 秒）的条数。
    /// 供演化扫描线程算「自上次反思以来攒了多少新经历」。`created_at` 存为 epoch 秒字符串
    /// （见 engine::now_string），故用 `cast(... as integer)` 与 i64 比较。
    pub fn count_since(&self, agent_id: &str, since_secs: i64) -> Result<i64, String> {
        self.db
            .with_connection(|c| {
                let n: i64 = c.query_row(
                    "select count(*) from memories
                     where agent_id = ?1 and cast(created_at as integer) > ?2",
                    rusqlite::params![agent_id, since_secs],
                    |r| r.get(0),
                )?;
                Ok(n)
            })
            .map_err(|e| e.to_string())
    }

    pub fn count_scoped_facts(&self, scope: MemoryScope<'_>) -> Result<i64, String> {
        let pid = scope.project_id().to_string();
        let aid = scope.agent_id().to_string();
        self.db
            .with_connection(|c| {
                let n: i64 = c.query_row(
                    "select count(*) from memories
                     where kind = 'fact' and project_id = ?1 and agent_id = ?2",
                    rusqlite::params![pid, aid],
                    |r| r.get(0),
                )?;
                Ok(n)
            })
            .map_err(|e| e.to_string())
    }

    /// 更新内容（保留 created_at；刷新 dedup_hash 与 updated_at）。
    pub fn update_memory(&self, id: &str, content: &str) -> Result<(), String> {
        let hash = dedup_hash(content);
        self.db
            .with_connection(|c| {
                c.execute(
                    "update memories set content = ?2, dedup_hash = ?3 where id = ?1",
                    rusqlite::params![id, content, hash],
                )?;
                c.execute(
                    "update memories_fts set content = ?2 where id = ?1",
                    rusqlite::params![id, content],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// 置顶/取消置顶（pinned=1 进 Tier1，始终注入）。
    pub fn set_pinned(&self, id: &str, pinned: bool) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                c.execute(
                    "update memories set pinned = ?2 where id = ?1",
                    rusqlite::params![id, i64::from(pinned)],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// 删除某项目（project_id）的全部项目层记忆（删项目时级联）。`project_id` 空串不删全局。
    pub fn delete_by_project(&self, project_id: &str) -> Result<(), String> {
        if project_id.is_empty() {
            return Ok(());
        }
        self.db
            .with_connection(|c| {
                c.execute(
                    "delete from memories_fts where id in (select id from memories where project_id = ?1)",
                    rusqlite::params![project_id],
                )?;
                c.execute(
                    "delete from memories where project_id = ?1",
                    rusqlite::params![project_id],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// 删除某伴随体（agent_id）的全部私有记忆（T69：删伴随体时级联）。`agent_id` 空串不删全局。
    pub fn delete_by_agent(&self, agent_id: &str) -> Result<(), String> {
        if agent_id.is_empty() {
            return Ok(());
        }
        self.db
            .with_connection(|c| {
                c.execute(
                    "delete from memories_fts where id in (select id from memories where agent_id = ?1)",
                    rusqlite::params![agent_id],
                )?;
                c.execute(
                    "delete from memories where agent_id = ?1",
                    rusqlite::params![agent_id],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// 删除一条长期记忆。
    pub fn delete_memory(&self, id: &str) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                c.execute("delete from memories where id = ?1", rusqlite::params![id])?;
                c.execute(
                    "delete from memories_fts where id = ?1",
                    rusqlite::params![id],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// 清空全部长期记忆。
    pub fn clear_memories(&self) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                c.execute("delete from memories", [])?;
                c.execute("delete from memories_fts", [])?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// 删除全部「未置顶的**全局** fact」（主动整理重建前用；保留置顶、画像/情景，及项目/私有层）。
    pub fn clear_unpinned_facts(&self) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                c.execute(
                    "delete from memories_fts where id in
                     (select id from memories where kind = 'fact' and pinned = 0
                      and project_id = '' and agent_id = '')",
                    [],
                )?;
                c.execute(
                    "delete from memories where kind = 'fact' and pinned = 0
                     and project_id = '' and agent_id = ''",
                    [],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }
}

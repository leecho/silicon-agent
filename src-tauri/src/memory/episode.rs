//! 情景记忆（episode）：会话历史的压缩摘要，kind='episode' + session_id 溯源。
//!
//! 对应 Hermes Tier2 情景层：会话被自动压缩时，由引擎把压缩摘要投递为一条 episode，
//! 进 FTS5 索引；后续按相关性召回「上次我们怎么处理 X」。见 spec §3.4 / §3.5。

use crate::memory::types::{Memory, MemoryScope};
use crate::memory::MemoryStore;
use crate::session::new_id;

fn memory_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Memory> {
    Ok(Memory {
        id: r.get(0)?,
        content: r.get(1)?,
        created_at: r.get(2)?,
    })
}

/// content 规范化哈希（与 store 一致），供 episode 写时去重。
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
    /// 写入一条情景记忆（kind='episode'，记 session_id 溯源），作用域由 `scope` 决定。
    /// 空摘要忽略；同作用域内同内容去重。
    pub fn add_episode(
        &self,
        session_id: &str,
        summary: &str,
        now: &str,
        scope: MemoryScope<'_>,
    ) -> Result<(), String> {
        let summary = summary.trim();
        if summary.is_empty() {
            return Ok(());
        }
        let hash = dedup_hash(summary);
        let id = new_id("ep");
        let pid = scope.project_id().to_string();
        let aid = scope.agent_id().to_string();
        self.db
            .with_connection(|c| {
                let dup: i64 = c.query_row(
                    "select count(*) from memories
                     where dedup_hash = ?1 and project_id = ?2 and agent_id = ?3",
                    rusqlite::params![hash, pid, aid],
                    |r| r.get(0),
                )?;
                if dup > 0 {
                    return Ok(());
                }
                c.execute(
                    "insert into memories (id, content, created_at, kind, tier, pinned, source, dedup_hash, session_id, updated_at, agent_id, project_id)
                     values (?1, ?2, ?3, 'episode', 2, 0, ?4, ?5, ?4, ?3, ?6, ?7)",
                    rusqlite::params![id, summary, now, format!("session:{session_id}"), hash, aid, pid],
                )?;
                c.execute(
                    "insert into memories_fts (id, content) values (?1, ?2)",
                    rusqlite::params![id, summary],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// 按 query 召回相关情景记忆（FTS5 trigram，bm25 top-K），作用域同 fact（全局 ∪ 当前层）。
    /// query 为空则返回最近若干条。
    pub fn recall_episodes(
        &self,
        query: &str,
        limit: usize,
        scope: MemoryScope<'_>,
    ) -> Result<Vec<Memory>, String> {
        let (frag, scope_val) = scope.predicate();
        let lim = limit as i64;
        match crate::memory::build_fts_match(query) {
            Some(expr) => {
                let sql = format!(
                    "select memories.id, memories.content, memories.created_at
                     from memories_fts join memories on memories.id = memories_fts.id
                     where memories_fts match :expr and memories.kind = 'episode' and {frag}
                     order by bm25(memories_fts) limit :limit"
                );
                self.db
                    .with_connection(|c| {
                        let mut stmt = c.prepare(&sql)?;
                        let mut out = Vec::new();
                        match &scope_val {
                            Some(v) => {
                                for r in stmt.query_map(
                                    rusqlite::named_params! {":expr": expr, ":limit": lim, ":scope": v},
                                    memory_from_row,
                                )? {
                                    out.push(r?);
                                }
                            }
                            None => {
                                for r in stmt.query_map(
                                    rusqlite::named_params! {":expr": expr, ":limit": lim},
                                    memory_from_row,
                                )? {
                                    out.push(r?);
                                }
                            }
                        }
                        Ok(out)
                    })
                    .map_err(|e| e.to_string())
            }
            None => {
                let sql = format!(
                    "select id, content, created_at from memories
                     where kind = 'episode' and {frag}
                     order by created_at desc, id desc limit :limit"
                );
                self.db
                    .with_connection(|c| {
                        let mut stmt = c.prepare(&sql)?;
                        let mut out = Vec::new();
                        match &scope_val {
                            Some(v) => {
                                for r in stmt.query_map(
                                    rusqlite::named_params! {":limit": lim, ":scope": v},
                                    memory_from_row,
                                )? {
                                    out.push(r?);
                                }
                            }
                            None => {
                                for r in stmt.query_map(
                                    rusqlite::named_params! {":limit": lim},
                                    memory_from_row,
                                )? {
                                    out.push(r?);
                                }
                            }
                        }
                        Ok(out)
                    })
                    .map_err(|e| e.to_string())
            }
        }
    }
}

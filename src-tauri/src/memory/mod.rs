//! 长期记忆模块：与 SessionStore 平行的顶层 store，独立 schema、CRUD、召回。
//! 与 session 正交——记忆是全局、跨会话的长期知识。
//!
//! 数据模型见 `docs/04-specs/2026-06-13-memory-module-hermes-design.md` §3.3：
//! 单表 `memories` 正交分层（kind/tier/pinned/source/tags/dedup_hash/session_id）。
//! W0 仅落库这些列并做写时去重；FTS5 召回/画像/情景/整理留给 W1/W2 切片。

use std::sync::Arc;

use crate::storage::AppDatabase;

pub mod curation;
mod episode;
mod profile;
pub mod prompt;
mod recall;
mod store;
pub mod types;

pub use recall::build_fts_match;
pub use types::{Memory, MemoryKind, MemoryScope};

pub struct MemoryStore {
    db: Arc<AppDatabase>,
}

impl MemoryStore {
    pub fn open(db: Arc<AppDatabase>) -> Result<Self, String> {
        let store = Self { db };
        store.ensure_schema()?;
        Ok(store)
    }

    fn ensure_schema(&self) -> Result<(), String> {
        // 新库：直接建含全部分层列的表。
        self.db
            .with_connection(|c| {
                c.execute_batch(
                    "
                    create table if not exists memories (
                        id          text primary key,
                        content     text not null,
                        created_at  text not null,
                        kind        text not null default 'fact',
                        tier        integer not null default 2,
                        pinned      integer not null default 0,
                        source      text,
                        tags        text,
                        dedup_hash  text,
                        session_id  text,
                        updated_at  text,
                        agent_id    text not null default '',
                        project_id  text not null default ''
                    );
                    ",
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())?;
        // 既有库（W0 之前仅 id/content/created_at 三列）幂等补列。
        self.ensure_memory_column("kind", "text not null default 'fact'")?;
        self.ensure_memory_column("tier", "integer not null default 2")?;
        self.ensure_memory_column("pinned", "integer not null default 0")?;
        self.ensure_memory_column("source", "text")?;
        self.ensure_memory_column("tags", "text")?;
        self.ensure_memory_column("dedup_hash", "text")?;
        self.ensure_memory_column("session_id", "text")?;
        self.ensure_memory_column("updated_at", "text")?;
        // T69：per-伴随体私有记忆 owner 维度。''=全局（存量与无伴随体会话），=X=伴随体 X 私有。加列保数据。
        self.ensure_memory_column("agent_id", "text not null default ''")?;
        // 项目层 owner 维度。''=非项目；=pid=该项目私有（同项目所有线程/Expert 共享）。与 agent_id 至多一非空。
        self.ensure_memory_column("project_id", "text not null default ''")?;
        // FTS5 全文索引（trigram 分词，适配中文子串召回）。独立表，写入由 store 方法手动同步。
        self.db
            .with_connection(|c| {
                c.execute_batch(
                    "create virtual table if not exists memories_fts
                     using fts5(id unindexed, content, tokenize='trigram');",
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())?;
        self.backfill_fts_if_empty()?;
        Ok(())
    }

    /// 既有库（W0 落库但 FTS 尚空）一次性回填：仅当 memories 非空且 fts 为空时执行。
    fn backfill_fts_if_empty(&self) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                let mem_n: i64 = c.query_row("select count(*) from memories", [], |r| r.get(0))?;
                let fts_n: i64 =
                    c.query_row("select count(*) from memories_fts", [], |r| r.get(0))?;
                if mem_n > 0 && fts_n == 0 {
                    c.execute_batch(
                        "insert into memories_fts(id, content) select id, content from memories;",
                    )?;
                }
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// 幂等补列（镜像 SessionStore::ensure_session_column 的 pragma_table_info 探测模式）。
    fn ensure_memory_column(&self, column: &str, decl: &str) -> Result<(), String> {
        let exists: bool = self
            .db
            .with_connection(|c| {
                let n: i64 = c.query_row(
                    "select count(*) from pragma_table_info('memories') where name = ?1",
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
                        &format!("alter table memories add column {column} {decl}"),
                        [],
                    )?;
                    Ok(())
                })
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

//! 知识库模块：用户主动管理的文档/资料库（RAG）。与 `memory` 正交——
//! memory 是自动整理的短事实，knowledge 是用户导入的大块资料 + 分块 + 主动检索。
//! 设计见 docs/04-specs/T87-knowledge-base-module-design.md。

use std::sync::Arc;

use crate::storage::AppDatabase;

pub mod chunk;
pub mod embed;
pub mod embed_gateway;
pub mod ingest;
pub mod parser;
pub mod retrieve;
pub mod store;
pub mod types;

pub use types::{Document, KnowledgeBase, RetrievedChunk};

pub struct KnowledgeStore {
    pub(crate) db: Arc<AppDatabase>,
}

impl KnowledgeStore {
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
                    create table if not exists knowledge_bases (
                        id          text primary key,
                        name        text not null,
                        description text,
                        icon        text,
                        created_at  text not null,
                        updated_at  text
                    );
                    create table if not exists knowledge_documents (
                        id          text primary key,
                        kb_id       text not null,
                        title       text not null,
                        source_type text not null,
                        source_ref  text,
                        status      text not null,
                        error       text,
                        char_size   integer not null default 0,
                        hash        text,
                        created_at  text not null
                    );
                    create table if not exists knowledge_chunks (
                        id           text primary key,
                        doc_id       text not null,
                        kb_id        text not null,
                        ordinal      integer not null,
                        content      text not null,
                        heading_path text,
                        char_count   integer not null default 0
                    );
                    create virtual table if not exists knowledge_chunks_fts using fts5(id unindexed, content, tokenize='trigram');
                    create table if not exists knowledge_bindings (
                        kb_id      text not null,
                        scope_type text not null,
                        scope_id   text not null,
                        created_at text not null,
                        primary key (kb_id, scope_type, scope_id)
                    );
                    create index if not exists idx_kdoc_kb on knowledge_documents(kb_id);
                    create index if not exists idx_kchunk_doc on knowledge_chunks(doc_id);
                    create index if not exists idx_kchunk_kb on knowledge_chunks(kb_id);
                    ",
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())?;
        // P3：向量列（幂等补列；列已存在时 alter 报错，用 let _ 吞掉）。
        let _ = self.db.with_connection(|c| {
            let _ = c.execute("alter table knowledge_chunks add column embedding blob", []);
            Ok(())
        });
        // 文档原文列（幂等补列）：存解析后的完整正文，供「查看资料内容」与回写 char_size。
        let _ = self.db.with_connection(|c| {
            let _ = c.execute("alter table knowledge_documents add column text", []);
            Ok(())
        });
        Ok(())
    }
}

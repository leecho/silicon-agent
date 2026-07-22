//! KnowledgeStore CRUD：知识库 / 文档 / 片段 / 挂载绑定。
use crate::knowledge::embed::{bytes_to_vec, vec_to_bytes};
use crate::knowledge::types::{Document, KnowledgeBase};
use crate::knowledge::KnowledgeStore;
use crate::session::new_id;

/// 暴力余弦的候选片段（含向量与展示元数据）。
pub struct VectorCandidate {
    pub chunk_id: String,
    pub doc_id: String,
    pub doc_title: String,
    pub heading_path: String,
    pub content: String,
    pub embedding: Vec<f32>,
}

fn kb_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<KnowledgeBase> {
    Ok(KnowledgeBase {
        id: r.get(0)?,
        name: r.get(1)?,
        description: r.get(2)?,
        icon: r.get(3)?,
        created_at: r.get(4)?,
        updated_at: r.get(5)?,
        doc_count: r.get(6)?,
    })
}

impl KnowledgeStore {
    /// 新建知识库。
    pub fn create_kb(
        &self,
        name: &str,
        description: Option<&str>,
        icon: Option<&str>,
        now: &str,
    ) -> Result<KnowledgeBase, String> {
        let id = new_id("kb");
        self.db
            .with_connection(|c| {
                c.execute(
                    "insert into knowledge_bases (id, name, description, icon, created_at, updated_at)
                     values (?1, ?2, ?3, ?4, ?5, ?5)",
                    rusqlite::params![id, name, description, icon, now],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())?;
        Ok(KnowledgeBase {
            id,
            name: name.into(),
            description: description.map(Into::into),
            icon: icon.map(Into::into),
            created_at: now.into(),
            updated_at: Some(now.into()),
            doc_count: 0,
        })
    }

    /// 列出全部知识库（按创建时间降序）。
    pub fn list_kbs(&self) -> Result<Vec<KnowledgeBase>, String> {
        self.db
            .with_connection(|c| {
                let mut stmt = c.prepare(
                    "select kb.id, kb.name, kb.description, kb.icon, kb.created_at, kb.updated_at,
                            (select count(*) from knowledge_documents d where d.kb_id = kb.id) as doc_count
                     from knowledge_bases kb order by kb.created_at desc",
                )?;
                let rows = stmt.query_map([], kb_from_row)?;
                let mut out = Vec::new();
                for r in rows {
                    out.push(r?);
                }
                Ok(out)
            })
            .map_err(|e| e.to_string())
    }

    /// 更新知识库名称/描述/图标。
    pub fn update_kb(
        &self,
        id: &str,
        name: &str,
        description: Option<&str>,
        icon: Option<&str>,
        now: &str,
    ) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                c.execute(
                    "update knowledge_bases set name=?2, description=?3, icon=?4, updated_at=?5 where id=?1",
                    rusqlite::params![id, name, description, icon, now],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// 删除知识库及其全部文档/片段/索引/绑定（事务级联），并清理各文档 blob。
    pub fn delete_kb(&self, id: &str) -> Result<(), String> {
        // 事务删行前先取全部 (doc_id, source_ref)，供事务成功后清理 blob。
        let docs = self.list_documents(id)?;
        self.db
            .with_transaction(|c| {
                c.execute(
                    "delete from knowledge_chunks_fts where id in
                       (select id from knowledge_chunks where kb_id=?1)",
                    rusqlite::params![id],
                )?;
                c.execute("delete from knowledge_chunks where kb_id=?1", rusqlite::params![id])?;
                c.execute("delete from knowledge_documents where kb_id=?1", rusqlite::params![id])?;
                c.execute("delete from knowledge_bindings where kb_id=?1", rusqlite::params![id])?;
                c.execute("delete from knowledge_bases where id=?1", rusqlite::params![id])?;
                Ok(())
            })
            .map_err(|e| e.to_string())?;
        for d in &docs {
            self.delete_blob(&d.id, d.source_ref.as_deref().unwrap_or_default());
        }
        Ok(())
    }

    /// 新增一篇文档，初始 status=pending。char_size 取正文字符数。
    pub fn add_document(
        &self,
        kb_id: &str,
        title: &str,
        source_type: &str,
        source_ref: Option<&str>,
        body: &str,
        now: &str,
    ) -> Result<Document, String> {
        let id = new_id("kdoc");
        let char_size = body.chars().count() as i64;
        self.db
            .with_connection(|c| {
                c.execute(
                    "insert into knowledge_documents
                       (id, kb_id, title, source_type, source_ref, status, error, char_size, hash, created_at)
                     values (?1, ?2, ?3, ?4, ?5, 'pending', null, ?6, null, ?7)",
                    rusqlite::params![id, kb_id, title, source_type, source_ref, char_size, now],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())?;
        Ok(Document {
            id,
            kb_id: kb_id.into(),
            title: title.into(),
            source_type: source_type.into(),
            source_ref: source_ref.map(Into::into),
            status: "pending".into(),
            error: None,
            char_size,
            created_at: now.into(),
        })
    }

    /// 列出某库的文档（按创建时间降序）。
    pub fn list_documents(&self, kb_id: &str) -> Result<Vec<Document>, String> {
        self.db
            .with_connection(|c| {
                let mut stmt = c.prepare(
                    "select id, kb_id, title, source_type, source_ref, status, error, char_size, created_at
                     from knowledge_documents where kb_id=?1 order by created_at desc",
                )?;
                let rows = stmt.query_map(rusqlite::params![kb_id], |r| {
                    Ok(Document {
                        id: r.get(0)?,
                        kb_id: r.get(1)?,
                        title: r.get(2)?,
                        source_type: r.get(3)?,
                        source_ref: r.get(4)?,
                        status: r.get(5)?,
                        error: r.get(6)?,
                        char_size: r.get(7)?,
                        created_at: r.get(8)?,
                    })
                })?;
                let mut out = Vec::new();
                for r in rows {
                    out.push(r?);
                }
                Ok(out)
            })
            .map_err(|e| e.to_string())
    }

    /// 置文档状态（ready/error 等）。
    pub fn set_document_status(&self, doc_id: &str, status: &str, error: Option<&str>) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                c.execute(
                    "update knowledge_documents set status=?2, error=?3 where id=?1",
                    rusqlite::params![doc_id, status, error],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// 回写文档原文 + 真实字数（解析后调用，修正 file/url 导入的 char_size=0）。
    pub fn set_document_text(&self, doc_id: &str, text: &str) -> Result<(), String> {
        let char_size = text.chars().count() as i64;
        self.db
            .with_connection(|c| {
                c.execute(
                    "update knowledge_documents set text=?2, char_size=?3 where id=?1",
                    rusqlite::params![doc_id, text, char_size],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// 取文档原文（供「查看资料内容」）。无则返回空串。
    pub fn get_document_text(&self, doc_id: &str) -> Result<String, String> {
        self.db
            .with_connection(|c| {
                let mut stmt = c.prepare("select text from knowledge_documents where id=?1")?;
                let mut rows = stmt.query_map(rusqlite::params![doc_id], |r| r.get::<_, Option<String>>(0))?;
                Ok(match rows.next() {
                    Some(r) => r?.unwrap_or_default(),
                    None => String::new(),
                })
            })
            .map_err(|e| e.to_string())
    }

    /// 用新片段集替换某文档的全部片段（先删旧片+FTS，再插新片+FTS），事务保证片段与索引一致。
    pub fn replace_chunks(
        &self,
        kb_id: &str,
        doc_id: &str,
        pieces: &[crate::knowledge::chunk::ChunkPiece],
    ) -> Result<(), String> {
        self.db
            .with_transaction(|c| {
                c.execute(
                    "delete from knowledge_chunks_fts where id in
                       (select id from knowledge_chunks where doc_id=?1)",
                    rusqlite::params![doc_id],
                )?;
                c.execute("delete from knowledge_chunks where doc_id=?1", rusqlite::params![doc_id])?;
                for p in pieces {
                    let cid = new_id("kchunk");
                    c.execute(
                        "insert into knowledge_chunks (id, doc_id, kb_id, ordinal, content, heading_path, char_count)
                         values (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                        rusqlite::params![
                            cid, doc_id, kb_id, p.ordinal as i64, p.content, p.heading_path,
                            p.content.chars().count() as i64
                        ],
                    )?;
                    c.execute(
                        "insert into knowledge_chunks_fts (id, content) values (?1, ?2)",
                        rusqlite::params![cid, p.content],
                    )?;
                }
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// 删除单篇文档及其片段/索引，并清理 blob。
    pub fn delete_document(&self, doc_id: &str) -> Result<(), String> {
        let source_ref = self
            .get_document(doc_id)?
            .and_then(|d| d.source_ref)
            .unwrap_or_default();
        self.db
            .with_transaction(|c| {
                c.execute(
                    "delete from knowledge_chunks_fts where id in
                       (select id from knowledge_chunks where doc_id=?1)",
                    rusqlite::params![doc_id],
                )?;
                c.execute("delete from knowledge_chunks where doc_id=?1", rusqlite::params![doc_id])?;
                c.execute("delete from knowledge_documents where id=?1", rusqlite::params![doc_id])?;
                Ok(())
            })
            .map_err(|e| e.to_string())?;
        self.delete_blob(doc_id, &source_ref);
        Ok(())
    }

    /// blob 根目录：与 db 文件同级的 knowledge_blobs/。
    fn blob_dir(&self) -> std::path::PathBuf {
        let db = self.db.path();
        let parent = db.parent().unwrap_or_else(|| std::path::Path::new("."));
        parent.join("knowledge_blobs")
    }

    /// 某文档 blob 的文件名：<doc_id>.<ext>（source_ref 无扩展名时仅 <doc_id>）。
    fn blob_file_name(doc_id: &str, source_ref: &str) -> String {
        let ext = crate::knowledge::parser::ext_of(source_ref);
        if ext.is_empty() {
            doc_id.to_string()
        } else {
            format!("{doc_id}.{ext}")
        }
    }

    /// 留存某文档的原始文件字节（导入时调用）。目录不存在则创建。
    pub fn write_blob(&self, doc_id: &str, source_ref: &str, bytes: &[u8]) -> Result<(), String> {
        let dir = self.blob_dir();
        std::fs::create_dir_all(&dir).map_err(|e| format!("创建 blob 目录失败：{e}"))?;
        let path = dir.join(Self::blob_file_name(doc_id, source_ref));
        std::fs::write(&path, bytes).map_err(|e| format!("写入 blob 失败：{e}"))
    }

    /// 读某文档的原始字节用于预览：blob 存在且 ≤5MB → Some((文件名, 字节))；否则 None。
    /// 文件名由 doc_id + source_ref 扩展名推导，供上层按扩展名分类。
    pub fn read_document_blob(&self, doc_id: &str) -> Result<Option<(String, Vec<u8>)>, String> {
        let source_ref = match self.get_document(doc_id)? {
            Some(d) => d.source_ref.unwrap_or_default(),
            None => return Ok(None),
        };
        let name = Self::blob_file_name(doc_id, &source_ref);
        let path = self.blob_dir().join(&name);
        let meta = match std::fs::metadata(&path) {
            Ok(m) => m,
            Err(_) => return Ok(None),
        };
        if meta.len() > 5 * 1024 * 1024 {
            return Ok(None);
        }
        let bytes = std::fs::read(&path).map_err(|e| format!("读取 blob 失败：{e}"))?;
        Ok(Some((name, bytes)))
    }

    /// 删除某文档 blob（best-effort，失败忽略）。
    pub fn delete_blob(&self, doc_id: &str, source_ref: &str) {
        let path = self.blob_dir().join(Self::blob_file_name(doc_id, source_ref));
        let _ = std::fs::remove_file(path);
    }

    /// 取单篇文档（含 source_type / source_ref）。不含原文 text。
    pub fn get_document(&self, doc_id: &str) -> Result<Option<Document>, String> {
        self.db
            .with_connection(|c| {
                let mut stmt = c.prepare(
                    "select id, kb_id, title, source_type, source_ref, status, error, char_size, created_at
                     from knowledge_documents where id=?1",
                )?;
                let mut rows = stmt.query_map(rusqlite::params![doc_id], |r| {
                    Ok(Document {
                        id: r.get(0)?,
                        kb_id: r.get(1)?,
                        title: r.get(2)?,
                        source_type: r.get(3)?,
                        source_ref: r.get(4)?,
                        status: r.get(5)?,
                        error: r.get(6)?,
                        char_size: r.get(7)?,
                        created_at: r.get(8)?,
                    })
                })?;
                Ok(match rows.next() {
                    Some(r) => Some(r?),
                    None => None,
                })
            })
            .map_err(|e| e.to_string())
    }

    /// 挂载知识库到某作用域（session/agent/team）。幂等。
    pub fn mount(&self, kb_id: &str, scope_type: &str, scope_id: &str, now: &str) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                c.execute(
                    "insert or ignore into knowledge_bindings (kb_id, scope_type, scope_id, created_at)
                     values (?1, ?2, ?3, ?4)",
                    rusqlite::params![kb_id, scope_type, scope_id, now],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// 取消挂载。
    pub fn unmount(&self, kb_id: &str, scope_type: &str, scope_id: &str) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                c.execute(
                    "delete from knowledge_bindings where kb_id=?1 and scope_type=?2 and scope_id=?3",
                    rusqlite::params![kb_id, scope_type, scope_id],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// 解析某作用域已挂载的知识库 id 列表。
    pub fn resolve_mounted_kb_ids(&self, scope_type: &str, scope_id: &str) -> Result<Vec<String>, String> {
        self.db
            .with_connection(|c| {
                let mut stmt = c.prepare(
                    "select kb_id from knowledge_bindings where scope_type=?1 and scope_id=?2 order by created_at",
                )?;
                let rows = stmt.query_map(rusqlite::params![scope_type, scope_id], |r| r.get::<_, String>(0))?;
                let mut out = Vec::new();
                for r in rows {
                    out.push(r?);
                }
                Ok(out)
            })
            .map_err(|e| e.to_string())
    }

    /// 某库内缺向量（embedding is null）的片段：(chunk_id, content)。供回填。
    pub fn chunks_missing_embedding(&self, kb_id: &str) -> Result<Vec<(String, String)>, String> {
        self.db.with_connection(|c| {
            let mut stmt = c.prepare(
                "select id, content from knowledge_chunks where kb_id=?1 and embedding is null order by doc_id, ordinal",
            )?;
            let rows = stmt.query_map(rusqlite::params![kb_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
            let mut out = Vec::new();
            for r in rows { out.push(r?); }
            Ok(out)
        }).map_err(|e| e.to_string())
    }

    /// 写某片段的向量。
    pub fn set_chunk_embedding(&self, chunk_id: &str, vec: &[f32]) -> Result<(), String> {
        let bytes = vec_to_bytes(vec);
        self.db.with_connection(|c| {
            c.execute("update knowledge_chunks set embedding=?2 where id=?1", rusqlite::params![chunk_id, bytes])?;
            Ok(())
        }).map_err(|e| e.to_string())
    }

    /// 某些库内所有「有向量」的候选片段（暴力余弦用）。
    pub fn vector_candidates(&self, kb_ids: &[String]) -> Result<Vec<VectorCandidate>, String> {
        if kb_ids.is_empty() { return Ok(Vec::new()); }
        let placeholders: Vec<String> = (0..kb_ids.len()).map(|i| format!("?{}", i + 1)).collect();
        let sql = format!(
            "select ch.id, ch.doc_id, d.title, coalesce(ch.heading_path,''), ch.content, ch.embedding
             from knowledge_chunks ch join knowledge_documents d on d.id = ch.doc_id
             where ch.kb_id in ({}) and ch.embedding is not null",
            placeholders.join(", ")
        );
        self.db.with_connection(|c| {
            let mut stmt = c.prepare(&sql)?;
            let params: Vec<&dyn rusqlite::ToSql> = kb_ids.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
            let rows = stmt.query_map(params.as_slice(), |r| {
                let blob: Vec<u8> = r.get(5)?;
                Ok(VectorCandidate {
                    chunk_id: r.get(0)?,
                    doc_id: r.get(1)?,
                    doc_title: r.get(2)?,
                    heading_path: r.get(3)?,
                    content: r.get(4)?,
                    embedding: bytes_to_vec(&blob),
                })
            })?;
            let mut out = Vec::new();
            for r in rows { out.push(r?); }
            Ok(out)
        }).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
pub(crate) fn test_store() -> KnowledgeStore {
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or_default();
    let p = std::env::temp_dir().join(format!("siw-kb-{}-{}.db", std::process::id(), nanos));
    let _ = std::fs::remove_file(&p);
    let db = Arc::new(crate::storage::AppDatabase::open(&p).unwrap());
    KnowledgeStore::open(db).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedding_store_and_missing() {
        use crate::knowledge::chunk::ChunkPiece;
        let s = test_store();
        let kb = s.create_kb("库", None, None, "t0").unwrap();
        let d = s.add_document(&kb.id, "doc", "paste", None, "x", "t0").unwrap();
        s.replace_chunks(&kb.id, &d.id, &[
            ChunkPiece { content: "甲".into(), heading_path: "".into(), ordinal: 0 },
            ChunkPiece { content: "乙".into(), heading_path: "".into(), ordinal: 1 },
        ]).unwrap();
        let missing = s.chunks_missing_embedding(&kb.id).unwrap();
        assert_eq!(missing.len(), 2);
        let cid = missing[0].0.clone();
        s.set_chunk_embedding(&cid, &[0.1, 0.2, 0.3]).unwrap();
        assert_eq!(s.chunks_missing_embedding(&kb.id).unwrap().len(), 1);
        let cands = s.vector_candidates(&[kb.id.clone()]).unwrap();
        assert!(cands.iter().any(|c| c.chunk_id == cid && !c.embedding.is_empty()));
    }

    #[test]
    fn create_list_update_delete_kb() {
        let s = test_store();
        let kb = s.create_kb("投研资料", Some("券商研报"), None, "2026-06-25T00:00:00Z").unwrap();
        assert_eq!(s.list_kbs().unwrap().len(), 1);
        s.update_kb(&kb.id, "投研资料库", None, None, "2026-06-25T01:00:00Z").unwrap();
        assert_eq!(s.list_kbs().unwrap()[0].name, "投研资料库");
        s.delete_kb(&kb.id).unwrap();
        assert!(s.list_kbs().unwrap().is_empty());
    }

    #[test]
    fn document_chunks_and_bindings() {
        use crate::knowledge::chunk::ChunkPiece;
        let s = test_store();
        let kb = s.create_kb("库", None, None, "t0").unwrap();
        // 新增文档（pending）。
        let doc = s.add_document(&kb.id, "标题", "paste", None, "正文内容", "t0").unwrap();
        assert_eq!(doc.status, "pending");
        // 落片段 + FTS，并置 ready。
        let pieces = vec![
            ChunkPiece { content: "片段甲".into(), heading_path: "A".into(), ordinal: 0 },
            ChunkPiece { content: "片段乙".into(), heading_path: "A › B".into(), ordinal: 1 },
        ];
        s.replace_chunks(&kb.id, &doc.id, &pieces).unwrap();
        s.set_document_status(&doc.id, "ready", None).unwrap();
        assert_eq!(s.list_documents(&kb.id).unwrap()[0].status, "ready");
        // 挂载到会话。
        s.mount(&kb.id, "session", "sess-1", "t0").unwrap();
        assert_eq!(s.resolve_mounted_kb_ids("session", "sess-1").unwrap(), vec![kb.id.clone()]);
        s.unmount(&kb.id, "session", "sess-1").unwrap();
        assert!(s.resolve_mounted_kb_ids("session", "sess-1").unwrap().is_empty());
    }
}

#[cfg(test)]
mod blob_tests {
    use crate::knowledge::store::test_store;

    #[test]
    fn write_read_delete_blob_roundtrip() {
        let s = test_store();
        let kb = s.create_kb("库", None, None, "t0").unwrap();
        // 建一篇 source_ref 带 .md 扩展名的文档
        let doc = s
            .add_document(&kb.id, "资料", "text", Some("/some/where/a.md"), "", "t0")
            .unwrap();

        // 写 blob
        s.write_blob(&doc.id, "/some/where/a.md", b"# hi\n").unwrap();

        // 读回：文件名保留扩展名，字节一致
        let (name, bytes) = s.read_document_blob(&doc.id).unwrap().unwrap();
        assert!(name.ends_with(".md"), "name={name}");
        assert_eq!(bytes, b"# hi\n");

        // 删除后读不到
        s.delete_blob(&doc.id, "/some/where/a.md");
        assert!(s.read_document_blob(&doc.id).unwrap().is_none());
    }

    #[test]
    fn read_blob_none_when_absent() {
        let s = test_store();
        let kb = s.create_kb("库", None, None, "t0").unwrap();
        let doc = s.add_document(&kb.id, "d", "paste", None, "", "t0").unwrap();
        assert!(s.read_document_blob(&doc.id).unwrap().is_none());
    }

    #[test]
    fn delete_kb_removes_document_blobs() {
        let s = test_store();
        let kb = s.create_kb("库", None, None, "t0").unwrap();
        let a = s
            .add_document(&kb.id, "a", "text", Some("/x/a.md"), "", "t0")
            .unwrap();
        let b = s
            .add_document(&kb.id, "b", "pdf", Some("/x/b.pdf"), "", "t0")
            .unwrap();
        s.write_blob(&a.id, "/x/a.md", b"# a").unwrap();
        s.write_blob(&b.id, "/x/b.pdf", b"%PDF-1.7").unwrap();

        // 直接看物理文件（删库会删文档行，read_document_blob 之后必然 None，无法验证文件是否真被删）。
        let a_path = s.blob_dir().join(format!("{}.md", a.id));
        let b_path = s.blob_dir().join(format!("{}.pdf", b.id));
        assert!(a_path.exists());
        assert!(b_path.exists());

        s.delete_kb(&kb.id).unwrap();

        // 删库后两篇 blob 物理文件都应被清理。
        assert!(!a_path.exists(), "a blob 未清理");
        assert!(!b_path.exists(), "b blob 未清理");
    }

    #[test]
    fn get_document_returns_source_fields() {
        let s = test_store();
        let kb = s.create_kb("库", None, None, "t0").unwrap();
        let doc = s
            .add_document(&kb.id, "资料", "text", Some("/x/a.md"), "", "t0")
            .unwrap();
        let got = s.get_document(&doc.id).unwrap().unwrap();
        assert_eq!(got.source_ref.as_deref(), Some("/x/a.md"));
        assert_eq!(got.source_type, "text");
    }
}

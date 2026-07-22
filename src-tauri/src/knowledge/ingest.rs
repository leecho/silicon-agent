//! 导入编排：正文 → 分块 → 落片段+FTS → 置文档 ready。P1 来源 text/paste 正文即纯文本，
//! 无需独立 parser；P2 接 pdf/url parser 时在此分发。

use crate::knowledge::chunk::chunk_text;
use crate::knowledge::types::Document;
use crate::knowledge::KnowledgeStore;

/// P1 单片上限与重叠（字符）。中文资料偏短，1000/100 是稳妥起点。
const MAX_CHARS: usize = 1000;
const OVERLAP: usize = 100;

impl KnowledgeStore {
    /// 导入一篇文档：建文档记录 → 分块落库 → 置 ready；空正文置 error。
    pub fn ingest_document(
        &self,
        kb_id: &str,
        title: &str,
        source_type: &str,
        source_ref: Option<&str>,
        body: &str,
        now: &str,
    ) -> Result<Document, String> {
        let doc = self.add_document(kb_id, title, source_type, source_ref, body, now)?;
        self.finish_ingest(kb_id, doc, body)
    }

    /// 从本地文件导入：按扩展名解析为文本再入库。解析失败 → status=error。
    pub fn ingest_file(&self, kb_id: &str, title: &str, path: &str, now: &str) -> Result<Document, String> {
        let ext = crate::knowledge::parser::ext_of(path);
        let source_type = if matches!(ext.as_str(), "md" | "markdown" | "txt" | "") {
            "text".to_string()
        } else {
            ext.clone()
        };
        let doc = self.add_document(kb_id, title, &source_type, Some(path), "", now)?;
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => {
                self.set_document_status(&doc.id, "error", Some(&crate::permissions::describe_read_error(&e, path)))?;
                return Ok(Document { status: "error".into(), ..doc });
            }
        };
        // 留存原始字节供富预览；失败不阻断导入（预览会回退文本）。
        if let Err(e) = self.write_blob(&doc.id, path, &bytes) {
            eprintln!("知识库 blob 留存失败（doc={}）：{e}", doc.id);
        }
        match crate::knowledge::parser::parse_bytes(&ext, &bytes) {
            Ok(text) => self.finish_ingest(kb_id, doc, &text),
            Err(e) => {
                self.set_document_status(&doc.id, "error", Some(&e))?;
                Ok(Document { status: "error".into(), ..doc })
            }
        }
    }

    /// 从 URL 导入：复用 web_fetch 抓取 + HTML→text。
    pub fn ingest_url(&self, kb_id: &str, title: &str, url: &str, now: &str) -> Result<Document, String> {
        let doc = self.add_document(kb_id, title, "url", Some(url), "", now)?;
        match crate::tools::web_fetch::fetch_url_text(url) {
            Ok((_, text)) => self.finish_ingest(kb_id, doc, &text),
            Err(e) => {
                self.set_document_status(&doc.id, "error", Some(&e))?;
                Ok(Document { status: "error".into(), ..doc })
            }
        }
    }

    /// 公共尾段：分块 → 落库 → 置 ready；空文本置 error。
    fn finish_ingest(&self, kb_id: &str, doc: Document, body: &str) -> Result<Document, String> {
        let pieces = chunk_text(body, MAX_CHARS, OVERLAP);
        if pieces.is_empty() {
            self.set_document_status(&doc.id, "error", Some("正文为空，未能提取任何内容"))?;
            return Ok(Document { status: "error".into(), ..doc });
        }
        self.replace_chunks(kb_id, &doc.id, &pieces)?;
        // 回写原文 + 真实字数（修正 file/url 导入先以空串建档导致的 char_size=0）。
        self.set_document_text(&doc.id, body)?;
        self.set_document_status(&doc.id, "ready", None)?;
        let char_size = body.chars().count() as i64;
        Ok(Document { status: "ready".into(), char_size, ..doc })
    }
}

#[cfg(test)]
mod tests {
    use crate::knowledge::store::test_store;

    #[test]
    fn ingest_chunks_and_marks_ready() {
        let s = test_store();
        let kb = s.create_kb("库", None, None, "t0").unwrap();
        let doc = s
            .ingest_document(&kb.id, "研报", "paste", None, "# 概述\n\n光伏装机持续增长。", "t0")
            .unwrap();
        assert_eq!(doc.status, "ready");
        assert_eq!(s.list_documents(&kb.id).unwrap()[0].status, "ready");
    }

    #[test]
    fn ingest_records_char_size_and_text() {
        let s = test_store();
        let kb = s.create_kb("库", None, None, "t0").unwrap();
        let body = "# 概述\n\n光伏装机持续增长。";
        let doc = s.ingest_document(&kb.id, "研报", "paste", None, body, "t0").unwrap();
        // 返回值与落库的 char_size 都应为真实字数（非 0）。
        let expect = body.chars().count() as i64;
        assert_eq!(doc.char_size, expect);
        assert_eq!(s.list_documents(&kb.id).unwrap()[0].char_size, expect);
        // 原文可取回。
        assert_eq!(s.get_document_text(&doc.id).unwrap(), body);
    }

    #[test]
    fn ingest_empty_marks_error() {
        let s = test_store();
        let kb = s.create_kb("库", None, None, "t0").unwrap();
        let doc = s.ingest_document(&kb.id, "空", "paste", None, "   \n\n ", "t0").unwrap();
        assert_eq!(doc.status, "error");
    }

    #[test]
    fn ingest_file_md_reads_and_chunks() {
        let s = test_store();
        let kb = s.create_kb("库", None, None, "t0").unwrap();
        let p = std::env::temp_dir().join(format!("kbt-{}-a.md", std::process::id()));
        std::fs::write(&p, "# 标题\n\n正文内容一二三。").unwrap();
        let doc = s.ingest_file(&kb.id, "资料", p.to_str().unwrap(), "t0").unwrap();
        assert_eq!(doc.status, "ready");
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn ingest_file_writes_blob() {
        let s = test_store();
        let kb = s.create_kb("库", None, None, "t0").unwrap();
        let p = std::env::temp_dir().join(format!("kbt-{}-blob.md", std::process::id()));
        std::fs::write(&p, "# 标题\n\n正文。").unwrap();
        let doc = s.ingest_file(&kb.id, "资料", p.to_str().unwrap(), "t0").unwrap();
        assert_eq!(doc.status, "ready");
        // blob 可读回，内容与原文件一致
        let (name, bytes) = s.read_document_blob(&doc.id).unwrap().unwrap();
        assert!(name.ends_with(".md"));
        assert_eq!(bytes, "# 标题\n\n正文。".as_bytes());
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn delete_document_removes_blob() {
        let s = test_store();
        let kb = s.create_kb("库", None, None, "t0").unwrap();
        let p = std::env::temp_dir().join(format!("kbt-{}-del.md", std::process::id()));
        std::fs::write(&p, "# x").unwrap();
        let doc = s.ingest_file(&kb.id, "资料", p.to_str().unwrap(), "t0").unwrap();
        assert!(s.read_document_blob(&doc.id).unwrap().is_some());
        s.delete_document(&doc.id).unwrap();
        assert!(s.read_document_blob(&doc.id).unwrap().is_none());
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn ingest_file_unsupported_ext_errors_status() {
        let s = test_store();
        let kb = s.create_kb("库", None, None, "t0").unwrap();
        let p = std::env::temp_dir().join(format!("kbt-{}-b.exe", std::process::id()));
        std::fs::write(&p, b"\x00\x01binary").unwrap();
        let doc = s.ingest_file(&kb.id, "x", p.to_str().unwrap(), "t0").unwrap();
        assert_eq!(doc.status, "error");
        let _ = std::fs::remove_file(&p);
    }
}

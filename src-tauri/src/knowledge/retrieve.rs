//! 检索抽象：FTS5/BM25 + 向量余弦 + RRF 混合，统一入口 `retrieve_knowledge`。
use std::sync::Arc;

use crate::knowledge::types::RetrievedChunk;
use crate::storage::AppDatabase;

pub struct RetrieveQuery<'a> {
    pub text: &'a str,
    pub kb_ids: &'a [String],
    pub top_k: usize,
}

pub trait Retriever: Send + Sync {
    fn retrieve(&self, q: &RetrieveQuery) -> Result<Vec<RetrievedChunk>, String>;
}

/// FTS5/BM25 检索：在指定 kb_ids 内按 BM25 排序取 Top-K。
/// 复用 `memory::build_fts_match` 做查询消毒（中文 trigram、拉丁词），
/// SQL 用表名做 match 与 bm25（与 `memory::fts_recall_facts` 同写法）。
pub struct Fts5Retriever {
    pub db: Arc<AppDatabase>,
}

impl Fts5Retriever {
    /// 取命中片 ordinal，并把同文档 [ordinal-1, ordinal+1] 三片正文按序拼接（上下文扩展）。
    pub(crate) fn expand_content(&self, chunk_id: &str, doc_id: &str) -> Result<String, String> {
        self.db
            .with_connection(|c| {
                let ord: i64 = c.query_row(
                    "select ordinal from knowledge_chunks where id=?1",
                    rusqlite::params![chunk_id],
                    |r| r.get(0),
                )?;
                let mut stmt = c.prepare(
                    "select content from knowledge_chunks
                     where doc_id=?1 and ordinal between ?2 and ?3 order by ordinal",
                )?;
                let rows = stmt.query_map(
                    rusqlite::params![doc_id, ord - 1, ord + 1],
                    |r| r.get::<_, String>(0),
                )?;
                let mut parts = Vec::new();
                for r in rows {
                    parts.push(r?);
                }
                Ok(parts.join("\n"))
            })
            .map_err(|e| e.to_string())
    }

    /// 纯 FTS/BM25 召回，不做上下文扩展。
    pub fn retrieve_raw(&self, q: &RetrieveQuery) -> Result<Vec<RetrievedChunk>, String> {
        if q.kb_ids.is_empty() {
            return Ok(Vec::new());
        }
        let Some(match_expr) = crate::memory::build_fts_match(q.text) else {
            return Ok(Vec::new());
        };
        // kb_ids 动态占位符：?1=match，?2=top_k，?3.. = kb_ids。
        let placeholders: Vec<String> = (0..q.kb_ids.len()).map(|i| format!("?{}", i + 3)).collect();
        let sql = format!(
            "select ch.id, ch.doc_id, d.title, coalesce(ch.heading_path, ''), ch.content,
                    bm25(knowledge_chunks_fts) as score
             from knowledge_chunks_fts
             join knowledge_chunks ch on ch.id = knowledge_chunks_fts.id
             join knowledge_documents d on d.id = ch.doc_id
             where knowledge_chunks_fts match ?1 and ch.kb_id in ({})
             order by bm25(knowledge_chunks_fts) asc
             limit ?2",
            placeholders.join(", ")
        );
        let top_k = q.top_k as i64;
        self.db
            .with_connection(|c| {
                let mut stmt = c.prepare(&sql)?;
                let mut params: Vec<&dyn rusqlite::ToSql> = vec![&match_expr, &top_k];
                for id in q.kb_ids {
                    params.push(id);
                }
                let rows = stmt.query_map(params.as_slice(), |r| {
                    Ok(RetrievedChunk {
                        chunk_id: r.get(0)?,
                        doc_id: r.get(1)?,
                        doc_title: r.get(2)?,
                        heading_path: r.get(3)?,
                        content: r.get(4)?,
                        score: r.get(5)?,
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
}

impl Retriever for Fts5Retriever {
    fn retrieve(&self, q: &RetrieveQuery) -> Result<Vec<RetrievedChunk>, String> {
        // 先做纯 FTS 召回，再对每条结果做上下文扩展。
        let mut out = self.retrieve_raw(q)?;
        // 上下文扩展：命中片补齐前后各 1 片。
        for hit in out.iter_mut() {
            if let Ok(expanded) = self.expand_content(&hit.chunk_id, &hit.doc_id) {
                if !expanded.trim().is_empty() {
                    hit.content = expanded;
                }
            }
        }
        Ok(out)
    }
}

use crate::knowledge::embed::{cosine, Embedder};
use crate::knowledge::KnowledgeStore;

/// 向量召回（暴力余弦）。embed 查询 → 与库内全量有向量片段打余弦分 → 降序 top_k。
fn vector_retrieve_raw(
    store: &KnowledgeStore,
    embedder: &dyn Embedder,
    query: &str,
    kb_ids: &[String],
    top_k: usize,
) -> Result<Vec<RetrievedChunk>, String> {
    let qvec = embedder
        .embed(&[query.to_string()])?
        .into_iter()
        .next()
        .ok_or("embedding 返回空")?;
    let mut scored: Vec<RetrievedChunk> = store
        .vector_candidates(kb_ids)?
        .into_iter()
        .map(|c| RetrievedChunk {
            chunk_id: c.chunk_id,
            doc_id: c.doc_id,
            doc_title: c.doc_title,
            heading_path: c.heading_path,
            content: c.content,
            score: cosine(&qvec, &c.embedding) as f64,
        })
        .collect();
    scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(top_k);
    Ok(scored)
}

/// RRF 融合两路排名（k=60），按 chunk_id 合并得分，取 top-k。
fn rrf_fuse(fts: Vec<RetrievedChunk>, vec: Vec<RetrievedChunk>, top_k: usize) -> Vec<RetrievedChunk> {
    const K: f64 = 60.0;
    use std::collections::HashMap;
    let mut score: HashMap<String, f64> = HashMap::new();
    let mut repr: HashMap<String, RetrievedChunk> = HashMap::new();
    for (rank, h) in fts.iter().enumerate() {
        *score.entry(h.chunk_id.clone()).or_insert(0.0) += 1.0 / (K + rank as f64 + 1.0);
        repr.entry(h.chunk_id.clone()).or_insert_with(|| h.clone());
    }
    for (rank, h) in vec.iter().enumerate() {
        *score.entry(h.chunk_id.clone()).or_insert(0.0) += 1.0 / (K + rank as f64 + 1.0);
        repr.entry(h.chunk_id.clone()).or_insert_with(|| h.clone());
    }
    let mut ids: Vec<(String, f64)> = score.into_iter().collect();
    ids.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    ids.truncate(top_k);
    ids.into_iter()
        .filter_map(|(id, s)| repr.remove(&id).map(|mut c| { c.score = s; c }))
        .collect()
}

/// 统一检索入口：vector_enabled 且向量召回非空 → BM25 + 向量 RRF 混合；
/// 否则纯 BM25。最后对所有命中做上下文扩展。
/// 向量任一步失败 → 静默回退 BM25。
pub fn retrieve_knowledge(
    store: &KnowledgeStore,
    embedder: &dyn Embedder,
    vector_enabled: bool,
    query: &str,
    kb_ids: &[String],
    top_k: usize,
) -> Result<Vec<RetrievedChunk>, String> {
    let fts = Fts5Retriever { db: store.db.clone() };
    // 宽召回再截断，给 RRF 更多候选。
    let widen = top_k.max(10);
    let fts_hits = fts.retrieve_raw(&RetrieveQuery { text: query, kb_ids, top_k: widen })?;
    let mut hits = if vector_enabled {
        match vector_retrieve_raw(store, embedder, query, kb_ids, widen) {
            Ok(vec_hits) if !vec_hits.is_empty() => rrf_fuse(fts_hits, vec_hits, top_k),
            _ => {
                // 向量失败或空 → 静默降级 BM25。
                let mut h = fts_hits;
                h.truncate(top_k);
                h
            }
        }
    } else {
        let mut h = fts_hits;
        h.truncate(top_k);
        h
    };
    // 统一上下文扩展：补齐前后各 1 片。
    for hit in hits.iter_mut() {
        if let Ok(expanded) = fts.expand_content(&hit.chunk_id, &hit.doc_id) {
            if !expanded.trim().is_empty() {
                hit.content = expanded;
            }
        }
    }
    Ok(hits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::chunk::ChunkPiece;
    use crate::knowledge::store::test_store;

    struct MockEmbedder;
    impl crate::knowledge::embed::Embedder for MockEmbedder {
        fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
            Ok(texts.iter().map(|t| {
                if t.contains("光伏") { vec![1.0, 0.0] }
                else if t.contains("销量") { vec![0.0, 1.0] }
                else { vec![0.5, 0.5] }
            }).collect())
        }
    }

    #[test]
    fn hybrid_fuses_fts_and_vector() {
        use crate::knowledge::chunk::ChunkPiece;
        let s = test_store();
        let kb = s.create_kb("库", None, None, "t0").unwrap();
        let d = s.add_document(&kb.id, "doc", "paste", None, "x", "t0").unwrap();
        s.replace_chunks(&kb.id, &d.id, &[
            ChunkPiece { content: "光伏装机量持续增长".into(), heading_path: "".into(), ordinal: 0 },
            ChunkPiece { content: "新能源汽车销量数据".into(), heading_path: "".into(), ordinal: 1 },
        ]).unwrap();
        for (cid, content) in s.chunks_missing_embedding(&kb.id).unwrap() {
            let v = if content.contains("光伏") { vec![1.0, 0.0] } else { vec![0.0, 1.0] };
            s.set_chunk_embedding(&cid, &v).unwrap();
        }
        let hits = retrieve_knowledge(&s, &MockEmbedder, true, "光伏装机", &[kb.id.clone()], 5).unwrap();
        assert!(!hits.is_empty());
        assert!(hits[0].content.contains("光伏装机量"));
    }

    #[test]
    fn disabled_falls_back_to_bm25_only() {
        use crate::knowledge::chunk::ChunkPiece;
        let s = test_store();
        let kb = s.create_kb("库", None, None, "t0").unwrap();
        let d = s.add_document(&kb.id, "doc", "paste", None, "x", "t0").unwrap();
        s.replace_chunks(&kb.id, &d.id, &[
            ChunkPiece { content: "光伏装机量持续增长".into(), heading_path: "".into(), ordinal: 0 },
        ]).unwrap();
        let hits = retrieve_knowledge(&s, &MockEmbedder, false, "光伏装机", &[kb.id.clone()], 5).unwrap();
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn retrieve_hits_within_mounted_kb_only() {
        let s = test_store();
        let kb1 = s.create_kb("库一", None, None, "t0").unwrap();
        let kb2 = s.create_kb("库二", None, None, "t0").unwrap();
        let d1 = s.add_document(&kb1.id, "doc1", "paste", None, "x", "t0").unwrap();
        let d2 = s.add_document(&kb2.id, "doc2", "paste", None, "x", "t0").unwrap();
        // 注意：trigram 检索要求查询与正文有 3 字重叠子串。
        s.replace_chunks(&kb1.id, &d1.id, &[ChunkPiece {
            content: "光伏装机量持续增长，行业景气向好".into(), heading_path: "趋势".into(), ordinal: 0,
        }]).unwrap();
        s.replace_chunks(&kb2.id, &d2.id, &[ChunkPiece {
            content: "新能源汽车销量数据统计".into(), heading_path: "".into(), ordinal: 0,
        }]).unwrap();

        let retr = Fts5Retriever { db: s.db.clone() };
        // 只在 kb1 内检索"光伏装机"，命中 doc1、不串 kb2。
        let hits = retr.retrieve(&RetrieveQuery { text: "光伏装机", kb_ids: &[kb1.id.clone()], top_k: 5 }).unwrap();
        assert_eq!(hits.len(), 1, "应命中 kb1 的 doc1");
        assert_eq!(hits[0].doc_title, "doc1");
        assert_eq!(hits[0].heading_path, "趋势");
        // kb_ids 为空 → 空结果。
        assert!(retr.retrieve(&RetrieveQuery { text: "光伏装机", kb_ids: &[], top_k: 5 }).unwrap().is_empty());
    }

    #[test]
    fn retrieve_expands_with_neighbor_chunks() {
        use crate::knowledge::chunk::ChunkPiece;
        let s = test_store();
        let kb = s.create_kb("库", None, None, "t0").unwrap();
        let d = s.add_document(&kb.id, "doc", "paste", None, "x", "t0").unwrap();
        s.replace_chunks(&kb.id, &d.id, &[
            ChunkPiece { content: "前文背景介绍".into(), heading_path: "".into(), ordinal: 0 },
            ChunkPiece { content: "光伏装机量持续增长".into(), heading_path: "".into(), ordinal: 1 },
            ChunkPiece { content: "后文结论部分".into(), heading_path: "".into(), ordinal: 2 },
        ]).unwrap();
        let retr = Fts5Retriever { db: s.db.clone() };
        let hits = retr.retrieve(&RetrieveQuery { text: "光伏装机", kb_ids: &[kb.id.clone()], top_k: 5 }).unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].content.contains("前文背景"), "应含前文片：{}", hits[0].content);
        assert!(hits[0].content.contains("光伏装机量"), "应含命中片：{}", hits[0].content);
        assert!(hits[0].content.contains("后文结论"), "应含后文片：{}", hits[0].content);
    }
}

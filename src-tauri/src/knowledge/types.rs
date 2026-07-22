//! 知识库模块对外数据类型。
use serde::{Deserialize, Serialize};

/// 一个知识库（资料库集合）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeBase {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub created_at: String,
    pub updated_at: Option<String>,
    /// 该库内资料数（仅列表查询填充；create/update 返回 0）。
    #[serde(default)]
    pub doc_count: i64,
}

/// 知识库内的一篇文档（导入单元）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    pub id: String,
    pub kb_id: String,
    pub title: String,
    pub source_type: String, // text | paste
    pub source_ref: Option<String>,
    pub status: String, // pending | parsing | ready | error
    pub error: Option<String>,
    pub char_size: i64,
    pub created_at: String,
}

/// 检索命中的片段（含来源标注，供工具拼引用）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetrievedChunk {
    pub chunk_id: String,
    pub doc_id: String,
    pub doc_title: String,
    pub heading_path: String,
    pub content: String,
    pub score: f64,
}

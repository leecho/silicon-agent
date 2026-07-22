//! 知识库命令（薄入口）。
use super::artifact::{classify_artifact, ArtifactContent};
use crate::app_state::{now_string, AppState};
use crate::knowledge::{Document, KnowledgeBase, RetrievedChunk};
use tauri::State;

#[tauri::command]
pub fn kb_list(services: State<'_, AppState>) -> Result<Vec<KnowledgeBase>, String> {
    services.knowledge.list_kbs()
}

#[tauri::command]
pub fn kb_create(
    services: State<'_, AppState>,
    name: String,
    description: Option<String>,
    icon: Option<String>,
) -> Result<KnowledgeBase, String> {
    services
        .knowledge
        .create_kb(&name, description.as_deref(), icon.as_deref(), &now_string())
}

#[tauri::command]
pub fn kb_update(
    services: State<'_, AppState>,
    id: String,
    name: String,
    description: Option<String>,
    icon: Option<String>,
) -> Result<(), String> {
    services
        .knowledge
        .update_kb(&id, &name, description.as_deref(), icon.as_deref(), &now_string())
}

#[tauri::command]
pub fn kb_delete(services: State<'_, AppState>, id: String) -> Result<(), String> {
    services.knowledge.delete_kb(&id)
}

#[tauri::command]
pub fn kb_document_list(services: State<'_, AppState>, kb_id: String) -> Result<Vec<Document>, String> {
    services.knowledge.list_documents(&kb_id)
}

/// 添加资料：粘贴文本（body）或选本地文件（filePath，按扩展名解析）。
#[tauri::command]
pub fn kb_document_add(
    services: State<'_, AppState>,
    kb_id: String,
    title: String,
    body: Option<String>,
    file_path: Option<String>,
) -> Result<Document, String> {
    match file_path {
        Some(p) => services.knowledge.ingest_file(&kb_id, &title, &p, &now_string()),
        None => services
            .knowledge
            .ingest_document(&kb_id, &title, "paste", None, &body.unwrap_or_default(), &now_string()),
    }
}

/// 从网址添加资料（抓取网页正文）。
#[tauri::command]
pub fn kb_document_add_url(
    services: State<'_, AppState>,
    kb_id: String,
    title: String,
    url: String,
) -> Result<Document, String> {
    let t = if title.trim().is_empty() { url.clone() } else { title };
    services.knowledge.ingest_url(&kb_id, &t, &url, &now_string())
}

#[tauri::command]
pub fn kb_document_delete(services: State<'_, AppState>, doc_id: String) -> Result<(), String> {
    services.knowledge.delete_document(&doc_id)
}

/// UI 检索预览：按全局向量开关走 BM25 或混合。
#[tauri::command]
pub fn kb_search(
    services: State<'_, AppState>,
    kb_ids: Vec<String>,
    query: String,
    top_k: Option<u32>,
) -> Result<Vec<RetrievedChunk>, String> {
    use crate::knowledge::embed_gateway::GatewayEmbedder;
    use crate::knowledge::retrieve::retrieve_knowledge;
    let enabled = services.app_settings.get_knowledge_vector_enabled()?;
    let model = services.app_settings.get_knowledge_embedding_model()?;
    let embedder = GatewayEmbedder { gateway: services.gateway.clone(), model_id: model };
    retrieve_knowledge(
        &services.knowledge,
        &embedder,
        enabled,
        &query,
        &kb_ids,
        top_k.unwrap_or(5).clamp(1, 20) as usize,
    )
}

/// 为某资料库内缺向量的片段批量建立向量索引。返回新建数量。
#[tauri::command]
pub fn kb_build_vector_index(services: State<'_, AppState>, kb_id: String) -> Result<usize, String> {
    use crate::knowledge::embed::Embedder;
    use crate::knowledge::embed_gateway::GatewayEmbedder;
    let model = services.app_settings.get_knowledge_embedding_model()?;
    if model.trim().is_empty() {
        return Err("请先在设置里选择 embedding 模型".into());
    }
    let missing = services.knowledge.chunks_missing_embedding(&kb_id)?;
    if missing.is_empty() {
        return Ok(0);
    }
    let embedder = GatewayEmbedder { gateway: services.gateway.clone(), model_id: model };
    let mut done = 0usize;
    for batch in missing.chunks(64) {
        let texts: Vec<String> = batch.iter().map(|(_, c)| c.clone()).collect();
        let vecs = embedder.embed(&texts)?;
        if vecs.len() != batch.len() {
            return Err("embedding 返回数量与请求不一致".into());
        }
        for ((cid, _), v) in batch.iter().zip(vecs.iter()) {
            services.knowledge.set_chunk_embedding(cid, v)?;
            done += 1;
        }
    }
    Ok(done)
}

#[tauri::command]
pub fn kb_mount(services: State<'_, AppState>, kb_id: String, session_id: String) -> Result<(), String> {
    services.knowledge.mount(&kb_id, "session", &session_id, &now_string())
}

#[tauri::command]
pub fn kb_unmount(services: State<'_, AppState>, kb_id: String, session_id: String) -> Result<(), String> {
    services.knowledge.unmount(&kb_id, "session", &session_id)
}

#[tauri::command]
pub fn kb_mounted_ids(services: State<'_, AppState>, session_id: String) -> Result<Vec<String>, String> {
    services.knowledge.resolve_mounted_kb_ids("session", &session_id)
}

/// 通用挂载：scope_type ∈ session | agent | project。供智能体/项目入口复用。
#[tauri::command]
pub fn kb_mount_scope(
    services: State<'_, AppState>,
    kb_id: String,
    scope_type: String,
    scope_id: String,
) -> Result<(), String> {
    services.knowledge.mount(&kb_id, &scope_type, &scope_id, &now_string())
}

/// 通用卸载。
#[tauri::command]
pub fn kb_unmount_scope(
    services: State<'_, AppState>,
    kb_id: String,
    scope_type: String,
    scope_id: String,
) -> Result<(), String> {
    services.knowledge.unmount(&kb_id, &scope_type, &scope_id)
}

/// 某作用域已挂载的资料库 id。
#[tauri::command]
pub fn kb_scoped_mounted_ids(
    services: State<'_, AppState>,
    scope_type: String,
    scope_id: String,
) -> Result<Vec<String>, String> {
    services.knowledge.resolve_mounted_kb_ids(&scope_type, &scope_id)
}

/// 读向量检索设置：(启用, 模型id)。
#[tauri::command]
pub fn kb_vector_settings(services: State<'_, AppState>) -> Result<(bool, String), String> {
    Ok((
        services.app_settings.get_knowledge_vector_enabled()?,
        services.app_settings.get_knowledge_embedding_model()?,
    ))
}

/// 写向量检索设置。
#[tauri::command]
pub fn kb_set_vector_settings(
    services: State<'_, AppState>,
    enabled: bool,
    model_id: String,
) -> Result<(), String> {
    services.app_settings.set_knowledge_vector_enabled(enabled)?;
    services.app_settings.set_knowledge_embedding_model(&model_id)?;
    Ok(())
}

/// 取一份资料的原文（供查看资料内容）。
#[tauri::command]
pub fn kb_document_text(services: State<'_, AppState>, doc_id: String) -> Result<String, String> {
    services.knowledge.get_document_text(&doc_id)
}

/// 预览纯逻辑：有 blob → 按字节分类；无 blob → 按 source_ref 扩展名决定 markdown/text 回退。
fn preview_from_parts(
    blob: Option<(String, Vec<u8>)>,
    fallback_text: &str,
    source_ref: Option<&str>,
) -> ArtifactContent {
    if let Some((name, bytes)) = blob {
        return classify_artifact(&name, &bytes);
    }
    let ext = source_ref.map(crate::knowledge::parser::ext_of).unwrap_or_default();
    let kind = if matches!(ext.as_str(), "md" | "markdown") {
        "markdown"
    } else {
        "text"
    };
    ArtifactContent {
        kind: kind.to_string(),
        content: fallback_text.to_string(),
    }
}

/// 预览一份资料：有原始文件字节走产物富渲染，否则回退已存文本。
#[tauri::command]
pub fn kb_document_preview(
    services: State<'_, AppState>,
    doc_id: String,
) -> Result<ArtifactContent, String> {
    // 有 blob 直接富渲染，避免加载可能很大的已存文本。
    if let Some((name, bytes)) = services.knowledge.read_document_blob(&doc_id)? {
        return Ok(classify_artifact(&name, &bytes));
    }
    // 无 blob → 回退已存文本，按 source_ref 扩展名决定 markdown/text。
    let source_ref = services
        .knowledge
        .get_document(&doc_id)?
        .and_then(|d| d.source_ref);
    let text = services.knowledge.get_document_text(&doc_id)?;
    Ok(preview_from_parts(None, &text, source_ref.as_deref()))
}

#[cfg(test)]
mod preview_tests {
    use super::preview_from_parts;

    #[test]
    fn blob_present_classifies_by_bytes() {
        // 有 blob：按文件名扩展名分类
        let c = preview_from_parts(Some(("a.md".into(), b"# hi".to_vec())), "fallback", None);
        assert_eq!(c.kind, "markdown");
        assert_eq!(c.content, "# hi");

        let c = preview_from_parts(Some(("r.pdf".into(), b"%PDF-1.7".to_vec())), "x", None);
        assert_eq!(c.kind, "pdf");
        assert!(c.content.starts_with("data:application/pdf;base64,"));
    }

    #[test]
    fn no_blob_markdown_by_source_ref_ext() {
        // 无 blob，source_ref 为 .md → markdown 回退
        let c = preview_from_parts(None, "# 老资料", Some("/x/old.md"));
        assert_eq!(c.kind, "markdown");
        assert_eq!(c.content, "# 老资料");
    }

    #[test]
    fn no_blob_plain_text_fallback() {
        // 无 blob，无 md 扩展名 → text 回退
        let c = preview_from_parts(None, "纯文本正文", Some("https://example.com"));
        assert_eq!(c.kind, "text");
        let c = preview_from_parts(None, "粘贴内容", None);
        assert_eq!(c.kind, "text");
    }
}

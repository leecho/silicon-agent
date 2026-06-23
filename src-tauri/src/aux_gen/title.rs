//! 会话标题的辅助生成：首条用户消息 → 简短标题（一次性非流式 LLM 调用）。

use std::sync::Arc;

use tauri::Emitter;

use crate::app_settings::AppSettingsStore;
use crate::app_state::now_string;
use crate::provider::ProviderGateway;
use crate::session::SessionStore;
use crate::storage::AppDatabase;

use super::shared::{extract_completion_text, resolve_aux_selection};

/// 清洗模型返回的标题：取首个非空行，去掉引号/书名号等包裹，截断到 24 字。
fn clean_title(raw: &str) -> String {
    let first = raw
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim();
    let trimmed = first.trim_matches(|c: char| {
        matches!(
            c,
            '"' | '\'' | '「' | '」' | '《' | '》' | '【' | '】' | ' ' | '。' | '：'
        )
    });
    trimmed
        .chars()
        .take(24)
        .collect::<String>()
        .trim()
        .to_string()
}

/// 用大模型把用户首条消息归纳成简短会话标题（一次性非流式调用）。失败返回 None。
fn generate_title(
    provider: &ProviderGateway,
    selection: Option<crate::provider::model::ResolvedModel>,
    session_id: &str,
    content: &str,
) -> Option<String> {
    use crate::provider::client::{ModelCallRequest, ModelClient};
    use crate::provider::message::ModelMessage;
    let request = ModelCallRequest {
        messages: vec![
            ModelMessage::system(
                "你是会话标题助手。请用不超过12个汉字概括用户的任务，只输出标题本身，不要引号、标点或任何前后缀。",
            ),
            ModelMessage::user(content),
        ],
        // 推理模型会先消耗 token 做思维链，太小会把答案截没；给足空间。
        max_output_tokens: Some(1024),
        timeout_ms: Some(30_000),
        stream: false,
        model_selection: selection.as_ref().map(|r| r.selection()),
        ..Default::default()
    };
    let mut request = request;
    request.attribution.session_id = session_id.to_string();
    request.attribution.usage_type = Some("title".to_string());
    let result = match provider.complete_model(request) {
        Ok(r) => r,
        Err(err) => {
            eprintln!("[title] 模型调用失败 会话={session_id}：{}", err.message);
            return None;
        }
    };
    let raw = extract_completion_text(&result);
    eprintln!(
        "[title] 模型原始输出 会话={session_id} finish={:?}：{raw}",
        result.finish_reason
    );
    let title = clean_title(&raw);
    if title.is_empty() {
        None
    } else {
        Some(title)
    }
}

/// 后台线程：生成 LLM 标题（失败回退首条消息截断），仅当标题仍为默认时写入，并通知前端刷新。
/// 从 db 打开所需 store 并解析辅助模型选择（与建议生成一致），调用方只需传 provider/db/ids/content。
pub fn spawn(
    app: tauri::AppHandle,
    provider: Arc<ProviderGateway>,
    db: Arc<AppDatabase>,
    session_id: String,
    content: String,
) {
    std::thread::spawn(move || {
        eprintln!("[title] 开始生成 会话={session_id}");
        let Ok(session) = SessionStore::open(db.clone()) else {
            eprintln!("[title] 打开会话存储失败 会话={session_id}");
            return;
        };
        let Ok(app_settings) = AppSettingsStore::open(db) else {
            eprintln!("[title] 打开配置存储失败 会话={session_id}");
            return;
        };
        let selection = resolve_aux_selection(&provider, &session, &app_settings, &session_id);
        let fallback: String = content.chars().take(20).collect();
        let title = generate_title(&provider, selection, &session_id, &content)
            .unwrap_or_else(|| fallback.trim().to_string());
        let now = now_string();
        let changed = session
            .set_title_if_default(&session_id, title.trim(), &now)
            .unwrap_or(false);
        eprintln!("[title] 标题={title:?} changed(仍默认才写入)={changed} 会话={session_id}");
        if changed {
            let _ = app.emit(
                "session_updated",
                serde_json::json!({ "sessionId": session_id }),
            );
        }
    });
}

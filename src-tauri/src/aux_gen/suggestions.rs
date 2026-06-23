//! 一轮 run 正常结束后的「下一步」快捷建议生成（一次性非流式 LLM 调用）。

use std::sync::Arc;

use tauri::{Emitter, Manager};

use crate::app_settings::AppSettingsStore;
use crate::provider::ProviderGateway;
use crate::session::SessionStore;
use crate::storage::AppDatabase;

use super::shared::{extract_completion_text, resolve_aux_selection};

/// 从模型输出里解析快捷建议：截取第一个 `[`…最后一个 `]` 当作 JSON 字符串数组解析。
fn parse_suggestions(text: &str) -> Vec<String> {
    if let (Some(s), Some(e)) = (text.find('['), text.rfind(']')) {
        if e > s {
            if let Ok(list) = serde_json::from_str::<Vec<String>>(&text[s..=e]) {
                return list
                    .into_iter()
                    .map(|x| x.trim().chars().take(40).collect::<String>())
                    .filter(|x| !x.is_empty())
                    .take(4)
                    .collect();
            }
        }
    }
    Vec::new()
}

/// 基于对话片段，用大模型生成若干「下一步」快捷指令（一次性非流式）。失败返回空。
fn generate_suggestions(
    provider: &ProviderGateway,
    selection: Option<crate::provider::model::ResolvedModel>,
    session_id: &str,
    transcript: &str,
) -> Vec<String> {
    use crate::provider::client::{ModelCallRequest, ModelClient};
    use crate::provider::message::ModelMessage;
    let prompt = format!(
        "以下是用户与 AI 的对话。请站在用户角度，给出 3 条「下一步」可能的快捷指令：\
         每条不超过15个字、是用户会对 AI 说的祈使句、彼此不同且贴合当前进展。\
         只输出 JSON 字符串数组（如 [\"...\",\"...\",\"...\"]），不要任何解释。\n\n对话：\n{transcript}"
    );
    let mut request = ModelCallRequest {
        messages: vec![ModelMessage::user(prompt)],
        // 推理模型先耗 token 做思维链，太小会把答案截没；给足空间。
        max_output_tokens: Some(1024),
        timeout_ms: Some(30_000),
        stream: false,
        model_selection: selection.as_ref().map(|r| r.selection()),
        ..Default::default()
    };
    request.attribution.session_id = session_id.to_string();
    request.attribution.usage_type = Some("suggestion".to_string());
    match provider.complete_model(request) {
        Ok(result) => {
            let raw = extract_completion_text(&result);
            eprintln!(
                "[suggest] 模型原始输出 会话={session_id} finish={:?}：{raw}",
                result.finish_reason
            );
            parse_suggestions(&raw)
        }
        Err(err) => {
            eprintln!("[suggest] 模型调用失败 会话={session_id}：{}", err.message);
            Vec::new()
        }
    }
}

/// 后台线程：一轮正常结束后生成快捷建议并发事件（开关关闭则跳过）。
pub fn spawn(
    app: tauri::AppHandle,
    provider: Arc<ProviderGateway>,
    db: Arc<AppDatabase>,
    session_id: String,
) {
    std::thread::spawn(move || {
        let Ok(session) = SessionStore::open(db.clone()) else {
            eprintln!("[suggest] 打开会话存储失败 会话={session_id}");
            return;
        };
        let Ok(app_settings) = AppSettingsStore::open(db) else {
            eprintln!("[suggest] 打开配置存储失败 会话={session_id}");
            return;
        };
        let enabled = app_settings.get_suggestions_enabled().unwrap_or(true);
        eprintln!("[suggest] 开始 会话={session_id} enabled={enabled}");
        if !enabled {
            return;
        }
        let Ok(Some(detail)) = session.get_session_detail(&session_id) else {
            eprintln!("[suggest] 读会话详情失败 会话={session_id}");
            return;
        };
        // 取最近最多 6 条 user/assistant 消息构成片段（每条截断）。
        let mut lines: Vec<String> = Vec::new();
        for m in detail.messages.iter().rev() {
            if lines.len() >= 6 {
                break;
            }
            let role = match m.role.as_str() {
                "user" => "用户",
                "assistant" => "助手",
                _ => continue,
            };
            let content: String = m.content.chars().take(300).collect();
            if content.trim().is_empty() {
                continue;
            }
            lines.push(format!("{role}：{content}"));
        }
        lines.reverse();
        if lines.is_empty() {
            eprintln!("[suggest] 无可用对话片段 会话={session_id}");
            return;
        }
        let selection = resolve_aux_selection(&provider, &session, &app_settings, &session_id);
        let suggestions =
            generate_suggestions(&provider, selection, &session_id, &lines.join("\n"));
        eprintln!(
            "[suggest] 生成 {} 条 会话={session_id}：{:?}",
            suggestions.len(),
            suggestions
        );
        // 用户在生成期间主动停止（建议是一次性 LLM 调用、较慢）→ 丢弃，不落不发。
        if app
            .state::<crate::app_state::AppState>()
            .coordinator
            .cancel_flag(&session_id)
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            eprintln!("[suggest] 用户已停止，丢弃建议 会话={session_id}");
            return;
        }
        if !suggestions.is_empty() {
            // 持久化，供 reload/切会话回显（发新消息时清空）。
            let _ = session.set_last_suggestions(&session_id, &suggestions);
            let _ = app.emit(
                "session_suggestions",
                serde_json::json!({ "sessionId": session_id, "suggestions": suggestions }),
            );
        }
    });
}

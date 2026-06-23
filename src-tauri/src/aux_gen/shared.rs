//! aux_gen 共用：从补全结果取文本、解析辅助模型选择。

use crate::app_settings::AppSettingsStore;
use crate::provider::ProviderGateway;
use crate::session::SessionStore;

/// 从一次性补全结果里取最终助手文本。
pub fn extract_completion_text(result: &crate::provider::client::ModelCallResult) -> String {
    use crate::provider::client::ModelEvent;
    for e in result.events.iter().rev() {
        if let ModelEvent::AssistantMessageCompleted { content } = e {
            return content.clone();
        }
    }
    let mut s = String::new();
    for e in &result.events {
        if let ModelEvent::Delta { text } = e {
            s.push_str(text);
        }
    }
    s
}

/// 解析标题/建议生成所用模型：优先配置的辅助模型；否则回退会话所选模型（再否则全局默认）。
pub fn resolve_aux_selection(
    provider: &ProviderGateway,
    session: &SessionStore,
    app_settings: &AppSettingsStore,
    session_id: &str,
) -> Option<crate::provider::model::ResolvedModel> {
    if let Some(aux_id) = app_settings.get_aux_model_id().ok().flatten() {
        if let Ok(sel) = provider.resolve_selection(Some(&aux_id)) {
            return Some(sel);
        }
    }
    let selected = session.get_selected_model_id(session_id).ok().flatten();
    provider.resolve_selection(selected.as_deref()).ok()
}

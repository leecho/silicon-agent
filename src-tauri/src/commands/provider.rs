//! 厂商/模型配置命令（薄入口）。
use crate::app_state::{now_string, AppState};
use crate::provider::{
    EnabledProviderModels, ModelInput, ModelView, ProviderCheckResult, ProviderInput, ProviderView,
};
use tauri::State;

/// 列出全部厂商（去敏）。
#[tauri::command]
pub fn list_providers(services: State<'_, AppState>) -> Result<Vec<ProviderView>, String> {
    services.provider.list_providers()
}

/// 新增/更新厂商（含 apiKey：null 保持，空串清除，非空设置）。
#[tauri::command]
pub fn upsert_provider(
    services: State<'_, AppState>,
    input: ProviderInput,
) -> Result<ProviderView, String> {
    services.provider.upsert_provider(input, &now_string())
}

/// 删除厂商（级联删模型 + 密钥 + 清引用）。
#[tauri::command]
pub fn delete_provider(services: State<'_, AppState>, id: String) -> Result<(), String> {
    services.provider.delete_provider(&id)
}

/// 启用/停用厂商。
#[tauri::command]
pub fn set_provider_enabled(
    services: State<'_, AppState>,
    id: String,
    enabled: bool,
) -> Result<(), String> {
    services
        .provider
        .set_provider_enabled(&id, enabled, &now_string())
}

/// 连通测试：GET {base_url}/models，成功返回提示并记录 last_check。
#[tauri::command]
pub fn test_provider(
    services: State<'_, AppState>,
    id: String,
) -> Result<ProviderCheckResult, String> {
    services.provider.check_provider(&id, &now_string())
}

/// 自动拉取厂商可用模型名列表。
#[tauri::command]
pub fn fetch_provider_models(
    services: State<'_, AppState>,
    id: String,
) -> Result<Vec<String>, String> {
    services.provider.fetch_models(&id)
}

/// 列出某厂商下全部模型。
#[tauri::command]
pub fn list_provider_models(
    services: State<'_, AppState>,
    provider_id: String,
) -> Result<Vec<ModelView>, String> {
    services.provider.list_models(&provider_id)
}

/// 新增/更新模型。
#[tauri::command]
pub fn upsert_provider_model(
    services: State<'_, AppState>,
    input: ModelInput,
) -> Result<ModelView, String> {
    services.provider.upsert_model(input, &now_string())
}

/// 删除模型（清引用）。
#[tauri::command]
pub fn delete_provider_model(services: State<'_, AppState>, id: String) -> Result<(), String> {
    services.provider.delete_model(&id)
}

/// 启用/停用模型。
#[tauri::command]
pub fn set_model_enabled(
    services: State<'_, AppState>,
    id: String,
    enabled: bool,
) -> Result<(), String> {
    services
        .provider
        .set_model_enabled(&id, enabled, &now_string())
}

/// 设全局默认模型。
#[tauri::command]
pub fn set_default_model(services: State<'_, AppState>, id: String) -> Result<(), String> {
    services.provider.set_default_model(&id, &now_string())
}

/// 设/清全局 fallback 模型（model_id=null 清除）。
#[tauri::command]
pub fn set_fallback_model(
    services: State<'_, AppState>,
    model_id: Option<String>,
) -> Result<(), String> {
    services.provider.set_fallback_model(model_id.as_deref())
}

/// 读取全局 fallback 模型 id。
#[tauri::command]
pub fn get_fallback_model(services: State<'_, AppState>) -> Result<Option<String>, String> {
    services.provider.get_fallback_model_id()
}

/// Composer 用：启用厂商下的启用模型，按厂商分组。
#[tauri::command]
pub fn list_enabled_models(
    services: State<'_, AppState>,
) -> Result<Vec<EnabledProviderModels>, String> {
    services.provider.list_enabled_models()
}

/// 设置会话选中的模型（modelId=null 表示用全局默认）。
#[tauri::command]
pub fn set_session_model(
    services: State<'_, AppState>,
    session_id: String,
    model_id: Option<String>,
) -> Result<(), String> {
    services
        .session
        .set_selected_model_id(&session_id, model_id.as_deref(), &now_string())
}

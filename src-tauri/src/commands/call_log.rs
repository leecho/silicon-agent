//! 模型调用日志命令（薄入口）。
use crate::app_state::AppState;
use crate::call_log::{CallLogDetail, CallLogFilter, CallLogRow, CallLogStats};
use tauri::State;

#[tauri::command]
pub fn get_model_call_log_enabled(services: State<'_, AppState>) -> Result<bool, String> {
    services.app_settings.get_model_call_log_enabled()
}

#[tauri::command]
pub fn set_model_call_log_enabled(
    services: State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    services.app_settings.set_model_call_log_enabled(enabled)
}

#[tauri::command]
pub fn list_model_calls(
    services: State<'_, AppState>,
    filter: CallLogFilter,
) -> Result<Vec<CallLogRow>, String> {
    services.call_log.list(&filter)
}

#[tauri::command]
pub fn get_model_call(
    services: State<'_, AppState>,
    id: String,
) -> Result<Option<CallLogDetail>, String> {
    services.call_log.get(&id)
}

#[tauri::command]
pub fn clear_model_calls(
    services: State<'_, AppState>,
    filter: Option<CallLogFilter>,
) -> Result<usize, String> {
    services.call_log.clear(&filter.unwrap_or_default())
}

#[tauri::command]
pub fn get_model_call_log_stats(services: State<'_, AppState>) -> Result<CallLogStats, String> {
    services.call_log.stats()
}

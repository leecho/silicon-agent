//! 运行期设置与压缩/重试命令（薄入口）。
use crate::app_state::AppState;
use crate::session::Session;
use tauri::State;

/// 手动压缩较早的对话历史（`/compact`）。复用引擎压缩内核；旧消息 < 4 条则原样返回。
#[tauri::command]
pub async fn compact_session(
    services: State<'_, AppState>,
    session_id: String,
) -> Result<Session, String> {
    services
        .engine_builder
        .engine(&session_id)?
        .compact_context(&session_id)?;
    services
        .facade
        .session_with_pending(&session_id)?
        .ok_or_else(|| "session not found".into())
}

/// 自动压缩开关读取（缺省 = 开）。
#[tauri::command]
pub fn get_auto_compact_enabled(services: State<'_, AppState>) -> Result<bool, String> {
    services.app_settings.get_auto_compact_enabled()
}

/// 自动压缩开关设置。
#[tauri::command]
pub fn set_auto_compact_enabled(
    services: State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    services.app_settings.set_auto_compact_enabled(enabled)
}

/// 自动压缩阈值读取（缺省 90）。
#[tauri::command]
pub fn get_auto_compact_threshold_pct(services: State<'_, AppState>) -> Result<u32, String> {
    services.app_settings.get_auto_compact_threshold_pct()
}

/// 自动压缩阈值设置（clamp 50..=95）。
#[tauri::command]
pub fn set_auto_compact_threshold_pct(services: State<'_, AppState>, n: u32) -> Result<(), String> {
    services.app_settings.set_auto_compact_threshold_pct(n)
}

/// 已完成轮次的思考与执行过程展示开关读取（缺省 = 开）。
#[tauri::command]
pub fn get_show_completed_process(services: State<'_, AppState>) -> Result<bool, String> {
    services.app_settings.get_show_completed_process()
}

/// 已完成轮次的思考与执行过程展示开关设置。
#[tauri::command]
pub fn set_show_completed_process(
    services: State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    services.app_settings.set_show_completed_process(enabled)
}

/// SessionPage 是否默认显示任务面板读取（缺省 = 开）。
#[tauri::command]
pub fn get_session_task_panel_default_visible(
    services: State<'_, AppState>,
) -> Result<bool, String> {
    services
        .app_settings
        .get_session_task_panel_default_visible()
}

/// SessionPage 是否默认显示任务面板设置。
#[tauri::command]
pub fn set_session_task_panel_default_visible(
    services: State<'_, AppState>,
    visible: bool,
) -> Result<(), String> {
    services
        .app_settings
        .set_session_task_panel_default_visible(visible)
}

/// 读失败自动重试次数（缺省 3）。
#[tauri::command]
pub fn get_auto_retry_max(services: State<'_, AppState>) -> Result<u32, String> {
    services.app_settings.get_auto_retry_max()
}

/// 设失败自动重试次数（clamp 0..=5）。
#[tauri::command]
pub fn set_auto_retry_max(services: State<'_, AppState>, n: u32) -> Result<(), String> {
    services.app_settings.set_auto_retry_max(n)
}

/// 读单次任务最大模型迭代次数（缺省 24）。
#[tauri::command]
pub fn get_max_iterations(services: State<'_, AppState>) -> Result<u32, String> {
    services.app_settings.get_max_iterations()
}

/// 设单次任务最大模型迭代次数（clamp 1..=100）。
#[tauri::command]
pub fn set_max_iterations(services: State<'_, AppState>, n: u32) -> Result<(), String> {
    services.app_settings.set_max_iterations(n)
}

/// 读单工具执行超时秒数（缺省 30，clamp 1..=1800）。
#[tauri::command]
pub fn get_tool_timeout_secs(services: State<'_, AppState>) -> Result<u64, String> {
    services.app_settings.get_tool_timeout_secs()
}

/// 设单工具执行超时秒数（clamp 1..=1800）。
#[tauri::command]
pub fn set_tool_timeout_secs(services: State<'_, AppState>, n: u64) -> Result<(), String> {
    services.app_settings.set_tool_timeout_secs(n)
}

/// 读工具并行执行上限（缺省 8，clamp 1..=32；1=串行）。
#[tauri::command]
pub fn get_tool_parallelism(services: State<'_, AppState>) -> Result<u64, String> {
    services.app_settings.get_tool_parallelism()
}

/// 设工具并行执行上限（clamp 1..=32）。
#[tauri::command]
pub fn set_tool_parallelism(services: State<'_, AppState>, n: u64) -> Result<(), String> {
    services.app_settings.set_tool_parallelism(n)
}

/// 读子代理执行方式（parallel|serial，缺省 parallel）。
#[tauri::command]
pub fn get_subagent_execution_mode(services: State<'_, AppState>) -> Result<String, String> {
    services.app_settings.get_subagent_execution_mode()
}

/// 设子代理执行方式（parallel|serial）。
#[tauri::command]
pub fn set_subagent_execution_mode(
    services: State<'_, AppState>,
    mode: String,
) -> Result<(), String> {
    services.app_settings.set_subagent_execution_mode(&mode)
}

/// 手动重试上一轮失败的模型调用：把失败的 partial assistant 标 compacted=1（保留痕迹、
/// 排除出模型上下文），然后走与 submit 相同的 run 生命周期 resume 重跑。
#[tauri::command]
pub async fn retry_session(
    services: State<'_, AppState>,
    session_id: String,
) -> Result<Session, String> {
    let guard = services
        .coordinator
        .run_registry()
        .try_begin(&session_id)
        .ok_or_else(|| "该会话正在处理中，请稍候。".to_string())?;

    // 清理（保留痕迹）：若尾部是 error，且其前一条是 assistant（失败 partial），标 compacted=1。
    let msgs = services.session.list_messages(&session_id)?;
    if let Some(last) = msgs.last() {
        if last.role == "error" && msgs.len() >= 2 {
            let prev = &msgs[msgs.len() - 2];
            if prev.role == "assistant" {
                services
                    .session
                    .mark_compacted(&session_id, &[prev.id.clone()])?;
            }
        }
    }

    // 后台跑引擎（detached）：统一编排见 AppState::spawn_run。
    services.coordinator.spawn_run(&session_id, guard)?;

    services
        .facade
        .session_with_pending(&session_id)?
        .ok_or_else(|| "session not found".into())
}

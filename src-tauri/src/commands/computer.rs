//! 桌面操作（computer use）命令：会话启停（薄入口）。
//!
//! - 工具激活：`computer` 在「桌面操作」总开关开启的主会话中由引擎自动激活（见 engine run loop），
//!   无需独立启动命令；模型亦可经 find_tools 激活（同一写入路径 `session.activate_tools`）。
//! - 停止：复用既有全局停止命令 `stop_session`（置 run 级 cancel_flag），此处不新增 cancel 机制。
//! - 权限探测已迁移至通用权限模块 `commands::permission`（T89）。
use crate::app_state::AppState;
use tauri::State;

/// 桌面操作总开关读取（缺省 = 关）。
#[tauri::command]
pub fn get_computer_use_enabled(services: State<'_, AppState>) -> Result<bool, String> {
    services.app_settings.get_computer_use_enabled()
}

/// 桌面操作总开关设置。
#[tauri::command]
pub fn set_computer_use_enabled(
    services: State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    services.app_settings.set_computer_use_enabled(enabled)
}


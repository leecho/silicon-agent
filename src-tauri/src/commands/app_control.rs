//! 应用控制命令（重启等）。FDA 授权后需重启生效，提供一键重启入口。
use tauri::AppHandle;

/// 重启应用（完全磁盘等「需重启生效」权限授权后调用）。
/// `AppHandle::restart()` 不返回（直接退出并重启进程），无需 tauri-plugin-process。
#[tauri::command]
pub fn app_relaunch(app: AppHandle) {
    app.restart();
}

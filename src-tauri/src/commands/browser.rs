//! 浏览器操作（browser automation）命令：浏览器探测、会话启停（薄入口）。
//!
//! - 探测：检测本机是否装有 Chrome/Chromium/Edge，回 "ready" | "not_installed"。
//! - 工具激活：`browser` 在「浏览器操作」总开关开启的主会话中由引擎自动激活（见 engine run loop），
//!   无需独立启动命令；浏览器窗口在模型首个动作或用户点「打开浏览器」时懒启动。
//! - 停止：复用既有全局停止命令（置 run 级 cancel_flag），此处不新增 cancel 机制。
use crate::app_state::AppState;
use tauri::State;

/// 浏览器可用性状态："ready"（已装 Chrome）| "not_installed"。
#[tauri::command]
pub fn browser_status() -> String {
    match crate::browser::cdp::detect_status() {
        crate::browser::BrowserStatus::Ready | crate::browser::BrowserStatus::Running => {
            "ready".to_string()
        }
        crate::browser::BrowserStatus::NotInstalled => "not_installed".to_string(),
    }
}

/// 浏览器操作总开关读取（缺省 = 关）。
#[tauri::command]
pub fn get_browser_use_enabled(services: State<'_, AppState>) -> Result<bool, String> {
    services.app_settings.get_browser_use_enabled()
}

/// 浏览器操作总开关设置。
#[tauri::command]
pub fn set_browser_use_enabled(services: State<'_, AppState>, enabled: bool) -> Result<(), String> {
    services.app_settings.set_browser_use_enabled(enabled)
}

/// 无头模式开关读取（缺省 = 关）。
#[tauri::command]
pub fn get_browser_headless(services: State<'_, AppState>) -> Result<bool, String> {
    services.app_settings.get_browser_headless()
}

/// 无头模式开关设置。
#[tauri::command]
pub fn set_browser_headless(services: State<'_, AppState>, enabled: bool) -> Result<(), String> {
    services.app_settings.set_browser_headless(enabled)
}

/// 常驻浏览器当前是否开着（窗口/进程存活）。前端据此决定是否还提示登录/打开，
/// 避免在没有窗口时也画一个点了没用（no-op）的关闭按钮。
#[tauri::command]
pub fn browser_is_open(services: tauri::State<'_, crate::app_state::AppState>) -> Result<bool, String> {
    Ok(services.shared_browser.is_open())
}

/// 用户显式打开常驻浏览器窗口（供先行登录常用网站）：obtain 即触发懒启动开窗（非无头时弹出窗口）。
/// **async + spawn_blocking**：启动 Chrome 是阻塞操作（最长数秒），放同步命令会卡死 UI 主线程。
#[tauri::command]
pub async fn browser_open(services: tauri::State<'_, crate::app_state::AppState>) -> Result<(), String> {
    let sb = services.shared_browser.clone();
    tauri::async_runtime::spawn_blocking(move || sb.open().map_err(|e| e.to_string()))
        .await
        .map_err(|e| format!("打开浏览器失败：{e}"))?
}

/// 浏览器空闲多久（分钟）自动关闭常驻窗口；0 = 不自动关。默认 10。
#[tauri::command]
pub fn get_browser_idle_close_min(services: tauri::State<'_, crate::app_state::AppState>) -> Result<u64, String> {
    services.app_settings.get_browser_idle_close_min()
}

/// 设置浏览器空闲自动关闭时长（分钟）；0 = 不自动关。
#[tauri::command]
pub fn set_browser_idle_close_min(services: tauri::State<'_, crate::app_state::AppState>, min: u64) -> Result<(), String> {
    services.app_settings.set_browser_idle_close_min(min)
}

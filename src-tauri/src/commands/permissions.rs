//! 系统授权通用命令：字符串化 kind/state，前端注册表驱动（取代 computer_* 权限专用命令）。
use crate::permissions::{self, PermissionKind, PermissionRow};
use tauri::AppHandle;

fn parse_kind(kind: &str) -> Result<PermissionKind, String> {
    PermissionKind::from_str(kind).ok_or_else(|| format!("未知权限类型: {kind}"))
}

/// 一次拉全四类权限的状态 + 能力位，供集中面板渲染。
#[tauri::command]
pub fn permission_status_all(app: AppHandle) -> Vec<PermissionRow> {
    permissions::status_all(&app)
}

/// 查单类权限状态："granted" | "denied" | "unknown" | "unsupported"。
#[tauri::command]
pub fn permission_status(app: AppHandle, kind: String) -> Result<String, String> {
    let k = parse_kind(&kind)?;
    Ok(permissions::status(&app, &k).as_str().to_string())
}

/// 主动唤起授权（仅 can_request 的类型有效），返回请求后状态。
#[tauri::command]
pub fn permission_request(app: AppHandle, kind: String) -> Result<String, String> {
    let k = parse_kind(&kind)?;
    Ok(permissions::request(&app, &k).as_str().to_string())
}

/// 打开对应系统设置面板。
#[tauri::command]
pub fn permission_open_settings(kind: String) -> Result<(), String> {
    let k = parse_kind(&kind)?;
    permissions::open_settings(&k)
}

//! 非 macOS 平台：辅助功能/自动化/完全磁盘无 TCC 闸 → Granted；通知按需 Unknown。
use super::{PermissionKind, PermissionState};

pub fn status(kind: &PermissionKind) -> PermissionState {
    match kind {
        // 桌面操作在 Windows 走 UIA 无授权门，沿用 T84「恒 Granted」语义。
        PermissionKind::Accessibility | PermissionKind::Automation | PermissionKind::FullDisk => {
            PermissionState::Granted
        }
        // 日历/提醒是 macOS EventKit 专属；非 mac 无对应能力（T90）。
        PermissionKind::Calendars | PermissionKind::Reminders => PermissionState::Unsupported,
        PermissionKind::Notification => PermissionState::Unknown,
    }
}

pub fn request(kind: &PermissionKind) -> PermissionState {
    status(kind)
}

pub fn open_settings(_kind: &PermissionKind) -> Result<(), String> {
    Ok(())
}

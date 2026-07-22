//! 系统授权统一门面：provider 中立地查/唤起/跳转四类 macOS 系统权限。
//! 能力位（Capability）建模平台不对称（见 T89 spec §2）；集中面板与功能内即时授权共用本模块。

use tauri::AppHandle;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(target_os = "macos"))]
mod other;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum PermissionState {
    Granted,
    Denied,
    Unknown,
    Unsupported,
}

impl PermissionState {
    pub fn as_str(&self) -> &'static str {
        match self {
            PermissionState::Granted => "granted",
            PermissionState::Denied => "denied",
            PermissionState::Unknown => "unknown",
            PermissionState::Unsupported => "unsupported",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionKind {
    Accessibility,
    Notification,
    Automation,
    /// 日历（EventKit 独立 TCC 桶，非自动化；T90）。
    Calendars,
    /// 提醒事项（EventKit 独立 TCC 桶，非自动化；T90）。
    Reminders,
    FullDisk,
}

impl PermissionKind {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "accessibility" => Some(PermissionKind::Accessibility),
            "notification" => Some(PermissionKind::Notification),
            "automation" => Some(PermissionKind::Automation),
            "calendars" => Some(PermissionKind::Calendars),
            "reminders" => Some(PermissionKind::Reminders),
            "full_disk" => Some(PermissionKind::FullDisk),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            PermissionKind::Accessibility => "accessibility",
            PermissionKind::Notification => "notification",
            PermissionKind::Automation => "automation",
            PermissionKind::Calendars => "calendars",
            PermissionKind::Reminders => "reminders",
            PermissionKind::FullDisk => "full_disk",
        }
    }
}

/// 平台能力位：驱动 UI 按钮行为分支（见 spec §2）。
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct Capability {
    pub can_query: bool,
    pub can_request: bool,
    pub per_target: bool,
    pub needs_relaunch: bool,
}

pub fn capability(kind: &PermissionKind) -> Capability {
    match kind {
        PermissionKind::Accessibility => Capability { can_query: true, can_request: true, per_target: false, needs_relaunch: false },
        PermissionKind::Notification => Capability { can_query: true, can_request: true, per_target: false, needs_relaunch: false },
        PermissionKind::Automation => Capability { can_query: true, can_request: true, per_target: true, needs_relaunch: false },
        // EventKit：可查可请求（首次用触发原生弹窗）、单一开关、无需重启（T90）。
        PermissionKind::Calendars | PermissionKind::Reminders => Capability { can_query: true, can_request: true, per_target: false, needs_relaunch: false },
        PermissionKind::FullDisk => Capability { can_query: true, can_request: false, per_target: false, needs_relaunch: true },
    }
}

/// 面板一次拉全用的行数据（命令层序列化给前端）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct PermissionRow {
    pub kind: String,
    pub state: String,
    pub can_query: bool,
    pub can_request: bool,
    pub per_target: bool,
    pub needs_relaunch: bool,
}

/// 面板展示的权限，顺序即 UI 顺序（T90 加 Calendars/Reminders）。
pub const PANEL_KINDS: [PermissionKind; 6] = [
    PermissionKind::FullDisk,
    PermissionKind::Accessibility,
    PermissionKind::Automation,
    PermissionKind::Calendars,
    PermissionKind::Reminders,
    PermissionKind::Notification,
];

pub fn status(app: &AppHandle, kind: &PermissionKind) -> PermissionState {
    #[cfg(target_os = "macos")]
    {
        macos::status(app, kind)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        other::status(kind)
    }
}

pub fn request(app: &AppHandle, kind: &PermissionKind) -> PermissionState {
    #[cfg(target_os = "macos")]
    {
        macos::request(app, kind)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        other::request(kind)
    }
}

pub fn open_settings(kind: &PermissionKind) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        macos::open_settings(kind)
    }
    #[cfg(not(target_os = "macos"))]
    {
        other::open_settings(kind)
    }
}

pub fn status_all(app: &AppHandle) -> Vec<PermissionRow> {
    PANEL_KINDS
        .iter()
        .map(|kind| {
            let cap = capability(kind);
            PermissionRow {
                kind: kind.as_str().to_string(),
                state: status(app, kind).as_str().to_string(),
                can_query: cap.can_query,
                can_request: cap.can_request,
                per_target: cap.per_target,
                needs_relaunch: cap.needs_relaunch,
            }
        })
        .collect()
}

/// 把文件读取错误描述为用户友好文案。`PermissionDenied` 在 macOS 上引导去开「完全磁盘访问」；
/// 其余错误回退通用文案。供知识库摄取、文件工具等读取点统一使用，避免裸 OS 文案泄漏。
pub fn describe_read_error(e: &std::io::Error, path: &str) -> String {
    if e.kind() == std::io::ErrorKind::PermissionDenied {
        #[cfg(target_os = "macos")]
        {
            return format!(
                "读取被拒绝，可能需要「完全磁盘访问」权限：请在「系统设置 → 隐私与安全性 → 完全磁盘访问权限」中允许本应用，授权后重启应用。无法读取：{path}"
            );
        }
        #[cfg(not(target_os = "macos"))]
        {
            return format!("读取被拒绝（权限不足）：{path}（{e}）");
        }
    }
    format!("读取失败：{e}（{path}）")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_flags_per_kind() {
        let c = capability(&PermissionKind::Accessibility);
        assert_eq!((c.can_query, c.can_request, c.per_target, c.needs_relaunch), (true, true, false, false));
        let c = capability(&PermissionKind::Notification);
        assert_eq!((c.can_query, c.can_request), (true, true));
        let c = capability(&PermissionKind::Automation);
        assert!(c.per_target);
        let c = capability(&PermissionKind::FullDisk);
        assert_eq!((c.can_request, c.needs_relaunch), (false, true));
        // T90：EventKit 两类——可查可请求、非 per_target、无需重启。
        for k in [PermissionKind::Calendars, PermissionKind::Reminders] {
            let c = capability(&k);
            assert_eq!((c.can_query, c.can_request, c.per_target, c.needs_relaunch), (true, true, false, false));
        }
    }

    #[test]
    fn state_string_roundtrip() {
        assert_eq!(PermissionState::Granted.as_str(), "granted");
        assert_eq!(PermissionState::Denied.as_str(), "denied");
        assert_eq!(PermissionState::Unknown.as_str(), "unknown");
        assert_eq!(PermissionState::Unsupported.as_str(), "unsupported");
    }

    #[test]
    fn kind_parses_from_str() {
        assert_eq!(PermissionKind::from_str("accessibility"), Some(PermissionKind::Accessibility));
        assert_eq!(PermissionKind::from_str("full_disk"), Some(PermissionKind::FullDisk));
        assert_eq!(PermissionKind::from_str("calendars"), Some(PermissionKind::Calendars));
        assert_eq!(PermissionKind::from_str("reminders"), Some(PermissionKind::Reminders));
        assert_eq!(PermissionKind::from_str("nope"), None);
        // as_str / from_str 往返
        for k in PANEL_KINDS {
            assert_eq!(PermissionKind::from_str(k.as_str()), Some(k));
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn describe_read_error_maps_permission_denied() {
        use std::io::{Error, ErrorKind};
        let denied = Error::from(ErrorKind::PermissionDenied);
        let msg = describe_read_error(&denied, "/some/file.pdf");
        assert!(msg.contains("完全磁盘访问"), "got: {msg}");
        assert!(msg.contains("/some/file.pdf"), "got: {msg}");
        let nf = Error::from(ErrorKind::NotFound);
        let msg2 = describe_read_error(&nf, "/some/file.pdf");
        assert!(!msg2.contains("完全磁盘访问"), "got: {msg2}");
    }
}

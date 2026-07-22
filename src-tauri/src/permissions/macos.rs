//! macOS 各权限的真实实现。辅助功能复用 `desktop::macos`（不搬动）；通知走 notification 插件；
//! 自动化/完全磁盘 P1 仅渲染（status=Unknown + 只跳设置），真实探测留 P2/P3。

use super::{PermissionKind, PermissionState};
use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

// `tauri_plugin_notification` re-exports `tauri::plugin::PermissionState`.
// Variants: Granted, Denied, Prompt, PromptWithRationale.
use tauri_plugin_notification::PermissionState as PluginState;

/// 把「试读受保护路径」的 io 结果分类为 FDA 状态：
/// 读成功=已授权；`PermissionDenied`≈无 FDA=未授权；其它错误无法判定=Unknown。
fn classify_fda_read(res: &std::io::Result<()>) -> PermissionState {
    match res {
        Ok(()) => PermissionState::Granted,
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => PermissionState::Denied,
        Err(_) => PermissionState::Unknown,
    }
}

/// 启发式探测完全磁盘访问：macOS 无任何编程查询/唤起 API，只能试读受保护路径。
/// 依次尝试若干 TCC 保护路径，命中可判定（Granted/Denied）即返回；全 Unknown 才返回 Unknown。
fn fulldisk_status() -> PermissionState {
    let home = match std::env::var_os("HOME") {
        Some(h) => std::path::PathBuf::from(h),
        None => return PermissionState::Unknown,
    };
    let candidates = [
        home.join("Library/Application Support/com.apple.TCC/TCC.db"),
        home.join("Library/Safari"),
    ];
    for path in candidates {
        let res: std::io::Result<()> = if path.is_dir() {
            std::fs::read_dir(&path).map(|_| ())
        } else {
            std::fs::File::open(&path).map(|_| ())
        };
        match classify_fda_read(&res) {
            PermissionState::Unknown => continue,
            decided => return decided,
        }
    }
    PermissionState::Unknown
}

/// 现代系统设置（Ventura+/Tahoe）隐私面板锚点；旧标识符在 macOS 26 上 `open` 静默失败。
fn settings_url(kind: &PermissionKind) -> &'static str {
    match kind {
        PermissionKind::Accessibility => "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_Accessibility",
        PermissionKind::FullDisk => "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_AllFiles",
        PermissionKind::Automation => "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_Automation",
        PermissionKind::Calendars => "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_Calendars",
        PermissionKind::Reminders => "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_Reminders",
        PermissionKind::Notification => "x-apple.systempreferences:com.apple.Notifications-Settings.extension",
    }
}

// TODO(T89 P2/P3): tauri-plugin-notification v2.3.3 desktop 把 `permission_state()` /
// `request_permission()` 硬编码为 `Granted`（无真实 macOS UNUserNotificationCenter 探测），
// 故 macOS 上通知当前总是解析为 Granted；真实探测需原生 UNUserNotificationCenter 调用，留 P2/P3。
fn map_notif_state(s: PluginState) -> PermissionState {
    match s {
        PluginState::Granted => PermissionState::Granted,
        PluginState::Denied => PermissionState::Denied,
        // Prompt / PromptWithRationale => not yet determined
        _ => PermissionState::Unknown,
    }
}

pub fn status(app: &AppHandle, kind: &PermissionKind) -> PermissionState {
    match kind {
        PermissionKind::Accessibility => {
            use crate::desktop::macos::MacosController;
            use crate::desktop::{DesktopController, PermissionStatus};
            match MacosController::new().permission_status() {
                PermissionStatus::Granted => PermissionState::Granted,
                PermissionStatus::Denied => PermissionState::Denied,
                PermissionStatus::Unknown => PermissionState::Unknown,
            }
        }
        PermissionKind::Notification => app
            .notification()
            .permission_state()
            .map(map_notif_state)
            .unwrap_or(PermissionState::Unknown),
        // EventKit 日历 / 提醒：真实授权态查询（T90）。
        PermissionKind::Calendars => crate::apple::eventkit::calendars_state(),
        PermissionKind::Reminders => crate::apple::eventkit::reminders_state(),
        // 完全磁盘：启发式探测（无编程 API，P2）。
        PermissionKind::FullDisk => fulldisk_status(),
        // 自动化：检测本应用控制「备忘录」的 AppleEvents 授权（T90 后自动化桶只剩备忘录，P3）。
        PermissionKind::Automation => crate::apple::automation::notes_automation_state(),
    }
}

pub fn request(app: &AppHandle, kind: &PermissionKind) -> PermissionState {
    match kind {
        PermissionKind::Accessibility => {
            if crate::desktop::macos::request_accessibility_prompt() {
                PermissionState::Granted
            } else {
                PermissionState::Denied
            }
        }
        PermissionKind::Notification => app
            .notification()
            .request_permission()
            .map(map_notif_state)
            .unwrap_or(PermissionState::Unknown),
        // EventKit 日历 / 提醒：触发原生授权弹窗（首次/未决定态），随后回查状态（T90）。
        PermissionKind::Calendars => {
            crate::apple::eventkit::request_calendars();
            status(app, kind)
        }
        PermissionKind::Reminders => {
            crate::apple::eventkit::request_reminders();
            status(app, kind)
        }
        // 自动化：唤起原生 TCC 授权弹窗（未决定态弹「允许控制 备忘录」），随后回查（P3）。
        PermissionKind::Automation => crate::apple::automation::request_notes_automation(),
        // 完全磁盘不可编程唤起，退回当前状态（前端按 can_request=false 只跳设置）。
        PermissionKind::FullDisk => status(app, kind),
    }
}

pub fn open_settings(kind: &PermissionKind) -> Result<(), String> {
    let url = settings_url(kind);
    let ok = std::process::Command::new("open")
        .arg(url)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("打开系统设置失败: {e}"));
    if ok.is_err() {
        let _ = std::process::Command::new("open")
            .args(["-a", "System Settings"])
            .spawn();
    }
    ok
}

#[cfg(test)]
mod tests {
    use super::*;
    use tauri_plugin_notification::PermissionState as PluginState;

    #[test]
    fn maps_plugin_notification_state() {
        assert_eq!(map_notif_state(PluginState::Granted), PermissionState::Granted);
        assert_eq!(map_notif_state(PluginState::Denied), PermissionState::Denied);
        assert_eq!(map_notif_state(PluginState::Prompt), PermissionState::Unknown);
        assert_eq!(
            map_notif_state(PluginState::PromptWithRationale),
            PermissionState::Unknown
        );
    }

    #[test]
    fn settings_url_per_kind() {
        assert!(settings_url(&PermissionKind::Accessibility).contains("Privacy_Accessibility"));
        assert!(settings_url(&PermissionKind::FullDisk).contains("Privacy_AllFiles"));
        assert!(settings_url(&PermissionKind::Automation).contains("Privacy_Automation"));
        assert!(settings_url(&PermissionKind::Calendars).contains("Privacy_Calendars"));
        assert!(settings_url(&PermissionKind::Reminders).contains("Privacy_Reminders"));
        assert!(settings_url(&PermissionKind::Notification).contains("Notifications-Settings"));
    }

    #[test]
    fn classify_fda_probe_results() {
        use std::io::{Error, ErrorKind};
        use crate::permissions::PermissionState;
        assert_eq!(classify_fda_read(&Ok(())), PermissionState::Granted);
        let denied: std::io::Result<()> = Err(Error::from(ErrorKind::PermissionDenied));
        assert_eq!(classify_fda_read(&denied), PermissionState::Denied);
        let nf: std::io::Result<()> = Err(Error::from(ErrorKind::NotFound));
        assert_eq!(classify_fda_read(&nf), PermissionState::Unknown);
    }
}

//! Apple EventKit 接入：授权状态/请求 + 日历与提醒事项 CRUD 后端（trait + 真实 FFI + 内存 mock）。
//!
//! - `eventkit`：EKAuthorizationStatus → PermissionState 映射，以及同步触发原生授权弹窗。
//! - `calendar` / `reminders`：CRUD trait，真实 EventKit 实现（`Ek*`）与内存 mock（`Mock*`）。
//!
//! 仅在 macOS 编译真实 FFI；其它平台只暴露错误类型与 trait 定义所需的类型。

#[cfg(target_os = "macos")]
pub mod eventkit;
#[cfg(target_os = "macos")]
pub mod calendar;
#[cfg(target_os = "macos")]
pub mod reminders;
// 备忘录：无公开框架，走 osascript（AppleScript）子进程（T90）。
#[cfg(target_os = "macos")]
pub mod notes;
#[cfg(target_os = "macos")]
pub mod automation;
#[cfg(target_os = "macos")]
pub mod osascript;

/// Apple 后端统一错误。面向用户消息使用中文（命令层会序列化展示）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppleError {
    /// 未授权访问对应实体（需在系统设置授权）。
    PermissionDenied,
    /// 未找到条目（携带 id）。
    NotFound(String),
    /// 当前版本暂不支持的操作（如重复事件写入）。
    Unsupported(String),
    /// 底层 EventKit / 系统调用失败，携带原始信息。
    Backend(String),
}

impl std::fmt::Display for AppleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppleError::PermissionDenied => write!(f, "未授权访问（需在系统设置授权）"),
            AppleError::NotFound(id) => write!(f, "未找到条目：{id}"),
            AppleError::Unsupported(m) => write!(f, "暂不支持：{m}"),
            AppleError::Backend(m) => write!(f, "操作失败：{m}"),
        }
    }
}

impl std::error::Error for AppleError {}

/// `Retained<EKEventStore>` 默认非 `Send + Sync`（objc 对象的保守默认），但 Apple 文档明确
/// EKEventStore 是线程安全的、可跨线程使用（无需主线程）。这里用 newtype 断言 Send + Sync，
/// 让 `Ek*` 后端能满足 trait 的 `Send + Sync` 约束。
///
/// # Safety
/// 仅用于包裹 EKEventStore（及其按文档线程安全的派生调用）。EKEvent/EKReminder 等可变对象
/// 不在此处跨线程共享——它们都在单次方法调用内部创建并消费，不逃逸到其它线程。
#[cfg(target_os = "macos")]
pub(crate) struct SendStore(pub objc2::rc::Retained<objc2_event_kit::EKEventStore>);

#[cfg(target_os = "macos")]
unsafe impl Send for SendStore {}
#[cfg(target_os = "macos")]
unsafe impl Sync for SendStore {}

#[cfg(target_os = "macos")]
impl SendStore {
    pub(crate) fn new() -> Self {
        SendStore(unsafe { objc2_event_kit::EKEventStore::new() })
    }
}

#[cfg(target_os = "macos")]
impl std::ops::Deref for SendStore {
    type Target = objc2_event_kit::EKEventStore;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

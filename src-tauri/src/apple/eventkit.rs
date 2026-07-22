//! EventKit 授权：查询状态 + 同步触发原生授权弹窗。
//!
//! 被 `permissions/macos.rs` 调用（函数名保持稳定）。状态查询纯只读、可在测试中无 TCC 调用；
//! 请求函数会弹出系统授权框，仅在用户主动授权流程中调用，不在测试里触发。

use crate::permissions::PermissionState;

use objc2_event_kit::{EKAuthorizationStatus, EKEntityType, EKEventStore};

/// 把 EKAuthorizationStatus 映射到统一 PermissionState。
fn map_status(status: EKAuthorizationStatus) -> PermissionState {
    match status {
        // FullAccess(==Authorized) / WriteOnly 都视为已授权（写入足够）。
        EKAuthorizationStatus::FullAccess => PermissionState::Granted,
        EKAuthorizationStatus::WriteOnly => PermissionState::Granted,
        EKAuthorizationStatus::Denied => PermissionState::Denied,
        EKAuthorizationStatus::Restricted => PermissionState::Denied,
        EKAuthorizationStatus::NotDetermined => PermissionState::Unknown,
        _ => PermissionState::Unknown,
    }
}

fn status_for(entity: EKEntityType) -> PermissionState {
    // authorizationStatusForEntityType: 是类方法，纯查询，不触发弹窗。
    let status = unsafe { EKEventStore::authorizationStatusForEntityType(entity) };
    map_status(status)
}

/// 日历授权状态。
pub fn calendars_state() -> PermissionState {
    status_for(EKEntityType::Event)
}

/// 提醒事项授权状态。
pub fn reminders_state() -> PermissionState {
    status_for(EKEntityType::Reminder)
}

/// 通过 mpsc 把异步 completion block 桥接为同步调用：发起请求，等待回调里的 granted。
///
/// `which` 决定调用哪类请求方法（events / reminders）。macOS 14+ 优先用 full-access 方法；
/// 若运行时不可用（老系统），回退到已废弃的 requestAccessToEntityType:completion:。
fn request_access(entity: EKEntityType) -> bool {
    use block2::RcBlock;
    use objc2::runtime::{Bool, NSObjectProtocol};
    use objc2_foundation::NSError;
    use std::sync::mpsc;

    let store = unsafe { EKEventStore::new() };

    let (tx, rx) = mpsc::channel::<bool>();
    // completion block 签名：(Bool, *mut NSError)。只取 granted 标志回送。
    let block = RcBlock::new(move |granted: Bool, _err: *mut NSError| {
        // 通道可能已因超时丢弃，忽略 send 失败。
        let _ = tx.send(granted.as_bool());
    });

    // 选择请求方法。优先 macOS 14+ full-access；若 selector 不存在则回退旧 API。
    unsafe {
        let block_ptr = &*block as *const _ as *mut _;
        let has_full_access = match entity {
            EKEntityType::Reminder => store
                .respondsToSelector(objc2::sel!(requestFullAccessToRemindersWithCompletion:)),
            _ => store
                .respondsToSelector(objc2::sel!(requestFullAccessToEventsWithCompletion:)),
        };

        if has_full_access {
            match entity {
                EKEntityType::Reminder => {
                    store.requestFullAccessToRemindersWithCompletion(block_ptr)
                }
                _ => store.requestFullAccessToEventsWithCompletion(block_ptr),
            }
        } else {
            // 旧系统回退：requestAccessToEntityType:completion:（已废弃但仍可用）。
            #[allow(deprecated)]
            store.requestAccessToEntityType_completion(entity, block_ptr);
        }
    }

    // 等待回调，最多 60s；超时按未授权处理。
    rx.recv_timeout(std::time::Duration::from_secs(60)).unwrap_or(false)
}

/// 同步请求日历授权，返回是否已授权。会弹出系统授权框。
pub fn request_calendars() -> bool {
    request_access(EKEntityType::Event)
}

/// 同步请求提醒事项授权，返回是否已授权。会弹出系统授权框。
pub fn request_reminders() -> bool {
    request_access(EKEntityType::Reminder)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// FFI 绑定真实可用：状态查询不 panic，且返回合法枚举值。不触发授权弹窗。
    #[test]
    fn states_are_valid_variants() {
        for state in [calendars_state(), reminders_state()] {
            assert!(matches!(
                state,
                PermissionState::Granted
                    | PermissionState::Denied
                    | PermissionState::Unknown
                    | PermissionState::Unsupported
            ));
        }
    }

    #[test]
    fn map_status_covers_all() {
        assert_eq!(map_status(EKAuthorizationStatus::FullAccess), PermissionState::Granted);
        assert_eq!(map_status(EKAuthorizationStatus::WriteOnly), PermissionState::Granted);
        assert_eq!(map_status(EKAuthorizationStatus::Denied), PermissionState::Denied);
        assert_eq!(map_status(EKAuthorizationStatus::Restricted), PermissionState::Denied);
        assert_eq!(map_status(EKAuthorizationStatus::NotDetermined), PermissionState::Unknown);
    }
}

//! 自动化（AppleEvents）权限：检测/唤起本应用控制「备忘录」（com.apple.Notes）的 TCC 授权。
//! T90 回灌后自动化桶只剩备忘录（日历/提醒走 EventKit）。被 `permissions/macos.rs` 调用。
use crate::permissions::PermissionState;
use std::os::raw::c_void;

const TYPE_APPLICATION_BUNDLE_ID: u32 = 0x6275_6E64; // 'bund'
const TYPE_WILDCARD: u32 = 0x2A2A_2A2A; // '****'
const ERR_AE_EVENT_NOT_PERMITTED: i32 = -1743;
const ERR_AE_EVENT_WOULD_REQUIRE_USER_CONSENT: i32 = -1744;
const PROC_NOT_FOUND: i32 = -600;
const NOTES_BUNDLE_ID: &str = "com.apple.Notes";

/// FFI mirror of `struct AEDesc { DescType descriptorType; AEDataStorage dataHandle; }`.
/// DescType = FourCharCode = u32; AEDataStorage = opaque pointer (*mut c_void sufficient).
///
/// `AEDataModel.h` 整体被 `#pragma pack(push, 2)` 包裹：C 端 `AEDesc` 是 12 字节、
/// `dataHandle` 在偏移 4。默认 `#[repr(C)]` 会按 8 对齐成 16 字节（handle 落在偏移 8），
/// 与 `AECreateDesc`(写偏移 4)/`AEDeterminePermissionToAutomateTarget`(读偏移 8) 不一致 → UB。
/// 用 `packed(2)` 对齐 C 布局。我们只对整个结构取 `&target`/`&mut target`，不取字段引用，故 packed 安全。
#[repr(C, packed(2))]
struct AEDesc {
    descriptor_type: u32,
    data_handle: *mut c_void,
}

// CoreServices umbrella contains the AE framework symbols.
// Verified against MacOSX26.2.sdk AEDataModel.h / AppleEvents.h:
//   AECreateDesc(DescType, const void*, Size, AEDesc*) -> OSErr  (OSErr = SInt16 = i16)
//   AEDisposeDesc(AEDesc*) -> OSErr                              (OSErr = SInt16 = i16)
//   AEDeterminePermissionToAutomateTarget(const AEAddressDesc*, AEEventClass, AEEventID, Boolean) -> OSStatus
//   OSStatus = SInt32 = i32; Size = long = isize; Boolean = unsigned char = u8.
#[link(name = "CoreServices", kind = "framework")]
extern "C" {
    fn AECreateDesc(
        type_code: u32,
        data_ptr: *const c_void,
        data_size: isize,
        result: *mut AEDesc,
    ) -> i16;
    fn AEDisposeDesc(desc: *mut AEDesc) -> i16;
    fn AEDeterminePermissionToAutomateTarget(
        target: *const AEDesc,
        the_ae_event_class: u32,
        the_ae_event_id: u32,
        ask_user_if_needed: u8,
    ) -> i32;
}

fn classify_automation_status(os_status: i32) -> PermissionState {
    match os_status {
        0 => PermissionState::Granted,
        ERR_AE_EVENT_NOT_PERMITTED => PermissionState::Denied,
        ERR_AE_EVENT_WOULD_REQUIRE_USER_CONSENT => PermissionState::Unknown,
        PROC_NOT_FOUND => PermissionState::Unknown,
        _ => PermissionState::Unknown,
    }
}

fn probe_notes_automation(ask_user: bool) -> i32 {
    let bytes = NOTES_BUNDLE_ID.as_bytes();
    let mut target = AEDesc {
        descriptor_type: 0,
        data_handle: std::ptr::null_mut(),
    };
    // SAFETY: `bytes` 为有效切片，`AECreateDesc` 仅按 (ptr,len) 拷贝 bundle-id 字节到新描述符；`target` 已初始化、可写。
    let create = unsafe {
        AECreateDesc(
            TYPE_APPLICATION_BUNDLE_ID,
            bytes.as_ptr() as *const c_void,
            bytes.len() as isize,
            &mut target,
        )
    };
    if create != 0 {
        return PROC_NOT_FOUND;
    }
    // SAFETY: `create == 0` 保证 `target` 是 AECreateDesc 成功填好的有效地址描述符；纯查询，仅 ask_user 时弹系统框。
    let status = unsafe {
        AEDeterminePermissionToAutomateTarget(
            &target,
            TYPE_WILDCARD,
            TYPE_WILDCARD,
            if ask_user { 1 } else { 0 },
        )
    };
    // SAFETY: 释放本函数经 AECreateDesc 创建、尚未释放的描述符；之后不再使用 `target`。
    unsafe { AEDisposeDesc(&mut target) };
    status
}

/// 静默查询自动化（备忘录）授权态。
pub fn notes_automation_state() -> PermissionState {
    classify_automation_status(probe_notes_automation(false))
}

/// 唤起自动化（备忘录）授权：未决定态弹原生 TCC 框，返回随后的状态。
pub fn request_notes_automation() -> PermissionState {
    classify_automation_status(probe_notes_automation(true))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permissions::PermissionState;

    #[test]
    fn classify_automation_osstatus() {
        assert_eq!(classify_automation_status(0), PermissionState::Granted);
        assert_eq!(classify_automation_status(-1743), PermissionState::Denied);
        assert_eq!(classify_automation_status(-1744), PermissionState::Unknown);
        assert_eq!(classify_automation_status(-600), PermissionState::Unknown);
        assert_eq!(classify_automation_status(12345), PermissionState::Unknown);
    }

    /// `AEDataModel.h` 的 `#pragma pack(push, 2)` 让 C 端 `AEDesc` 为 12 字节；packed(2) 必须匹配。
    #[test]
    fn aedesc_matches_c_layout() {
        assert_eq!(std::mem::size_of::<AEDesc>(), 12);
    }
}

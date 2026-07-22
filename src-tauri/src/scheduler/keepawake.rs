//! 跨平台「保持系统唤醒」：持有 guard 即阻止系统休眠，drop 即释放。

/// macOS：spawn `caffeinate -i`，drop 时 kill。
#[cfg(target_os = "macos")]
pub struct KeepAwakeGuard {
    child: std::process::Child,
}

#[cfg(target_os = "macos")]
impl KeepAwakeGuard {
    pub fn acquire() -> Result<Self, String> {
        let child = std::process::Command::new("caffeinate")
            .arg("-i")
            .spawn()
            .map_err(|e| format!("启动 caffeinate 失败：{e}"))?;
        Ok(Self { child })
    }
}

#[cfg(target_os = "macos")]
impl Drop for KeepAwakeGuard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Windows：在长驻线程上调 SetThreadExecutionState(ES_CONTINUOUS|ES_SYSTEM_REQUIRED)；
/// drop 时通知线程退出，线程退出前复位 ES_CONTINUOUS。
#[cfg(target_os = "windows")]
pub struct KeepAwakeGuard {
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

#[cfg(target_os = "windows")]
impl KeepAwakeGuard {
    pub fn acquire() -> Result<Self, String> {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;
        use windows::Win32::System::Power::{
            SetThreadExecutionState, ES_CONTINUOUS, ES_SYSTEM_REQUIRED,
        };
        let stop = Arc::new(AtomicBool::new(false));
        let stop_t = stop.clone();
        let handle = std::thread::spawn(move || {
            // ES_CONTINUOUS は线程级状态：本线程持续持有直到退出。
            unsafe { SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED) };
            while !stop_t.load(Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
            // 复位：仅 ES_CONTINUOUS 清除 SYSTEM_REQUIRED。
            unsafe { SetThreadExecutionState(ES_CONTINUOUS) };
        });
        Ok(Self {
            stop,
            handle: Some(handle),
        })
    }
}

#[cfg(target_os = "windows")]
impl Drop for KeepAwakeGuard {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

/// 其它平台：no-op。
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub struct KeepAwakeGuard;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
impl KeepAwakeGuard {
    pub fn acquire() -> Result<Self, String> {
        Ok(Self)
    }
}

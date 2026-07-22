//! 测试用桌面控制器：记录动作、回放预设快照、可模拟权限缺失。
use std::sync::Mutex;
use super::*;

#[derive(Default)]
pub struct MockController {
    pub snapshots: Mutex<Vec<UiSnapshot>>, // 依次返回；耗尽则返回最后一个
    pub actions: Mutex<Vec<String>>,       // 记录动作日志，断言用
    pub permission: PermissionStatus,
}

impl MockController {
    pub fn with_snapshot(s: UiSnapshot) -> Self {
        MockController {
            snapshots: Mutex::new(vec![s]),
            actions: Mutex::new(Vec::new()),
            permission: PermissionStatus::Granted,
        }
    }
    pub fn log(&self) -> Vec<String> {
        self.actions.lock().unwrap().clone()
    }
}

impl DesktopController for MockController {
    fn snapshot_ui(&self) -> Result<UiSnapshot, DesktopError> {
        if self.permission != PermissionStatus::Granted {
            return Err(DesktopError::PermissionDenied);
        }
        let snaps = self.snapshots.lock().unwrap();
        snaps.last().cloned().ok_or(DesktopError::Backend("无快照".into()))
    }
    fn click(&self, target: ClickTarget, button: MouseButton) -> Result<(), DesktopError> {
        if self.permission != PermissionStatus::Granted {
            return Err(DesktopError::PermissionDenied);
        }
        self.actions.lock().unwrap().push(format!("click {target:?} {button:?}"));
        Ok(())
    }
    fn type_text(&self, text: &str) -> Result<(), DesktopError> {
        self.actions.lock().unwrap().push(format!("type {text:?}"));
        Ok(())
    }
    fn key(&self, combo: &KeyCombo) -> Result<(), DesktopError> {
        self.actions.lock().unwrap().push(format!("key {combo:?}"));
        Ok(())
    }
    fn scroll(&self, dx: i32, dy: i32) -> Result<(), DesktopError> {
        self.actions.lock().unwrap().push(format!("scroll {dx},{dy}"));
        Ok(())
    }
    fn permission_status(&self) -> PermissionStatus {
        self.permission.clone()
    }
}

#[cfg(test)]
mod mock_smoke {
    use super::*;
    #[test]
    fn records_click() {
        let m = MockController::with_snapshot(UiSnapshot { elements: vec![], truncated: false, coverage_hint: 1.0 });
        m.click(ClickTarget::Point { x: 1, y: 2 }, MouseButton::Left).unwrap();
        assert_eq!(m.log(), vec!["click Point { x: 1, y: 2 } Left"]);
    }
}

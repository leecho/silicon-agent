//! 测试用浏览器控制器：记录动作、回放预设快照、可模拟未安装。
use std::sync::Mutex;
use super::*;

pub struct MockController {
    pub snapshots: Mutex<Vec<DomSnapshot>>, // 依次返回；耗尽则返回最后一个
    pub actions: Mutex<Vec<String>>,        // 动作日志，断言用
    pub status: BrowserStatus,
    /// describe_submits 的返回（逃生舱测试用）。
    pub selector_submits: bool,
}

impl MockController {
    pub fn with_snapshot(s: DomSnapshot) -> Self {
        MockController {
            snapshots: Mutex::new(vec![s]),
            actions: Mutex::new(Vec::new()),
            status: BrowserStatus::Running,
            selector_submits: false,
        }
    }
    pub fn log(&self) -> Vec<String> {
        self.actions.lock().unwrap().clone()
    }
}

impl BrowserController for MockController {
    fn launch(&self, _opts: &LaunchOptions) -> Result<(), BrowserError> {
        if self.status == BrowserStatus::NotInstalled {
            return Err(BrowserError::NotInstalled);
        }
        self.actions.lock().unwrap().push("launch".into());
        Ok(())
    }
    fn navigate(&self, url: &str) -> Result<(), BrowserError> {
        self.actions.lock().unwrap().push(format!("navigate {url}"));
        Ok(())
    }
    fn snapshot(&self) -> Result<DomSnapshot, BrowserError> {
        let snaps = self.snapshots.lock().unwrap();
        snaps.last().cloned().ok_or(BrowserError::Backend("无快照".into()))
    }
    fn click(&self, target: &ElementTarget) -> Result<(), BrowserError> {
        self.actions.lock().unwrap().push(format!("click {target:?}"));
        Ok(())
    }
    fn fill(&self, target: &ElementTarget, text: &str) -> Result<(), BrowserError> {
        self.actions.lock().unwrap().push(format!("fill {target:?} {text:?}"));
        Ok(())
    }
    fn select(&self, target: &ElementTarget, value: &str) -> Result<(), BrowserError> {
        self.actions.lock().unwrap().push(format!("select {target:?} {value:?}"));
        Ok(())
    }
    fn scroll(&self, dx: i32, dy: i32) -> Result<(), BrowserError> {
        self.actions.lock().unwrap().push(format!("scroll {dx},{dy}"));
        Ok(())
    }
    fn extract(&self, query: &ExtractQuery) -> Result<String, BrowserError> {
        self.actions.lock().unwrap().push(format!("extract {query:?}"));
        Ok("（抽取结果）".into())
    }
    fn wait(&self, condition: &WaitCondition) -> Result<(), BrowserError> {
        self.actions.lock().unwrap().push(format!("wait {condition:?}"));
        Ok(())
    }
    fn back(&self) -> Result<(), BrowserError> {
        self.actions.lock().unwrap().push("back".into());
        Ok(())
    }
    fn tabs(&self) -> Result<Vec<TabInfo>, BrowserError> {
        self.actions.lock().unwrap().push("tabs".into());
        Ok(vec![TabInfo {
            index: 0,
            title: "T".into(),
            url: "https://e.com".into(),
            active: true,
        }])
    }
    fn switch_tab(&self, index: usize) -> Result<(), BrowserError> {
        self.actions.lock().unwrap().push(format!("switch_tab {index}"));
        Ok(())
    }
    fn close_tab(&self, index: usize) -> Result<(), BrowserError> {
        self.actions.lock().unwrap().push(format!("close_tab {index}"));
        Ok(())
    }
    fn describe_submits(&self, _selector: &str) -> Result<bool, BrowserError> {
        Ok(self.selector_submits)
    }
    fn status(&self) -> BrowserStatus {
        self.status.clone()
    }
}

#[cfg(test)]
mod mock_smoke {
    use super::*;
    fn empty_snap() -> DomSnapshot {
        DomSnapshot { url: "about:blank".into(), title: "".into(), elements: vec![], truncated: false, coverage_hint: 1.0 }
    }
    #[test]
    fn records_click() {
        let m = MockController::with_snapshot(empty_snap());
        m.click(&ElementTarget::Ref(3)).unwrap();
        assert_eq!(m.log(), vec!["click Ref(3)"]);
    }
}

//! browser 工具：把模型 action 翻译成 BrowserController 调用，返回文本结果。
//! 披露 Deferred，仅「浏览器会话」激活。高风险动作经 policy 判定，由引擎确认（见 engine）。

use std::sync::{Arc, Mutex};

use crate::browser::policy::{self, Decision};
use crate::browser::{BrowserController, DomSnapshot, ElementTarget, ExtractQuery, WaitCondition};
use crate::tools::{Disclosure, Tool};

pub const BROWSER_TOOL: &str = "browser";

pub struct Browser {
    backend: Arc<dyn BrowserController>,
    /// 当前会话工作区；下载落到 `<workspace>/downloads`（per-run，见 T85-P2 / T92-P1-T3）。
    workspace: std::path::PathBuf,
    /// 本工具实例所属逻辑会话 id（T92 P2-T2）：每个动作前 `bind_session`，
    /// 让常驻 Chrome 把动作路由到本会话**自己的** tab（跨会话顺序隔离）。
    session_id: String,
    /// 最近一次 observe 的快照，供 ref→元素解析与高风险判定。
    last_snapshot: Mutex<Option<DomSnapshot>>,
    actions_done: std::sync::atomic::AtomicUsize,
    /// 无进展检测：(上次快照文本哈希, 连续相同次数)。
    no_progress: Mutex<(u64, u32)>,
}

impl Browser {
    pub fn new(
        backend: Arc<dyn BrowserController>,
        workspace: std::path::PathBuf,
        session_id: String,
    ) -> Self {
        Browser {
            backend,
            workspace,
            session_id,
            last_snapshot: Mutex::new(None),
            actions_done: std::sync::atomic::AtomicUsize::new(0),
            no_progress: Mutex::new((0, 0)),
        }
    }

    fn observe(&self) -> String {
        match self.backend.snapshot() {
            Ok(snap) => {
                let text = snap.to_text();
                // 先更新快照（单独持锁，不与 no_progress 嵌套）。
                *self.last_snapshot.lock().unwrap() = Some(snap);
                // 无进展检测：连续相同快照文本达到阈值时追加软提示。
                use std::hash::{Hash, Hasher};
                let mut h = std::collections::hash_map::DefaultHasher::new();
                text.hash(&mut h);
                let hash = h.finish();
                let warn = {
                    let mut guard = self.no_progress.lock().unwrap();
                    let (last_hash, count) = &mut *guard;
                    if hash == *last_hash {
                        *count += 1;
                    } else {
                        *last_hash = hash;
                        *count = 0;
                    }
                    // 第 1 次 observe → count=0（不警告）
                    // 第 2 次相同   → count=1（不警告）
                    // 第 3 次相同   → count=2，2+1≥3 → 警告
                    *count + 1 >= NO_PROGRESS_THRESHOLD
                };
                if warn {
                    format!("{text}\n（页面连续多次无变化，可能卡住，请换策略或交还用户）")
                } else {
                    text
                }
            }
            Err(e) => format!("observe 失败: {e}"),
        }
    }

    /// 解析 ref / selector → ElementTarget。
    fn resolve_target(&self, args: &serde_json::Value) -> Result<ElementTarget, String> {
        if let Some(id) = args.get("ref").and_then(|v| v.as_u64()) {
            let guard = self.last_snapshot.lock().unwrap();
            let snap = guard.as_ref().ok_or_else(|| "请先 observe 再按元素操作".to_string())?;
            snap.resolve(id as u32).map_err(|e| e.to_string())?;
            Ok(ElementTarget::Ref(id as u32))
        } else if let Some(sel) = args.get("selector").and_then(|v| v.as_str()) {
            Ok(ElementTarget::Selector(sel.to_string()))
        } else {
            Err("需要 ref 或 selector".into())
        }
    }

    /// 目标是否为提交控件：ref 走快照、selector 走后端探测。
    fn target_submits(&self, args: &serde_json::Value) -> Option<bool> {
        if let Some(id) = args.get("ref").and_then(|v| v.as_u64()) {
            let guard = self.last_snapshot.lock().unwrap();
            let snap = guard.as_ref()?;
            snap.resolve(id as u32).ok().map(|e| e.submits)
        } else if let Some(sel) = args.get("selector").and_then(|v| v.as_str()) {
            self.backend.describe_submits(sel).ok()
        } else {
            None
        }
    }
}

impl Tool for Browser {
    fn name(&self) -> &str { BROWSER_TOOL }
    fn label(&self) -> &str { "操作浏览器" }
    fn disclosure(&self) -> Disclosure { Disclosure::Deferred }
    /// 高风险结构化判定（提交类动作 = submit 控件）→ `Low` 风险，从而**跟随权限模式**确认：
    /// 自动审批/full 放行、手动模式首次确认（统一走 T90 `risk_for` + `needs_confirmation`）。
    /// 不再用独立的 `confirm_for` 门——后者无视权限模式（自动审批也强行暂停），且与 reconcile/
    /// `pending_interaction`（按 `risk_for` 重建）口径不一致，会留下「看不见又清不掉」的卡死队头。
    fn risk_for(&self, args: &serde_json::Value) -> crate::tools::RiskLevel {
        let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("");
        let submits = self.target_submits(args).unwrap_or(false);
        if policy::evaluate(action, submits) == Decision::Confirm {
            crate::tools::RiskLevel::Low
        } else {
            crate::tools::RiskLevel::Safe
        }
    }
    fn description(&self) -> &str {
        "操作浏览器完成网页任务：navigate 打开网址，observe 读取当前页面的可交互元素（带编号），\
         再用 click/fill/select/scroll 按编号或 CSS 选择器操作，extract 抽取内容。\
         每步操作后页面可能变化，行动前先 observe。提交类操作会请你确认。"
    }
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": { "type": "string",
                    "enum": ["navigate","observe","click","double_click","fill","select","scroll","extract","wait","back","tabs","switch_tab","close_tab","close"] },
                "url": { "type": "string", "description": "navigate 目标网址" },
                "ref": { "type": "integer", "description": "目标元素编号（来自最近一次 observe）" },
                "index": { "type": "integer", "description": "switch_tab/close_tab 的标签序号（来自 tabs）" },
                "selector": { "type": "string", "description": "CSS 选择器（编号无法覆盖时的逃生舱）" },
                "text": { "type": "string", "description": "fill 要输入的文本" },
                "value": { "type": "string", "description": "select 要选的值" },
                "dx": { "type": "integer" }, "dy": { "type": "integer" },
                "ms": { "type": "integer", "description": "wait 毫秒" }
            },
            "required": ["action"]
        })
    }
    fn execute(&self, args: &serde_json::Value) -> Result<String, String> {
        use std::sync::atomic::Ordering;
        let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("");

        // 绑定本会话的 tab（T92 P2-T2）：除 close 外，所有动作（含 observe/tabs）操作前
        // 都先把常驻 Chrome 路由到本会话自己的 tab，避免跨会话覆写导航。最佳努力/幂等。
        if action != "close" {
            self.backend.bind_session(&self.session_id);
        }

        // observe 无副作用、不计入失控计数。
        if action == "observe" {
            return Ok(self.observe());
        }

        // close：显式关闭浏览器（read-ish，不计入失控计数）。
        if action == "close" {
            self.backend.close();
            return Ok("已关闭浏览器".into());
        }

        // tabs 只读、不计入失控计数（同 observe）。
        if action == "tabs" {
            return Ok(match self.backend.tabs() {
                Ok(list) => {
                    let mut out = String::from("当前标签：\n");
                    for t in &list {
                        let star = if t.active { "*" } else { " " };
                        out.push_str(&format!("[{}]{} {} — {}\n", t.index, star, t.title, t.url));
                    }
                    out
                }
                Err(e) => format!("读取标签失败: {e}"),
            });
        }

        // 失控兜底：非 observe 动作计数。
        let n = self.actions_done.fetch_add(1, Ordering::SeqCst);
        if n >= MAX_ACTIONS {
            return Ok("操作次数已达上限，已停止，请人工接管或新开浏览器会话".into());
        }

        // per-run 下载目录：确保下载落到本会话 workspace（T85-P2）。
        // 幂等/低成本；最佳努力（mock/desktop 默认 no-op）。
        self.backend
            .set_download_dir(self.workspace.join("downloads"));

        let result: Result<String, String> = match action {
            "navigate" => {
                match args.get("url").and_then(|v| v.as_str()) {
                    None => Err("navigate 需要 url".into()),
                    Some(url) => self.backend.navigate(url).map(|_| format!("已打开 {url}")).map_err(|e| e.to_string()),
                }
            }
            "click" => {
                match self.resolve_target(args) {
                    Err(e) => Err(e),
                    Ok(target) => self.backend.click(&target).map(|_| "已点击".to_string()).map_err(|e| e.to_string()),
                }
            }
            "double_click" => {
                match self.resolve_target(args) {
                    Err(e) => Err(e),
                    Ok(target) => self.backend.double_click(&target).map(|_| "已双击".to_string()).map_err(|e| e.to_string()),
                }
            }
            "fill" => {
                match self.resolve_target(args) {
                    Err(e) => Err(e),
                    Ok(target) => match args.get("text").and_then(|v| v.as_str()) {
                        None => Err("fill 需要 text".into()),
                        Some(text) => self.backend.fill(&target, text).map(|_| format!("已输入 {text:?}")).map_err(|e| e.to_string()),
                    },
                }
            }
            "select" => {
                match self.resolve_target(args) {
                    Err(e) => Err(e),
                    Ok(target) => match args.get("value").and_then(|v| v.as_str()) {
                        None => Err("select 需要 value".into()),
                        Some(value) => self.backend.select(&target, value).map(|_| format!("已选择 {value:?}")).map_err(|e| e.to_string()),
                    },
                }
            }
            "scroll" => {
                let dx = args.get("dx").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                let dy = args.get("dy").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                self.backend.scroll(dx, dy).map(|_| "已滚动".to_string()).map_err(|e| e.to_string())
            }
            "extract" => {
                let selector = args.get("selector").and_then(|v| v.as_str()).map(str::to_string);
                self.backend.extract(&ExtractQuery { selector }).map_err(|e| e.to_string())
            }
            "wait" => {
                let cond = if let Some(sel) = args.get("selector").and_then(|v| v.as_str()) {
                    WaitCondition::Selector(sel.to_string())
                } else {
                    let ms = args.get("ms").and_then(|v| v.as_u64()).unwrap_or(500).min(10_000);
                    WaitCondition::Millis(ms)
                };
                self.backend.wait(&cond).map(|_| "已等待".to_string()).map_err(|e| e.to_string())
            }
            "back" => self.backend.back().map(|_| "已后退".to_string()).map_err(|e| e.to_string()),
            "switch_tab" => {
                match args.get("index").and_then(|v| v.as_u64()) {
                    None => Err("switch_tab 需要 index".into()),
                    Some(i) => self.backend.switch_tab(i as usize).map(|_| format!("已切换到标签 {i}")).map_err(|e| e.to_string()),
                }
            }
            "close_tab" => {
                match args.get("index").and_then(|v| v.as_u64()) {
                    None => Err("close_tab 需要 index".into()),
                    Some(i) => self.backend.close_tab(i as usize).map(|_| format!("已关闭标签 {i}")).map_err(|e| e.to_string()),
                }
            }
            other => return Ok(format!("未知 action: {other}")),
        };

        Ok(match result {
            Ok(s) => format!("{s}（如页面已变化，请 observe 后再操作）"),
            Err(e) => format!("操作失败: {e}"),
        })
    }
}

/// 单次浏览器会话动作上限（失控兜底）。
pub const MAX_ACTIONS: usize = 80;
/// 连续相同 observe 快照次数达到此值时追加软警告。
const NO_PROGRESS_THRESHOLD: u32 = 3;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser::mock::MockController;
    use crate::browser::{DomElement, DomSnapshot};
    use std::sync::Arc;

    /// 测试用构造：注入后端 + 临时 workspace（下载目录无关测试断言）。
    fn mk(backend: Arc<dyn BrowserController>) -> Browser {
        Browser::new(backend, std::env::temp_dir(), "test".to_string())
    }

    fn snap() -> DomSnapshot {
        DomSnapshot {
            url: "https://e.com".into(),
            title: "T".into(),
            elements: vec![
                DomElement { id: 12, role: "button".into(), name: "登录".into(), value: None, selector: "#a".into(), submits: true },
                DomElement { id: 13, role: "textbox".into(), name: "邮箱".into(), value: Some(String::new()), selector: "#b".into(), submits: false },
            ],
            truncated: false,
            coverage_hint: 1.0,
        }
    }

    #[test]
    fn observe_returns_text_tree() {
        let b = mk(Arc::new(MockController::with_snapshot(snap())));
        let out = b.execute(&serde_json::json!({"action": "observe"})).unwrap();
        assert!(out.contains("[12] button \"登录\""));
    }

    #[test]
    fn click_ref_translates_to_controller() {
        let mock = Arc::new(MockController::with_snapshot(snap()));
        let b = mk(mock.clone());
        b.execute(&serde_json::json!({"action": "observe"})).unwrap();
        b.execute(&serde_json::json!({"action": "click", "ref": 13})).unwrap();
        assert!(mock.log().iter().any(|l| l.contains("click Ref(13)")));
    }

    #[test]
    fn click_selector_escape_hatch() {
        let mock = Arc::new(MockController::with_snapshot(snap()));
        let b = mk(mock.clone());
        b.execute(&serde_json::json!({"action": "click", "selector": ".x"})).unwrap();
        assert!(mock.log().iter().any(|l| l.contains("click Selector(\".x\")")));
    }

    #[test]
    fn navigate_and_fill_translate() {
        let mock = Arc::new(MockController::with_snapshot(snap()));
        let b = mk(mock.clone());
        b.execute(&serde_json::json!({"action": "navigate", "url": "https://e.com"})).unwrap();
        b.execute(&serde_json::json!({"action": "observe"})).unwrap();
        b.execute(&serde_json::json!({"action": "fill", "ref": 13, "text": "a@b.c"})).unwrap();
        let log = mock.log();
        assert!(log.iter().any(|l| l.contains("navigate https://e.com")));
        assert!(log.iter().any(|l| l.contains("fill Ref(13) \"a@b.c\"")));
    }

    #[test]
    fn click_unknown_ref_returns_error_text() {
        let b = mk(Arc::new(MockController::with_snapshot(snap())));
        b.execute(&serde_json::json!({"action": "observe"})).unwrap();
        let out = b.execute(&serde_json::json!({"action": "click", "ref": 999})).unwrap();
        assert!(out.contains("不存在") || out.contains("observe"));
    }

    #[test]
    fn unknown_action_is_error_text_not_panic() {
        let b = mk(Arc::new(MockController::with_snapshot(snap())));
        let out = b.execute(&serde_json::json!({"action": "frobnicate"})).unwrap();
        assert!(out.contains("未知 action"));
    }

    #[test]
    fn switch_tab_translates_to_controller() {
        let mock = Arc::new(MockController::with_snapshot(snap()));
        let b = mk(mock.clone());
        let out = b.execute(&serde_json::json!({"action": "switch_tab", "index": 1})).unwrap();
        assert!(mock.log().iter().any(|l| l == "switch_tab 1"));
        assert!(out.contains("已切换到标签 1"));
    }

    #[test]
    fn close_tab_translates_to_controller() {
        let mock = Arc::new(MockController::with_snapshot(snap()));
        let b = mk(mock.clone());
        let out = b.execute(&serde_json::json!({"action": "close_tab", "index": 0})).unwrap();
        assert!(mock.log().iter().any(|l| l == "close_tab 0"));
        assert!(out.contains("已关闭标签 0"));
    }

    #[test]
    fn tabs_returns_text_list() {
        let mock = Arc::new(MockController::with_snapshot(snap()));
        let b = mk(mock.clone());
        let out = b.execute(&serde_json::json!({"action": "tabs"})).unwrap();
        assert!(out.contains("T"));
        assert!(out.contains("https://e.com"));
        assert!(mock.log().iter().any(|l| l == "tabs"));
    }

    #[test]
    fn is_deferred() {
        let b = mk(Arc::new(MockController::with_snapshot(snap())));
        assert_eq!(b.disclosure(), crate::tools::Disclosure::Deferred);
    }

    #[test]
    fn high_risk_ref_click_reports_confirm_needed() {
        use crate::tools::RiskLevel;
        let b = mk(Arc::new(MockController::with_snapshot(snap())));
        b.execute(&serde_json::json!({"action": "observe"})).unwrap();
        // ref 12 = submit 控件 → Low（跟随权限模式确认）；ref 13 = 普通输入 → Safe（放行）。
        assert_eq!(b.risk_for(&serde_json::json!({"action": "click", "ref": 12})), RiskLevel::Low);
        assert_eq!(b.risk_for(&serde_json::json!({"action": "click", "ref": 13})), RiskLevel::Safe);
    }

    #[test]
    fn high_risk_selector_click_uses_backend_describe_submits() {
        use crate::tools::RiskLevel;
        // 逃生舱（CSS 选择器）路径：高风险由 backend.describe_submits 探测决定。
        let mut m = MockController::with_snapshot(snap());
        m.selector_submits = true;
        let b = mk(Arc::new(m));
        assert_eq!(b.risk_for(&serde_json::json!({"action": "click", "selector": ".pay"})), RiskLevel::Low);

        // 默认 selector_submits=false：非提交控件 → Safe（放行）。
        let b2 = mk(Arc::new(MockController::with_snapshot(snap())));
        assert_eq!(b2.risk_for(&serde_json::json!({"action": "click", "selector": ".link"})), RiskLevel::Safe);
    }

    #[test]
    fn repeated_identical_observe_warns_no_progress() {
        let b = mk(Arc::new(MockController::with_snapshot(snap())));
        // 前两次 observe 不应警告；第三次起出现「无变化/卡住」提示。
        let first = b.execute(&serde_json::json!({"action":"observe"})).unwrap();
        assert!(!first.contains("无变化") && !first.contains("卡住"));
        let _second = b.execute(&serde_json::json!({"action":"observe"})).unwrap();
        let third = b.execute(&serde_json::json!({"action":"observe"})).unwrap();
        assert!(third.contains("无变化") || third.contains("卡住"));
    }
}

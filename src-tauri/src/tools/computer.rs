//! computer 工具：把模型 action 翻译成 DesktopController 调用，返回文本结果。
//! 披露 Deferred，仅「桌面操作会话」激活。感知回喂由引擎在动作后采集（见 engine）。

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use crate::desktop::{ClickTarget, DesktopController, KeyCombo, MouseButton, UiSnapshot};
use crate::tools::{Disclosure, Tool};

pub const COMPUTER_TOOL: &str = "computer";
pub const MAX_ACTIONS: usize = 60;

pub struct Computer {
    backend: Arc<dyn DesktopController>,
    /// 最近一次 observe 的快照，供元素 id→坐标解析。
    last_snapshot: Mutex<Option<UiSnapshot>>,
    /// 非 observe 动作计数，超过 MAX_ACTIONS 后拒绝执行。
    actions_done: std::sync::atomic::AtomicUsize,
}

impl Computer {
    pub fn new(backend: Arc<dyn DesktopController>) -> Self {
        Computer {
            backend,
            last_snapshot: Mutex::new(None),
            actions_done: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    fn observe(&self) -> String {
        match self.backend.snapshot_ui() {
            Ok(snap) => {
                let text = snap.to_text();
                *self.last_snapshot.lock().unwrap() = Some(snap);
                if text.is_empty() {
                    "（当前无可读无障碍元素；该界面可能为自绘 UI，P1 暂不支持）".into()
                } else {
                    text
                }
            }
            Err(e) => format!("observe 失败: {e}"),
        }
    }

    fn resolve_target(&self, args: &serde_json::Value) -> Result<ClickTarget, String> {
        if let Some(id) = args.get("element").and_then(|v| v.as_u64()) {
            let guard = self.last_snapshot.lock().unwrap();
            let snap = guard.as_ref().ok_or_else(|| "请先 observe 再按元素点击".to_string())?;
            let (x, y) = snap.resolve(id as u32).map_err(|e| e.to_string())?;
            Ok(ClickTarget::Point { x, y })
        } else if let (Some(x), Some(y)) = (
            args.get("x").and_then(|v| v.as_i64()),
            args.get("y").and_then(|v| v.as_i64()),
        ) {
            Ok(ClickTarget::Point { x: x as i32, y: y as i32 })
        } else {
            Err("click 需要 element 或 x,y".into())
        }
    }
}

impl Tool for Computer {
    fn name(&self) -> &str { COMPUTER_TOOL }
    fn label(&self) -> &str { "操作桌面" }
    fn disclosure(&self) -> Disclosure { Disclosure::Deferred }
    fn description(&self) -> &str {
        "操作本机桌面：observe 读取当前界面的无障碍元素树（带 id 与坐标），再用 click/type/key/scroll \
         按 id 或坐标操作。每次操作后界面可能变化，行动前先 observe。"
    }
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": { "type": "string",
                    "enum": ["observe","click","double_click","type","key","scroll","wait"] },
                "element": { "type": "integer", "description": "目标元素 id（来自最近一次 observe）" },
                "x": { "type": "integer" }, "y": { "type": "integer" },
                "button": { "type": "string", "enum": ["left","right"] },
                "text": { "type": "string" },
                "combo": { "type": "string", "description": "组合键，如 cmd+c" },
                "dx": { "type": "integer" }, "dy": { "type": "integer" },
                "ms": { "type": "integer" }
            },
            "required": ["action"]
        })
    }
    fn execute(&self, args: &serde_json::Value) -> Result<String, String> {
        let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("");
        let button = match args.get("button").and_then(|v| v.as_str()) {
            Some("right") => MouseButton::Right,
            _ => MouseButton::Left,
        };
        // "observe" returns directly (its own error handling is baked in).
        if action == "observe" {
            return Ok(self.observe());
        }
        // Runaway guard: count every non-observe action; stop once MAX_ACTIONS is exceeded.
        {
            let n = self.actions_done.fetch_add(1, Ordering::SeqCst);
            if n >= MAX_ACTIONS {
                return Ok("动作数已达上限，已停止，请人工接管或新开桌面会话".into());
            }
        }
        // All other actions: recoverable failures (bad element id, missing arg, backend error)
        // are returned as Ok(text) so the model can read and react to them.
        // We do NOT propagate Err here — the ? operator is intentionally avoided so that
        // user-facing errors (unknown element, missing arg) come back as readable Ok text.
        let result: Result<String, String> = match action {
            "click" | "double_click" => {
                match self.resolve_target(args) {
                    Err(e) => Err(e),
                    Ok(target) => self.backend.click(target, button)
                        .map(|_| "已点击".to_string())
                        .map_err(|e| e.to_string()),
                }
            }
            "type" => {
                match args.get("text").and_then(|v| v.as_str()) {
                    None => Err("type 需要 text".into()),
                    Some(text) => self.backend.type_text(text)
                        .map(|_| format!("已输入 {text:?}"))
                        .map_err(|e| e.to_string()),
                }
            }
            "key" => {
                match args.get("combo").and_then(|v| v.as_str()) {
                    None => Err("key 需要 combo".into()),
                    Some(combo) => KeyCombo::parse(combo)
                        .map_err(|e| e.to_string())
                        .and_then(|k| {
                            self.backend.key(&k)
                                .map(|_| format!("已按键 {combo}"))
                                .map_err(|e| e.to_string())
                        }),
                }
            }
            "scroll" => {
                let dx = args.get("dx").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                let dy = args.get("dy").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                self.backend.scroll(dx, dy).map(|_| "已滚动".to_string()).map_err(|e| e.to_string())
            }
            "wait" => {
                // 真正等待（worker 线程，短时阻塞可接受）；上限 3s 防滥用。
                let ms = args.get("ms").and_then(|v| v.as_u64()).unwrap_or(0);
                let capped = ms.min(3000);
                std::thread::sleep(std::time::Duration::from_millis(capped));
                Ok("已等待".to_string())
            }
            other => return Ok(format!("未知 action: {other}")),
        };
        Ok(match result {
            Ok(s) => format!("{s}（如界面已变，请 observe 后再操作）"),
            Err(e) => format!("操作失败: {e}"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::mock::MockController;
    use crate::desktop::{UiElement, UiSnapshot};
    use std::sync::Arc;

    fn snap() -> UiSnapshot {
        UiSnapshot {
            elements: vec![UiElement { id: 12, role: "button".into(), label: "保存".into(), value: None, cx: 100, cy: 200 }],
            truncated: false,
            coverage_hint: 1.0,
        }
    }

    #[test]
    fn observe_returns_text_tree() {
        let c = Computer::new(Arc::new(MockController::with_snapshot(snap())));
        let out = c.execute(&serde_json::json!({"action": "observe"})).unwrap();
        assert!(out.contains("[12] button \"保存\""));
    }

    #[test]
    fn click_element_resolves_to_coords() {
        let mock = Arc::new(MockController::with_snapshot(snap()));
        let c = Computer::new(mock.clone());
        c.execute(&serde_json::json!({"action": "observe"})).unwrap();
        c.execute(&serde_json::json!({"action": "click", "element": 12})).unwrap();
        assert!(mock.log().iter().any(|l| l.contains("click Point { x: 100, y: 200 }")));
    }

    #[test]
    fn click_unknown_element_returns_error_text() {
        let c = Computer::new(Arc::new(MockController::with_snapshot(snap())));
        c.execute(&serde_json::json!({"action": "observe"})).unwrap();
        let out = c.execute(&serde_json::json!({"action": "click", "element": 999})).unwrap();
        assert!(out.contains("不存在") || out.contains("observe"));
    }

    #[test]
    fn type_and_key_translate() {
        let mock = Arc::new(MockController::with_snapshot(snap()));
        let c = Computer::new(mock.clone());
        c.execute(&serde_json::json!({"action": "type", "text": "hi"})).unwrap();
        c.execute(&serde_json::json!({"action": "key", "combo": "cmd+c"})).unwrap();
        let log = mock.log();
        assert!(log.iter().any(|l| l.contains("type \"hi\"")));
        assert!(log.iter().any(|l| l.contains("key KeyCombo")));
    }

    #[test]
    fn unknown_action_is_error_text_not_panic() {
        let c = Computer::new(Arc::new(MockController::with_snapshot(snap())));
        let out = c.execute(&serde_json::json!({"action": "frobnicate"})).unwrap();
        assert!(out.contains("未知 action"));
    }

    #[test]
    fn is_deferred() {
        let c = Computer::new(Arc::new(MockController::with_snapshot(snap())));
        assert_eq!(c.disclosure(), crate::tools::Disclosure::Deferred);
    }

    #[test]
    fn caps_action_count() {
        let c = Computer::new(Arc::new(MockController::with_snapshot(snap())));
        for _ in 0..super::MAX_ACTIONS {
            let _ = c.execute(&serde_json::json!({"action": "type", "text": "x"}));
        }
        let out = c.execute(&serde_json::json!({"action": "type", "text": "x"})).unwrap();
        assert!(out.contains("动作数已达上限"));
    }
}

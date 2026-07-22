//! Windows 桌面控制器：真机实现。
//!
//! - 动作合成走 `enigo`（鼠标移动/点击、文本输入、按键、滚动），API 与 macOS 完全一致。
//! - 无障碍快照走 `uiautomation`（封装 Windows UI Automation / COM）：从聚焦元素出发，
//!   用 control-view TreeWalker 深度优先遍历，收集 control type / name / value / 包围盒中心。
//! - 权限：Windows 无 macOS 式辅助功能授权门，UIA 对同/低完整性级别进程恒可用，
//!   故 `permission_status()` 恒为 `Granted`。
//!
//! 设计约束（见 trait `DesktopController: Send + Sync`）：
//! `enigo::Enigo`、`uiautomation::UIAutomation`（COM 对象，线程亲和）及任何 UIA 元素
//! 都不是 `Send`/`Sync`，因此本控制器是**无字段单元结构体**，所有句柄都在方法内部就地构造，
//! 绝不存进结构体。

use enigo::{Axis, Button, Coordinate, Direction, Enigo, Key, Keyboard, Mouse, Settings};
use uiautomation::patterns::UIValuePattern;
use uiautomation::types::ControlType;
use uiautomation::{UIAutomation, UIElement, UITreeWalker};

use super::{
    ClickTarget, DesktopController, DesktopError, KeyCombo, MouseButton, PermissionStatus,
    UiElement, UiSnapshot,
};

/// 元素数量上限：命中后置 `truncated = true` 并停止遍历。
const MAX_ELEMENTS: usize = 120;
/// 遍历深度上限，避免病态深树拖垮快照。
const MAX_DEPTH: usize = 40;

/// Windows 真机桌面控制器。无字段，`Send + Sync` 平凡成立。
pub struct WindowsController;

impl WindowsController {
    pub fn new() -> Self {
        WindowsController
    }
}

impl Default for WindowsController {
    fn default() -> Self {
        Self::new()
    }
}

/// 就地构造 `Enigo`，错误统一映射为 `Backend`。
fn make_enigo() -> Result<Enigo, DesktopError> {
    Enigo::new(&Settings::default()).map_err(|e| DesktopError::Backend(e.to_string()))
}

/// enigo 动作错误 → `Backend`。
fn enigo_err<E: std::fmt::Display>(e: E) -> DesktopError {
    DesktopError::Backend(e.to_string())
}

/// uiautomation 错误 → `Backend`。
fn uia_err(e: uiautomation::Error) -> DesktopError {
    DesktopError::Backend(e.to_string())
}

/// 将 `KeyCombo.key`（小写串）映射为 enigo `Key`。
/// 单个可打印字符走 `Key::Unicode`；具名键走显式 match。
/// 与 macOS 实现保持一致（enigo 的 `Key` 枚举跨平台同名）。
fn map_key(key: &str) -> Result<Key, DesktopError> {
    let k = match key {
        "enter" | "return" => Key::Return,
        "tab" => Key::Tab,
        "esc" | "escape" => Key::Escape,
        "space" => Key::Space,
        "backspace" => Key::Backspace,
        "delete" | "del" => Key::Delete,
        "up" => Key::UpArrow,
        "down" => Key::DownArrow,
        "left" => Key::LeftArrow,
        "right" => Key::RightArrow,
        "home" => Key::Home,
        "end" => Key::End,
        "pageup" => Key::PageUp,
        "pagedown" => Key::PageDown,
        "f1" => Key::F1,
        "f2" => Key::F2,
        "f3" => Key::F3,
        "f4" => Key::F4,
        "f5" => Key::F5,
        "f6" => Key::F6,
        "f7" => Key::F7,
        "f8" => Key::F8,
        "f9" => Key::F9,
        "f10" => Key::F10,
        "f11" => Key::F11,
        "f12" => Key::F12,
        other => {
            let mut chars = other.chars();
            match (chars.next(), chars.next()) {
                // 恰好一个字符 → Unicode 键
                (Some(c), None) => Key::Unicode(c),
                _ => {
                    return Err(DesktopError::Backend(format!("未知主键: {other:?}")));
                }
            }
        }
    };
    Ok(k)
}

impl DesktopController for WindowsController {
    fn snapshot_ui(&self) -> Result<UiSnapshot, DesktopError> {
        // 初始化 COM 并取 UIA 入口（就地构造，COM 对象不跨方法存留）。
        let automation = UIAutomation::new().map_err(uia_err)?;
        let walker = automation.get_control_view_walker().map_err(uia_err)?;
        // 从聚焦元素出发覆盖「当前活动窗口/控件」，与 macOS 取聚焦应用一致。
        let root = automation.get_focused_element().map_err(uia_err)?;

        let mut ctx = SnapshotCtx::default();
        traverse(&walker, &root, 0, &mut ctx);

        let coverage_hint = if ctx.visited == 0 {
            0.0
        } else {
            (ctx.elements.len() as f32 / ctx.visited as f32).clamp(0.0, 1.0)
        };

        Ok(UiSnapshot {
            elements: ctx.elements,
            truncated: ctx.truncated,
            coverage_hint,
        })
    }

    fn click(&self, target: ClickTarget, button: MouseButton) -> Result<(), DesktopError> {
        let (x, y) = match target {
            ClickTarget::Point { x, y } => (x, y),
            // 控制器层无快照上下文，无法解析裸 Element id；上游应已解析为 Point。
            ClickTarget::Element(id) => {
                return Err(DesktopError::Backend(format!(
                    "控制器收到未解析的 Element({id})，请先 observe 解析为坐标"
                )));
            }
        };
        let btn = match button {
            MouseButton::Left => Button::Left,
            MouseButton::Right => Button::Right,
        };
        let mut enigo = make_enigo()?;
        enigo.move_mouse(x, y, Coordinate::Abs).map_err(enigo_err)?;
        enigo.button(btn, Direction::Click).map_err(enigo_err)?;
        Ok(())
    }

    fn type_text(&self, text: &str) -> Result<(), DesktopError> {
        let mut enigo = make_enigo()?;
        enigo.text(text).map_err(enigo_err)?;
        Ok(())
    }

    fn key(&self, combo: &KeyCombo) -> Result<(), DesktopError> {
        let main = map_key(&combo.key)?;
        let mut enigo = make_enigo()?;

        // 按下激活的修饰键 → 点击主键 → 逆序释放修饰键。
        // Windows 上 cmd 语义映射到 Win 键（enigo Key::Meta）。
        let mut mods: Vec<Key> = Vec::new();
        if combo.cmd {
            mods.push(Key::Meta);
        }
        if combo.ctrl {
            mods.push(Key::Control);
        }
        if combo.alt {
            mods.push(Key::Alt);
        }
        if combo.shift {
            mods.push(Key::Shift);
        }

        for m in &mods {
            enigo.key(*m, Direction::Press).map_err(enigo_err)?;
        }
        let click_res = enigo.key(main, Direction::Click);
        // 无论主键是否成功，都要释放已按下的修饰键，避免修饰键卡死。
        for m in mods.iter().rev() {
            let _ = enigo.key(*m, Direction::Release);
        }
        click_res.map_err(enigo_err)?;
        Ok(())
    }

    fn scroll(&self, dx: i32, dy: i32) -> Result<(), DesktopError> {
        let mut enigo = make_enigo()?;
        if dy != 0 {
            enigo.scroll(dy, Axis::Vertical).map_err(enigo_err)?;
        }
        if dx != 0 {
            enigo.scroll(dx, Axis::Horizontal).map_err(enigo_err)?;
        }
        Ok(())
    }

    fn permission_status(&self) -> PermissionStatus {
        // Windows 无 macOS 式辅助功能授权门；UIA 对同/低完整性级别进程恒可用。
        PermissionStatus::Granted
    }
}

#[derive(Default)]
struct SnapshotCtx {
    elements: Vec<UiElement>,
    /// 访问过的节点总数（含被裁掉的），用于估算 coverage_hint。
    visited: usize,
    truncated: bool,
}

/// control type 白名单 → 归一化 role 串：仅保留可交互 / 文本类元素。
/// 命中返回 `Some(role)`，否则 `None`（节点仍下钻子节点）。
fn role_for(ct: ControlType) -> Option<&'static str> {
    let role = match ct {
        ControlType::Button => "button",
        ControlType::SplitButton => "button",
        ControlType::MenuItem => "menuitem",
        ControlType::Edit => "textfield",
        ControlType::Document => "textarea",
        ControlType::CheckBox => "checkbox",
        ControlType::RadioButton => "radiobutton",
        ControlType::ComboBox => "combobox",
        ControlType::Hyperlink => "link",
        ControlType::Text => "statictext",
        ControlType::TabItem => "tab",
        ControlType::Slider => "slider",
        ControlType::ListItem => "listitem",
        ControlType::TreeItem => "treeitem",
        ControlType::DataItem => "cell",
        ControlType::Spinner => "incrementor",
        _ => return None,
    };
    Some(role)
}

/// 通过 ValuePattern 读取 value（缺该 pattern 或空值则忽略）。
fn read_value_string(el: &UIElement) -> Option<String> {
    let pattern: UIValuePattern = el.get_pattern().ok()?;
    let v = pattern.get_value().ok()?;
    if v.is_empty() {
        None
    } else {
        Some(v)
    }
}

/// 读取包围盒中心点（屏幕坐标）。退化（宽高为 0 或读取失败）时返回 None。
fn read_center(el: &UIElement) -> Option<(i32, i32)> {
    let r = el.get_bounding_rectangle().ok()?;
    let left = r.get_left();
    let top = r.get_top();
    let width = r.get_width();
    let height = r.get_height();
    if width <= 0 || height <= 0 {
        return None;
    }
    Some((left + width / 2, top + height / 2))
}

/// 深度优先遍历 control-view 树，收集白名单元素。id 按遍历顺序从 1 递增。
fn traverse(walker: &UITreeWalker, el: &UIElement, depth: usize, ctx: &mut SnapshotCtx) {
    if ctx.truncated || depth > MAX_DEPTH {
        return;
    }
    ctx.visited += 1;

    // 读 control type（缺失则跳过本节点的收集，但仍下钻子节点）。
    if let Ok(ct) = el.get_control_type() {
        if let Some(role) = role_for(ct) {
            // label：name → value(字符串) 兜底（空串视为缺失）。
            let label = el
                .get_name()
                .ok()
                .filter(|s| !s.is_empty())
                .or_else(|| read_value_string(el))
                .unwrap_or_default();
            let value = read_value_string(el);
            if let Some((cx, cy)) = read_center(el) {
                let id = (ctx.elements.len() + 1) as u32;
                ctx.elements.push(UiElement {
                    id,
                    role: role.to_string(),
                    label,
                    value,
                    cx,
                    cy,
                });
                if ctx.elements.len() >= MAX_ELEMENTS {
                    ctx.truncated = true;
                    return;
                }
            }
        }
    }

    // 下钻子节点：first_child → next_sibling 链式遍历（UIA 无 children() 集合 API）。
    if let Ok(mut child) = walker.get_first_child(el) {
        loop {
            if ctx.truncated {
                return;
            }
            traverse(walker, &child, depth + 1, ctx);
            match walker.get_next_sibling(&child) {
                Ok(next) => child = next,
                // 无更多兄弟（或到达末端）→ 结束本层。
                Err(_) => break,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 关键设计不变量：控制器必须 Send + Sync（无字段单元结构体）。
    fn _assert_send_sync<T: Send + Sync>() {}
    #[test]
    fn controller_is_send_sync() {
        _assert_send_sync::<WindowsController>();
    }

    #[test]
    fn map_key_named_and_unicode() {
        assert!(matches!(map_key("enter").unwrap(), Key::Return));
        assert!(matches!(map_key("tab").unwrap(), Key::Tab));
        assert!(matches!(map_key("left").unwrap(), Key::LeftArrow));
        assert!(matches!(map_key("c").unwrap(), Key::Unicode('c')));
        assert!(map_key("notakey").is_err());
        assert!(map_key("").is_err());
    }

    #[test]
    fn role_whitelist_basic() {
        assert_eq!(role_for(ControlType::Button), Some("button"));
        assert_eq!(role_for(ControlType::Edit), Some("textfield"));
        assert_eq!(role_for(ControlType::Text), Some("statictext"));
        assert_eq!(role_for(ControlType::Hyperlink), Some("link"));
        assert_eq!(role_for(ControlType::Window), None);
        assert_eq!(role_for(ControlType::Pane), None);
    }

    #[test]
    fn unresolved_element_target_is_backend_error() {
        let c = WindowsController::new();
        let err = c
            .click(ClickTarget::Element(7), MouseButton::Left)
            .unwrap_err();
        assert!(matches!(err, DesktopError::Backend(_)));
    }
}

//! macOS 桌面控制器：真机实现。
//!
//! - 动作合成走 `enigo`（鼠标移动/点击、文本输入、按键、滚动）。
//! - 无障碍快照走 `accessibility` 高层封装读 role/title/children 等，
//!   position/size 因 0.2.0 未提供访问器，降级用 `accessibility-sys` 原始 FFI。
//! - 权限探测走 `AXIsProcessTrusted()`。
//!
//! 设计约束（见 trait `DesktopController: Send + Sync`）：
//! `enigo::Enigo` 与 AXUIElement / Core Foundation 句柄都不是 `Send`/`Sync`，
//! 因此本控制器是**无字段单元结构体**，所有句柄都在方法内部就地构造，绝不存进结构体。

use accessibility::{AXAttribute, AXUIElement, AXUIElementAttributes};
use accessibility_sys::{
    kAXTrustedCheckOptionPrompt, kAXValueTypeCGPoint, kAXValueTypeCGSize, AXIsProcessTrusted,
    AXIsProcessTrustedWithOptions, AXValueGetValue, AXValueRef,
};
use core_foundation::base::{CFType, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::CFDictionary;
use core_foundation::string::CFString;
use core_graphics_types::geometry::{CGPoint, CGSize};
use enigo::{
    Axis, Button, Coordinate, Direction, Enigo, Key, Keyboard, Mouse, Settings,
};

use super::{
    ClickTarget, DesktopController, DesktopError, KeyCombo, MouseButton, PermissionStatus,
    UiElement, UiSnapshot,
};

/// 元素数量上限：命中后置 `truncated = true` 并停止遍历。
const MAX_ELEMENTS: usize = 120;
/// 遍历深度上限，避免病态深树拖垮快照。
const MAX_DEPTH: usize = 40;
/// `kAXFocusedApplicationAttribute` 常量名（高层 crate 的 `define_attributes!` 未含，
/// 故就地用其字面值构造自定义属性）。
const K_AX_FOCUSED_APPLICATION: &str = "AXFocusedApplication";

/// macOS 真机桌面控制器。无字段，`Send + Sync` 平凡成立。
pub struct MacosController;

impl MacosController {
    pub fn new() -> Self {
        MacosController
    }
}

impl Default for MacosController {
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

/// 将 `KeyCombo.key`（小写串）映射为 enigo `Key`。
/// 单个可打印字符走 `Key::Unicode`；具名键走显式 match。
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

/// 取前台应用的 AXUIElement。优先用 NSWorkspace 的前台应用 PID（稳定、无需额外授权）→
/// `AXUIElement::application(pid)`；拿不到 PID 才回退到 system-wide 的 `AXFocusedApplication`
/// ——后者偶尔返回 `kAXErrorNoValue`(-25212)，正是「observe 失败: AX 错误码 -25212」的来源。
fn frontmost_app_element() -> Result<AXUIElement, DesktopError> {
    if let Some(pid) = frontmost_app_pid() {
        return Ok(AXUIElement::application(pid));
    }
    let system_wide = AXUIElement::system_wide();
    let focused_attr =
        AXAttribute::<CFType>::new(&CFString::from_static_string(K_AX_FOCUSED_APPLICATION));
    let app_cf = system_wide.attribute(&focused_attr).map_err(map_ax_err)?;
    app_cf
        .downcast_into::<AXUIElement>()
        .ok_or_else(|| DesktopError::Backend("聚焦应用不是 AXUIElement".into()))
}

/// NSWorkspace 当前前台应用的 PID（无前台应用返回 None）。
/// NSWorkspace 线程安全，无需主线程，也不触发额外 TCC 授权。
fn frontmost_app_pid() -> Option<i32> {
    use objc2_app_kit::NSWorkspace;
    // SAFETY: 仅读取共享 NSWorkspace 的前台应用与其 PID，无可变状态、无所有权转移。
    unsafe {
        let ws = NSWorkspace::sharedWorkspace();
        let app = ws.frontmostApplication()?;
        Some(app.processIdentifier())
    }
}

impl DesktopController for MacosController {
    fn snapshot_ui(&self) -> Result<UiSnapshot, DesktopError> {
        if !is_trusted() {
            return Err(DesktopError::PermissionDenied);
        }

        let app = frontmost_app_element()?;

        let mut ctx = SnapshotCtx::default();
        traverse(&app, 0, &mut ctx);

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
        enigo
            .move_mouse(x, y, Coordinate::Abs)
            .map_err(enigo_err)?;
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
        // 仅查 AXIsProcessTrusted() 不够：dev 模式每次 cargo build 出新 cdhash 的二进制，
        // 旧 TCC 授权对新二进制实际失效，但 AXIsProcessTrusted() 仍可能返回「陈旧 true」
        // （假 granted）。叠加一次真实 AX 探针：被 APIDisabled 拒绝即如实报 Denied，
        // 让前端权限卡 +「去开启」出现，形成重新授权闭环。
        if is_trusted() && ax_probe_ok() {
            PermissionStatus::Granted
        } else {
            PermissionStatus::Denied
        }
    }
}

/// `AXIsProcessTrusted()` 是否信任当前进程（辅助功能已授权）。
fn is_trusted() -> bool {
    // SAFETY: 无参纯查询 FFI，无副作用。
    unsafe { AXIsProcessTrusted() }
}

/// 主动唤起 macOS 原生辅助功能授权：构造 `{kAXTrustedCheckOptionPrompt: true}` 调
/// `AXIsProcessTrustedWithOptions` —— 这会把本应用**注册进「辅助功能」列表**并弹出
/// 「允许…控制你的电脑」原生对话框（含「打开系统设置」）。返回当前是否已受信。
///
/// 关键：未授权 app 的普通 AX 调用**不会**自动弹窗，必须经此带 prompt 的查询才唤起授权界面。
/// 原生提示每个 app 通常只弹一次；已在列表后再调不再弹，但应用已出现在列表中可手动勾选。
pub fn request_accessibility_prompt() -> bool {
    // SAFETY: kAXTrustedCheckOptionPrompt 为常量 CFStringRef，get-rule 包装不夺所有权。
    let key = unsafe { CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt) };
    let value = CFBoolean::true_value();
    let options = CFDictionary::from_CFType_pairs(&[(key.as_CFType(), value.as_CFType())]);
    // SAFETY: 传入合法 CFDictionaryRef；纯查询 + 触发系统提示，无所有权转移。
    unsafe { AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef()) }
}

/// 真实 AX 探针：发一次 system-wide 读取，仅当返回 `kAXErrorAPIDisabled`
/// （≈ 辅助功能未授权）时判失败；其余成功或结构性错误（如无聚焦应用）都视为可用。
/// 用于识破「假 granted」：`AXIsProcessTrusted()` 说 true 但实际 AX 调用被拒。
fn ax_probe_ok() -> bool {
    let system_wide = AXUIElement::system_wide();
    let focused_attr =
        AXAttribute::<CFType>::new(&CFString::from_static_string(K_AX_FOCUSED_APPLICATION));
    match system_wide.attribute(&focused_attr) {
        Ok(_) => true,
        Err(accessibility::Error::Ax(code)) => code != accessibility_sys::kAXErrorAPIDisabled,
        Err(_) => true,
    }
}

/// accessibility crate 的 `Error` → `DesktopError`。
/// `Ax(...)` 多为 NotAuthorized/APIDisabled，统一按权限失败处理偏保守，
/// 其余结构性错误归 `Backend`。
fn map_ax_err(e: accessibility::Error) -> DesktopError {
    match e {
        accessibility::Error::Ax(code) => {
            use accessibility_sys::kAXErrorAPIDisabled;
            // AX API 被禁用 ≈ 辅助功能未授权（最贴近权限语义的稳定错误码）。
            if code == kAXErrorAPIDisabled {
                DesktopError::PermissionDenied
            } else {
                DesktopError::Backend(format!("AX 错误码 {code}"))
            }
        }
        other => DesktopError::Backend(other.to_string()),
    }
}

#[derive(Default)]
struct SnapshotCtx {
    elements: Vec<UiElement>,
    /// 访问过的节点总数（含被裁掉的），用于估算 coverage_hint。
    visited: usize,
    truncated: bool,
}

/// 角色白名单：仅保留可交互 / 文本类元素。已归一化（去掉 `AX` 前缀、转小写）。
fn role_is_interesting(role_norm: &str) -> bool {
    matches!(
        role_norm,
        "button"
            | "menuitem"
            | "menubaritem"
            | "textfield"
            | "textarea"
            | "checkbox"
            | "radiobutton"
            | "popupbutton"
            | "link"
            | "statictext"
            | "tab"
            | "slider"
            | "combobox"
            | "searchfield"
            | "incrementor"
            | "disclosuretriangle"
            | "cell"
    )
}

/// 归一化 AX role：去掉 `AX` 前缀并转小写。
fn normalize_role(raw: &str) -> String {
    raw.strip_prefix("AX").unwrap_or(raw).to_ascii_lowercase()
}

/// 读取 `value` 属性（`CFType`），尝试转成可读字符串。
fn read_value_string(el: &AXUIElement) -> Option<String> {
    let v = el.value().ok()?;
    // 多数控件 value 为 CFString；非字符串（如数值/布尔）此处忽略，避免噪声。
    v.downcast::<CFString>().map(|s| s.to_string())
}

/// 从一个承载 `AXValue` 的 `CFType` 中按指定 AXValueType 解出 `T`（CGPoint/CGSize）。
/// SAFETY: `cf` 必须确实是一个 AXValue，且 `T` 与 `ax_type` 对应。
unsafe fn ax_value_unwrap<T>(cf: &CFType, ax_type: u32) -> Option<T> {
    let mut out = std::mem::MaybeUninit::<T>::uninit();
    let ax_ref = cf.as_CFTypeRef() as AXValueRef;
    if AXValueGetValue(ax_ref, ax_type, out.as_mut_ptr() as *mut std::ffi::c_void) {
        Some(out.assume_init())
    } else {
        None
    }
}

/// 读取中心点坐标：position(左上角 CGPoint) + size(CGSize) → 中心。
///
/// 注：已发布的 `accessibility 0.2.0` 高层封装未提供 position/size 访问器，
/// 故这里用 `kAXPositionAttribute`/`kAXSizeAttribute` 自定义属性取回 `AXValue`，
/// 再经 `AXValueGetValue` 原始 FFI 解出 `CGPoint`/`CGSize`。
fn read_center(el: &AXUIElement) -> Option<(i32, i32)> {
    let pos_attr = AXAttribute::<CFType>::new(&CFString::from_static_string("AXPosition"));
    let size_attr = AXAttribute::<CFType>::new(&CFString::from_static_string("AXSize"));
    let pos_cf = el.attribute(&pos_attr).ok()?;
    let size_cf = el.attribute(&size_attr).ok()?;
    // SAFETY: 两个属性返回的 CFType 即 AXValue，类型与下方 AXValueType 一致。
    let pos: CGPoint = unsafe { ax_value_unwrap(&pos_cf, kAXValueTypeCGPoint)? };
    let size: CGSize = unsafe { ax_value_unwrap(&size_cf, kAXValueTypeCGSize)? };
    let cx = (pos.x + size.width / 2.0).round() as i32;
    let cy = (pos.y + size.height / 2.0).round() as i32;
    Some((cx, cy))
}

/// 深度优先遍历，收集白名单元素。id 按遍历顺序从 1 递增。
fn traverse(el: &AXUIElement, depth: usize, ctx: &mut SnapshotCtx) {
    if ctx.truncated || depth > MAX_DEPTH {
        return;
    }
    ctx.visited += 1;

    // 读 role（缺失则跳过本节点的收集，但仍下钻子节点）。
    if let Ok(role_cf) = el.role() {
        let role_norm = normalize_role(&role_cf.to_string());
        if role_is_interesting(&role_norm) {
            // label：title → description → value(字符串) 依次兜底（空串视为缺失）。
            let nonempty = |s: String| if s.is_empty() { None } else { Some(s) };
            let label = el
                .title()
                .ok()
                .map(|s| s.to_string())
                .and_then(nonempty)
                .or_else(|| el.description().ok().map(|s| s.to_string()).and_then(nonempty))
                .or_else(|| read_value_string(el))
                .unwrap_or_default();
            let value = read_value_string(el);
            if let Some((cx, cy)) = read_center(el) {
                let id = (ctx.elements.len() + 1) as u32;
                ctx.elements.push(UiElement {
                    id,
                    role: role_norm,
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

    // 下钻子节点。
    if let Ok(children) = el.children() {
        for child in children.iter() {
            if ctx.truncated {
                return;
            }
            traverse(&child, depth + 1, ctx);
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
        _assert_send_sync::<MacosController>();
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
    fn normalize_role_strips_prefix_and_lowercases() {
        assert_eq!(normalize_role("AXButton"), "button");
        assert_eq!(normalize_role("AXStaticText"), "statictext");
        assert_eq!(normalize_role("button"), "button");
    }

    #[test]
    fn role_whitelist_basic() {
        assert!(role_is_interesting("button"));
        assert!(role_is_interesting("textfield"));
        assert!(role_is_interesting("statictext"));
        assert!(!role_is_interesting("group"));
        assert!(!role_is_interesting("window"));
    }

    #[test]
    fn unresolved_element_target_is_backend_error() {
        let c = MacosController::new();
        let err = c
            .click(ClickTarget::Element(7), MouseButton::Left)
            .unwrap_err();
        assert!(matches!(err, DesktopError::Backend(_)));
    }
}

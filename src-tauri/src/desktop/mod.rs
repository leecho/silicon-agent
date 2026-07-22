//! 桌面操作内核：抽象 OS 的「无障碍树读取 + 鼠标键盘合成」。
//! 唯一触碰系统 API 的层；上层（computer 工具、引擎）只依赖本 trait。

use std::fmt;

/// 鼠标键。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
}

/// 点击目标：无障碍元素 id（本次快照内序号）或绝对屏幕坐标。
#[derive(Debug, Clone, PartialEq)]
pub enum ClickTarget {
    Element(u32),
    Point { x: i32, y: i32 },
}

/// 组合键，已解析为修饰键 + 主键。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyCombo {
    pub cmd: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    /// 主键名，小写，如 "c"/"enter"/"tab"/"left"。
    pub key: String,
}

/// 无障碍树中的一个元素。
#[derive(Debug, Clone, PartialEq)]
pub struct UiElement {
    pub id: u32,
    pub role: String,
    pub label: String,
    pub value: Option<String>,
    /// 中心点屏幕坐标，供 id→坐标解析。
    pub cx: i32,
    pub cy: i32,
}

/// 一次无障碍树快照（已裁剪）。
#[derive(Debug, Clone, PartialEq)]
pub struct UiSnapshot {
    pub elements: Vec<UiElement>,
    /// 是否因上限被截断。
    pub truncated: bool,
    /// 白名单命中率：保留的可交互元素数 / 遍历过的节点数（kept / visited）。
    /// 越低代表可读无障碍元素越稀疏（可能是自绘/非无障碍 UI）；为 P2 视觉兜底预留。
    pub coverage_hint: f32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum PermissionStatus {
    Granted,
    Denied,
    #[default]
    Unknown,
}

#[cfg(test)]
pub mod mock;

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(target_os = "windows")]
pub mod windows;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DesktopError {
    PermissionDenied,
    ElementNotFound(u32),
    Backend(String),
}

impl fmt::Display for DesktopError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DesktopError::PermissionDenied => write!(f, "辅助功能未授权"),
            DesktopError::ElementNotFound(id) => write!(f, "元素 {id} 不存在，请先 observe"),
            DesktopError::Backend(m) => write!(f, "桌面后端错误: {m}"),
        }
    }
}

impl UiSnapshot {
    /// 序列化为稳定文本树，回喂模型。
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        for e in &self.elements {
            out.push_str(&format!("[{}] {} \"{}\"", e.id, e.role, e.label));
            if let Some(v) = &e.value {
                out.push_str(&format!(" value=\"{v}\""));
            }
            out.push_str(&format!(" ({},{})\n", e.cx, e.cy));
        }
        if self.truncated {
            out.push_str("…已截断（元素超上限，可缩小范围或就近 observe）\n");
        }
        out
    }

    /// 解析元素 id → 中心坐标。
    pub fn resolve(&self, id: u32) -> Result<(i32, i32), DesktopError> {
        self.elements
            .iter()
            .find(|e| e.id == id)
            .map(|e| (e.cx, e.cy))
            .ok_or(DesktopError::ElementNotFound(id))
    }
}

impl KeyCombo {
    /// 解析 "cmd+shift+c" 风格组合键；末段为主键，前段为修饰键。
    pub fn parse(s: &str) -> Result<KeyCombo, DesktopError> {
        let parts: Vec<&str> = s.split('+').map(|p| p.trim()).collect();
        if parts.is_empty() || parts.iter().any(|p| p.is_empty()) {
            return Err(DesktopError::Backend(format!("非法组合键: {s:?}")));
        }
        let (mods, key) = parts.split_at(parts.len() - 1);
        let key = key[0].to_ascii_lowercase();
        if key.is_empty() {
            return Err(DesktopError::Backend(format!("缺少主键: {s:?}")));
        }
        let mut k = KeyCombo { cmd: false, ctrl: false, alt: false, shift: false, key };
        for m in mods {
            match m.to_ascii_lowercase().as_str() {
                "cmd" | "command" | "meta" | "super" => k.cmd = true,
                "ctrl" | "control" => k.ctrl = true,
                "alt" | "option" | "opt" => k.alt = true,
                "shift" => k.shift = true,
                other => return Err(DesktopError::Backend(format!("未知修饰键: {other}"))),
            }
        }
        Ok(k)
    }
}

/// 桌面操作内核接口。实现者：macOS（真机）/ Mock（测试）。
pub trait DesktopController: Send + Sync {
    fn snapshot_ui(&self) -> Result<UiSnapshot, DesktopError>;
    fn click(&self, target: ClickTarget, button: MouseButton) -> Result<(), DesktopError>;
    fn type_text(&self, text: &str) -> Result<(), DesktopError>;
    fn key(&self, combo: &KeyCombo) -> Result<(), DesktopError>;
    fn scroll(&self, dx: i32, dy: i32) -> Result<(), DesktopError>;
    fn permission_status(&self) -> PermissionStatus;
}

#[cfg(test)]
mod keycombo_tests {
    use super::*;

    #[test]
    fn parses_modifiers_and_key() {
        let k = KeyCombo::parse("cmd+shift+c").unwrap();
        assert!(k.cmd && k.shift && !k.ctrl && !k.alt);
        assert_eq!(k.key, "c");
    }

    #[test]
    fn parses_bare_key() {
        let k = KeyCombo::parse("enter").unwrap();
        assert_eq!(k.key, "enter");
        assert!(!k.cmd && !k.ctrl && !k.alt && !k.shift);
    }

    #[test]
    fn rejects_empty() {
        assert!(KeyCombo::parse("").is_err());
        assert!(KeyCombo::parse("cmd+").is_err());
    }
}

#[cfg(test)]
mod snapshot_tests {
    use super::*;

    fn sample() -> UiSnapshot {
        UiSnapshot {
            elements: vec![
                UiElement { id: 12, role: "button".into(), label: "保存".into(), value: None, cx: 1180, cy: 40 },
                UiElement { id: 13, role: "textfield".into(), label: "搜索".into(), value: Some(String::new()), cx: 320, cy: 120 },
            ],
            truncated: false,
            coverage_hint: 1.0,
        }
    }

    #[test]
    fn serializes_stable_text_lines() {
        let text = sample().to_text();
        assert!(text.contains("[12] button \"保存\" (1180,40)"));
        assert!(text.contains("[13] textfield \"搜索\" value=\"\" (320,120)"));
    }

    #[test]
    fn truncated_marker_appended() {
        let mut s = sample();
        s.truncated = true;
        assert!(s.to_text().contains("…已截断"));
    }
}

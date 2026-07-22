//! 浏览器操作内核：抽象 CDP 的「浏览器生命周期 + 结构化 DOM 感知 + 动作合成」。
//! 唯一触碰 CDP 的层；上层（browser 工具、引擎）只依赖本 trait。
//! trait 同步：契合同步 `Tool::execute`（见 tools/mod.rs）。

pub mod cdp;
pub mod policy;
pub mod shared;

#[cfg(test)]
pub mod mock;

use std::fmt;

/// 元素定位：来自最近一次快照的 ref 序号，或模型直接给的 CSS 选择器（逃生舱）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElementTarget {
    Ref(u32),
    Selector(String),
}

/// 结构化快照中的一个可交互/可读元素。
#[derive(Debug, Clone, PartialEq)]
pub struct DomElement {
    pub id: u32,
    /// 无障碍/语义角色：button/link/textbox/select…
    pub role: String,
    /// 可见文本或 accessible name。
    pub name: String,
    pub value: Option<String>,
    /// 稳定选择器（CDP 计算所得），供回放/调试；模型主要用 id。
    pub selector: String,
    /// 结构化高风险信号：点击该元素会触发表单提交
    /// （button[type=submit] / input[type=submit] / form 内默认提交控件）。
    /// 由 CDP 实现据 DOM 计算，**不靠按钮文字猜**（守 AGENTS.md 红线）。
    pub submits: bool,
}

/// 一次结构化快照（已裁剪）。
#[derive(Debug, Clone, PartialEq)]
pub struct DomSnapshot {
    pub url: String,
    pub title: String,
    pub elements: Vec<DomElement>,
    /// 是否因上限被截断。
    pub truncated: bool,
    /// 元素稀疏度提示（为 P2 视觉兜底预留：shadow DOM/canvas 多时偏低）。
    pub coverage_hint: f32,
}

/// 一个浏览器标签的摘要。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabInfo {
    pub index: usize,
    pub title: String,
    pub url: String,
    pub active: bool,
}

/// 抽取查询（研究类任务）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractQuery {
    /// CSS 选择器；空则抽当前页可读正文。
    pub selector: Option<String>,
}

/// 等待条件（有上限，防卡死）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WaitCondition {
    /// 选择器出现。
    Selector(String),
    /// 固定毫秒。
    Millis(u64),
}

/// 启动参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchOptions {
    /// 自动化配置档目录（独立于用户默认 profile）。
    pub user_data_dir: String,
}

/// 浏览器/Chrome 探测状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowserStatus {
    /// 已探测到 Chrome 且可启动。
    Ready,
    /// 未探测到任何 Chrome/Chromium/Edge 二进制。
    NotInstalled,
    /// 已启动并连接。
    Running,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowserError {
    NotInstalled,
    LaunchFailed(String),
    ElementNotFound(u32),
    SelectorNoMatch(String),
    NavigationTimeout,
    Backend(String),
}

impl fmt::Display for BrowserError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BrowserError::NotInstalled => write!(f, "未检测到 Chrome 浏览器"),
            BrowserError::LaunchFailed(m) => write!(f, "浏览器启动失败: {m}"),
            BrowserError::ElementNotFound(id) => write!(f, "元素 {id} 不存在，请先 observe"),
            BrowserError::SelectorNoMatch(s) => write!(f, "选择器无匹配: {s}"),
            BrowserError::NavigationTimeout => write!(f, "页面加载超时"),
            BrowserError::Backend(m) => write!(f, "浏览器后端错误: {m}"),
        }
    }
}

impl DomSnapshot {
    /// 序列化为稳定文本树，回喂模型。首行带页面上下文。
    pub fn to_text(&self) -> String {
        let mut out = format!("页面《{}》 {}\n", self.title, self.url);
        for e in &self.elements {
            out.push_str(&format!("[{}] {} \"{}\"", e.id, e.role, e.name));
            if let Some(v) = &e.value {
                out.push_str(&format!(" value=\"{v}\""));
            }
            out.push('\n');
        }
        if self.truncated {
            out.push_str("…已截断（元素超上限，可滚动或缩小范围后再 observe）\n");
        }
        out
    }

    /// 解析元素 ref → 元素（供动作执行与高风险判定）。
    pub fn resolve(&self, id: u32) -> Result<&DomElement, BrowserError> {
        self.elements
            .iter()
            .find(|e| e.id == id)
            .ok_or(BrowserError::ElementNotFound(id))
    }
}

/// 浏览器操作内核接口（同步）。实现者：CDP（真机）/ Mock（测试）。
pub trait BrowserController: Send + Sync {
    fn launch(&self, opts: &LaunchOptions) -> Result<(), BrowserError>;
    fn navigate(&self, url: &str) -> Result<(), BrowserError>;
    fn snapshot(&self) -> Result<DomSnapshot, BrowserError>;
    fn click(&self, target: &ElementTarget) -> Result<(), BrowserError>;
    /// 双击：默认连点两次（修复 `double_click` 动作此前只单击的 bug）。
    /// CDP 实现可覆写为真正的 dblclick 事件。
    fn double_click(&self, target: &ElementTarget) -> Result<(), BrowserError> {
        self.click(target)?;
        self.click(target)
    }
    fn fill(&self, target: &ElementTarget, text: &str) -> Result<(), BrowserError>;
    fn select(&self, target: &ElementTarget, value: &str) -> Result<(), BrowserError>;
    fn scroll(&self, dx: i32, dy: i32) -> Result<(), BrowserError>;
    fn extract(&self, query: &ExtractQuery) -> Result<String, BrowserError>;
    fn wait(&self, condition: &WaitCondition) -> Result<(), BrowserError>;
    fn back(&self) -> Result<(), BrowserError>;
    /// 列出当前所有标签页（含活动标记）。
    fn tabs(&self) -> Result<Vec<TabInfo>, BrowserError>;
    /// 切换活动标签到指定序号。
    fn switch_tab(&self, index: usize) -> Result<(), BrowserError>;
    /// 关闭指定序号的标签。
    fn close_tab(&self, index: usize) -> Result<(), BrowserError>;
    /// 描述一个选择器目标是否为提交控件（逃生舱路径的高风险判定）。
    /// Ref 路径直接用快照里的 `DomElement.submits`，不走此方法。
    fn describe_submits(&self, selector: &str) -> Result<bool, BrowserError>;
    fn status(&self) -> BrowserStatus;
    /// 绑定当前操作所属会话（T92 P2-T2）。CDP 实现据此把动作路由到该会话**自己的** tab，
    /// 实现跨会话的顺序隔离（A 用 A 的 tab、B 用 B 的 tab，互不覆写导航）。
    /// 最佳努力/幂等；mock/desktop 等无标签概念者默认 no-op。
    fn bind_session(&self, _session_id: &str) {}
    /// 显式打开浏览器窗口（懒启动 Chrome；已开则无副作用）。供「打开浏览器」按钮先行登录。
    /// 默认 no-op（mock/desktop 等无窗口）；CDP 实现 = ensure_session（用默认独立配置档真正开窗）。
    fn open_window(&self) -> Result<(), BrowserError> { Ok(()) }
    /// 关闭浏览器（常驻持有者：drop inner → Chrome 关）。默认 no-op（mock/desktop 等无需）。
    fn close(&self) {}
    /// 设置下载落盘目录（运行期可改）。默认 no-op。
    fn set_download_dir(&self, _dir: std::path::PathBuf) {}
}

#[cfg(test)]
mod snapshot_tests {
    use super::*;

    fn sample() -> DomSnapshot {
        DomSnapshot {
            url: "https://example.com/login".into(),
            title: "登录".into(),
            elements: vec![
                DomElement { id: 12, role: "button".into(), name: "登录".into(), value: None, selector: "#login".into(), submits: true },
                DomElement { id: 13, role: "textbox".into(), name: "邮箱".into(), value: Some(String::new()), selector: "#email".into(), submits: false },
            ],
            truncated: false,
            coverage_hint: 1.0,
        }
    }

    #[test]
    fn serializes_stable_text_lines() {
        let text = sample().to_text();
        assert!(text.contains("[12] button \"登录\""));
        assert!(text.contains("[13] textbox \"邮箱\" value=\"\""));
    }

    #[test]
    fn header_carries_url_and_title() {
        let text = sample().to_text();
        assert!(text.contains("登录") && text.contains("https://example.com/login"));
    }

    #[test]
    fn truncated_marker_appended() {
        let mut s = sample();
        s.truncated = true;
        assert!(s.to_text().contains("…已截断"));
    }

    #[test]
    fn resolve_ref_returns_element() {
        let s = sample();
        assert_eq!(s.resolve(12).unwrap().selector, "#login");
        assert!(s.resolve(999).is_err());
    }
}

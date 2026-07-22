//! 真机浏览器控制器：基于 `headless_chrome`(CDP) 驱动用户本地 Chrome/Chromium/Edge。
//! 唯一触碰 CDP 的实现层；通过 [`BrowserController`] trait 暴露给上层。
//!
//! 设计要点：
//! - Chrome 仅在 `Browser` 值存活期间保持运行，tab 为 `Arc<Tab>`，二者一起放进 `Mutex<Option<Session>>`。
//! - 快照走「注入一段 JS → JSON.stringify → serde 解析」，比逐个 CDP 调用稳健且快。
//! - 高风险 `submits` 由 DOM 结构计算，不靠按钮文字猜（守 AGENTS.md 红线）。

use std::sync::{Arc, Mutex};
use std::time::Duration;

use headless_chrome::{Browser, LaunchOptionsBuilder, Tab};
use serde::Deserialize;

use super::{
    BrowserController, BrowserError, BrowserStatus, DomElement, DomSnapshot, ElementTarget,
    ExtractQuery, LaunchOptions, TabInfo, WaitCondition,
};

/// 某个逻辑会话（silicon-worker session_id）在常驻 Chrome 中拥有的标签集合（T92 P2-T2）。
/// 每会话独立 tab 列表 + 自己的活动序号，互不干扰：A 的导航不会覆写 B 的标签。
struct SessionTabs {
    /// 本会话拥有的标签（首个由 bind 时 `new_tab()` 创建，弹窗追加进来）。
    tabs: Vec<Arc<Tab>>,
    /// 本会话内的活动标签序号（索引 `tabs`）。
    active: usize,
}

/// 默认会话键：尚未绑定任何 session 之前（懒启动后的首个动作）落在此桶。
const DEFAULT_SESSION: &str = "";

/// 一次浏览器进程会话：持有 `Browser`（保活）+ 按逻辑会话切分的标签集合。
struct Session {
    /// 仅为保活而持有；drop 即关闭 Chrome。
    /// 验证（headless_chrome 1.0.21）：`Browser` 持 `Process`→`TemporaryProcess`，其 `Drop`
    /// 执行 `child.kill()+wait()`。故 `CdpController` 随 run 结束/停止被析构时，Chrome 窗口随之关闭，
    /// 无需显式关闭逻辑（P2「停止关窗」即由此 Drop 链兜底）。
    _browser: Browser,
    /// 逻辑会话 → 其标签集合。每会话独立 tab，跨会话顺序隔离（T92 P2-T2）。
    by_session: std::collections::HashMap<String, SessionTabs>,
    /// 当前绑定的逻辑会话 id；`with_tab`/`tabs`/`switch`/`close` 都作用于它。
    current: String,
    /// 上次同步时见到的浏览器级标签总数；用于「数量严格增加才跟随最新（弹窗归当前会话）」。
    seen_tabs: usize,
}

impl Session {
    /// 当前会话的标签集合（current 必定存在，launch/bind 已保证）。
    fn cur(&self) -> Option<&SessionTabs> {
        self.by_session.get(&self.current)
    }
    fn cur_mut(&mut self) -> Option<&mut SessionTabs> {
        self.by_session.get_mut(&self.current)
    }
    /// 当前会话的活动 tab 克隆。
    fn cur_tab(&self) -> Option<Arc<Tab>> {
        self.cur().and_then(|st| st.tabs.get(st.active).cloned())
    }
}

/// 真机 CDP 控制器。`Send + Sync` 由 `Mutex` 保证。
pub struct CdpController {
    session: Mutex<Option<Session>>,
    /// 构造时的会话工作区；下载默认落到 `<workspace>/downloads`。
    workspace: std::path::PathBuf,
    /// 运行期下载目录覆盖（per-run）。有值时优先于 `workspace`。
    download_override: Mutex<Option<std::path::PathBuf>>,
    /// 无头模式（true=不弹可见窗口）。
    headless: bool,
}

impl CdpController {
    pub fn new(workspace: std::path::PathBuf, headless: bool) -> Self {
        Self {
            session: Mutex::new(None),
            workspace,
            download_override: Mutex::new(None),
            headless,
        }
    }

    /// 当前生效的下载目录：优先 per-run 覆盖，否则构造时 workspace 下的 `downloads`。
    fn current_download_dir(&self) -> std::path::PathBuf {
        self.download_override
            .lock()
            .unwrap()
            .clone()
            .unwrap_or_else(|| self.workspace.join("downloads"))
    }

    /// 若尚未启动，则用默认独立配置档启动（懒启动：模型首个动作即开窗）。
    fn ensure_session(&self) -> Result<(), BrowserError> {
        if self.session.lock().unwrap().is_some() {
            return Ok(());
        }
        self.launch(&LaunchOptions { user_data_dir: default_profile_dir() })
    }

    /// 取**当前绑定会话**的活动 tab 克隆（`Arc`），未启动则懒启动后返回。
    fn with_tab(&self) -> Result<Arc<Tab>, BrowserError> {
        // 确保会话存在（懒启动）；ensure_session 检查后释放锁再调用 launch，无死锁风险。
        self.ensure_session()?;
        let guard = self.session.lock().unwrap();
        match guard.as_ref().and_then(|s| s.cur_tab()) {
            Some(tab) => Ok(tab),
            None => Err(BrowserError::Backend("浏览器未启动".into())),
        }
    }

    /// 克隆出**当前会话**的标签集（`Arc`），及其活动序号。每会话独立列表（T92 P2-T2）。
    fn current_tabs(&self) -> Result<(Vec<Arc<Tab>>, usize), BrowserError> {
        let guard = self.session.lock().unwrap();
        let s = guard
            .as_ref()
            .ok_or_else(|| BrowserError::Backend("浏览器未启动".into()))?;
        let st = s
            .cur()
            .ok_or_else(|| BrowserError::Backend("当前会话无标签".into()))?;
        Ok((st.tabs.clone(), st.active))
    }

    /// 自动跟随：若**浏览器级**标签数量自上次同步后严格增加（说明当前会话的动作开了新标签/弹窗），
    /// 把新标签归属给当前绑定会话并设为其活动标签（弹窗属于谁绑定就归谁）。
    /// 数量不变或减少则不动，避免误跟随。内部不持锁做 CDP 调用：仅读 `get_tabs()` 长度与克隆最后一个 `Arc`。
    fn sync_active_to_newest(&self) {
        // 读取浏览器级全量 tab（与 seen_tabs 同口径）。
        let all = {
            let guard = self.session.lock().unwrap();
            let Some(s) = guard.as_ref() else { return };
            let Ok(tabs) = s._browser.get_tabs().lock().map(|g| g.clone()) else {
                return;
            };
            tabs
        };
        let len = all.len();
        if len == 0 {
            return;
        }
        let mut guard = self.session.lock().unwrap();
        if let Some(s) = guard.as_mut() {
            if len > s.seen_tabs {
                // 新出现的标签归当前会话：追加并设为其活动标签。
                let newest = all[len - 1].clone();
                if let Some(st) = s.cur_mut() {
                    st.tabs.push(newest);
                    st.active = st.tabs.len() - 1;
                }
            }
            s.seen_tabs = len;
        }
    }
}

/// 独立的浏览器可用性探测（命令层用，无需已启动的控制器）。
pub fn detect_status() -> super::BrowserStatus {
    if chrome_path().is_some() {
        super::BrowserStatus::Ready
    } else {
        super::BrowserStatus::NotInstalled
    }
}

/// 自动化配置档默认目录（独立于用户日常 Chrome，登录态在此持久）。
fn default_profile_dir() -> String {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    format!("{home}/.siliconworker/browser-profile")
}

/// 探测本机可用的 Chrome/Chromium/Edge 二进制，返回首个存在者。
fn chrome_path() -> Option<std::path::PathBuf> {
    use std::path::PathBuf;
    #[cfg(target_os = "macos")]
    let cands: Vec<PathBuf> = [
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Chromium.app/Contents/MacOS/Chromium",
        "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
    ]
    .iter()
    .map(PathBuf::from)
    .collect();
    // Windows：用环境变量定位（盘符/「Program Files」本地化名都可能不同），
    // 覆盖系统级 + 用户级（LOCALAPPDATA）安装，Edge 兜底。
    #[cfg(target_os = "windows")]
    let cands: Vec<PathBuf> = {
        let pf = std::env::var("ProgramFiles").unwrap_or_else(|_| r"C:\Program Files".into());
        let pf86 =
            std::env::var("ProgramFiles(x86)").unwrap_or_else(|_| r"C:\Program Files (x86)".into());
        let mut v = vec![
            PathBuf::from(format!(r"{pf}\Google\Chrome\Application\chrome.exe")),
            PathBuf::from(format!(r"{pf86}\Google\Chrome\Application\chrome.exe")),
        ];
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            v.push(PathBuf::from(format!(
                r"{local}\Google\Chrome\Application\chrome.exe"
            )));
        }
        v.push(PathBuf::from(format!(
            r"{pf}\Microsoft\Edge\Application\msedge.exe"
        )));
        v.push(PathBuf::from(format!(
            r"{pf86}\Microsoft\Edge\Application\msedge.exe"
        )));
        v
    };
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let cands: Vec<PathBuf> = [
        "/usr/bin/google-chrome",
        "/usr/bin/chromium",
        "/usr/bin/chromium-browser",
    ]
    .iter()
    .map(PathBuf::from)
    .collect();
    cands.into_iter().find(|p| p.exists())
}

/// 对给定 tab 设置 CDP 下载行为，落盘到 `dir`。最佳努力：失败仅记日志、不阻断。
/// 由 `launch`（建会话时）与 `set_download_dir`（运行期改目录时）共用。
fn apply_download_behavior(tab: &Tab, dir: &std::path::Path) {
    if let Err(e) = std::fs::create_dir_all(dir) {
        eprintln!("[browser] 创建下载目录失败({}): {e}", dir.display());
        return;
    }
    use headless_chrome::protocol::cdp::Browser::{
        SetDownloadBehavior, SetDownloadBehaviorBehaviorOption,
    };
    let result = tab.call_method(SetDownloadBehavior {
        behavior: SetDownloadBehaviorBehaviorOption::Allow,
        browser_context_id: None,
        // 绝对路径字符串：create_dir_all 已确保目录存在。
        download_path: Some(dir.to_string_lossy().into_owned()),
        events_enabled: None,
    });
    if let Err(e) = result {
        eprintln!("[browser] 设置下载目录失败（下载功能降级，不影响其它操作）: {e}");
    }
}

/// 把 [`ElementTarget`] 解析为 CSS 选择器。Ref 走快照写入的 `data-siw-ref` 属性。
fn resolve_selector(target: &ElementTarget) -> String {
    match target {
        ElementTarget::Ref(n) => format!("[data-siw-ref=\"{n}\"]"),
        ElementTarget::Selector(s) => s.clone(),
    }
}

/// 把 `evaluate` 返回的 RemoteObject.value（JSON 字符串）取出为 `&str`。
fn value_as_string(v: Option<serde_json::Value>) -> Result<String, BrowserError> {
    match v {
        Some(serde_json::Value::String(s)) => Ok(s),
        Some(other) => Ok(other.to_string()),
        None => Err(BrowserError::Backend("脚本无返回值".into())),
    }
}

// ---- 快照 JS（注入一次，返回 JSON 字符串）----
const SNAPSHOT_JS: &str = r#"
(function () {
  var MAX = 150;
  var sel = 'a, button, input, textarea, select, [role], h1, h2, h3, label, p';
  var nodes = document.querySelectorAll(sel);
  var out = [];
  var truncated = false;
  var id = 0;
  function visible(el) {
    if (el.offsetParent !== null) return true;
    return el.getClientRects().length > 0;
  }
  function coarseRole(el) {
    var r = el.getAttribute('role');
    if (r) return r.toLowerCase();
    return el.tagName.toLowerCase();
  }
  function nameOf(el) {
    var t = (el.innerText || el.textContent || '').trim();
    if (!t) t = (el.getAttribute('aria-label') || '').trim();
    if (!t) t = (el.getAttribute('placeholder') || '').trim();
    if (!t && (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA')) t = (el.value || '').trim();
    if (t.length > 80) t = t.slice(0, 80);
    return t;
  }
  function valueOf(el) {
    if (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA' || el.tagName === 'SELECT') {
      return el.value == null ? null : String(el.value);
    }
    return null;
  }
  function submitsOf(el) {
    if (el.matches('button[type=submit], input[type=submit], input[type=image]')) return true;
    if (el.tagName === 'BUTTON' && !el.hasAttribute('type') && !!el.closest('form')) return true;
    return false;
  }
  for (var i = 0; i < nodes.length; i++) {
    var el = nodes[i];
    if (!visible(el)) continue;
    if (out.length >= MAX) { truncated = true; break; }
    id += 1;
    el.setAttribute('data-siw-ref', id);
    out.push({
      id: id,
      role: coarseRole(el),
      name: nameOf(el),
      value: valueOf(el),
      selector: '[data-siw-ref="' + id + '"]',
      submits: submitsOf(el)
    });
  }
  return JSON.stringify({
    url: location.href,
    title: document.title,
    truncated: truncated,
    elements: out
  });
})()
"#;

#[derive(Deserialize)]
struct RawElement {
    id: u32,
    role: String,
    name: String,
    value: Option<String>,
    selector: String,
    submits: bool,
}

#[derive(Deserialize)]
struct RawSnapshot {
    url: String,
    title: String,
    truncated: bool,
    elements: Vec<RawElement>,
}

impl BrowserController for CdpController {
    fn launch(&self, opts: &LaunchOptions) -> Result<(), BrowserError> {
        let chrome = chrome_path().ok_or(BrowserError::NotInstalled)?;
        let launch = LaunchOptionsBuilder::default()
            .headless(self.headless)
            .sandbox(false)
            .path(Some(chrome))
            .user_data_dir(Some(std::path::PathBuf::from(&opts.user_data_dir)))
            .build()
            .map_err(|e| BrowserError::LaunchFailed(e.to_string()))?;
        let browser = Browser::new(launch).map_err(|e| BrowserError::LaunchFailed(e.to_string()))?;
        let tab = browser
            .new_tab()
            .map_err(|e| BrowserError::LaunchFailed(e.to_string()))?;
        // 下载落到当前生效目录（per-run 覆盖优先，否则 `<workspace>/downloads`）；
        // 非致命：失败仅记日志，不阻断启动。
        apply_download_behavior(&tab, &self.current_download_dir());
        // 替换既有会话（旧 Browser drop 即关闭旧 Chrome）。
        // 首个 tab 落到默认会话桶（""）：bind 之前的动作（懒启动后首动作）也能工作。
        let mut by_session = std::collections::HashMap::new();
        by_session.insert(
            DEFAULT_SESSION.to_string(),
            SessionTabs { tabs: vec![tab], active: 0 },
        );
        *self.session.lock().unwrap() = Some(Session {
            _browser: browser,
            by_session,
            current: DEFAULT_SESSION.to_string(),
            seen_tabs: 1,
        });
        Ok(())
    }

    fn navigate(&self, url: &str) -> Result<(), BrowserError> {
        let tab = self.with_tab()?;
        tab.navigate_to(url)
            .map_err(|e| BrowserError::Backend(e.to_string()))?;
        tab.wait_until_navigated()
            .map_err(|_| BrowserError::NavigationTimeout)?;
        self.sync_active_to_newest();
        Ok(())
    }

    fn snapshot(&self) -> Result<DomSnapshot, BrowserError> {
        let tab = self.with_tab()?;
        let obj = tab
            .evaluate(SNAPSHOT_JS, false)
            .map_err(|e| BrowserError::Backend(e.to_string()))?;
        let json = value_as_string(obj.value)?;
        let raw: RawSnapshot = serde_json::from_str(&json)
            .map_err(|e| BrowserError::Backend(format!("快照解析失败: {e}")))?;
        let elements: Vec<DomElement> = raw
            .elements
            .into_iter()
            .map(|r| DomElement {
                id: r.id,
                role: r.role,
                name: r.name,
                value: r.value,
                selector: r.selector,
                submits: r.submits,
            })
            .collect();
        // coverage_hint 为 P2 视觉兜底预留的稀疏度提示，这里保持简单。
        let coverage_hint = if elements.is_empty() {
            0.0
        } else {
            1.0
        };
        Ok(DomSnapshot {
            url: raw.url,
            title: raw.title,
            elements,
            truncated: raw.truncated,
            coverage_hint,
        })
    }

    fn click(&self, target: &ElementTarget) -> Result<(), BrowserError> {
        let tab = self.with_tab()?;
        let selector = resolve_selector(target);
        let el = tab.find_element(&selector).map_err(|_| match target {
            ElementTarget::Ref(n) => BrowserError::ElementNotFound(*n),
            ElementTarget::Selector(s) => BrowserError::SelectorNoMatch(s.clone()),
        })?;
        el.click()
            .map_err(|e| BrowserError::Backend(e.to_string()))?;
        self.sync_active_to_newest();
        Ok(())
    }

    fn fill(&self, target: &ElementTarget, text: &str) -> Result<(), BrowserError> {
        let tab = self.with_tab()?;
        let selector = resolve_selector(target);
        let el = tab.find_element(&selector).map_err(|_| match target {
            ElementTarget::Ref(n) => BrowserError::ElementNotFound(*n),
            ElementTarget::Selector(s) => BrowserError::SelectorNoMatch(s.clone()),
        })?;
        // 聚焦并清空既有值，再输入（type_into 会聚焦，但先清空避免追加）。
        el.click().ok();
        el.call_js_fn("function(){this.value='';}", vec![], false)
            .ok();
        el.type_into(text)
            .map_err(|e| BrowserError::Backend(e.to_string()))?;
        self.sync_active_to_newest();
        Ok(())
    }

    fn select(&self, target: &ElementTarget, value: &str) -> Result<(), BrowserError> {
        let tab = self.with_tab()?;
        let selector = resolve_selector(target);
        // 按 option 的 value 或可见文本匹配（快照向模型展示的是文本），选中后触发 change。
        // 选择器与目标串都经 JSON 转义，防注入。
        let sel_lit =
            serde_json::to_string(&selector).map_err(|e| BrowserError::Backend(e.to_string()))?;
        let val_lit =
            serde_json::to_string(value).map_err(|e| BrowserError::Backend(e.to_string()))?;
        let js = format!(
            "(() => {{const e=document.querySelector({sel_lit}); if(!e || e.tagName!=='SELECT') return false; const want={val_lit}; const opt=Array.from(e.options).find(o => o.value===want || o.text.trim()===want); if(!opt) return false; e.value=opt.value; e.dispatchEvent(new Event('change',{{bubbles:true}})); return true;}})()"
        );
        let obj = tab
            .evaluate(&js, false)
            .map_err(|e| BrowserError::Backend(e.to_string()))?;
        match obj.value {
            Some(serde_json::Value::Bool(true)) => {
                self.sync_active_to_newest();
                Ok(())
            }
            _ => Err(BrowserError::Backend(format!("未找到匹配的下拉选项: {value}"))),
        }
    }

    fn scroll(&self, dx: i32, dy: i32) -> Result<(), BrowserError> {
        let tab = self.with_tab()?;
        tab.evaluate(&format!("window.scrollBy({dx},{dy})"), false)
            .map_err(|e| BrowserError::Backend(e.to_string()))?;
        Ok(())
    }

    fn extract(&self, query: &ExtractQuery) -> Result<String, BrowserError> {
        let tab = self.with_tab()?;
        let text = match &query.selector {
            Some(sel) => {
                let el = tab
                    .find_element(sel)
                    .map_err(|_| BrowserError::SelectorNoMatch(sel.clone()))?;
                el.get_inner_text()
                    .map_err(|e| BrowserError::Backend(e.to_string()))?
            }
            None => {
                let obj = tab
                    .evaluate("document.body ? document.body.innerText : ''", false)
                    .map_err(|e| BrowserError::Backend(e.to_string()))?;
                value_as_string(obj.value)?
            }
        };
        // 截断到约 10k 字符（按字符边界，避免切坏 UTF-8）。
        let capped: String = text.chars().take(10_000).collect();
        Ok(capped)
    }

    fn wait(&self, condition: &WaitCondition) -> Result<(), BrowserError> {
        match condition {
            WaitCondition::Selector(s) => {
                let tab = self.with_tab()?;
                tab.wait_for_element_with_custom_timeout(s, Duration::from_secs(10))
                    .map_err(|_| BrowserError::NavigationTimeout)?;
                Ok(())
            }
            WaitCondition::Millis(ms) => {
                std::thread::sleep(Duration::from_millis((*ms).min(10_000)));
                Ok(())
            }
        }
    }

    fn back(&self) -> Result<(), BrowserError> {
        let tab = self.with_tab()?;
        tab.evaluate("history.back()", false)
            .map_err(|e| BrowserError::Backend(e.to_string()))?;
        self.sync_active_to_newest();
        Ok(())
    }

    fn tabs(&self) -> Result<Vec<TabInfo>, BrowserError> {
        // 仅列出**当前会话**自己的标签（跨会话隔离：不暴露别的会话的标签）。
        let (tabs, active) = self.current_tabs()?;
        let infos = tabs
            .iter()
            .enumerate()
            .map(|(index, t)| TabInfo {
                index,
                title: t.get_title().unwrap_or_default(),
                url: t.get_url(),
                active: index == active,
            })
            .collect();
        Ok(infos)
    }

    fn switch_tab(&self, index: usize) -> Result<(), BrowserError> {
        // 在**当前会话**自己的标签集内切换。
        let (tabs, _active) = self.current_tabs()?;
        if index >= tabs.len() {
            return Err(BrowserError::Backend("标签序号越界".into()));
        }
        let target = tabs[index].clone();
        // 锁外做 bring_to_front 的 CDP 调用。
        target
            .bring_to_front()
            .map_err(|e| BrowserError::Backend(e.to_string()))?;
        let mut guard = self.session.lock().unwrap();
        if let Some(st) = guard.as_mut().and_then(|s| s.cur_mut()) {
            st.active = index;
        }
        Ok(())
    }

    fn close_tab(&self, index: usize) -> Result<(), BrowserError> {
        // 在**当前会话**自己的标签集内关闭。
        let (tabs, active) = self.current_tabs()?;
        if index >= tabs.len() {
            return Err(BrowserError::Backend("标签序号越界".into()));
        }
        // 锁外做 close 的 CDP 调用。
        tabs[index]
            .close(true)
            .map_err(|e| BrowserError::Backend(e.to_string()))?;
        // 从本会话标签列表移除已关标签，并夹紧活动序号，确保不悬挂到已关标签。
        let mut guard = self.session.lock().unwrap();
        if let Some(st) = guard.as_mut().and_then(|s| s.cur_mut()) {
            if index < st.tabs.len() {
                st.tabs.remove(index);
            }
            if st.tabs.is_empty() {
                st.active = 0;
            } else {
                // 关掉活动标签或其前面的标签都会使原序号失效：夹紧到合法范围。
                let new_active = if index < active {
                    active - 1
                } else if active >= st.tabs.len() {
                    st.tabs.len() - 1
                } else {
                    active
                };
                st.active = new_active;
            }
        }
        Ok(())
    }

    fn describe_submits(&self, selector: &str) -> Result<bool, BrowserError> {
        let tab = self.with_tab()?;
        let sel_lit =
            serde_json::to_string(selector).map_err(|e| BrowserError::Backend(e.to_string()))?;
        let js = format!(
            "(function(){{var e=document.querySelector({sel_lit}); if(!e) return false; if(e.matches('button[type=submit], input[type=submit], input[type=image]')) return true; if(e.tagName==='BUTTON' && !e.hasAttribute('type') && !!e.closest('form')) return true; return false;}})()"
        );
        let obj = tab
            .evaluate(&js, false)
            .map_err(|e| BrowserError::Backend(e.to_string()))?;
        Ok(matches!(obj.value, Some(serde_json::Value::Bool(true))))
    }

    fn status(&self) -> BrowserStatus {
        if self.session.lock().unwrap().is_some() {
            BrowserStatus::Running
        } else if chrome_path().is_some() {
            BrowserStatus::Ready
        } else {
            BrowserStatus::NotInstalled
        }
    }

    /// 运行期改下载目录：先存覆盖；若已有活动会话则即时对当前 tab 下发 CDP
    /// `Browser::SetDownloadBehavior`（最佳努力，非致命）。无活动会话时只存，下次 launch 生效。
    fn open_window(&self) -> Result<(), BrowserError> {
        // 真正开窗：懒启动 Chrome（默认独立配置档），用户随后可在窗口里手动登录常用站点。
        self.ensure_session()
    }

    fn set_download_dir(&self, dir: std::path::PathBuf) {
        *self.download_override.lock().unwrap() = Some(dir.clone());
        let tab = {
            let guard = self.session.lock().unwrap();
            guard.as_ref().and_then(|s| s.cur_tab())
        };
        if let Some(tab) = tab {
            apply_download_behavior(&tab, &dir);
        }
    }

    /// 绑定逻辑会话（T92 P2-T2）：把后续动作路由到该会话**自己的** tab。
    /// 首次见到的会话为其 `new_tab()` 建一张专属标签；之后把 `current` 切到它并 `bring_to_front()`。
    /// 最佳努力/幂等：失败不抛（真实动作随后会暴露错误）。
    fn bind_session(&self, session_id: &str) {
        // 懒启动（ensure_session 内部释放锁后再 launch，无死锁）。
        if self.ensure_session().is_err() {
            return;
        }
        // 1) 该会话已有标签 → 仅切 current + 取活动 tab 去置顶；否则记录需新建。
        let existing = {
            let mut guard = self.session.lock().unwrap();
            let Some(s) = guard.as_mut() else { return };
            if s.by_session.contains_key(session_id) {
                s.current = session_id.to_string();
                s.cur_tab()
            } else {
                None
            }
        };
        if let Some(tab) = existing {
            // 锁外做 bring_to_front 的 CDP 调用（最佳努力）。
            let _ = tab.bring_to_front();
            return;
        }
        // 2) 首见会话：锁外 new_tab（长 CDP 调用），再持锁登记。
        let browser = {
            let guard = self.session.lock().unwrap();
            // 注意：Browser 内部为 Arc 句柄，clone 仅克隆句柄、不复制进程。
            guard.as_ref().map(|s| s._browser.clone())
        };
        let Some(browser) = browser else { return };
        let new_tab = match browser.new_tab() {
            Ok(t) => t,
            Err(_) => return,
        };
        // 新标签套用当前下载目录（最佳努力）。
        apply_download_behavior(&new_tab, &self.current_download_dir());
        let _ = new_tab.bring_to_front();
        let mut guard = self.session.lock().unwrap();
        if let Some(s) = guard.as_mut() {
            // 并发兜底：若期间别处已为该会话建桶，则不覆盖（保留先到者）。
            s.by_session
                .entry(session_id.to_string())
                .or_insert_with(|| SessionTabs {
                    tabs: vec![new_tab],
                    active: 0,
                });
            s.current = session_id.to_string();
            // 浏览器新增了一张标签：同步 seen_tabs，避免 sync_active_to_newest 把这张
            // bind 建的标签当成「弹窗」再追加一次。
            if let Ok(all) = s._browser.get_tabs().lock().map(|g| g.len()) {
                s.seen_tabs = all;
            }
        }
    }
}

#[cfg(test)]
mod smoke {
    use super::*;
    use crate::browser::{BrowserController, ExtractQuery, LaunchOptions};

    #[test]
    #[ignore = "needs real Chrome; run with --ignored"]
    fn navigate_and_snapshot_real_chrome() {
        let c = CdpController::new(std::env::temp_dir().join("siw-t85-cdp-smoke-ws"), false);
        let dir = std::env::temp_dir().join("siw-t85-cdp-smoke");
        c.launch(&LaunchOptions {
            user_data_dir: dir.to_string_lossy().into(),
        })
        .unwrap();
        c.navigate("https://example.com").unwrap();
        let snap = c.snapshot().unwrap();
        assert!(snap.title.contains("Example"));
        assert!(!snap.elements.is_empty());
        // ref→selector round trip
        let first = snap.elements[0].id;
        c.click(&crate::browser::ElementTarget::Ref(first)).ok();
        let body = c.extract(&ExtractQuery { selector: None }).unwrap();
        assert!(body.contains("Example") || body.contains("domain"));
    }

    /// P2 smoke: 验证下载目录创建 + 多标签页 API 正确性。
    ///
    /// 下载目录：launch 内部调用 `create_dir_all(<workspace>/downloads)`，
    /// 本测试只断言目录存在，不触发真实下载（触发下载需要服务端配合，过于脆弱）。
    ///
    /// 多标签：`tabs()` 返回 ≥1 项且恰好有 1 项 active==true；
    /// `switch_tab(0)` 返回 Ok（切换到同一标签也算切换成功）。
    /// 不通过 window.open 触发弹窗——需要弹窗权限且结果不稳定，保守断言更健壮。
    #[test]
    #[ignore = "needs real Chrome; run with --ignored"]
    fn p2_download_dir_and_tabs() {
        let ws = std::env::temp_dir().join("siw-t85-p2-ws");
        let profile = std::env::temp_dir().join("siw-t85-p2-profile");
        let c = CdpController::new(ws.clone(), false);
        c.launch(&LaunchOptions {
            user_data_dir: profile.to_string_lossy().into(),
        })
        .unwrap();

        // --- 下载目录 ---
        let downloads = ws.join("downloads");
        assert!(
            downloads.exists(),
            "downloads 目录应在 launch 后创建：{}",
            downloads.display()
        );

        // --- 多标签页 API ---
        c.navigate("https://example.com").unwrap();
        let tab_list = c.tabs().unwrap();
        assert!(
            tab_list.len() >= 1,
            "tabs() 应返回至少 1 项，实际：{}",
            tab_list.len()
        );
        let active_count = tab_list.iter().filter(|t| t.active).count();
        assert_eq!(active_count, 1, "恰好应有 1 个 active 标签，实际：{active_count}");

        // switch_tab(0) 切换到序号 0 的标签（即当前唯一标签），应成功。
        c.switch_tab(0).unwrap();
    }

    /// P2 smoke: 验证无头模式（headless=true）可正常驱动 Chrome 完成导航与快照。
    ///
    /// 无头模式不弹可见窗口，适合后台抓取/检索场景。
    /// 本测试在设置项「无头模式」开启时的代码路径上跑完整的 navigate→snapshot，
    /// 断言标题与元素集合，与有头模式结果等价。
    #[test]
    #[ignore = "needs real Chrome; run with --ignored"]
    fn p2_headless_navigate_and_snapshot() {
        let ws = std::env::temp_dir().join("siw-t85-p2-headless-ws");
        let profile = std::env::temp_dir().join("siw-t85-p2-headless-profile");
        let c = CdpController::new(ws, true); // headless = true
        c.launch(&LaunchOptions {
            user_data_dir: profile.to_string_lossy().into(),
        })
        .unwrap();
        c.navigate("https://example.com").unwrap();
        let snap = c.snapshot().unwrap();
        assert!(
            snap.title.contains("Example"),
            "无头模式快照 title 应含 'Example'，实际：{}",
            snap.title
        );
        assert!(
            !snap.elements.is_empty(),
            "无头模式快照 elements 不应为空"
        );
    }

    /// T92 P2-T2 smoke: 每会话独立 tab → 跨会话顺序隔离。
    /// bind "a" → 导航 example.com；bind "b" → 导航 example.org；bind "a" 回来 →
    /// 快照仍是 example.com（A 的 tab 未被 B 污染）。
    #[test]
    #[ignore = "needs real Chrome; run with --ignored"]
    fn p2_per_session_tab_isolation() {
        let ws = std::env::temp_dir().join("siw-t92-p2-iso-ws");
        let profile = std::env::temp_dir().join("siw-t92-p2-iso-profile");
        let c = CdpController::new(ws, true); // headless
        c.launch(&LaunchOptions {
            user_data_dir: profile.to_string_lossy().into(),
        })
        .unwrap();

        c.bind_session("a");
        c.navigate("https://example.com").unwrap();
        let sa = c.snapshot().unwrap();
        assert!(sa.url.contains("example.com"), "A 应在 example.com，实际 {}", sa.url);

        c.bind_session("b");
        c.navigate("https://example.org").unwrap();
        let sb = c.snapshot().unwrap();
        assert!(sb.url.contains("example.org"), "B 应在 example.org，实际 {}", sb.url);

        // 回到 A：其 tab 不应被 B 的导航覆写。
        c.bind_session("a");
        let sa2 = c.snapshot().unwrap();
        assert!(
            sa2.url.contains("example.com"),
            "回到 A 后应仍是 example.com（未被 B 污染），实际 {}",
            sa2.url
        );

        // A 只见自己的 1 张标签（跨会话隔离）。
        let a_tabs = c.tabs().unwrap();
        assert_eq!(a_tabs.len(), 1, "A 应只见自己 1 张标签，实际 {}", a_tabs.len());
    }
}

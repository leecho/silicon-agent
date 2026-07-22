//! app 级常驻浏览器持有者：实现 `BrowserController` 的委派包装器。
//! 懒建 inner（工厂注入，便于测试与读取 headless 设置），跨 run/跨会话复用同一 Chrome；
//! 可显式 close（drop inner → Chrome 关）。取代每 run 新建 CdpController（见 T92）。
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use super::{
    BrowserController, BrowserError, BrowserStatus, DomSnapshot, ElementTarget, ExtractQuery,
    LaunchOptions, TabInfo, WaitCondition,
};

type Factory = Box<dyn Fn() -> Result<Arc<dyn BrowserController>, BrowserError> + Send + Sync>;

pub struct SharedBrowser {
    inner: Mutex<Option<Arc<dyn BrowserController>>>,
    factory: Factory,
    last_active_ms: AtomicU64,
    download_dir: Mutex<Option<std::path::PathBuf>>,
}

impl SharedBrowser {
    pub fn new(
        factory: impl Fn() -> Result<Arc<dyn BrowserController>, BrowserError> + Send + Sync + 'static,
    ) -> Self {
        Self {
            inner: Mutex::new(None),
            factory: Box::new(factory),
            last_active_ms: AtomicU64::new(0),
            download_dir: Mutex::new(None),
        }
    }

    /// 懒建 + 缓存 + 刷新 last_active；返回 inner 的 Arc clone。
    /// 锁只在「建/取」期间短暂持有；返回的 clone 在锁释放后才被调用方委派，避免跨委派持锁。
    fn obtain(&self) -> Result<Arc<dyn BrowserController>, BrowserError> {
        let arc = {
            let mut guard = self.inner.lock().unwrap();
            if guard.is_none() {
                let fresh = (self.factory)()?;
                // 新 inner 落地前，先把已存的 per-run 下载目录套上去
                // （在浏览器启动前就设过 set_download_dir 的场景也能生效）。
                if let Some(dir) = self.download_dir.lock().unwrap().clone() {
                    fresh.set_download_dir(dir);
                }
                *guard = Some(fresh);
            }
            guard.as_ref().unwrap().clone()
        };
        self.last_active_ms.store(now_ms(), Ordering::Relaxed);
        Ok(arc)
    }

    pub fn is_open(&self) -> bool {
        self.inner.lock().unwrap().is_some()
    }

    /// 显式打开浏览器窗口（懒启动 Chrome；已开则无副作用）。供「打开浏览器」按钮先行登录。
    /// 注意：obtain 只构造控制器、并不开窗；必须再调 open_window 触发真正的 Chrome 启动。
    pub fn open(&self) -> Result<(), BrowserError> {
        self.obtain()?.open_window()
    }

    /// 纯逻辑：自上次活动以来空闲毫秒（未活动过返回 0，避免误关）。
    pub fn idle_ms(&self, now_ms: u64) -> u64 {
        let last = self.last_active_ms.load(Ordering::Relaxed);
        if last == 0 {
            0
        } else {
            now_ms.saturating_sub(last)
        }
    }

    /// 是否应因空闲而关闭：开着 ∧ 阈值>0 ∧ 已空闲≥阈值。纯逻辑，便于单测。
    pub fn should_idle_close(&self, now_ms: u64, threshold_ms: u64) -> bool {
        threshold_ms > 0 && self.is_open() && self.idle_ms(now_ms) >= threshold_ms
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

impl BrowserController for SharedBrowser {
    fn launch(&self, opts: &LaunchOptions) -> Result<(), BrowserError> {
        self.obtain()?.launch(opts)
    }
    fn navigate(&self, url: &str) -> Result<(), BrowserError> {
        self.obtain()?.navigate(url)
    }
    fn snapshot(&self) -> Result<DomSnapshot, BrowserError> {
        self.obtain()?.snapshot()
    }
    fn click(&self, target: &ElementTarget) -> Result<(), BrowserError> {
        self.obtain()?.click(target)
    }
    fn fill(&self, target: &ElementTarget, text: &str) -> Result<(), BrowserError> {
        self.obtain()?.fill(target, text)
    }
    fn select(&self, target: &ElementTarget, value: &str) -> Result<(), BrowserError> {
        self.obtain()?.select(target, value)
    }
    fn scroll(&self, dx: i32, dy: i32) -> Result<(), BrowserError> {
        self.obtain()?.scroll(dx, dy)
    }
    fn extract(&self, query: &ExtractQuery) -> Result<String, BrowserError> {
        self.obtain()?.extract(query)
    }
    fn wait(&self, condition: &WaitCondition) -> Result<(), BrowserError> {
        self.obtain()?.wait(condition)
    }
    fn back(&self) -> Result<(), BrowserError> {
        self.obtain()?.back()
    }
    fn tabs(&self) -> Result<Vec<TabInfo>, BrowserError> {
        self.obtain()?.tabs()
    }
    fn switch_tab(&self, index: usize) -> Result<(), BrowserError> {
        self.obtain()?.switch_tab(index)
    }
    fn close_tab(&self, index: usize) -> Result<(), BrowserError> {
        self.obtain()?.close_tab(index)
    }
    fn describe_submits(&self, selector: &str) -> Result<bool, BrowserError> {
        self.obtain()?.describe_submits(selector)
    }
    /// 不强制建 inner——若已建则委派，否则走独立探测（避免「查状态」就开浏览器）。
    fn status(&self) -> BrowserStatus {
        if let Some(i) = self.inner.lock().unwrap().as_ref() {
            return i.status();
        }
        super::cdp::detect_status()
    }
    /// 绑定逻辑会话（T92 P2-T2）：委派给 inner（懒建）。最佳努力——obtain 失败不抛，
    /// 随后的真实动作会再次 obtain 并暴露错误。
    fn bind_session(&self, session_id: &str) {
        if let Ok(i) = self.obtain() {
            i.bind_session(session_id);
        }
    }
    /// 关闭常驻浏览器：drop inner（其 CdpController Drop 链 kill Chrome）。幂等。
    fn close(&self) {
        let _ = self.inner.lock().unwrap().take();
    }
    /// 记下 per-run 下载目录；若 inner 已建则即时委派，未建则在 obtain() 时套用。最佳努力。
    fn set_download_dir(&self, dir: std::path::PathBuf) {
        *self.download_dir.lock().unwrap() = Some(dir.clone());
        if let Some(i) = self.inner.lock().unwrap().as_ref() {
            i.set_download_dir(dir);
        }
    }
}

#[cfg(test)]
mod smoke {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[test]
    #[ignore = "needs real Chrome; run with --ignored"]
    fn resident_reuses_one_chrome_then_recreates_after_close() {
        let n = std::sync::Arc::new(AtomicU32::new(0));
        let n2 = n.clone();
        let sb = SharedBrowser::new(move || {
            let count = n2.fetch_add(1, Ordering::Relaxed) + 1;
            let dir = std::env::temp_dir()
                .join(format!("siw-t92-resident-{count}"));
            Ok(std::sync::Arc::new(
                crate::browser::cdp::CdpController::new(dir, true /* headless for smoke */),
            ) as std::sync::Arc<dyn BrowserController>)
        });

        // 两次动作复用同一 Chrome（factory 只调一次）。
        sb.navigate("https://example.com").unwrap();
        let s1 = sb.snapshot().unwrap();
        assert!(
            s1.title.contains("Example"),
            "s1.title = {:?}",
            s1.title
        );
        sb.navigate("https://example.org").unwrap();
        let s2 = sb.snapshot().unwrap();
        assert!(!s2.title.is_empty(), "s2.title should not be empty");
        assert_eq!(
            n.load(Ordering::Relaxed),
            1,
            "resident: 同一 Chrome 复用，factory 只建一次"
        );

        // close 后再动作 → 重建。
        sb.close();
        sb.navigate("https://example.com").unwrap();
        assert!(
            sb.snapshot().unwrap().title.contains("Example"),
            "close 后重建的 Chrome 应能加载 example.com"
        );
        assert_eq!(n.load(Ordering::Relaxed), 2, "close 后重建一次");
        sb.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser::mock::MockController;
    use crate::browser::DomSnapshot;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn empty_snap() -> DomSnapshot {
        DomSnapshot {
            url: "about:blank".into(),
            title: "".into(),
            elements: vec![],
            truncated: false,
            coverage_hint: 1.0,
        }
    }

    fn counting() -> (Arc<AtomicU32>, SharedBrowser) {
        let n = Arc::new(AtomicU32::new(0));
        let n2 = n.clone();
        let sb = SharedBrowser::new(move || {
            n2.fetch_add(1, Ordering::Relaxed);
            Ok(Arc::new(MockController::with_snapshot(empty_snap())) as Arc<dyn BrowserController>)
        });
        (n, sb)
    }

    #[test]
    fn lazily_creates_inner_once_and_reuses() {
        let (n, sb) = counting();
        assert_eq!(n.load(Ordering::Relaxed), 0); // 未动作不建
        sb.navigate("https://a").unwrap();
        sb.navigate("https://b").unwrap();
        assert_eq!(n.load(Ordering::Relaxed), 1); // 两次动作复用同一 inner
        assert!(sb.is_open());
    }

    #[test]
    fn close_then_action_recreates() {
        let (n, sb) = counting();
        sb.navigate("https://a").unwrap();
        sb.close();
        assert!(!sb.is_open());
        sb.navigate("https://b").unwrap();
        assert_eq!(n.load(Ordering::Relaxed), 2); // close 后重建
    }

    #[test]
    fn close_is_idempotent() {
        let (_n, sb) = counting();
        sb.close();
        sb.close(); // inner None → no-op
        assert!(!sb.is_open());
    }

    #[test]
    fn idle_ms_zero_before_first_activity() {
        let (_n, sb) = counting();
        assert_eq!(sb.idle_ms(now_ms() + 1000), 0); // 未活动 → 0
        sb.navigate("https://a").unwrap();
        let later = now_ms() + 5000;
        assert!(sb.idle_ms(later) >= 4000); // 活动后计 idle
    }

    #[test]
    fn should_idle_close_threshold_zero_never_closes() {
        let (_n, sb) = counting();
        sb.navigate("https://a").unwrap(); // 开着 + 有活动
        assert!(sb.is_open());
        let later = now_ms() + 60_000;
        // 阈值 0 → 永不关，即便开着且空闲很久。
        assert!(!sb.should_idle_close(later, 0));
    }

    #[test]
    fn should_idle_close_open_and_idle_over_threshold() {
        let (_n, sb) = counting();
        let base = now_ms();
        sb.navigate("https://a").unwrap(); // last_active ≈ base
        // 空闲 ≥ 阈值 → true。
        assert!(sb.should_idle_close(base + 5000, 4000));
        // 空闲 < 阈值 → false。
        assert!(!sb.should_idle_close(base + 1000, 4000));
    }

    #[test]
    fn should_idle_close_false_when_not_open() {
        let (_n, sb) = counting();
        assert!(!sb.is_open());
        let later = now_ms() + 60_000;
        // 没开 → false（即便阈值>0）。
        assert!(!sb.should_idle_close(later, 1000));
    }

    #[test]
    fn status_does_not_create_inner() {
        let (n, sb) = counting();
        let _ = sb.status(); // 走 detect_status，不建 inner
        assert_eq!(n.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn set_download_dir_stores_path() {
        let (_n, sb) = counting();
        sb.set_download_dir(std::path::PathBuf::from("/tmp/dl"));
        assert_eq!(
            *sb.download_dir.lock().unwrap(),
            Some(std::path::PathBuf::from("/tmp/dl"))
        );
    }
}

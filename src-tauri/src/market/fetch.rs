//! 市场的**传输层**：HTTP 拉取 + ETag 条件请求 + 磁盘缓存 + 失败回退。
//!
//! 这里刻意**不含任何领域概念** —— 没有货架、没有插件/技能/专家/团队。
//! 四个市场（插件 / 技能 / 专家 / 团队）各有各的类型与服务，但都得发 HTTP、都得缓存，
//! 共用这一层不会把它们耦在一起：它进去是 URL，出来是字节。

use std::path::PathBuf;

use sha2::{Digest, Sha256};

/// 一次拉取的结果。`status_304` 为真时 body 为空、应复用缓存。
#[derive(Debug)]
pub struct Fetched {
    pub status_304: bool,
    pub body: Vec<u8>,
    pub etag: Option<String>,
}

/// 拉取抽象：给定 URL 与可选的已缓存 ETag，返回 body 或 304。
///
/// 抽成 trait 只为一件事：让市场逻辑**可以不打真实网络地单测**（测试注入内存实现）。
pub trait Fetcher: Send + Sync {
    fn get(&self, url: &str, etag: Option<&str>) -> Result<Fetched, String>;
}

/// 生产实现：基于统一 HttpClient 的阻塞 GET。**仅允许 https**；带 ETag 时发 `If-None-Match`。
pub struct HttpFetcher;

impl Fetcher for HttpFetcher {
    fn get(&self, url: &str, etag: Option<&str>) -> Result<Fetched, String> {
        // https-only：市场是不可信来源，明文传输等于让中间人换包。
        if !url.starts_with("https://") {
            return Err(format!("市场地址必须为 https：{url}"));
        }
        use std::time::Duration;
        let mut req = crate::http::HttpRequest::get(url).timeout(Duration::from_secs(20));
        if let Some(tag) = etag {
            req = req.header("If-None-Match", tag);
        }
        let resp = crate::http::HttpClient::new()
            .send(req)
            .map_err(|e| e.to_string())?;
        if resp.status == 304 {
            return Ok(Fetched {
                status_304: true,
                body: Vec::new(),
                etag: etag.map(str::to_string),
            });
        }
        if !resp.is_success() {
            return Err(format!("HTTP {}", resp.status));
        }
        Ok(Fetched {
            status_304: false,
            etag: resp.header("ETag").map(str::to_string),
            body: resp.body,
        })
    }
}

/// 带磁盘缓存的取字节器。每个 URL 一个缓存文件（`{cache_dir}/{sha256(url)}`，
/// ETag 存在 `.etag` 旁文件）。
pub struct CachedHttp {
    cache_dir: Option<PathBuf>,
    fetcher: Box<dyn Fetcher>,
}

impl CachedHttp {
    /// 带磁盘缓存（静态市场仓用：内容稳定、ETag 命中率高）。
    pub fn cached(cache_dir: PathBuf) -> Self {
        Self {
            cache_dir: Some(cache_dir),
            fetcher: Box::new(HttpFetcher),
        }
    }

    /// 不缓存（REST 市场用：分页列表按查询变化、下载是一次性的，缓存没有收益）。
    pub fn direct() -> Self {
        Self {
            cache_dir: None,
            fetcher: Box::new(HttpFetcher),
        }
    }

    /// 注入 Fetcher（测试用内存实现替换真实网络）。
    pub fn with_fetcher(cache_dir: Option<PathBuf>, fetcher: Box<dyn Fetcher>) -> Self {
        Self { cache_dir, fetcher }
    }

    /// 取一个 URL 的字节。
    ///
    /// 无缓存目录 → 直取。有缓存目录 → 条件请求；304 复用缓存；
    /// **拉取失败且有缓存 → 回退缓存**（断网时市场仍能浏览，而不是白屏）。
    pub fn get(&self, url: &str) -> Result<Vec<u8>, String> {
        let Some(dir) = self.cache_dir.as_ref() else {
            return Ok(self.fetcher.get(url, None)?.body);
        };

        let key = cache_key(url);
        let body_path = dir.join(&key);
        let etag_path = dir.join(format!("{key}.etag"));
        let cached_etag = std::fs::read_to_string(&etag_path)
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        match self.fetcher.get(url, cached_etag.as_deref()) {
            Ok(f) if f.status_304 => std::fs::read(&body_path)
                .map_err(|e| format!("拉取 {url} 返回 304 但本地缓存缺失：{e}")),
            Ok(f) => {
                let _ = std::fs::create_dir_all(dir);
                let _ = std::fs::write(&body_path, &f.body);
                match &f.etag {
                    Some(tag) => {
                        let _ = std::fs::write(&etag_path, tag);
                    }
                    None => {
                        let _ = std::fs::remove_file(&etag_path);
                    }
                }
                Ok(f.body)
            }
            Err(e) => match std::fs::read(&body_path) {
                Ok(body) => {
                    eprintln!("[market] 拉取 {url} 失败（{e}），回退本地缓存");
                    Ok(body)
                }
                Err(_) => Err(format!("拉取 {url} 失败且无本地缓存：{e}")),
            },
        }
    }
}

/// URL → 缓存文件名（sha256 十六进制）。
fn cache_key(url: &str) -> String {
    let digest = Sha256::digest(url.as_bytes());
    let mut s = String::with_capacity(64);
    for b in digest {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// 最小 percent-encoding：只保留 URL 安全字符（关键词可能含中文与空格）。
pub fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
pub(crate) mod testing {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// 内存 Fetcher：url → body。同时记录请求过的 URL，
    /// 便于断言「分页/搜索确实下推到了服务端」「浏览没有逐包拉文件」这类**行为**。
    pub struct MapFetcher {
        pub map: HashMap<String, Vec<u8>>,
        pub requested: Mutex<Vec<String>>,
        /// 这些 URL 一律失败（模拟断网 / 404）。
        pub errors: std::collections::HashSet<String>,
    }

    impl MapFetcher {
        pub fn new(entries: Vec<(String, Vec<u8>)>) -> Self {
            Self {
                map: entries.into_iter().collect(),
                requested: Mutex::new(Vec::new()),
                errors: std::collections::HashSet::new(),
            }
        }
        pub fn requested(&self) -> Vec<String> {
            self.requested.lock().unwrap().clone()
        }
    }

    impl Fetcher for MapFetcher {
        fn get(&self, url: &str, _etag: Option<&str>) -> Result<Fetched, String> {
            self.requested.lock().unwrap().push(url.to_string());
            if self.errors.contains(url) {
                return Err(format!("模拟拉取失败：{url}"));
            }
            match self.map.get(url) {
                Some(b) => Ok(Fetched {
                    status_304: false,
                    body: b.clone(),
                    etag: None,
                }),
                None => Err(format!("map 中无此 URL：{url}")),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urlencode_escapes_non_ascii_and_spaces() {
        assert_eq!(urlencode("tdd"), "tdd");
        assert_eq!(urlencode("测试"), "%E6%B5%8B%E8%AF%95");
        assert_eq!(urlencode("a b"), "a%20b");
    }

    #[test]
    fn rejects_non_https() {
        let http = CachedHttp::direct();
        assert!(
            http.get("http://insecure.example.com/x").is_err(),
            "明文来源必须拒绝——市场是不可信输入"
        );
    }
}

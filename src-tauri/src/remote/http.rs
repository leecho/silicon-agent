//! 远程 connector 的 HTTP 边界：抽成 trait 以便单测 mock，不打真实网络。
//! 真实实现用统一 HttpClient（与全仓一致，同步门面）。

/// 最小 HTTP 客户端：POST/GET JSON，返回响应体字符串。
pub trait HttpClient: Send + Sync {
    fn post_json(&self, url: &str, body: &str, headers: &[(&str, &str)]) -> Result<String, String>;
    fn get_json(&self, url: &str, headers: &[(&str, &str)]) -> Result<String, String>;
}

/// 统一 HttpClient 实现。读超时按长轮询放宽（构造时传入）。
pub struct UreqHttp {
    read_timeout_ms: u64,
}

impl UreqHttp {
    pub fn new(read_timeout_ms: u64) -> Self {
        Self { read_timeout_ms }
    }
}

impl HttpClient for UreqHttp {
    fn post_json(&self, url: &str, body: &str, headers: &[(&str, &str)]) -> Result<String, String> {
        let hdrs = headers
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let resp = crate::http::HttpClient::new()
            .send(
                crate::http::HttpRequest::post(url)
                    .content_type("application/json")
                    .headers(hdrs)
                    .string_body(body)
                    .timeout(std::time::Duration::from_millis(self.read_timeout_ms)),
            )
            .map_err(|e| format!("remote http error: {e}"))?;
        if !resp.is_success() {
            return Err(format!("remote http {}: {}", resp.status, resp.text()));
        }
        Ok(resp.text())
    }

    fn get_json(&self, url: &str, headers: &[(&str, &str)]) -> Result<String, String> {
        let hdrs = headers
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let resp = crate::http::HttpClient::new()
            .send(
                crate::http::HttpRequest::get(url)
                    .headers(hdrs)
                    .timeout(std::time::Duration::from_millis(self.read_timeout_ms)),
            )
            .map_err(|e| format!("remote http error: {e}"))?;
        if !resp.is_success() {
            return Err(format!("remote http {}: {}", resp.status, resp.text()));
        }
        Ok(resp.text())
    }
}

#[cfg(test)]
pub struct MockHttp {
    routes: Vec<(String, String)>, // (url 包含的子串, 响应体)
}

#[cfg(test)]
impl MockHttp {
    pub fn new(routes: Vec<(String, String)>) -> Self {
        Self { routes }
    }
}

#[cfg(test)]
impl MockHttp {
    fn route(&self, url: &str) -> Result<String, String> {
        for (frag, resp) in &self.routes {
            if url.contains(frag.as_str()) {
                return Ok(resp.clone());
            }
        }
        Err(format!("mock: no route for {url}"))
    }
}

#[cfg(test)]
impl HttpClient for MockHttp {
    fn post_json(
        &self,
        url: &str,
        _body: &str,
        _headers: &[(&str, &str)],
    ) -> Result<String, String> {
        self.route(url)
    }
    fn get_json(&self, url: &str, _headers: &[(&str, &str)]) -> Result<String, String> {
        self.route(url)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_returns_preset_body() {
        let mock = MockHttp::new(vec![("getUpdates".into(), "{\"ok\":true}".into())]);
        let body = mock
            .post_json("https://x/getUpdates", "{}", &[("h", "v")])
            .unwrap();
        assert_eq!(body, "{\"ok\":true}");
    }

    #[test]
    fn mock_get_routes_by_url() {
        let mock = MockHttp::new(vec![(
            "get_bot_qrcode".into(),
            "{\"qrcode\":\"q1\"}".into(),
        )]);
        let body = mock
            .get_json(
                "https://x/ilink/bot/get_bot_qrcode?bot_type=3",
                &[("h", "v")],
            )
            .unwrap();
        assert_eq!(body, "{\"qrcode\":\"q1\"}");
    }
}

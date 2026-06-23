//! 远程 connector 的 HTTP 边界：抽成 trait 以便单测 mock，不打真实网络。
//! 真实实现用 ureq（与 provider 一致，同步阻塞）。

/// 最小 HTTP 客户端：POST/GET JSON，返回响应体字符串。
pub trait HttpClient: Send + Sync {
    fn post_json(&self, url: &str, body: &str, headers: &[(&str, &str)]) -> Result<String, String>;
    fn get_json(&self, url: &str, headers: &[(&str, &str)]) -> Result<String, String>;
}

/// ureq 实现。连接超时 10s，读超时按长轮询放宽。
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
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(std::time::Duration::from_secs(10))
            .timeout_read(std::time::Duration::from_millis(self.read_timeout_ms))
            .build();
        let mut req = agent.post(url).set("Content-Type", "application/json");
        for (k, v) in headers {
            req = req.set(k, v);
        }
        match req.send_string(body) {
            Ok(resp) => resp
                .into_string()
                .map_err(|e| format!("read remote response: {e}")),
            Err(ureq::Error::Status(code, resp)) => {
                let detail = resp.into_string().unwrap_or_default();
                Err(format!("remote http {code}: {detail}"))
            }
            Err(e) => Err(format!("remote http error: {e}")),
        }
    }

    fn get_json(&self, url: &str, headers: &[(&str, &str)]) -> Result<String, String> {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(std::time::Duration::from_secs(10))
            .timeout_read(std::time::Duration::from_millis(self.read_timeout_ms))
            .build();
        let mut req = agent.get(url);
        for (k, v) in headers {
            req = req.set(k, v);
        }
        match req.call() {
            Ok(resp) => resp
                .into_string()
                .map_err(|e| format!("read remote response: {e}")),
            Err(ureq::Error::Status(code, resp)) => {
                let detail = resp.into_string().unwrap_or_default();
                Err(format!("remote http {code}: {detail}"))
            }
            Err(e) => Err(format!("remote http error: {e}")),
        }
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

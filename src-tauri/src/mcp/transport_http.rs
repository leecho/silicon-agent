//! Streamable HTTP 传输：单端点 POST，响应为 JSON 或 SSE 流。

use std::time::Duration;

use crate::mcp::transport::McpTransport;

pub struct HttpTransport {
    url: String,
    /// 静态请求头（含 API Key / OAuth Bearer，由 manager 构造时注入）。
    headers: Vec<(String, String)>,
    /// server 分配的会话 id（initialize 响应头带回则后续请求透传）。
    session_id: Option<String>,
}

impl HttpTransport {
    pub fn new(url: String, headers: Vec<(String, String)>) -> Self {
        Self {
            url,
            headers,
            session_id: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }
}

/// 从 SSE 文本中按事件提取 data 行拼出的 JSON 消息列表。
pub fn parse_sse_json(body: &str) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    let mut data_lines: Vec<&str> = Vec::new();
    for line in body.lines().chain(std::iter::once("")) {
        if let Some(rest) = line.strip_prefix("data:") {
            data_lines.push(rest.trim_start());
        } else if line.is_empty() && !data_lines.is_empty() {
            if let Ok(v) = serde_json::from_str(&data_lines.join("\n")) {
                out.push(v);
            }
            data_lines.clear();
        }
    }
    out
}

impl McpTransport for HttpTransport {
    fn request(
        &mut self,
        msg: &serde_json::Value,
        timeout: Duration,
    ) -> Result<Option<serde_json::Value>, String> {
        let mut req = crate::http::HttpRequest::post(self.url.clone())
            .content_type("application/json")
            .header("Accept", "application/json, text/event-stream")
            .headers(self.headers.clone())
            .string_body(msg.to_string())
            .timeout(timeout);
        if let Some(sid) = &self.session_id {
            req = req.header("Mcp-Session-Id", sid.clone());
        }
        let resp = crate::http::HttpClient::new()
            .send(req)
            .map_err(|e| format!("请求 MCP server 失败：{e}"))?;
        if resp.status == 401 {
            return Err("[unauthorized] MCP server 返回 401，鉴权无效或已过期".into());
        }
        if resp.status != 202 && resp.status != 204 && !resp.is_success() {
            let head: String = resp.text().chars().take(300).collect();
            return Err(format!("MCP server HTTP {}: {head}", resp.status));
        }
        if let Some(sid) = resp.header("Mcp-Session-Id") {
            self.session_id = Some(sid.to_string());
        }
        let has_id = msg.get("id").is_some();
        let content_type = resp.header("content-type").unwrap_or("").to_string();
        // 202/204：通知被接受、无 body。
        if resp.status == 202 || resp.status == 204 {
            return Ok(None);
        }
        let body = resp.text();
        if !has_id {
            return Ok(None);
        }
        let want_id = msg.get("id").cloned();
        if content_type.contains("text/event-stream") {
            for v in parse_sse_json(&body) {
                if v.get("id") == want_id.as_ref()
                    && (v.get("result").is_some() || v.get("error").is_some())
                {
                    return Ok(Some(v));
                }
            }
            Err("SSE 流中未找到对应响应".into())
        } else {
            serde_json::from_str(&body)
                .map(Some)
                .map_err(|e| format!("响应不是合法 JSON：{e}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::parse_sse_json;
    use super::HttpTransport;
    use crate::mcp::transport::McpTransport;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn parses_sse_events_and_skips_noise() {
        let body = ": comment\nevent: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}\n\ndata: not-json\n\n";
        let msgs = parse_sse_json(body);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["id"], 1);
    }

    #[test]
    fn parses_multiline_data() {
        let body = "data: {\"a\":\ndata: 1}\n\n";
        let msgs = parse_sse_json(body);
        assert_eq!(msgs[0]["a"], 1);
    }

    #[test]
    fn request_maps_401_and_roundtrips_session_id() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();

        // channel 把第二次请求原文传回主线程断言。
        let (tx, rx) = mpsc::channel::<String>();

        std::thread::spawn(move || {
            // 第一次连接：返回 200 + Mcp-Session-Id: sess-1
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf);
            let result_body = r#"{"jsonrpc":"2.0","id":1,"result":{}}"#;
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nMcp-Session-Id: sess-1\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
                result_body.len(),
                result_body
            );
            let _ = stream.write_all(resp.as_bytes());
            drop(stream);

            // 第二次连接：读请求原文，返回 401
            let (mut stream2, _) = listener.accept().unwrap();
            let mut req_buf = vec![0u8; 8192];
            let n = stream2.read(&mut req_buf).unwrap_or(0);
            let req_text = String::from_utf8_lossy(&req_buf[..n]).to_string();
            let _ = tx.send(req_text);
            let resp401 =
                "HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
            let _ = stream2.write_all(resp401.as_bytes());
            drop(stream2);
        });

        let mut transport = HttpTransport::new(format!("http://127.0.0.1:{}/mcp", port), vec![]);

        // 第一次请求：应成功，session_id 被记录。
        let r1 = transport.request(
            &serde_json::json!({"jsonrpc":"2.0","id":1,"method":"x"}),
            Duration::from_secs(5),
        );
        assert!(r1.is_ok(), "第一次请求应成功，得到：{:?}", r1);
        assert!(r1.unwrap().is_some());
        assert_eq!(
            transport.session_id(),
            Some("sess-1"),
            "session_id 应被记录为 sess-1"
        );

        // 第二次请求：应返回 Err 且以 [unauthorized] 开头。
        let r2 = transport.request(
            &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"y"}),
            Duration::from_secs(5),
        );
        assert!(r2.is_err(), "第二次请求应返回 Err");
        let err_msg = r2.unwrap_err();
        assert!(
            err_msg.starts_with("[unauthorized]"),
            "错误应以 [unauthorized] 开头，实际：{err_msg}"
        );

        // 断言服务器捕获的第二次请求包含会话头。
        let req_text = rx
            .recv_timeout(Duration::from_secs(5))
            .expect("未收到第二次请求原文");
        // 头名大小写不敏感：reqwest/hyper 线上小写发送（ureq 曾保留原样大小写）。
        assert!(
            req_text.to_lowercase().contains("mcp-session-id: sess-1"),
            "第二次请求应携带 Mcp-Session-Id: sess-1，实际请求：\n{req_text}"
        );
    }
}

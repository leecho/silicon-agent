//! 旧版 HTTP+SSE 传输（MCP 2024-11-05 协议）：双通道。
//! GET 开一条 SSE 流 → server 先推 `endpoint` 事件给出 POST 地址 →
//! JSON-RPC 消息 POST 到该地址，响应从 SSE 流回来。
//! 用于 `/sse` 这类服务（如百度网盘），区别于新版单端点 Streamable HTTP。

use std::io::{BufRead, BufReader};
use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant};

use crate::mcp::transport::McpTransport;

/// 等待 `endpoint` 事件的上限（开流后 server 应很快推出）。
const ENDPOINT_TIMEOUT: Duration = Duration::from_secs(30);

pub struct SseTransport {
    /// server 通过 endpoint 事件给出的 POST 地址（已解析为绝对 URL）。
    post_url: String,
    headers: Vec<(String, String)>,
    /// SSE 流里收到的 JSON-RPC 消息。
    rx: Receiver<serde_json::Value>,
}

impl SseTransport {
    /// 连接：GET 开流，后台线程读 SSE；等到 `endpoint` 事件拿到 POST 地址才返回。
    pub fn connect(url: &str, headers: Vec<(String, String)>) -> Result<Self, String> {
        // 阻塞模式流（open_sse）：std reader.lines() 不容忍 WouldBlock；读超时 120s 容忍 idle。
        let reader = crate::http::HttpClient::new()
            .open_sse(
                crate::http::HttpRequest::get(url)
                    .header("Accept", "text/event-stream")
                    .headers(headers.clone()),
            )
            .map_err(|e| match e {
                crate::http::HttpError::Status { code: 401, .. } => {
                    "[unauthorized] MCP server 返回 401，鉴权无效或已过期".to_string()
                }
                crate::http::HttpError::Status { code, body } => {
                    let head: String = body.chars().take(200).collect();
                    format!("打开 SSE 流失败 HTTP {code}: {head}")
                }
                other => format!("打开 SSE 流失败：{other}"),
            })?;

        let base = url.to_string();
        let (endpoint_tx, endpoint_rx) = std::sync::mpsc::channel::<String>();
        let (msg_tx, msg_rx) = std::sync::mpsc::channel::<serde_json::Value>();
        let buf = BufReader::new(reader);
        std::thread::spawn(move || {
            sse_reader_loop(buf, &base, &endpoint_tx, &msg_tx);
        });

        let post_url = endpoint_rx.recv_timeout(ENDPOINT_TIMEOUT).map_err(|_| {
            "未收到 SSE endpoint 事件（该地址可能不是 SSE 传输，或服务无响应）".to_string()
        })?;

        Ok(Self {
            post_url,
            headers,
            rx: msg_rx,
        })
    }
}

/// 逐行读 SSE：按空行分帧，`event: endpoint` 解析 POST 地址，其余 `data:` 当 JSON-RPC 消息。
fn sse_reader_loop<R: BufRead>(
    reader: R,
    base_url: &str,
    endpoint_tx: &Sender<String>,
    msg_tx: &Sender<serde_json::Value>,
) {
    let mut event = String::new();
    let mut data: Vec<String> = Vec::new();
    let mut endpoint_sent = false;
    for line in reader.lines() {
        let Ok(line) = line else { break };
        if line.is_empty() {
            if !data.is_empty() {
                let payload = data.join("\n");
                if event == "endpoint" && !endpoint_sent {
                    if let Some(abs) = resolve_url(base_url, payload.trim()) {
                        endpoint_sent = endpoint_tx.send(abs).is_ok();
                    }
                } else if let Ok(v) = serde_json::from_str::<serde_json::Value>(&payload) {
                    if msg_tx.send(v).is_err() {
                        break;
                    }
                }
            }
            event.clear();
            data.clear();
        } else if let Some(rest) = line.strip_prefix("event:") {
            event = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix("data:") {
            data.push(rest.strip_prefix(' ').unwrap_or(rest).to_string());
        }
        // 其余行（`:` 注释心跳、`id:`、`retry:`）忽略。
    }
}

/// 把 endpoint 事件里的（可能相对的）路径解析为绝对 URL（基于开流 URL）。
fn resolve_url(base: &str, path: &str) -> Option<String> {
    url::Url::parse(base)
        .ok()?
        .join(path)
        .ok()
        .map(|u| u.to_string())
}

impl McpTransport for SseTransport {
    fn request(
        &mut self,
        msg: &serde_json::Value,
        timeout: Duration,
    ) -> Result<Option<serde_json::Value>, String> {
        let resp = crate::http::HttpClient::new()
            .send(
                crate::http::HttpRequest::post(self.post_url.clone())
                    .content_type("application/json")
                    .headers(self.headers.clone())
                    .string_body(msg.to_string())
                    .timeout(timeout),
            )
            .map_err(|e| format!("请求 MCP server 失败：{e}"))?;
        if resp.status == 401 {
            return Err("[unauthorized] MCP server 返回 401，鉴权无效或已过期".into());
        }
        if !resp.is_success() && resp.status != 202 {
            let head: String = resp.text().chars().take(200).collect();
            return Err(format!("MCP server HTTP {}: {head}", resp.status));
        }
        let Some(want_id) = msg.get("id").cloned() else {
            return Ok(None); // 通知：吞掉 ack（多为 202），不等响应。
        };
        // 个别服务直接在 POST 响应体回 JSON-RPC；否则（202 等）从 SSE 流等。
        {
            let body = resp.text();
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&body) {
                if is_reply_for(&v, &want_id) {
                    return Ok(Some(v));
                }
            }
        }
        // 从 SSE 流等待匹配 id 的响应。
        let deadline = Instant::now() + timeout;
        loop {
            let remain = deadline
                .checked_duration_since(Instant::now())
                .ok_or_else(|| "等待 SSE 响应超时".to_string())?;
            let v = self
                .rx
                .recv_timeout(remain)
                .map_err(|_| "等待 SSE 响应超时或连接已断开".to_string())?;
            if is_reply_for(&v, &want_id) {
                return Ok(Some(v));
            }
            // 其它消息（server 通知 / 别的 id）忽略，继续等。
        }
    }
}

fn is_reply_for(v: &serde_json::Value, want_id: &serde_json::Value) -> bool {
    v.get("id") == Some(want_id) && (v.get("result").is_some() || v.get("error").is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_relative_endpoint() {
        assert_eq!(
            resolve_url("https://h.com/sse", "/message?sessionId=abc").as_deref(),
            Some("https://h.com/message?sessionId=abc")
        );
        // 绝对 URL 原样。
        assert_eq!(
            resolve_url("https://h.com/sse", "https://h.com/x").as_deref(),
            Some("https://h.com/x")
        );
    }

    #[test]
    fn reader_emits_endpoint_then_message() {
        let body = "event: endpoint\ndata: /message?sessionId=s1\n\nevent: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}\n\n";
        let (etx, erx) = std::sync::mpsc::channel::<String>();
        let (mtx, mrx) = std::sync::mpsc::channel::<serde_json::Value>();
        sse_reader_loop(BufReader::new(body.as_bytes()), "https://h.com/sse", &etx, &mtx);
        assert_eq!(
            erx.recv().unwrap(),
            "https://h.com/message?sessionId=s1"
        );
        let m = mrx.recv().unwrap();
        assert_eq!(m["id"], 1);
        assert!(m.get("result").is_some());
    }
}

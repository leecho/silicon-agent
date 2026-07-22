//! MCP 协议客户端：在任一传输上完成 initialize 握手、tools/list、tools/call。

use std::time::Duration;

use crate::mcp::transport::McpTransport;
use crate::mcp::types::{rpc_notification, rpc_request, rpc_result, McpToolDef, PROTOCOL_VERSION};

/// 默认单次调用超时（spec §4.2：一期固定 60s）。
pub const CALL_TIMEOUT: Duration = Duration::from_secs(60);
/// 握手/list 等管理类请求超时。
pub const ADMIN_TIMEOUT: Duration = Duration::from_secs(15);
/// initialize 握手超时：放宽到 60s，容纳 stdio server 的冷启动（如 `npx` 首次下载包）。
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(60);

pub struct McpClient {
    transport: Box<dyn McpTransport>,
    // id=1 已被 initialize 占用
    next_id: i64,
    /// server 协商出的协议版本（可能比我们请求的老）。
    pub protocol_version: String,
}

impl McpClient {
    /// 完成 initialize 握手并发送 initialized 通知。
    pub fn connect(mut transport: Box<dyn McpTransport>) -> Result<Self, String> {
        let req = rpc_request(
            1,
            "initialize",
            serde_json::json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": {"name": "silicon-worker", "version": env!("CARGO_PKG_VERSION")},
            }),
        );
        let resp = transport
            .request(&req, CONNECT_TIMEOUT)?
            .ok_or_else(|| "initialize 未收到响应".to_string())?;
        // 兜底校验：响应 id 必须等于请求 id（1）。
        let resp_id = resp.get("id");
        if resp_id != Some(&serde_json::json!(1)) {
            return Err(format!("响应 id 不匹配：期望 1，得到 {:?}", resp_id));
        }
        let result = rpc_result(&resp).map_err(|e| format!("initialize 失败: {e}"))?;
        let version = result
            .get("protocolVersion")
            .and_then(|v| v.as_str())
            .unwrap_or(PROTOCOL_VERSION)
            .to_string();
        transport.request(
            &rpc_notification("notifications/initialized"),
            ADMIN_TIMEOUT,
        )?;
        Ok(Self {
            transport,
            next_id: 2,
            protocol_version: version,
        })
    }

    fn call(
        &mut self,
        method: &str,
        params: serde_json::Value,
        timeout: Duration,
    ) -> Result<serde_json::Value, String> {
        let id = self.next_id;
        self.next_id += 1;
        let resp = self
            .transport
            .request(&rpc_request(id, method, params), timeout)?
            .ok_or_else(|| format!("{method} 未收到响应"))?;
        // 兜底校验：响应 id 必须与本次请求 id 一致。
        let resp_id = resp.get("id");
        if resp_id != Some(&serde_json::json!(id)) {
            return Err(format!("响应 id 不匹配：期望 {id}，得到 {:?}", resp_id));
        }
        rpc_result(&resp)
    }

    /// 拉全量工具列表（跟随 cursor 分页）。
    pub fn list_tools(&mut self) -> Result<Vec<McpToolDef>, String> {
        let mut out = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let params = match &cursor {
                Some(c) => serde_json::json!({"cursor": c}),
                None => serde_json::json!({}),
            };
            let result = self.call("tools/list", params, ADMIN_TIMEOUT)?;
            if let Some(items) = result.get("tools").and_then(|t| t.as_array()) {
                for item in items {
                    match serde_json::from_value::<McpToolDef>(item.clone()) {
                        Ok(def) => out.push(def),
                        Err(e) => eprintln!("[mcp] 跳过无法解析的工具定义：{e}"),
                    }
                }
            }
            cursor = result
                .get("nextCursor")
                .and_then(|c| c.as_str())
                .map(String::from);
            if cursor.is_none() {
                break;
            }
        }
        Ok(out)
    }

    /// 调用远端工具：content 中 text 项拼接；非文本项给类型占位；isError 转 Err。
    pub fn call_tool(&mut self, name: &str, args: &serde_json::Value) -> Result<String, String> {
        let result = self.call(
            "tools/call",
            serde_json::json!({"name": name, "arguments": args}),
            CALL_TIMEOUT,
        )?;
        let mut parts: Vec<String> = Vec::new();
        if let Some(content) = result.get("content").and_then(|c| c.as_array()) {
            for item in content {
                match item.get("type").and_then(|t| t.as_str()) {
                    Some("text") => {
                        if let Some(t) = item.get("text").and_then(|t| t.as_str()) {
                            parts.push(t.to_string());
                        }
                    }
                    Some(other) => parts.push(format!("[{other} content omitted]")),
                    None => parts.push("[unknown content omitted]".to_string()),
                }
            }
        }
        let text = parts.join("\n");
        if result
            .get("isError")
            .and_then(|e| e.as_bool())
            .unwrap_or(false)
        {
            return Err(if text.is_empty() {
                "远端工具返回错误".into()
            } else {
                text
            });
        }
        Ok(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::transport::MockTransport;

    fn init_reply() -> serde_json::Value {
        serde_json::json!({"jsonrpc":"2.0","id":1,"result":{
            "protocolVersion":"2025-03-26","capabilities":{"tools":{}},
            "serverInfo":{"name":"mock","version":"0"}}})
    }

    #[test]
    fn connect_handshakes_and_sends_initialized() {
        let mock = MockTransport::new(vec![init_reply()]);
        let sent = std::sync::Arc::clone(&mock.sent);
        let client = McpClient::connect(Box::new(mock)).unwrap();
        assert_eq!(client.protocol_version, "2025-03-26");
        let sent_msgs = sent.lock().unwrap();
        let has_initialized = sent_msgs
            .iter()
            .any(|m| m.get("method").and_then(|v| v.as_str()) == Some("notifications/initialized"));
        assert!(has_initialized, "应发出 notifications/initialized 通知");
    }

    #[test]
    fn connect_accepts_older_protocol_version() {
        let mut reply = init_reply();
        reply["result"]["protocolVersion"] = "2024-11-05".into();
        let client = McpClient::connect(Box::new(MockTransport::new(vec![reply]))).unwrap();
        assert_eq!(client.protocol_version, "2024-11-05");
    }

    #[test]
    fn mismatched_response_id_is_rejected() {
        // initialize 响应的 id 故意设为 99，应触发兜底校验报错。
        let bad_reply = serde_json::json!({"jsonrpc":"2.0","id":99,"result":{
            "protocolVersion":"2025-03-26","capabilities":{"tools":{}},
            "serverInfo":{"name":"mock","version":"0"}}});
        let mock = MockTransport::new_raw(vec![bad_reply]);
        match McpClient::connect(Box::new(mock)) {
            Ok(_) => panic!("应返回 Err，但得到 Ok"),
            Err(err) => assert!(
                err.contains("id 不匹配"),
                "错误信息应包含 'id 不匹配'，实际：{err}"
            ),
        }
    }

    #[test]
    fn list_tools_follows_pagination() {
        let page1 = serde_json::json!({"jsonrpc":"2.0","result":{
            "tools":[{"name":"a"}],"nextCursor":"c1"}});
        let page2 = serde_json::json!({"jsonrpc":"2.0","result":{"tools":[{"name":"b"}]}});
        let mock = MockTransport::new(vec![init_reply(), page1, page2]);
        let mut client = McpClient::connect(Box::new(mock)).unwrap();
        let tools = client.list_tools().unwrap();
        assert_eq!(
            tools.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
            vec!["a", "b"]
        );
    }

    #[test]
    fn call_tool_joins_text_and_maps_is_error() {
        let ok = serde_json::json!({"jsonrpc":"2.0","result":{
        "content":[
            {"type":"text","text":"hello"},
            {"type":"image","data":"x"},
            {"data":"no-type"}
        ]}});
        let err = serde_json::json!({"jsonrpc":"2.0","result":{
            "content":[{"type":"text","text":"boom"}],"isError":true}});
        let mock = MockTransport::new(vec![init_reply(), ok, err]);
        let mut client = McpClient::connect(Box::new(mock)).unwrap();
        let out = client.call_tool("t", &serde_json::json!({})).unwrap();
        assert_eq!(
            out,
            "hello\n[image content omitted]\n[unknown content omitted]"
        );
        assert_eq!(
            client.call_tool("t", &serde_json::json!({})).unwrap_err(),
            "boom"
        );
    }
}

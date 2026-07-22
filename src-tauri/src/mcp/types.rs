//! MCP 配置、工具定义与 JSON-RPC 2.0 消息类型。

use serde::{Deserialize, Serialize};

/// MCP 协议版本：握手时请求的目标版本；server 返回更老版本则按其版本工作。
pub const PROTOCOL_VERSION: &str = "2025-03-26";

/// 传输配置。config_json 中仅存非敏感部分；headers 的敏感值在 secret store。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum McpTransportConfig {
    Stdio {
        command: String,
        #[serde(default)]
        args: Vec<String>,
        /// 非敏感环境变量。敏感值的键名记在这里、值在 secret store（key=`{server_id}:env:{名}`）。
        #[serde(default)]
        env: std::collections::BTreeMap<String, String>,
        /// 子进程工作目录（plugin MCP 常用 `${CLAUDE_PLUGIN_ROOT}`）。None = 继承当前进程。
        #[serde(default)]
        cwd: Option<String>,
    },
    Http {
        url: String,
        /// 非敏感请求头。鉴权头不在此（由 auth 配置注入）。
        #[serde(default)]
        headers: std::collections::BTreeMap<String, String>,
    },
    /// 旧版 HTTP+SSE 双通道传输（`/sse` 端点：GET 开流 + server 给出 POST 地址）。
    Sse {
        url: String,
        #[serde(default)]
        headers: std::collections::BTreeMap<String, String>,
    },
}

/// 一条 MCP server 实例配置（用户配置层，对应 mcp_servers 表一行）。
/// 凭证内联在 transport（http→headers、stdio→env），不再有独立鉴权配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerConfig {
    pub id: String,
    pub name: String,
    /// 历史字段（vestigial，恒为 None）：保留以减少 DB 列改动。
    #[serde(default)]
    pub preset_id: Option<String>,
    /// 拥有此 server 的插件 id；空串 = 用户手动添加的独立 server。
    #[serde(default)]
    pub plugin_id: String,
    /// OAuth 手填 client_id（JSON 扩展字段 `clientId`）；None=动态注册。
    #[serde(default)]
    pub oauth_client_id: Option<String>,
    /// RFC 8707 的 `resource` 覆盖（插件清单的 `oauth_resource`）。
    /// 仅当无法从 PRM 拿到 canonical resource 时才用；再退回 server_url。
    #[serde(default)]
    pub oauth_resource: Option<String>,
    pub transport: McpTransportConfig,
    pub auto_approve: bool,
    pub enabled: bool,
}

/// 远端工具定义（tools/list 一项）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolDef {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(rename = "inputSchema", default = "default_schema")]
    pub input_schema: serde_json::Value,
}

fn default_schema() -> serde_json::Value {
    serde_json::json!({"type": "object", "properties": {}})
}

/// 连接状态（推给前端的事件载荷）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerStatus {
    pub server_id: String,
    /// disconnected | connecting | connected | failed | unauthorized
    pub state: String,
    pub error: Option<String>,
    pub tool_count: usize,
}

/// 构造一条 JSON-RPC 2.0 请求。
pub fn rpc_request(id: i64, method: &str, params: serde_json::Value) -> serde_json::Value {
    serde_json::json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
}

/// 构造一条 JSON-RPC 通知（无 id，无响应）。
pub fn rpc_notification(method: &str) -> serde_json::Value {
    serde_json::json!({"jsonrpc": "2.0", "method": method})
}

/// 从响应中取 result；error 转 Err 文本。非响应消息（无 result/error）返回 Err。
pub fn rpc_result(resp: &serde_json::Value) -> Result<serde_json::Value, String> {
    if let Some(err) = resp.get("error") {
        let code = err.get("code").and_then(|c| c.as_i64()).unwrap_or(0);
        let msg = err
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("未知错误");
        return Err(format!("JSON-RPC 错误 {code}: {msg}"));
    }
    resp.get("result")
        .cloned()
        .ok_or_else(|| "响应缺少 result".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_config_serde_roundtrip() {
        let cfg = McpTransportConfig::Http {
            url: "https://mcp.example.com/mcp".into(),
            headers: Default::default(),
        };
        let raw = serde_json::to_string(&cfg).unwrap();
        assert!(raw.contains("\"type\":\"http\""));
        let back: McpTransportConfig = serde_json::from_str(&raw).unwrap();
        assert_eq!(back, cfg);
    }

    #[test]
    fn stdio_config_cwd_serde_roundtrip() {
        let cfg = McpTransportConfig::Stdio {
            command: "node".into(),
            args: vec!["server.js".into()],
            env: Default::default(),
            cwd: Some("/tmp/plugin-root".into()),
        };
        let raw = serde_json::to_string(&cfg).unwrap();
        assert!(raw.contains("\"type\":\"stdio\""));
        assert!(raw.contains("/tmp/plugin-root"));
        let back: McpTransportConfig = serde_json::from_str(&raw).unwrap();
        assert_eq!(back, cfg);
        // 缺省 cwd（旧配置）反序列化为 None。
        let legacy: McpTransportConfig =
            serde_json::from_str(r#"{"type":"stdio","command":"node"}"#).unwrap();
        assert_eq!(
            legacy,
            McpTransportConfig::Stdio {
                command: "node".into(),
                args: vec![],
                env: Default::default(),
                cwd: None,
            }
        );
    }

    #[test]
    fn tool_def_defaults_schema_and_description() {
        let def: McpToolDef = serde_json::from_str(r#"{"name":"t"}"#).unwrap();
        assert_eq!(def.description, "");
        assert_eq!(def.input_schema["type"], "object");
    }

    #[test]
    fn rpc_result_extracts_error() {
        let resp =
            serde_json::json!({"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"no"}});
        let err = rpc_result(&resp).unwrap_err();
        assert!(err.contains("-32601"));
    }
}

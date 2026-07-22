//! 标准 `mcpServers` JSON（Claude Desktop 风格）↔ Vec<McpServerConfig>。
//! 纯函数、无 I/O、无 secret。凭证内联在 headers/env。

use std::collections::BTreeMap;

use crate::mcp::types::{McpServerConfig, McpTransportConfig};

/// 解析标准 mcpServers JSON。每个 entry：有 `command`→stdio、有 `url`→http；
/// 二者皆无/皆有 → 报错（带服务名）。扩展字段 `disabled`/`autoApprove`。
/// id 留空（由 store import 时按 name 匹配复用或新建）。
pub fn parse_mcp_servers(json: &str) -> Result<Vec<McpServerConfig>, String> {
    let root: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("JSON 解析失败: {e}"))?;
    let obj = root
        .get("mcpServers")
        .and_then(|v| v.as_object())
        .ok_or_else(|| "缺少顶层对象 mcpServers".to_string())?;
    let mut out = Vec::new();
    for (name, v) in obj {
        let has_cmd = v.get("command").is_some();
        let has_url = v.get("url").is_some();
        let transport = match (has_cmd, has_url) {
            (true, false) => {
                let command = v
                    .get("command")
                    .and_then(|c| c.as_str())
                    .ok_or_else(|| format!("服务 {name}: command 必须是字符串"))?
                    .to_string();
                let args = v
                    .get("args")
                    .and_then(|a| a.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|x| x.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                let env = v
                    .get("env")
                    .and_then(|e| e.as_object())
                    .map(|e| {
                        e.iter()
                            .filter_map(|(k, val)| val.as_str().map(|s| (k.clone(), s.to_string())))
                            .collect()
                    })
                    .unwrap_or_else(BTreeMap::new);
                let cwd = v.get("cwd").and_then(|c| c.as_str()).map(String::from);
                McpTransportConfig::Stdio {
                    command,
                    args,
                    env,
                    cwd,
                }
            }
            (false, true) => {
                let url = v
                    .get("url")
                    .and_then(|u| u.as_str())
                    .ok_or_else(|| format!("服务 {name}: url 必须是字符串"))?
                    .to_string();
                let headers = v
                    .get("headers")
                    .and_then(|h| h.as_object())
                    .map(|h| {
                        h.iter()
                            .filter_map(|(k, val)| val.as_str().map(|s| (k.clone(), s.to_string())))
                            .collect()
                    })
                    .unwrap_or_else(BTreeMap::new);
                // SSE 判定：显式 `type:"sse"`，或未指定 type 且 url 路径以 /sse 结尾（自动识别）。
                let typ = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
                let is_sse = typ.eq_ignore_ascii_case("sse")
                    || (typ.is_empty()
                        && url
                            .split(['?', '#'])
                            .next()
                            .unwrap_or(&url)
                            .trim_end_matches('/')
                            .ends_with("/sse"));
                if is_sse {
                    McpTransportConfig::Sse { url, headers }
                } else {
                    McpTransportConfig::Http { url, headers }
                }
            }
            (false, false) => {
                return Err(format!("服务 {name}: 必须含 command(stdio) 或 url(http)"))
            }
            (true, true) => return Err(format!("服务 {name}: command 与 url 不能同时存在")),
        };
        let disabled = v.get("disabled").and_then(|d| d.as_bool()).unwrap_or(false);
        let auto_approve = v
            .get("autoApprove")
            .and_then(|a| a.as_bool())
            .unwrap_or(false);
        let oauth_client_id = v.get("clientId").and_then(|c| c.as_str()).map(String::from);
        let oauth_resource = v
            .get("oauth_resource")
            .or_else(|| v.get("oauthResource"))
            .and_then(|c| c.as_str())
            .map(String::from);
        out.push(McpServerConfig {
            id: String::new(),
            name: name.clone(),
            preset_id: None,
            plugin_id: String::new(),
            oauth_client_id,
            oauth_resource,
            transport,
            auto_approve,
            enabled: !disabled,
        });
    }
    Ok(out)
}

/// 序列化为标准 mcpServers JSON（含 disabled/autoApprove 扩展字段）。
pub fn to_mcp_servers_json(servers: &[McpServerConfig]) -> String {
    let mut map = serde_json::Map::new();
    for s in servers {
        let mut entry = serde_json::Map::new();
        match &s.transport {
            McpTransportConfig::Stdio {
                command,
                args,
                env,
                cwd,
            } => {
                entry.insert("command".into(), command.clone().into());
                entry.insert("args".into(), serde_json::json!(args));
                if !env.is_empty() {
                    entry.insert("env".into(), serde_json::json!(env));
                }
                if let Some(c) = cwd {
                    entry.insert("cwd".into(), c.clone().into());
                }
            }
            McpTransportConfig::Http { url, headers } => {
                entry.insert("url".into(), url.clone().into());
                if !headers.is_empty() {
                    entry.insert("headers".into(), serde_json::json!(headers));
                }
            }
            McpTransportConfig::Sse { url, headers } => {
                entry.insert("url".into(), url.clone().into());
                entry.insert("type".into(), "sse".into());
                if !headers.is_empty() {
                    entry.insert("headers".into(), serde_json::json!(headers));
                }
            }
        }
        if !s.enabled {
            entry.insert("disabled".into(), true.into());
        }
        if s.auto_approve {
            entry.insert("autoApprove".into(), true.into());
        }
        if let Some(cid) = &s.oauth_client_id {
            entry.insert("clientId".into(), cid.clone().into());
        }
        if let Some(res) = &s.oauth_resource {
            entry.insert("oauth_resource".into(), res.clone().into());
        }
        map.insert(s.name.clone(), serde_json::Value::Object(entry));
    }
    let root = serde_json::json!({ "mcpServers": serde_json::Value::Object(map) });
    serde_json::to_string_pretty(&root).unwrap_or_else(|_| "{}".into())
}

#[cfg(test)]
mod tests {
    use super::{parse_mcp_servers, to_mcp_servers_json};
    use crate::mcp::types::McpTransportConfig;

    #[test]
    fn detects_sse_by_suffix_and_type_and_exports() {
        // url 以 /sse 结尾、无 type → Sse
        let a = parse_mcp_servers(r#"{"mcpServers":{"x":{"url":"https://h/sse"}}}"#).unwrap();
        assert!(matches!(a[0].transport, McpTransportConfig::Sse { .. }));
        // 显式 type:"sse" → Sse（即便 url 不以 /sse 结尾）
        let b =
            parse_mcp_servers(r#"{"mcpServers":{"x":{"url":"https://h/stream","type":"sse"}}}"#)
                .unwrap();
        assert!(matches!(b[0].transport, McpTransportConfig::Sse { .. }));
        // type:"http" 覆盖 /sse 后缀 → Http
        let c = parse_mcp_servers(r#"{"mcpServers":{"x":{"url":"https://h/sse","type":"http"}}}"#)
            .unwrap();
        assert!(matches!(c[0].transport, McpTransportConfig::Http { .. }));
        // 普通 url → Http
        let d = parse_mcp_servers(r#"{"mcpServers":{"x":{"url":"https://h/mcp"}}}"#).unwrap();
        assert!(matches!(d[0].transport, McpTransportConfig::Http { .. }));
        // 导出 Sse 带 type:"sse"
        let out = to_mcp_servers_json(&a);
        assert!(out.contains("\"type\""));
        assert!(out.contains("sse"));
    }

    #[test]
    fn parses_and_exports_client_id() {
        let json = r#"{"mcpServers":{"x":{"url":"https://h/mcp","clientId":"cid-1"}}}"#;
        let servers = parse_mcp_servers(json).unwrap();
        assert_eq!(servers[0].oauth_client_id.as_deref(), Some("cid-1"));
        let out = to_mcp_servers_json(&servers);
        assert!(out.contains("\"clientId\""));
        assert!(out.contains("cid-1"));
    }

    #[test]
    fn parse_stdio_and_http_with_extensions() {
        let json = r#"{
          "mcpServers": {
            "fs": { "command": "npx", "args": ["-y","x"], "env": {"K":"v"} },
            "web": { "url": "https://h/mcp", "headers": {"Authorization":"Bearer t"},
                     "disabled": true, "autoApprove": true }
          }
        }"#;
        let mut servers = parse_mcp_servers(json).expect("parse ok");
        servers.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(servers.len(), 2);
        let fs = &servers[0];
        assert_eq!(fs.name, "fs");
        assert!(fs.enabled && !fs.auto_approve);
        assert!(matches!(fs.transport, McpTransportConfig::Stdio { .. }));
        let web = &servers[1];
        assert!(!web.enabled && web.auto_approve);
        match &web.transport {
            McpTransportConfig::Http { url, headers } => {
                assert_eq!(url, "https://h/mcp");
                assert_eq!(headers.get("Authorization").unwrap(), "Bearer t");
            }
            _ => panic!("expected http"),
        }
    }

    #[test]
    fn rejects_entry_without_command_or_url() {
        let json = r#"{"mcpServers":{"bad":{"args":[]}}}"#;
        let err = parse_mcp_servers(json).unwrap_err();
        assert!(err.contains("bad"));
    }

    #[test]
    fn roundtrip_export_then_parse_stable() {
        let json = r#"{"mcpServers":{"fs":{"command":"node","args":["s.js"]}}}"#;
        let servers = parse_mcp_servers(json).unwrap();
        let out = to_mcp_servers_json(&servers);
        let again = parse_mcp_servers(&out).unwrap();
        assert_eq!(again.len(), 1);
        assert_eq!(again[0].name, "fs");
    }
}

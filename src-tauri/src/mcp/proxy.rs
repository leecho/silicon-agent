//! McpToolProxy：把一个远端 MCP 工具包装为一等 Tool。

use std::sync::Arc;

use crate::mcp::manager::McpService;
use crate::tools::{RiskLevel, Tool};

pub struct McpToolProxy {
    pub server_id: String,
    /// 远端原始工具名（tools/call 用）。
    pub remote_name: String,
    /// 注册进 registry 的名字：mcp__{server_slug}__{tool_slug}。
    pub exposed_name: String,
    pub label: String,
    pub description: String,
    pub schema: serde_json::Value,
    pub auto_approve: bool,
    pub service: Arc<McpService>,
}

impl Tool for McpToolProxy {
    fn name(&self) -> &str {
        &self.exposed_name
    }
    fn label(&self) -> &str {
        &self.label
    }
    fn description(&self) -> &str {
        &self.description
    }
    fn parameters(&self) -> serde_json::Value {
        self.schema.clone()
    }
    fn disclosure(&self) -> crate::tools::Disclosure {
        // 全部 MCP 工具按需披露——真正的 Token 大头（T83）。
        crate::tools::Disclosure::Deferred
    }
    fn risk_level(&self) -> RiskLevel {
        // 外部服务调用默认 High（AGENTS.md 安全节）；用户对该 server 显式开了自动批准则降为 Safe。
        if self.auto_approve {
            RiskLevel::Safe
        } else {
            RiskLevel::High
        }
    }
    fn execute(&self, args: &serde_json::Value) -> Result<String, String> {
        self.service
            .call_tool(&self.server_id, &self.remote_name, args)
            .map_err(|e| {
                if e.starts_with("[unauthorized]") {
                    format!(
                        "MCP「{}」未授权，请到「连接器」页面完成授权后重试。",
                        self.label
                    )
                } else {
                    e
                }
            })
    }
}

/// 规范化为 [a-z0-9_]，连续分隔符折叠为单个下划线。
pub fn slug(s: &str) -> String {
    let mut out = String::new();
    let mut prev_us = false;
    for ch in s.chars() {
        let c = ch.to_ascii_lowercase();
        if c.is_ascii_alphanumeric() {
            out.push(c);
            prev_us = false;
        } else if !prev_us && !out.is_empty() {
            out.push('_');
            prev_us = true;
        }
    }
    out.trim_end_matches('_').to_string()
}

/// 组合暴露名：mcp__{server(≤16)}__{tool}，整体 ≤64；与已占用名冲突则尾部追加序号。
pub fn exposed_name(
    server_name: &str,
    tool_name: &str,
    taken: &std::collections::HashSet<String>,
) -> String {
    let server_part: String = slug(server_name).chars().take(16).collect();
    let mut base = format!("mcp__{server_part}__{}", slug(tool_name));
    if base.len() > 64 {
        base.truncate(64);
        let trimmed = base.trim_end_matches('_').to_string();
        base = trimmed;
    }
    if !taken.contains(&base) {
        return base;
    }
    for i in 2..100 {
        let candidate_suffix = format!("_{i}");
        let mut c = base.clone();
        c.truncate(64 - candidate_suffix.len());
        let c = format!("{}{candidate_suffix}", c.trim_end_matches('_'));
        if !taken.contains(&c) {
            return c;
        }
    }
    base
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_normalizes() {
        assert_eq!(slug("My Server-01"), "my_server_01");
        assert_eq!(slug("高德 maps"), "maps");
    }

    #[test]
    fn exposed_name_caps_length_and_dedupes() {
        let mut taken = std::collections::HashSet::new();
        let long_tool = "t".repeat(100);
        let n1 = exposed_name("server", &long_tool, &taken);
        assert!(n1.len() <= 64 && n1.starts_with("mcp__server__"));
        taken.insert(n1.clone());
        let n2 = exposed_name("server", &long_tool, &taken);
        assert_ne!(n1, n2);
        assert!(n2.len() <= 64);
    }
}

#[cfg(test)]
mod disclosure_tests {
    use super::*;
    use crate::mcp::store::McpStore;
    use crate::storage::AppDatabase;
    use crate::tools::Disclosure;

    /// 构造一个最小可用的 McpToolProxy（真实 McpService）。
    fn dummy_proxy() -> McpToolProxy {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("siw-mcpproxy-disclosure-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        let db = Arc::new(AppDatabase::open(dir.join("app.sqlite3")).expect("open db"));
        let store = McpStore::new(db, dir.join("mcp.secrets.json")).expect("open store");
        let service = McpService::new(store);
        McpToolProxy {
            server_id: "s".into(),
            remote_name: "t".into(),
            exposed_name: "mcp__s__t".into(),
            label: "t".into(),
            description: "d".into(),
            schema: serde_json::json!({"type":"object"}),
            auto_approve: false,
            service,
        }
    }

    #[test]
    fn mcp_proxy_is_deferred() {
        // 全部 MCP 工具应按需披露（T83）。
        assert_eq!(dummy_proxy().disclosure(), Disclosure::Deferred);
    }
}

//! mcp manager 集成测试：测试连接失败处理、删除清理（凭证内联，无独立密钥库）。

use std::sync::Arc;

use silicon_worker::mcp::manager::McpService;
use silicon_worker::mcp::store::McpStore;
use silicon_worker::mcp::types::{McpServerConfig, McpTransportConfig};
use silicon_worker::storage::AppDatabase;

fn temp_dir(tag: &str) -> std::path::PathBuf {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "siw-cmd-{tag}_{}_{}_{nanos}",
        std::process::id(),
        seq,
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn open_store(dir: &std::path::Path) -> McpStore {
    let db = Arc::new(AppDatabase::open(dir.join("app.sqlite3")).expect("open db"));
    McpStore::new(db, dir.join("mcp.secrets.json")).expect("open mcp store")
}

fn http_cfg(id: &str, name: &str, url: &str) -> McpServerConfig {
    McpServerConfig {
        id: id.to_string(),
        name: name.to_string(),
        preset_id: None,
        plugin_id: String::new(),
        oauth_client_id: None,
        oauth_resource: None,
        transport: McpTransportConfig::Http {
            url: url.into(),
            headers: Default::default(),
        },
        auto_approve: false,
        enabled: true,
    }
}

/// 凭证内联：test_connection 直接用 config 的 headers，连不通的端口返回 Err。
#[test]
fn test_connection_fails_on_unreachable_port() {
    let dir = temp_dir("unreach");
    let svc = McpService::new(open_store(&dir));
    let cfg = http_cfg("", "draft", "http://127.0.0.1:1/mcp"); // 端口 1 必拒连
    let result = svc.test_connection(&cfg);
    assert!(result.is_err(), "连接 127.0.0.1:1 应失败");
}

/// upsert + delete：删除后行不存在。
#[test]
fn upsert_then_delete_removes_row() {
    let dir = temp_dir("del");
    let svc = McpService::new(open_store(&dir));
    let cfg = http_cfg("mcp-c-001", "c-server", "https://mcp.example.com/mcp");
    svc.store.upsert(cfg).expect("upsert");
    assert!(svc.store.get("mcp-c-001").unwrap().is_some());
    svc.store.delete("mcp-c-001").expect("delete");
    assert!(svc.store.get("mcp-c-001").unwrap().is_none());
}

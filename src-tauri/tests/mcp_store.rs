//! mcp_store 集成测试：mcp_servers CRUD 与密钥存取。

use std::sync::Arc;

use rusqlite;
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
        "siw-mcp-{tag}_{}_{}_{nanos}",
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

fn make_config(id: &str, name: &str) -> McpServerConfig {
    McpServerConfig {
        id: id.to_string(),
        name: name.to_string(),
        preset_id: None,
        plugin_id: String::new(),
        oauth_client_id: None,
        oauth_resource: None,
        transport: McpTransportConfig::Http {
            url: "https://mcp.example.com/mcp".into(),
            headers: Default::default(),
        },
        auto_approve: false,
        enabled: true,
    }
}

/// upsert 空 id 自动生成；list 长度 1；set_enabled(false) 后 get 确认；
/// secrets.set 后 delete，确认表空且 has_secret 为 false。
#[test]
fn upsert_list_toggle_delete_roundtrip() {
    let dir = temp_dir("roundtrip");
    let store = open_store(&dir);

    // upsert 空 id → 自动生成
    let cfg = make_config("", "my-server");
    let saved = store.upsert(cfg).expect("upsert");
    assert!(!saved.id.is_empty(), "id 应自动生成");
    assert!(
        saved.id.starts_with("mcp-"),
        "id 格式应为 mcp-{{ms}}-{{hex}}"
    );

    let id = saved.id.clone();

    // list 长度 1
    let list = store.list().expect("list");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, id);
    assert_eq!(list[0].name, "my-server");
    assert!(list[0].enabled);

    // set_enabled(false) 后 get 确认
    store.set_enabled(&id, false).expect("set_enabled");
    let got = store.get(&id).expect("get").expect("present");
    assert!(!got.enabled, "应已禁用");

    // secrets.set 后 delete，确认表空且 has_secret 为 false
    let apikey_key = format!("{id}:apikey");
    store
        .secrets
        .set(&apikey_key, "sk-test-1234")
        .expect("set secret");
    assert!(store.secrets.has_secret(&apikey_key));

    store.delete(&id).expect("delete");
    let list_after = store.list().expect("list after delete");
    assert!(list_after.is_empty(), "删除后表应为空");
    assert!(!store.secrets.has_secret(&apikey_key), "密钥应随删除清除");
}

/// T3：list() 跳过 config_json 损坏行，保留正常行。
#[test]
fn list_skips_corrupted_row_keeps_good_ones() {
    let dir = temp_dir("corrupt");
    let store = open_store(&dir);

    // 先 upsert 一条好的
    let cfg = make_config("mcp-good-001", "good-server");
    store.upsert(cfg).expect("upsert good");

    // 用 rusqlite 直接打开同一个 SQLite 文件，插一行 config_json 为垃圾内容
    let db_path = dir.join("app.sqlite3");
    let conn = rusqlite::Connection::open(&db_path).expect("open raw db");
    conn.execute(
        "insert into mcp_servers (id, name, preset_id, transport, config_json, auto_approve, enabled, created_at, updated_at)
         values ('mcp-bad-001', 'bad-server', null, 'http', 'NOT_VALID_JSON!!!', 0, 1, '0', '0')",
        [],
    )
    .expect("insert corrupted row");
    drop(conn);

    // list() 应只返回 1 条（损坏行被跳过）
    let list = store.list().expect("list");
    assert_eq!(list.len(), 1, "损坏行应被跳过，仅保留 1 条好的");
    assert_eq!(list[0].id, "mcp-good-001");
}

/// 同名两次 upsert（不同 id），第二次 Err 含「名称已存在」。
#[test]
fn duplicate_name_rejected() {
    let dir = temp_dir("dupname");
    let store = open_store(&dir);

    let cfg1 = make_config("mcp-id-001", "shared-name");
    store.upsert(cfg1).expect("first upsert");

    let cfg2 = make_config("mcp-id-002", "shared-name");
    let err = store.upsert(cfg2).expect_err("第二次 upsert 同名应报错");
    assert!(
        err.contains("名称已存在"),
        "错误信息应含「名称已存在」，实际: {err}"
    );
}

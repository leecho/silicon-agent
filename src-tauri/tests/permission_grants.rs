use std::path::PathBuf;
use std::sync::Arc;

use silicon_worker::session::SessionStore;
use silicon_worker::storage::AppDatabase;
use silicon_worker::tools::{
    command_tool::CommandExecute,
    fs_search::{Glob, Grep},
    fs_tools::{EditFile, ReadFile, WriteFile},
    web_search::WebSearch,
    Tool,
};

fn temp_db() -> Arc<AppDatabase> {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "siw-perm_{}_{}_{}",
        std::process::id(),
        seq,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    Arc::new(AppDatabase::open(dir.join("app.sqlite3")).expect("db"))
}

fn dummy_ws() -> PathBuf {
    std::env::temp_dir()
}

// --- requires_confirmation 标记断言 ---

#[test]
fn write_file_requires_confirmation() {
    assert!(WriteFile {
        workspace: dummy_ws()
    }
    .requires_confirmation());
}

#[test]
fn edit_file_requires_confirmation() {
    assert!(EditFile {
        workspace: dummy_ws()
    }
    .requires_confirmation());
}

#[test]
fn command_execute_requires_confirmation() {
    assert!(CommandExecute {
        workspace: dummy_ws()
    }
    .requires_confirmation());
}

#[test]
fn read_file_no_confirmation() {
    assert!(!ReadFile {
        workspace: dummy_ws()
    }
    .requires_confirmation());
}

#[test]
fn glob_no_confirmation() {
    assert!(!Glob {
        workspace: dummy_ws()
    }
    .requires_confirmation());
}

#[test]
fn grep_no_confirmation() {
    assert!(!Grep {
        workspace: dummy_ws()
    }
    .requires_confirmation());
}

#[test]
fn web_search_no_confirmation() {
    assert!(!WebSearch::new().requires_confirmation());
}

// --- permission_grants 存储行为 ---

#[test]
fn grant_then_is_granted_true() {
    let store = SessionStore::open(temp_db()).expect("store");
    store
        .create_session("s1", "test", "100", false)
        .expect("create");
    store.grant_tool("s1", "write_file", "101").expect("grant");
    let granted = store.is_tool_granted("s1", "write_file").expect("query");
    assert!(granted, "授权后应返回 true");
}

#[test]
fn not_granted_returns_false() {
    let store = SessionStore::open(temp_db()).expect("store");
    store
        .create_session("s2", "test", "100", false)
        .expect("create");
    let granted = store.is_tool_granted("s2", "run_command").expect("query");
    assert!(!granted, "未授权应返回 false");
}

#[test]
fn grant_is_idempotent() {
    let store = SessionStore::open(temp_db()).expect("store");
    store
        .create_session("s3", "test", "100", false)
        .expect("create");
    store
        .grant_tool("s3", "edit_file", "101")
        .expect("first grant");
    // 再次 grant 同一工具不应报错（insert or ignore）。
    store
        .grant_tool("s3", "edit_file", "102")
        .expect("second grant idempotent");
    let granted = store.is_tool_granted("s3", "edit_file").expect("query");
    assert!(granted);
}

#[test]
fn different_sessions_are_isolated() {
    let store = SessionStore::open(temp_db()).expect("store");
    store
        .create_session("sess-a", "a", "100", false)
        .expect("create a");
    store
        .create_session("sess-b", "b", "100", false)
        .expect("create b");

    // 只给 sess-a 授权。
    store
        .grant_tool("sess-a", "write_file", "101")
        .expect("grant a");

    assert!(
        store.is_tool_granted("sess-a", "write_file").unwrap(),
        "sess-a 应已授权"
    );
    assert!(
        !store.is_tool_granted("sess-b", "write_file").unwrap(),
        "sess-b 不应受影响"
    );
}

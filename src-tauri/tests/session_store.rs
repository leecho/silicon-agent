use silicon_agent::session::SessionStore;
use silicon_agent::storage::AppDatabase;
use std::sync::Arc;

fn temp_db() -> Arc<AppDatabase> {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "siw-sess_{}_{}_{}",
        std::process::id(),
        seq,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    Arc::new(AppDatabase::open(dir.join("app.sqlite3")).expect("db"))
}

#[test]
fn create_session_and_append_messages_roundtrip() {
    let store = SessionStore::open(temp_db()).expect("store");
    let session = store
        .create_session("s1", "测试", "100", false)
        .expect("create");
    store
        .append_message("m1", &session.id, "user", "你好", None, "101")
        .expect("user msg");
    store
        .append_message("m2", &session.id, "assistant", "你好，我在。", None, "102")
        .expect("asst msg");
    let detail = store
        .get_session_detail("s1")
        .expect("detail")
        .expect("present");
    assert_eq!(detail.messages.len(), 2);
    assert_eq!(detail.messages[0].role, "user");
    assert_eq!(detail.messages[1].content, "你好，我在。");
}

#[test]
fn get_or_create_default_is_idempotent_until_new() {
    let store = SessionStore::open(temp_db()).expect("store");
    let a = store.get_or_create_default("100").expect("a");
    let b = store.get_or_create_default("101").expect("b");
    assert_eq!(a.id, b.id, "已有会话时复用最近会话，不重复建");
}

use silicon_worker::session::SessionStore;
use silicon_worker::storage::AppDatabase;
use std::sync::Arc;

fn temp_db() -> Arc<AppDatabase> {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "siw-manage_{}_{}_{}",
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
fn set_title_if_default_first_call_returns_true_second_call_returns_false() {
    let store = SessionStore::open(temp_db()).expect("store");
    store
        .create_session("s1", "新会话", "100", false)
        .expect("create");

    // 首次调用：title 是 '新会话'，应更新并返回 true。
    let changed = store
        .set_title_if_default("s1", "我的第一条消息", "101")
        .expect("set_title");
    assert!(changed, "首次应返回 true");

    // 验证标题已更改。
    let session = store.get_session("s1").expect("get").expect("present");
    assert_eq!(session.title, "我的第一条消息");

    // 第二次调用：title 已非 '新会话'，不应更新，返回 false。
    let changed2 = store
        .set_title_if_default("s1", "另一条消息", "102")
        .expect("set_title_2");
    assert!(!changed2, "二次调用应返回 false（不再是默认标题）");

    // 验证标题未被更改。
    let session2 = store.get_session("s1").expect("get2").expect("present2");
    assert_eq!(session2.title, "我的第一条消息", "标题不应被二次改写");
}

#[test]
fn update_session_title_changes_title() {
    let store = SessionStore::open(temp_db()).expect("store");
    store
        .create_session("s1", "新会话", "100", false)
        .expect("create");

    store
        .update_session_title("s1", "改了名字", "101")
        .expect("update_title");

    let session = store.get_session("s1").expect("get").expect("present");
    assert_eq!(session.title, "改了名字");
    assert_eq!(session.updated_at, "101");
}

#[test]
fn delete_session_removes_session_and_messages() {
    let store = SessionStore::open(temp_db()).expect("store");
    store
        .create_session("s1", "待删除会话", "100", false)
        .expect("create");
    store
        .append_message("m1", "s1", "user", "你好", None, "101")
        .expect("msg");

    // 确认会话和消息存在。
    let detail = store
        .get_session_detail("s1")
        .expect("detail")
        .expect("present");
    assert_eq!(detail.messages.len(), 1);

    // 删除会话。
    store.delete_session("s1").expect("delete");

    // 删除后 get_session_detail 应返回 None。
    let after = store.get_session_detail("s1").expect("detail_after");
    assert!(after.is_none(), "删除后会话应不存在");

    // get_session 也应返回 None。
    let sess = store.get_session("s1").expect("get_after");
    assert!(sess.is_none(), "删除后 get_session 应返回 None");
}

#[test]
fn rename_session_equivalent_to_update_and_get() {
    let store = SessionStore::open(temp_db()).expect("store");
    store
        .create_session("s1", "新会话", "100", false)
        .expect("create");

    store
        .update_session_title("s1", "重命名的会话", "101")
        .expect("update_title");

    let session = store.get_session("s1").expect("get").expect("present");
    assert_eq!(session.title, "重命名的会话");
    assert_eq!(session.id, "s1");
}

// Tests for Slice Todos Task 1 Step 2: sessions.todos_json 持久化。
//
// ① set_session_todos → get_session_todos 往返一致。
// ② get_session_detail.todos 含已落库的清单。
// ③ 从未写入 todos_json → 空 Vec。

use silicon_worker::session::{SessionStore, TodoItem};
use silicon_worker::storage::AppDatabase;
use std::sync::Arc;

fn temp_db() -> Arc<AppDatabase> {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "siw-todos-store_{}_{}_{}",
        std::process::id(),
        seq,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    Arc::new(AppDatabase::open(dir.join("app.sqlite3")).expect("db"))
}

fn sample() -> Vec<TodoItem> {
    vec![
        TodoItem {
            id: 1,
            content: "步骤1".into(),
            status: "in_progress".into(),
        },
        TodoItem {
            id: 2,
            content: "步骤2".into(),
            status: "pending".into(),
        },
    ]
}

#[test]
fn set_then_get_round_trips() {
    let store = SessionStore::open(temp_db()).expect("store");
    store
        .create_session("s1", "todos", "100", false)
        .expect("session");

    let todos = sample();
    store
        .set_session_todos("s1", &todos, "200")
        .expect("set todos");

    let fetched = store.get_session_todos("s1").expect("get todos");
    assert_eq!(fetched, todos, "round trip should preserve todos");
}

#[test]
fn session_detail_includes_todos() {
    let store = SessionStore::open(temp_db()).expect("store");
    store
        .create_session("s1", "todos", "100", false)
        .expect("session");
    let todos = sample();
    store
        .set_session_todos("s1", &todos, "200")
        .expect("set todos");

    let detail = store
        .get_session_detail("s1")
        .expect("detail")
        .expect("present");
    assert_eq!(
        detail.todos, todos,
        "detail.todos should reflect stored list"
    );
}

#[test]
fn missing_todos_json_is_empty() {
    let store = SessionStore::open(temp_db()).expect("store");
    store
        .create_session("s1", "todos", "100", false)
        .expect("session");

    let fetched = store.get_session_todos("s1").expect("get todos");
    assert!(fetched.is_empty(), "no todos_json should yield empty Vec");

    let detail = store
        .get_session_detail("s1")
        .expect("detail")
        .expect("present");
    assert!(detail.todos.is_empty(), "detail.todos should be empty");
}

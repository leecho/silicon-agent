//! get_session_detail 经 AppState 回填 is_running（运行锁状态）。
//! 注：AppState 需 AppHandle 无法在集成测试构造，这里直接测 SessionStore 默认值 +
//! RunRegistry.is_running 组合出的判定逻辑（与 session_detail_with_pending 一致）。

use std::sync::Arc;

use silicon_worker::engine::RunRegistry;
use silicon_worker::session::SessionStore;
use silicon_worker::storage::AppDatabase;

fn temp_db() -> Arc<AppDatabase> {
    static C: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = C.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "siw-running_{}_{}_{}",
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
fn detail_default_not_running_and_registry_reflects_lock() {
    let db = temp_db();
    let store = SessionStore::open(db.clone()).expect("store");
    let s = store
        .create_session("s1", "t", "100", false)
        .expect("session");

    let detail = store
        .get_session_detail(&s.id)
        .expect("detail")
        .expect("some");
    assert!(!detail.is_running, "store 层默认 is_running 应为 false");

    let reg = RunRegistry::default();
    assert!(!reg.is_running(&s.id));
    let _g = reg.try_begin(&s.id).unwrap();
    assert!(reg.is_running(&s.id), "占锁后 is_running=true（回填来源）");
}

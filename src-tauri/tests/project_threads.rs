// T59 P1 Task5：项目线程 store——set_project_id + list_project_threads（只列顶层、不含 child）。
use silicon_worker::session::SessionStore;
use silicon_worker::storage::AppDatabase;
use std::sync::Arc;

fn store() -> SessionStore {
    use std::time::{SystemTime, UNIX_EPOCH};
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let p = std::env::temp_dir().join(format!("siw-t59thr-{}-{}.db", std::process::id(), n));
    let _ = std::fs::remove_file(&p);
    SessionStore::open(Arc::new(AppDatabase::open(&p).unwrap())).unwrap()
}

#[test]
fn list_project_threads_only_top_level() {
    let s = store();
    // 顶层线程：project_id=P，无父。
    s.create_session("thread", "线程", "1", false).unwrap();
    s.set_project_id("thread", "P", "2").unwrap();
    // child：有父（成员任务 run），同 project_id。
    s.create_child_session(
        "child", "thread", "tc", "writer", "任务", None, None, false, "3", None,
    )
    .unwrap();
    s.set_project_id("child", "P", "3").unwrap();
    // 另一个项目的线程，不应混入。
    s.create_session("other", "别的", "4", false).unwrap();
    s.set_project_id("other", "Q", "4").unwrap();

    let threads = s.list_project_threads("P").unwrap();
    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0].id, "thread");
    assert_eq!(threads[0].project_id.as_deref(), Some("P"));
}

// T59 P2 Task1：任务看板投影的数据源——项目线程 + 其下 child 运行的聚合关系。
// （list_project_child_runs 依赖 AppState，状态映射靠 session_children 既有测试；此处回归 store 聚合源。）
use silicon_worker::session::SessionStore;
use silicon_worker::storage::AppDatabase;
use std::sync::Arc;

fn store() -> SessionStore {
    use std::time::{SystemTime, UNIX_EPOCH};
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let p = std::env::temp_dir().join(format!("siw-t59run-{}-{}.db", std::process::id(), n));
    let _ = std::fs::remove_file(&p);
    SessionStore::open(Arc::new(AppDatabase::open(&p).unwrap())).unwrap()
}

#[test]
fn project_thread_children_aggregation() {
    let s = store();
    s.create_session("thread", "主线程", "1", false).unwrap();
    s.set_project_id("thread", "P", "1").unwrap();
    // 两条成员 child run 挂在该线程下。
    s.create_child_session(
        "c1", "thread", "tc1", "writer", "写稿", None, None, false, "2", None,
    )
    .unwrap();
    s.create_child_session(
        "c2", "thread", "tc2", "designer", "配图", None, None, false, "3", None,
    )
    .unwrap();

    // 投影源：项目线程 1 个；该线程下 child 2 个。
    let threads = s.list_project_threads("P").unwrap();
    assert_eq!(threads.len(), 1);
    let children = s.list_children("thread").unwrap();
    assert_eq!(children.len(), 2);
    assert!(children
        .iter()
        .all(|c| c.parent_session_id.as_deref() == Some("thread")));
    let names: Vec<&str> = children
        .iter()
        .filter_map(|c| c.expert_name.as_deref())
        .collect();
    assert!(names.contains(&"writer") && names.contains(&"designer"));
}

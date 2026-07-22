// T59 P1 Task1：projects.permission_mode 默认 manual；sessions.project_id 列存在且可读回。
use silicon_worker::expert::ExpertService;
use silicon_worker::project::ProjectService;
use silicon_worker::session::SessionStore;
use silicon_worker::storage::AppDatabase;
use std::sync::Arc;

fn fresh() -> (Arc<AppDatabase>, std::path::PathBuf) {
    use std::time::{SystemTime, UNIX_EPOCH};
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dbp = std::env::temp_dir().join(format!("siw-t59sch-{}-{}.db", std::process::id(), n));
    let _ = std::fs::remove_file(&dbp);
    let root = std::env::temp_dir().join(format!("siw-t59sch-root-{}-{}", std::process::id(), n));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    (Arc::new(AppDatabase::open(&dbp).unwrap()), root)
}

#[test]
fn project_has_permission_mode_default_manual() {
    let (db, root) = fresh();
    let agents = Arc::new(ExpertService::new(db.clone(), root));
    let svc = ProjectService::new(db, agents);
    let p = svc.create("内容工作室", "做内容", "", None).unwrap();
    assert_eq!(p.permission_mode, "manual");
    let got = svc.get(&p.id).unwrap().unwrap();
    assert_eq!(got.permission_mode, "manual");
    assert_eq!(svc.list().unwrap()[0].permission_mode, "manual");
}

#[test]
fn session_project_id_roundtrips_and_defaults_none() {
    let (db, _root) = fresh();
    let store = SessionStore::open(db).unwrap();
    let s = store.create_session("s1", "普通会话", "1", false).unwrap();
    assert_eq!(s.project_id, None);
    // 顶层会话读回 project_id 仍为 None（向后兼容）。
    let got = store.get_session("s1").unwrap().unwrap();
    assert_eq!(got.project_id, None);
}

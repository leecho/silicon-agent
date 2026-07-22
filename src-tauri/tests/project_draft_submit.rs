use silicon_worker::expert::ExpertService;
use silicon_worker::project::ProjectService;
use silicon_worker::session::SessionStore;
use silicon_worker::storage::AppDatabase;
use std::{path::PathBuf, sync::Arc};

fn test_paths(label: &str) -> (Arc<AppDatabase>, PathBuf, PathBuf) {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("siw-{label}-{}-{seq}", std::process::id()));
    let db = Arc::new(AppDatabase::open(root.join("app.sqlite3")).expect("db"));
    let agents_root = root.join("agents");
    let workspace = root.join("workspace");
    (db, agents_root, workspace)
}

#[test]
fn project_draft_submit_creates_project_session_shape() {
    let (db, agents_root, workspace) = test_paths("project-draft-shape");
    let sessions = SessionStore::open(db.clone()).expect("sessions");
    let agents = Arc::new(ExpertService::new(db.clone(), agents_root));
    let projects = ProjectService::new(db.clone(), agents);
    let project = projects
        .create("项目A", "", "", Some(workspace.to_string_lossy().as_ref()))
        .expect("project");

    let now = "100";
    let session_id = "session-project-draft";
    sessions
        .create_session(session_id, "新会话", now, false)
        .expect("session");
    sessions
        .set_session_origin(session_id, "project")
        .expect("origin");
    sessions
        .set_project_id(session_id, &project.id, now)
        .expect("project id");
    sessions
        .set_working_dir(
            session_id,
            project.workspace_dir.as_deref().expect("workspace"),
            now,
        )
        .expect("workspace");
    sessions
        .set_session_permission_mode(session_id, Some(&project.permission_mode), now)
        .expect("permission");

    let got = sessions
        .get_session(session_id)
        .expect("get")
        .expect("present");
    assert_eq!(got.origin, "project");
    assert_eq!(got.project_id.as_deref(), Some(project.id.as_str()));
    assert!(got.agent_id.is_none());
    assert!(got.role_kind.is_none());
    assert!(got.role_id.is_none());
    assert_eq!(got.working_dir.as_deref(), project.workspace_dir.as_deref());
    assert_eq!(
        got.permission_mode.as_deref(),
        Some(project.permission_mode.as_str())
    );
    assert!(!got.is_draft);
}

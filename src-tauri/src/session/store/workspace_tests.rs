use super::*;

fn temp_store() -> SessionStore {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("sw-ws-test-{nanos}.sqlite3"));
    let db = std::sync::Arc::new(crate::storage::AppDatabase::open(path).unwrap());
    SessionStore::open(db).unwrap()
}

#[test]
fn create_draft_then_promote_clears_flag() {
    let store = temp_store();
    let d = store
        .create_session("session-d1", "草稿", "1", true)
        .unwrap();
    assert!(d.is_draft);
    store
        .set_draft_content("session-d1", "你好 ⟦@a.md⟧", "2")
        .unwrap();
    store.promote_draft("session-d1").unwrap();
    let list = store.list_sessions().unwrap();
    let got = list.iter().find(|x| x.id == "session-d1").unwrap();
    assert!(!got.is_draft);
    assert_eq!(got.draft_content, "");
}

#[test]
fn cleanup_removes_only_empty_drafts() {
    let store = temp_store();
    store
        .create_session("session-empty", "空草稿", "1", true)
        .unwrap();
    store
        .create_session("session-filled", "有内容", "1", true)
        .unwrap();
    store.set_draft_content("session-filled", "x", "2").unwrap();
    let removed = store.cleanup_empty_drafts().unwrap();
    assert_eq!(removed, 1);
    let list = store.list_sessions().unwrap();
    assert!(list.iter().any(|x| x.id == "session-filled"));
    assert!(!list.iter().any(|x| x.id == "session-empty"));
}

#[test]
fn working_dir_defaults_none_then_persists() {
    let store = temp_store();
    let s = store
        .create_session("session-ws-1", "t", "1", false)
        .unwrap();
    assert_eq!(s.working_dir, None);
    assert_eq!(store.get_working_dir("session-ws-1").unwrap(), None);

    store
        .set_working_dir("session-ws-1", "/tmp/proj", "2")
        .unwrap();
    assert_eq!(
        store.get_working_dir("session-ws-1").unwrap(),
        Some("/tmp/proj".to_string())
    );
    // 列也随 SessionInfo 读回。
    let info = store.get_session("session-ws-1").unwrap().unwrap();
    assert_eq!(info.working_dir, Some("/tmp/proj".to_string()));
}

#[test]
fn selected_model_id_round_trip() {
    let store = temp_store();
    store.create_session("s-model-1", "t", "1", false).unwrap();
    assert_eq!(store.get_selected_model_id("s-model-1").unwrap(), None);
    store
        .set_selected_model_id("s-model-1", Some("mdl_abc"), "2")
        .unwrap();
    assert_eq!(
        store.get_selected_model_id("s-model-1").unwrap(),
        Some("mdl_abc".to_string())
    );
    // 置空。
    store.set_selected_model_id("s-model-1", None, "3").unwrap();
    assert_eq!(store.get_selected_model_id("s-model-1").unwrap(), None);
}

#[test]
fn session_has_messages_reflects_appended() {
    let store = temp_store();
    store
        .create_session("session-ws-2", "t", "1", false)
        .unwrap();
    assert!(!store.session_has_messages("session-ws-2").unwrap());
    store
        .append_message(&new_id("msg"), "session-ws-2", "user", "hi", None, "2")
        .unwrap();
    assert!(store.session_has_messages("session-ws-2").unwrap());
}

#[test]
fn recent_workspaces_dedup_order_and_limit() {
    let store = temp_store();
    store.add_recent_workspace("/a", "1").unwrap();
    store.add_recent_workspace("/b", "2").unwrap();
    store.add_recent_workspace("/a", "3").unwrap(); // 重复 → 提到最前
    let recents = store.list_recent_workspaces(8).unwrap();
    assert_eq!(recents, vec!["/a".to_string(), "/b".to_string()]);
    // limit 生效。
    let one = store.list_recent_workspaces(1).unwrap();
    assert_eq!(one, vec!["/a".to_string()]);
}

#[test]
fn session_permission_mode_defaults_none_then_overrides() {
    let store = temp_store();
    let s = store
        .create_session("session-pm-1", "t", "1", false)
        .unwrap();
    assert_eq!(s.permission_mode, None);
    assert_eq!(
        store.get_session_permission_mode("session-pm-1").unwrap(),
        None
    );
    store
        .set_session_permission_mode("session-pm-1", Some("full"), "2")
        .unwrap();
    assert_eq!(
        store.get_session_permission_mode("session-pm-1").unwrap(),
        Some("full".to_string())
    );
    store
        .set_session_permission_mode("session-pm-1", None, "3")
        .unwrap();
    assert_eq!(
        store.get_session_permission_mode("session-pm-1").unwrap(),
        None
    );
}

#[test]
fn artifacts_upsert_keeps_created_at_and_lists_in_order() {
    let store = temp_store();
    store
        .create_session("session-art-1", "t", "1", false)
        .unwrap();
    store
        .add_artifact(
            "session-art-1",
            "a.md",
            "A",
            "final",
            Some("m1"),
            Some("c1"),
            "10",
        )
        .unwrap();
    store
        .add_artifact(
            "session-art-1",
            "gen.py",
            "B",
            "working",
            Some("m2"),
            Some("c2"),
            "20",
        )
        .unwrap();
    // 重复登记 a.md：更新 title/message_id/kind，保留首次 created_at（10）。
    store
        .add_artifact(
            "session-art-1",
            "a.md",
            "A2",
            "final",
            Some("m3"),
            Some("c3"),
            "30",
        )
        .unwrap();

    let list = store.list_artifacts("session-art-1").unwrap();
    // 按 created_at 升序：a(10) 在 gen.py(20) 前。
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].path, "a.md");
    assert_eq!(list[0].title, "A2");
    assert_eq!(list[0].kind, "final");
    assert_eq!(list[0].message_id.as_deref(), Some("m3"));
    assert_eq!(list[0].created_at, "10");
    assert_eq!(list[1].path, "gen.py");
    assert_eq!(list[1].kind, "working");
}

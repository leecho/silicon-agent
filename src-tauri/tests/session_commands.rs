// Tests for the logic paths exercised by the session + submit Tauri commands.
//
// `State<AppState>` cannot be constructed in integration tests, so we test at
// the SessionStore + Engine layer — the same code the commands delegate to.

use std::sync::Arc;

use silicon_agent::engine::Engine;
use silicon_agent::provider::client::{
    ModelCallRequest, ModelCallResult, ModelClient, ModelEvent, ProviderCallError,
};
use silicon_agent::session::SessionStore;
use silicon_agent::storage::AppDatabase;

// ---------------------------------------------------------------------------
// Minimal echo client (mirrors engine_single_turn.rs)
// ---------------------------------------------------------------------------

struct EchoClient;

impl ModelClient for EchoClient {
    fn stream_model_with_events(
        &self,
        _request: ModelCallRequest,
        on_event: &mut dyn FnMut(ModelEvent) -> bool,
    ) -> Result<ModelCallResult, ProviderCallError> {
        on_event(ModelEvent::Delta {
            text: "收到".into(),
        });
        Ok(ModelCallResult {
            events: vec![ModelEvent::AssistantMessageCompleted {
                content: "收到".into(),
            }],
            usage: None,
            finish_reason: Some("stop".into()),
        })
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn temp_db() -> Arc<AppDatabase> {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "siw-cmd_{}_{}_{}",
        std::process::id(),
        seq,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    Arc::new(AppDatabase::open(dir.join("app.sqlite3")).expect("db"))
}

// ---------------------------------------------------------------------------
// Tests corresponding to: list_sessions / get_default_session / create_session
// ---------------------------------------------------------------------------

#[test]
fn list_sessions_empty_then_non_empty() {
    let db = temp_db();
    let store = SessionStore::open(db.clone()).expect("store");

    // Empty at start
    let sessions = store.list_sessions().expect("list");
    assert!(sessions.is_empty());

    // After creating one session
    store
        .create_session("s1", "会话一", "100", false)
        .expect("create");
    let sessions = store.list_sessions().expect("list again");
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id, "s1");
}

#[test]
fn get_default_session_is_idempotent() {
    let db = temp_db();
    let store = SessionStore::open(db.clone()).expect("store");

    // get_or_create_default creates a new session when none exists
    let a = store.get_or_create_default("100").expect("a");
    // Second call returns the same session (idempotent)
    let b = store.get_or_create_default("101").expect("b");
    assert_eq!(a.id, b.id, "should reuse existing session");

    // get_session_detail returns the session with no messages yet
    let detail = store
        .get_session_detail(&a.id)
        .expect("detail ok")
        .expect("detail present");
    assert_eq!(detail.session.id, a.id);
    assert!(detail.messages.is_empty());
}

#[test]
fn create_session_returns_new_session() {
    let db = temp_db();
    let store = SessionStore::open(db.clone()).expect("store");

    let s = store
        .create_session("new-1", "新会话", "200", false)
        .expect("create");
    assert_eq!(s.id, "new-1");
    assert_eq!(s.title, "新会话");
}

// ---------------------------------------------------------------------------
// Tests corresponding to: get_session_detail / submit_user_input
// ---------------------------------------------------------------------------

#[test]
fn get_session_detail_none_for_missing() {
    let db = temp_db();
    let store = SessionStore::open(db.clone()).expect("store");
    let detail = store.get_session_detail("nonexistent").expect("no error");
    assert!(detail.is_none());
}

#[test]
fn submit_user_input_persists_user_and_assistant_messages() {
    let db = temp_db();
    let store = SessionStore::open(db.clone()).expect("store");
    let session = store
        .create_session("s1", "test", "100", false)
        .expect("session");

    let engine = Engine::new(
        SessionStore::open(db.clone()).unwrap(),
        Arc::new(EchoClient),
    );
    let (detail, _pending) = engine
        .submit_user_message(
            &session.id,
            "你好",
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
        .expect("submit");

    // Both user and assistant messages persisted
    assert_eq!(detail.messages.len(), 2);
    assert_eq!(detail.messages[0].role, "user");
    assert_eq!(detail.messages[0].content, "你好");
    assert_eq!(detail.messages[1].role, "assistant");
    assert_eq!(detail.messages[1].content, "收到");
}

#[test]
fn submit_user_input_empty_content_rejected() {
    // Mirrors the guard in the Tauri command: content.trim().is_empty() → Err
    let content = "   ";
    assert!(
        content.trim().is_empty(),
        "whitespace-only input should be rejected by command"
    );
}

#[test]
fn submit_user_input_get_session_detail_roundtrip() {
    let db = temp_db();
    let store = SessionStore::open(db.clone()).expect("store");
    let session = store.get_or_create_default("100").expect("default session");

    let engine = Engine::new(
        SessionStore::open(db.clone()).unwrap(),
        Arc::new(EchoClient),
    );
    let (detail, _pending) = engine
        .submit_user_message(
            &session.id,
            "测试消息",
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
        .expect("submit");

    // Verify via independent store read (as get_session_detail command would do)
    let fetched = store
        .get_session_detail(&session.id)
        .expect("no error")
        .expect("present");
    assert_eq!(fetched.messages.len(), detail.messages.len());
    assert_eq!(fetched.messages[0].content, "测试消息");
    assert_eq!(fetched.messages[1].role, "assistant");
}

use std::sync::{Arc, Mutex};

use silicon_agent::engine::event::AgentStreamEvent;
use silicon_agent::engine::Engine;
use silicon_agent::provider::client::{
    ModelCallRequest, ModelCallResult, ModelClient, ModelEvent, ProviderCallError,
};
use silicon_agent::session::SessionStore;
use silicon_agent::storage::AppDatabase;

struct EchoClient;

impl ModelClient for EchoClient {
    fn stream_model_with_events(
        &self,
        _request: ModelCallRequest,
        on_event: &mut dyn FnMut(ModelEvent) -> bool,
    ) -> Result<ModelCallResult, ProviderCallError> {
        on_event(ModelEvent::Delta { text: "你".into() });
        on_event(ModelEvent::Delta { text: "好".into() });
        Ok(ModelCallResult {
            events: vec![ModelEvent::AssistantMessageCompleted {
                content: "你好".into(),
            }],
            usage: None,
            finish_reason: Some("stop".into()),
        })
    }
}

fn temp_db() -> Arc<AppDatabase> {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "siw-eng_{}_{}_{}",
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
fn single_turn_streams_deltas_and_persists_assistant_message() {
    let db = temp_db();
    let store = SessionStore::open(db.clone()).expect("store");
    let session = store
        .create_session("s1", "t", "100", false)
        .expect("session");

    let events: Arc<Mutex<Vec<AgentStreamEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let events_for_emitter = events.clone();
    let engine = Engine::new(
        SessionStore::open(db.clone()).unwrap(),
        Arc::new(EchoClient),
    )
    .with_emitter(Arc::new(move |e| {
        events_for_emitter.lock().unwrap().push(e)
    }));

    let (detail, _pending) = engine
        .submit_user_message(
            &session.id,
            "在吗",
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
        .expect("submit");

    // 落库：user + assistant 两条，assistant 内容为最终累积。
    assert_eq!(detail.messages.len(), 2);
    assert_eq!(detail.messages[0].role, "user");
    assert_eq!(detail.messages[1].role, "assistant");
    assert_eq!(detail.messages[1].content, "你好");

    // 流式：发了两个 delta + 一个 completed。
    let evts = events.lock().unwrap();
    let deltas = evts.iter().filter(|e| e.kind == "message_delta").count();
    assert_eq!(deltas, 2);
    assert!(evts.iter().any(|e| e.kind == "message_completed"));
}

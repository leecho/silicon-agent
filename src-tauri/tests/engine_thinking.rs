use std::sync::{Arc, Mutex};

use silicon_agent::engine::event::AgentStreamEvent;
use silicon_agent::engine::Engine;
use silicon_agent::provider::client::{
    ModelCallRequest, ModelCallResult, ModelClient, ModelEvent, ProviderCallError,
};
use silicon_agent::session::SessionStore;
use silicon_agent::storage::AppDatabase;

/// EchoClient that emits 2 ThinkingDeltas + 1 Delta + completed.
struct ThinkingEchoClient;

impl ModelClient for ThinkingEchoClient {
    fn stream_model_with_events(
        &self,
        _request: ModelCallRequest,
        on_event: &mut dyn FnMut(ModelEvent) -> bool,
    ) -> Result<ModelCallResult, ProviderCallError> {
        on_event(ModelEvent::ThinkingDelta { text: "想".into() });
        on_event(ModelEvent::ThinkingDelta {
            text: "一下".into(),
        });
        on_event(ModelEvent::Delta {
            text: "答案".into(),
        });
        Ok(ModelCallResult {
            events: vec![ModelEvent::AssistantMessageCompleted {
                content: "答案".into(),
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
        "siw-think_{}_{}_{}",
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
fn thinking_deltas_are_emitted_and_reasoning_is_persisted() {
    let db = temp_db();
    let store = SessionStore::open(db.clone()).expect("store");
    let session = store
        .create_session("s1", "think-test", "100", false)
        .expect("session");

    let events: Arc<Mutex<Vec<AgentStreamEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let events_for_emitter = events.clone();
    let engine = Engine::new(
        SessionStore::open(db.clone()).unwrap(),
        Arc::new(ThinkingEchoClient),
    )
    .with_emitter(Arc::new(move |e| {
        events_for_emitter.lock().unwrap().push(e)
    }));

    let (detail, _pending) = engine
        .submit_user_message(
            &session.id,
            "想一想",
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
        .expect("submit");

    // 断言：emit 了 2 个 thinking_delta 事件
    let evts = events.lock().unwrap();
    let thinking_deltas: Vec<_> = evts.iter().filter(|e| e.kind == "thinking_delta").collect();
    assert_eq!(
        thinking_deltas.len(),
        2,
        "should emit 2 thinking_delta events"
    );

    // 断言：assistant 消息 reasoning == Some("想一下")
    assert_eq!(detail.messages.len(), 2);
    let assistant = &detail.messages[1];
    assert_eq!(assistant.role, "assistant");
    assert_eq!(
        assistant.reasoning,
        Some("想一下".to_string()),
        "reasoning should be accumulated ThinkingDelta texts"
    );
    // 断言：content == "答案"
    assert_eq!(assistant.content, "答案");
}

use std::sync::Arc;

use silicon_agent::engine::Engine;
use silicon_agent::provider::client::{
    ModelCallRequest, ModelCallResult, ModelClient, ModelEvent, ModelUsage, ProviderCallError,
};
use silicon_agent::session::SessionStore;
use silicon_agent::storage::AppDatabase;
use silicon_agent::usage::UsageStore;

struct UsageClient;

impl ModelClient for UsageClient {
    fn active_model_provider(&self) -> Option<(String, String)> {
        Some(("deepseek".into(), "deepseek-chat".into()))
    }
    fn stream_model_with_events(
        &self,
        _request: ModelCallRequest,
        on_event: &mut dyn FnMut(ModelEvent) -> bool,
    ) -> Result<ModelCallResult, ProviderCallError> {
        on_event(ModelEvent::Delta { text: "ok".into() });
        Ok(ModelCallResult {
            events: vec![ModelEvent::AssistantMessageCompleted {
                content: "ok".into(),
            }],
            usage: Some(ModelUsage {
                input_tokens: Some(1000),
                output_tokens: Some(40),
                cache_read_tokens: Some(600),
                cache_create_tokens: Some(0),
            }),
            finish_reason: Some("stop".into()),
        })
    }
}

fn temp_db() -> Arc<AppDatabase> {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "siw-engusage_{}_{}_{}",
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
fn engine_records_usage_after_model_call() {
    let db = temp_db();
    let store = SessionStore::open(db.clone()).expect("store");
    let session = store
        .create_session("s1", "t", "100", false)
        .expect("session");
    let usage = UsageStore::open(db.clone()).expect("usage");

    let engine = Engine::new(
        SessionStore::open(db.clone()).unwrap(),
        Arc::new(UsageClient),
    )
    .with_usage(UsageStore::open(db.clone()).expect("usage2"));

    engine
        .submit_user_message(
            &session.id,
            "hi",
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
        .expect("submit");

    let view = usage.analytics("all", 9_999_999_999).expect("analytics");
    assert_eq!(view.totals.calls, 1);
    assert_eq!(view.totals.cache_read, 600);
    assert_eq!(view.totals.input, 400); // 1000 - 600
    assert_eq!(view.by_model.len(), 1);
    assert_eq!(view.by_model[0].model, "deepseek-chat");
}

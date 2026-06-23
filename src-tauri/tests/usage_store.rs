use std::sync::Arc;

use silicon_agent::provider::client::ModelUsage;
use silicon_agent::session::SessionStore;
use silicon_agent::storage::AppDatabase;
use silicon_agent::usage::{UsageRecord, UsageStore};

fn temp_db() -> Arc<AppDatabase> {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "siw-usage_{}_{}_{}",
        std::process::id(),
        seq,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    Arc::new(AppDatabase::open(dir.join("app.sqlite3")).expect("db"))
}

fn rec(
    session: &str,
    model: &str,
    prompt: u64,
    out: u64,
    cread: u64,
    ccreate: u64,
    at: &str,
) -> UsageRecord {
    UsageRecord {
        session_id: session.into(),
        message_id: Some("m1".into()),
        provider: "deepseek".into(),
        model: model.into(),
        usage_type: "main_agent".into(),
        created_at: at.into(),
        usage: ModelUsage {
            input_tokens: Some(prompt),
            output_tokens: Some(out),
            cache_read_tokens: Some(cread),
            cache_create_tokens: Some(ccreate),
        },
    }
}

#[test]
fn record_splits_non_cached_input() {
    let db = temp_db();
    let store = UsageStore::open(db.clone()).expect("store");
    store
        .record("u1", &rec("s1", "deepseek-chat", 1000, 50, 800, 0, "100"))
        .expect("record");

    let (input, cache_read, output): (i64, i64, i64) = db
        .with_connection(|c| {
            Ok(c.query_row(
                "select input_tokens, cache_read_tokens, output_tokens from token_usage where id='u1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )?)
        })
        .expect("query");
    assert_eq!(input, 200); // 1000 - 800
    assert_eq!(cache_read, 800);
    assert_eq!(output, 50);
}

#[test]
fn ensure_schema_is_idempotent() {
    let db = temp_db();
    UsageStore::open(db.clone()).expect("first");
    UsageStore::open(db.clone()).expect("second");
    assert!(db.table_exists("token_usage").expect("exists"));
}

#[test]
fn analytics_aggregates_totals_models_sessions() {
    let db = temp_db();
    let store = UsageStore::open(db.clone()).expect("store");
    let _sessions = SessionStore::open(db.clone()).expect("session store");
    store
        .record(
            "u1",
            &rec("s1", "deepseek-chat", 1000, 50, 800, 0, "1700000000"),
        )
        .expect("u1");
    store
        .record(
            "u2",
            &rec("s1", "deepseek-chat", 500, 30, 0, 0, "1700000100"),
        )
        .expect("u2");
    store
        .record("u3", &rec("s2", "gpt-4o", 200, 20, 0, 100, "1700000200"))
        .expect("u3");

    let view = store.analytics("all", 1_700_000_300).expect("analytics");

    assert_eq!(view.totals.calls, 3);
    // u1: input200(=1000-800)+cr800+out50=1050; u2: 500+0+30=530; u3: input200+cc100+out20=320
    assert_eq!(view.totals.total, 1050 + 530 + 320);
    assert_eq!(view.totals.cache_read, 800);
    assert_eq!(view.totals.cache_create, 100);

    assert_eq!(view.by_model.len(), 2);
    let chat = view
        .by_model
        .iter()
        .find(|m| m.model == "deepseek-chat")
        .expect("chat");
    assert_eq!(chat.calls, 2);
    assert_eq!(chat.total, 1050 + 530);

    assert_eq!(view.by_session.len(), 2);
    assert_eq!(view.sessions, 2);

    assert_eq!(view.by_hour.len(), 24);
    let hour_sum: u64 = view.by_hour.iter().map(|h| h.calls).sum();
    assert_eq!(hour_sum, 3);
}

#[test]
fn analytics_range_cutoff_filters_old_rows() {
    let db = temp_db();
    let store = UsageStore::open(db.clone()).expect("store");
    let _sessions = SessionStore::open(db.clone()).expect("session store");
    let now = 1_700_000_000_i64;
    store
        .record(
            "old",
            &rec("s1", "m", 100, 10, 0, 0, &(now - 40 * 86_400).to_string()),
        )
        .expect("old");
    store
        .record(
            "new",
            &rec("s1", "m", 200, 20, 0, 0, &(now - 1 * 86_400).to_string()),
        )
        .expect("new");

    let view7 = store.analytics("7d", now).expect("7d");
    assert_eq!(view7.totals.calls, 1);
    let all = store.analytics("all", now).expect("all");
    assert_eq!(all.totals.calls, 2);
}

#[test]
fn analytics_recent_cache_calls_only_with_cache() {
    let db = temp_db();
    let store = UsageStore::open(db.clone()).expect("store");
    let _sessions = SessionStore::open(db.clone()).expect("session store");
    store
        .record("nocache", &rec("s1", "m", 100, 10, 0, 0, "1700000000"))
        .expect("nocache");
    store
        .record("withcache", &rec("s1", "m", 100, 10, 50, 0, "1700000100"))
        .expect("withcache");
    let view = store.analytics("all", 1_700_000_200).expect("analytics");
    assert_eq!(view.recent_cache_calls.len(), 1);
    assert_eq!(view.recent_calls.len(), 2);
}

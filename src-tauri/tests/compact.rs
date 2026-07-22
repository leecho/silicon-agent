use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use silicon_worker::engine::Engine;
use silicon_worker::provider::client::{
    ModelCallRequest, ModelCallResult, ModelClient, ModelEvent, ProviderCallError,
};
use silicon_worker::provider::message::ModelMessage;
use silicon_worker::session::SessionStore;
use silicon_worker::storage::AppDatabase;

fn temp_db() -> Arc<AppDatabase> {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "siw-compact_{}_{}_{}.sqlite3",
        std::process::id(),
        seq,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    Arc::new(AppDatabase::open(path).expect("db"))
}

/// 往会话里塞 `count` 条交替的 user/assistant 消息（created_at 单调递增，保序）。
fn seed_messages(store: &SessionStore, session_id: &str, count: usize) {
    for i in 0..count {
        let role = if i % 2 == 0 { "user" } else { "assistant" };
        store
            .append_message(
                &format!("m{i}"),
                session_id,
                role,
                &format!("内容-{i}"),
                None,
                &format!("{:04}", 100 + i),
            )
            .expect("append");
    }
}

#[test]
fn messages_to_compact_returns_all_but_recent_keep() {
    let db = temp_db();
    let store = SessionStore::open(db).expect("store");
    let s = store
        .create_session("s1", "t", "1", false)
        .expect("session");
    seed_messages(&store, &s.id, 20);

    let old = store.messages_to_compact(&s.id, 12).expect("to_compact");
    // 20 总数 - 12 保留 = 8 条待压缩。
    assert_eq!(old.len(), 8);
    // 保序：第一条应是最早那条。
    assert_eq!(old[0].id, "m0");
    assert_eq!(old[7].id, "m7");
    // 都尚未压缩。
    assert!(old.iter().all(|m| !m.compacted));
}

#[test]
fn mark_compacted_flips_flag_and_excludes_already_compacted() {
    let db = temp_db();
    let store = SessionStore::open(db).expect("store");
    let s = store
        .create_session("s1", "t", "1", false)
        .expect("session");
    seed_messages(&store, &s.id, 20);

    let old = store.messages_to_compact(&s.id, 12).expect("to_compact");
    let ids: Vec<String> = old.iter().map(|m| m.id.clone()).collect();
    store.mark_compacted(&s.id, &ids).expect("mark");

    // list_messages 里这些 id 现在 compacted=true。
    let all = store.list_messages(&s.id).expect("list");
    for m in &all {
        let expected = ids.contains(&m.id);
        assert_eq!(
            m.compacted, expected,
            "消息 {} compacted 应为 {expected}",
            m.id
        );
    }

    // 再次 messages_to_compact：已压缩的不再返回。
    let again = store.messages_to_compact(&s.id, 12).expect("again");
    assert!(again.is_empty(), "已压缩的旧消息不应再被返回");
}

#[test]
fn compaction_summary_roundtrips_empty_is_none() {
    let db = temp_db();
    let store = SessionStore::open(db).expect("store");
    let s = store
        .create_session("s1", "t", "1", false)
        .expect("session");

    // 初始无摘要 → None。
    assert_eq!(store.get_compaction_summary(&s.id).expect("get"), None);

    store
        .set_compaction_summary(&s.id, "这是一段摘要", "2")
        .expect("set");
    assert_eq!(
        store.get_compaction_summary(&s.id).expect("get"),
        Some("这是一段摘要".to_string())
    );

    // 空字符串视为 None。
    store
        .set_compaction_summary(&s.id, "   ", "3")
        .expect("set empty");
    assert_eq!(store.get_compaction_summary(&s.id).expect("get"), None);
}

/// 记录最近一次请求里 messages 的 client：用于断言引擎组装的上下文。
struct RecordingClient {
    last_messages: Mutex<Vec<ModelMessage>>,
}

impl ModelClient for RecordingClient {
    fn stream_model_with_events(
        &self,
        request: ModelCallRequest,
        _cancel: &std::sync::atomic::AtomicBool,
        on_event: &mut dyn FnMut(ModelEvent) -> bool,
    ) -> Result<ModelCallResult, ProviderCallError> {
        *self.last_messages.lock().unwrap() = request.messages.clone();
        on_event(ModelEvent::Delta {
            text: "好的。".into(),
        });
        Ok(ModelCallResult {
            events: vec![ModelEvent::AssistantMessageCompleted {
                content: "好的。".into(),
            }],
            usage: None,
            finish_reason: Some("stop".into()),
        })
    }
}

#[test]
fn engine_context_uses_summary_and_skips_compacted() {
    let db = temp_db();
    let store = SessionStore::open(db.clone()).expect("store");
    let s = store
        .create_session("s1", "t", "1", false)
        .expect("session");
    seed_messages(&store, &s.id, 6);

    // 压缩前 3 条：标 compacted + 存摘要。
    let all = store.list_messages(&s.id).expect("list");
    let compact_ids: Vec<String> = all.iter().take(3).map(|m| m.id.clone()).collect();
    store.mark_compacted(&s.id, &compact_ids).expect("mark");
    store
        .set_compaction_summary(&s.id, "早前讨论了项目计划。", "10")
        .expect("set summary");

    let client = Arc::new(RecordingClient {
        last_messages: Mutex::new(Vec::new()),
    });
    let engine = Engine::new(
        SessionStore::open(db.clone()).expect("store"),
        client.clone(),
    );

    let _ = engine
        .submit_user_message(&s.id, "新问题", Arc::new(AtomicBool::new(false)))
        .expect("submit");

    let captured = client.last_messages.lock().unwrap().clone();
    let rendered = captured
        .iter()
        .map(|m| format!("{:?}", m))
        .collect::<Vec<_>>()
        .join("\n");

    // 上下文含压缩摘要片段。
    assert!(
        rendered.contains("早前讨论了项目计划。"),
        "组装的上下文应含压缩摘要，实际:\n{rendered}"
    );
    // 已压缩的 3 条内容（内容-0/1/2）不进上下文。
    for i in 0..3 {
        assert!(
            !rendered.contains(&format!("内容-{i}")),
            "已压缩消息 内容-{i} 不应进入上下文，实际:\n{rendered}"
        );
    }
    // 未压缩的（内容-3/4/5）仍在上下文。
    for i in 3..6 {
        assert!(
            rendered.contains(&format!("内容-{i}")),
            "未压缩消息 内容-{i} 应进入上下文，实际:\n{rendered}"
        );
    }
}

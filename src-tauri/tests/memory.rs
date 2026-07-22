// Tests for Memory（W0）：remember 引擎拦截 + system_prompt 注入。
//
// ① 引擎 mock 请求 remember{content:"用户喜欢简洁回答"} → memories 落库该条 + tool 结果含「已记入」。
// ② system_prompt(.., &[memory]) 含「已知记忆」「用户喜欢简洁回答」；空记忆不含「已知记忆」。
//
// MemoryStore 的 CRUD 与写时去重在 tests/memory_store.rs 覆盖。

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use silicon_worker::context::prompt::system_prompt;
use silicon_worker::engine::event::AgentStreamEvent;
use silicon_worker::engine::Engine;
use silicon_worker::memory::{Memory, MemoryStore};
use silicon_worker::provider::client::{
    ModelCallRequest, ModelCallResult, ModelClient, ModelEvent, ProviderCallError,
};
use silicon_worker::session::SessionStore;
use silicon_worker::storage::AppDatabase;
use silicon_worker::tools::remember::Remember;
use silicon_worker::tools::ToolRegistry;

fn temp_dir() -> std::path::PathBuf {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "siw-memory_{}_{}_{}",
        std::process::id(),
        seq,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ))
}

// ── ① 引擎拦截 remember ──────────────────────────────────────────────────────

/// 两轮 mock：第一轮请求 remember{content}，第二轮最终答案。
/// 镜像真实 provider 流式：live ToolCallCreated 的 args 为空，完整 args 在最终 result.events。
struct RememberClient {
    calls: AtomicUsize,
    content: String,
}

impl ModelClient for RememberClient {
    fn stream_model_with_events(
        &self,
        _request: ModelCallRequest,
        _cancel: &std::sync::atomic::AtomicBool,
        on_event: &mut dyn FnMut(ModelEvent) -> bool,
    ) -> Result<ModelCallResult, ProviderCallError> {
        let turn = self.calls.fetch_add(1, Ordering::SeqCst);
        if turn == 0 {
            let args = serde_json::json!({ "content": self.content });
            on_event(ModelEvent::ToolCallCreated {
                id: "call-1".into(),
                name: "remember".into(),
                arguments_json: String::new(),
            });
            Ok(ModelCallResult {
                events: vec![ModelEvent::ToolCallCreated {
                    id: "call-1".into(),
                    name: "remember".into(),
                    arguments_json: args.to_string(),
                }],
                usage: None,
                finish_reason: Some("tool_calls".into()),
            })
        } else {
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
}

#[test]
fn remember_is_intercepted_and_persisted() {
    let base = temp_dir();
    let db = Arc::new(AppDatabase::open(base.join("app.sqlite3")).expect("db"));
    let store = SessionStore::open(db.clone()).expect("store");
    let session = store
        .create_session("s1", "memory", "100", false)
        .expect("session");

    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(Remember));

    let events: Arc<Mutex<Vec<AgentStreamEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let events_for_emitter = events.clone();
    let engine = Engine::new(
        SessionStore::open(db.clone()).unwrap(),
        Arc::new(RememberClient {
            calls: AtomicUsize::new(0),
            content: "用户喜欢简洁回答".into(),
        }),
    )
    .with_registry(registry)
    .with_memory(MemoryStore::open(db.clone()).expect("memory"))
    .with_emitter(Arc::new(move |e| {
        events_for_emitter.lock().unwrap().push(e)
    }));

    let (detail, pending) = engine
        .submit_user_message("s1", "记住这个", Arc::new(AtomicBool::new(false)))
        .expect("submit");

    // 不暂停。
    assert!(pending.is_none(), "remember 即时拦截，不应暂停");

    // memories 落库该条（同一 db，故独立 MemoryStore 可见引擎写入）。
    let mems = MemoryStore::open(db.clone())
        .expect("memory")
        .list_memories()
        .expect("list");
    assert_eq!(mems.len(), 1);
    assert_eq!(mems[0].content, "用户喜欢简洁回答");

    // tool 结果含「已记入」。
    let evts = events.lock().unwrap().clone();
    let res = evts
        .iter()
        .find(|e| e.kind == "tool_result" && e.tool_name.as_deref() == Some("remember"))
        .expect("应 emit remember 的 tool_result");
    assert!(
        res.text.as_deref().unwrap_or("").contains("已记入"),
        "tool 结果应含「已记入」，实际：{:?}",
        res.text
    );

    // 第二轮最终答案。
    let final_msg = detail.messages.last().expect("有消息");
    assert_eq!(final_msg.role, "assistant");
    assert_eq!(final_msg.content, "好的。");

    let _ = session;
}

// ── ② 记忆段渲染 + system_prompt 注入 ────────────────────────────────────────

#[test]
fn memory_block_renders_profile_and_facts() {
    let memory = Memory {
        id: "mem-1".into(),
        content: "用户喜欢简洁回答".into(),
        created_at: "100".into(),
    };
    let block = silicon_worker::memory::prompt::render(
        Some("用户是 Rust 工程师"),
        std::slice::from_ref(&memory),
        &[],
    );
    assert!(block.contains("## 用户画像"));
    assert!(block.contains("用户是 Rust 工程师"));
    assert!(block.contains("## 相关记忆"));
    assert!(block.contains("用户喜欢简洁回答"));

    // system_prompt 把预渲染段拼入整体。
    let prompt = system_prompt(
        &[],
        &[],
        &block,
        "normal",
        "",
        None,
        None,
        None,
        true,
        false,
        &[],
    );
    assert!(prompt.contains("## 用户画像"));
    assert!(prompt.contains("用户喜欢简洁回答"));
}

#[test]
fn system_prompt_without_memory_block_has_no_section() {
    let prompt = system_prompt(&[], &[], "", "normal", "", None, None, None, true, false, &[]);
    assert!(!prompt.contains("## 用户画像"));
    assert!(!prompt.contains("## 相关记忆"));
}

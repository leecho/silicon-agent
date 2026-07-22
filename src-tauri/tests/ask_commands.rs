// Tests for the logic paths exercised by the `submit_ask_response` Tauri command.
//
// `State<AppState>` cannot be constructed in integration tests, so we test at
// the SessionStore + Engine layer — the same code the command delegates to.
// The test structure mirrors what the command does:
//   1. submit_user_input → pending Ask (ask_user 拦截)
//   2. append_tool_result(answer) + engine.resume → 续跑最终答案

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use silicon_worker::engine::event::AgentStreamEvent;
use silicon_worker::engine::{Engine, PendingInteraction};
use silicon_worker::provider::client::{
    ModelCallRequest, ModelCallResult, ModelClient, ModelEvent, ProviderCallError,
};
use silicon_worker::session::{new_id, PendingAsk, SessionStore};
use silicon_worker::storage::AppDatabase;
use silicon_worker::tools::ask_user::AskUser;
use silicon_worker::tools::ToolRegistry;

// ---------------------------------------------------------------------------
// Two-turn mock client: turn 0 requests ask_user, turn 1 gives final answer.
// ---------------------------------------------------------------------------

struct AskClient {
    calls: AtomicUsize,
}

impl ModelClient for AskClient {
    fn stream_model_with_events(
        &self,
        _request: ModelCallRequest,
        _cancel: &std::sync::atomic::AtomicBool,
        on_event: &mut dyn FnMut(ModelEvent) -> bool,
    ) -> Result<ModelCallResult, ProviderCallError> {
        let turn = self.calls.fetch_add(1, Ordering::SeqCst);
        if turn == 0 {
            // First turn: model requests ask_user with questions array.
            let args = serde_json::json!({ "questions": [{"question": "要哪种?", "options": ["A", "B"]}] });
            on_event(ModelEvent::ToolCallCreated {
                id: "call-1".into(),
                name: "ask_user".into(),
                arguments_json: String::new(),
            });
            Ok(ModelCallResult {
                events: vec![ModelEvent::ToolCallCreated {
                    id: "call-1".into(),
                    name: "ask_user".into(),
                    arguments_json: args.to_string(),
                }],
                usage: None,
                finish_reason: Some("tool_calls".into()),
            })
        } else {
            // Second turn: final answer after user replied.
            on_event(ModelEvent::Delta {
                text: "好的，给你 A。".into(),
            });
            Ok(ModelCallResult {
                events: vec![ModelEvent::AssistantMessageCompleted {
                    content: "好的，给你 A。".into(),
                }],
                usage: None,
                finish_reason: Some("stop".into()),
            })
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn temp_dir() -> std::path::PathBuf {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "siw-ask-cmd_{}_{}_{}",
        std::process::id(),
        seq,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ))
}

fn now_string() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_default()
}

fn expect_ask(pending: Option<PendingInteraction>) -> PendingAsk {
    match pending {
        Some(PendingInteraction::Ask(a)) => a,
        _ => panic!("应为 Ask 暂停"),
    }
}

// ---------------------------------------------------------------------------
// Test: submit_ask_response command path
//
//   submit_user_input → Ask pending (ask_user 拦截)
//   → append_tool_result("call-1","ask_user","我选A") + engine.resume
//   → 续跑最终答案、None、messages 含 tool("我选A") + assistant(final)
// ---------------------------------------------------------------------------

#[test]
fn submit_ask_response_appends_answer_and_resumes() {
    let base = temp_dir();
    let db = Arc::new(AppDatabase::open(base.join("app.sqlite3")).expect("db"));

    let store = SessionStore::open(db.clone()).expect("store");
    let session = store
        .create_session("s-ask-cmd", "ask-cmd", "100", false)
        .expect("session");

    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(AskUser));

    let events: Arc<Mutex<Vec<AgentStreamEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let events_for_emitter = events.clone();
    let engine = Engine::new(
        SessionStore::open(db.clone()).unwrap(),
        Arc::new(AskClient {
            calls: AtomicUsize::new(0),
        }),
    )
    .with_registry(registry)
    .with_emitter(Arc::new(move |e| {
        events_for_emitter.lock().unwrap().push(e)
    }));

    // 1. submit_user_input → 引擎拦截 ask_user → Ask pending（等价命令层）。
    let (detail, pending) = engine
        .submit_user_message(
            &session.id,
            "给我推荐一个",
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
        .expect("submit");

    let ask = expect_ask(pending);
    assert_eq!(ask.tool_call_id, "call-1");
    assert_eq!(ask.questions[0].question, "要哪种?");
    assert_eq!(
        ask.questions[0].options,
        vec!["A".to_string(), "B".to_string()]
    );

    // 未执行 ask_user：消息只有 user + assistant(tool_calls)，无 tool 结果。
    let roles: Vec<&str> = detail.messages.iter().map(|m| m.role.as_str()).collect();
    assert_eq!(roles, vec!["user", "assistant"]);

    // emit 了 ask_required 事件。
    {
        let evts = events.lock().unwrap();
        let req = evts
            .iter()
            .find(|e| e.kind == "ask_required")
            .expect("应 emit ask_required");
        assert_eq!(req.tool_call_id.as_deref(), Some("call-1"));
        assert_eq!(req.text.as_deref(), Some("要哪种?"));
    }

    // 2. 命令层：append_tool_result(答案) — 等价 submit_ask_response 的第一步。
    store
        .append_tool_result(
            &new_id("msg"),
            &session.id,
            "call-1",
            "ask_user",
            "我选A",
            "done",
            &now_string(),
        )
        .expect("append answer");

    // 3. engine.resume — 等价 submit_ask_response 的第二步。
    let (detail2, pending2) = engine
        .resume(
            &session.id,
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
        .expect("resume");

    // 续跑后不再暂停。
    assert!(pending2.is_none(), "回答后续跑应完成，pending 应为 None");

    // 消息序列：user / assistant(tool_calls) / tool(答案) / assistant(final)。
    let roles2: Vec<&str> = detail2.messages.iter().map(|m| m.role.as_str()).collect();
    assert_eq!(roles2, vec!["user", "assistant", "tool", "assistant"]);

    // tool 消息：答案正确落库。
    let tool_msg = &detail2.messages[2];
    assert_eq!(tool_msg.role, "tool");
    assert_eq!(tool_msg.tool_call_id.as_deref(), Some("call-1"));
    assert_eq!(tool_msg.tool_name.as_deref(), Some("ask_user"));
    assert_eq!(tool_msg.content, "我选A");

    // 最终 assistant 消息。
    let final_msg = &detail2.messages[3];
    assert_eq!(final_msg.role, "assistant");
    assert_eq!(final_msg.content, "好的，给你 A。");
    assert!(final_msg.tool_calls_json.is_none());
}

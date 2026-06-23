// Tests for the ask_user control tool + engine interception (Slice 5b Task 1).
//
// 模型第一轮调用 ask_user（控制工具）→ 引擎按名拦截：不真执行、emit ask_required、
// 返回 PendingInteraction::Ask 暂停。命令层把用户答案落为该 ask_user 调用的 tool 结果后
// resume → 模型据此续跑给最终答案。

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use silicon_agent::engine::event::AgentStreamEvent;
use silicon_agent::engine::{Engine, PendingInteraction};
use silicon_agent::provider::client::{
    ModelCallRequest, ModelCallResult, ModelClient, ModelEvent, ProviderCallError,
};
use silicon_agent::session::{new_id, PendingAsk, SessionStore};
use silicon_agent::storage::AppDatabase;
use silicon_agent::tools::ask_user::AskUser;
use silicon_agent::tools::ToolRegistry;

/// 两轮 mock：第一轮请求 ask_user（带 question + options），第二轮基于用户答案给最终答案。
/// 镜像真实 provider 流式：live ToolCallCreated 的 args 为空，完整 args 在最终 result.events。
struct AskClient {
    calls: AtomicUsize,
}

impl ModelClient for AskClient {
    fn stream_model_with_events(
        &self,
        _request: ModelCallRequest,
        on_event: &mut dyn FnMut(ModelEvent) -> bool,
    ) -> Result<ModelCallResult, ProviderCallError> {
        let turn = self.calls.fetch_add(1, Ordering::SeqCst);
        if turn == 0 {
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

fn temp_dir() -> std::path::PathBuf {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "siw-ask_{}_{}_{}",
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

/// submit → 引擎拦截 ask_user：返回 PendingInteraction::Ask、ask_user 未执行（无 tool 结果）、emit ask_required。
/// append_tool_result(答案) + resume → 第二轮最终答案、返回 None、messages 含 user/assistant(tool_calls)/tool(答案)/assistant(final)。
#[test]
fn ask_user_pauses_until_answered_then_resumes() {
    let base = temp_dir();
    let db = Arc::new(AppDatabase::open(base.join("app.sqlite3")).expect("db"));

    let store = SessionStore::open(db.clone()).expect("store");
    let session = store
        .create_session("s1", "ask", "100", false)
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

    // 1. submit → 暂停（ask_user 拦截）。
    let (detail, pending) = engine
        .submit_user_message(
            &session.id,
            "给我一个东西",
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
        .expect("submit");

    let ask: PendingAsk = match pending {
        Some(PendingInteraction::Ask(a)) => a,
        _ => panic!("应为 ask_user 暂停"),
    };
    assert_eq!(ask.tool_call_id, "call-1");
    assert_eq!(ask.questions[0].question, "要哪种?");
    assert_eq!(
        ask.questions[0].options,
        vec!["A".to_string(), "B".to_string()]
    );

    // ask_user 未执行：消息只有 user + assistant(tool_calls)，无 tool 结果消息。
    let roles: Vec<&str> = detail.messages.iter().map(|m| m.role.as_str()).collect();
    assert_eq!(roles, vec!["user", "assistant"]);
    assert!(
        detail.messages.iter().all(|m| m.role != "tool"),
        "暂停时不应有 ask_user 的 tool 结果消息"
    );

    // emit 了 ask_required（带 question + tool_call_id），且无 tool_result。
    {
        let evts = events.lock().unwrap();
        let req = evts
            .iter()
            .find(|e| e.kind == "ask_required")
            .expect("应 emit ask_required");
        assert_eq!(req.tool_name.as_deref(), Some("ask_user"));
        assert_eq!(req.tool_call_id.as_deref(), Some("call-1"));
        assert_eq!(req.text.as_deref(), Some("要哪种?"));
        assert!(
            !evts.iter().any(|e| e.kind == "tool_result"),
            "拦截 ask_user 时不应 emit tool_result"
        );
    }

    // 2. 落用户答案为 ask_user 的 tool 结果 + resume → 续跑给最终答案。
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

    let (detail2, pending2) = engine
        .resume(
            &session.id,
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
        .expect("resume");
    assert!(pending2.is_none(), "回答后续跑应完成，不再暂停");

    // 消息序列：user / assistant(tool_calls) / tool(答案) / assistant(final)。
    let roles2: Vec<&str> = detail2.messages.iter().map(|m| m.role.as_str()).collect();
    assert_eq!(roles2, vec!["user", "assistant", "tool", "assistant"]);

    let answer_msg = &detail2.messages[2];
    assert_eq!(answer_msg.tool_call_id.as_deref(), Some("call-1"));
    assert_eq!(answer_msg.tool_name.as_deref(), Some("ask_user"));
    assert_eq!(answer_msg.content, "我选A");

    let final_msg = &detail2.messages[3];
    assert_eq!(final_msg.content, "好的，给你 A。");
    assert!(final_msg.tool_calls_json.is_none());
}

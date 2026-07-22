use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use silicon_worker::engine::event::AgentStreamEvent;
use silicon_worker::engine::Engine;
use silicon_worker::provider::client::{
    ModelCallRequest, ModelCallResult, ModelClient, ModelEvent, ProviderCallError,
};
use silicon_worker::session::SessionStore;
use silicon_worker::storage::AppDatabase;
use silicon_worker::tools::ToolRegistry;

/// 两轮 mock：第一轮请求未注册工具 no_such_tool（→ registry.execute Err），
/// 第二轮基于错误结果给最终答案。
struct FailingToolClient {
    calls: AtomicUsize,
}

impl ModelClient for FailingToolClient {
    fn stream_model_with_events(
        &self,
        _request: ModelCallRequest,
        _cancel: &std::sync::atomic::AtomicBool,
        on_event: &mut dyn FnMut(ModelEvent) -> bool,
    ) -> Result<ModelCallResult, ProviderCallError> {
        let turn = self.calls.fetch_add(1, Ordering::SeqCst);
        if turn == 0 {
            // 第一轮：请求未注册的工具，live 回调参数为空（同真实 provider 行为）。
            on_event(ModelEvent::ToolCallCreated {
                id: "call-1".into(),
                name: "no_such_tool".into(),
                arguments_json: String::new(),
            });
            Ok(ModelCallResult {
                events: vec![ModelEvent::ToolCallCreated {
                    id: "call-1".into(),
                    name: "no_such_tool".into(),
                    arguments_json: "{}".into(),
                }],
                usage: None,
                finish_reason: Some("tool_calls".into()),
            })
        } else {
            // 第二轮：基于工具失败结果给最终答案。
            on_event(ModelEvent::Delta {
                text: "工具调用失败，无法完成操作。".into(),
            });
            Ok(ModelCallResult {
                events: vec![ModelEvent::AssistantMessageCompleted {
                    content: "工具调用失败，无法完成操作。".into(),
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
        "siw-tool-failed_{}_{}_{}",
        std::process::id(),
        seq,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ))
}

#[test]
fn tool_failed_emits_failed_status_and_continues_to_final_answer() {
    let base = temp_dir();
    std::fs::create_dir_all(&base).expect("base dir");
    let db = Arc::new(AppDatabase::open(base.join("app.sqlite3")).expect("db"));

    let store = SessionStore::open(db.clone()).expect("store");
    let session = store
        .create_session("s1", "tool-failed", "100", false)
        .expect("session");

    // 空注册表：no_such_tool 未注册 → registry.execute 返回 Err。
    let registry = ToolRegistry::new();

    let events: Arc<Mutex<Vec<AgentStreamEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let events_for_emitter = events.clone();
    let engine = Engine::new(
        SessionStore::open(db.clone()).unwrap(),
        Arc::new(FailingToolClient {
            calls: AtomicUsize::new(0),
        }),
    )
    .with_registry(registry)
    .with_emitter(Arc::new(move |e| {
        events_for_emitter.lock().unwrap().push(e)
    }));

    let (detail, pending) = engine
        .submit_user_message(
            &session.id,
            "调用未知工具",
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
        .expect("submit");

    // 流程应正常收口（无 pending 交互）。
    assert!(pending.is_none(), "未知工具不应产生 pending");

    // 消息序列：user / assistant(tool_calls) / tool(error result) / assistant(final)。
    let roles: Vec<&str> = detail.messages.iter().map(|m| m.role.as_str()).collect();
    assert_eq!(
        roles,
        vec!["user", "assistant", "tool", "assistant"],
        "消息序列应为 user/assistant/tool/assistant"
    );

    // tool 结果消息应含错误文本（未知工具提示）且 tool_status="failed"。
    let tool_msg = &detail.messages[2];
    assert_eq!(tool_msg.tool_call_id.as_deref(), Some("call-1"));
    assert_eq!(tool_msg.tool_name.as_deref(), Some("no_such_tool"));
    assert!(
        tool_msg.content.contains("未知工具") || tool_msg.content.contains("no_such_tool"),
        "tool 结果消息应含错误文本，实际: {}",
        tool_msg.content
    );
    assert_eq!(
        tool_msg.tool_status.as_deref(),
        Some("failed"),
        "失败工具的 tool 消息 tool_status 应为 failed，实际: {:?}",
        tool_msg.tool_status
    );

    // 最终答案应落库。
    let final_msg = &detail.messages[3];
    assert!(!final_msg.content.is_empty(), "最终答案应非空");

    let evts = events.lock().unwrap();

    // tool_result 事件 status 应为 "failed"。
    let tool_result_evt = evts
        .iter()
        .find(|e| e.kind == "tool_result" && e.tool_call_id.as_deref() == Some("call-1"))
        .expect("应 emit tool_result 事件");
    assert_eq!(
        tool_result_evt.status.as_deref(),
        Some("failed"),
        "未注册工具的 tool_result 事件 status 应为 failed，实际: {:?}",
        tool_result_evt.status
    );

    // tool_result 事件 text 应含错误信息。
    let result_text = tool_result_evt
        .text
        .as_deref()
        .expect("tool_result 事件应有 text");
    assert!(
        result_text.contains("未知工具") || result_text.contains("no_such_tool"),
        "tool_result text 应含错误文本，实际: {result_text}"
    );

    // 流程继续到第二轮：应 emit message_completed。
    assert!(
        evts.iter()
            .any(|e| e.kind == "message_completed" && e.status.as_deref() == Some("done")),
        "应 emit status=done 的 message_completed（流程继续到最终答案）"
    );
}

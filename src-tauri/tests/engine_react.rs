use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use silicon_worker::engine::event::AgentStreamEvent;
use silicon_worker::engine::Engine;
use silicon_worker::provider::client::{
    ModelCallRequest, ModelCallResult, ModelClient, ModelEvent, ProviderCallError,
};
use silicon_worker::session::SessionStore;
use silicon_worker::storage::AppDatabase;
use silicon_worker::tools::fs_tools::WriteFile;
use silicon_worker::tools::ToolRegistry;

/// 两轮 mock：第一次请求 write_file 工具（无最终文本），第二次给最终答案。
struct ReActClient {
    calls: AtomicUsize,
    target: String,
}

impl ModelClient for ReActClient {
    fn stream_model_with_events(
        &self,
        _request: ModelCallRequest,
        _cancel: &std::sync::atomic::AtomicBool,
        on_event: &mut dyn FnMut(ModelEvent) -> bool,
    ) -> Result<ModelCallResult, ProviderCallError> {
        let turn = self.calls.fetch_add(1, Ordering::SeqCst);
        if turn == 0 {
            // 第一轮：请求写文件，没有最终文本。镜像真实 provider 流式行为：
            // live 回调发 ToolCallCreated 时 arguments 常为空（工具名先到、args 分片后到）；
            // 完整累积的 tool_call(含 args)在最终 result.events 里。引擎应从 result 取，不从 live。
            let args = serde_json::json!({ "path": self.target, "content": "hi" });
            on_event(ModelEvent::ThinkingDelta {
                text: "先想想".into(),
            });
            on_event(ModelEvent::ToolCallCreated {
                id: "call-1".into(),
                name: "write_file".into(),
                arguments_json: String::new(), // live: args 尚未到达
            });
            Ok(ModelCallResult {
                events: vec![ModelEvent::ToolCallCreated {
                    id: "call-1".into(),
                    name: "write_file".into(),
                    arguments_json: args.to_string(), // 最终: 完整累积 args
                }],
                usage: None,
                finish_reason: Some("tool_calls".into()),
            })
        } else {
            // 第二轮：基于工具结果给最终答案。
            on_event(ModelEvent::Delta {
                text: "已写入文件。".into(),
            });
            Ok(ModelCallResult {
                events: vec![ModelEvent::AssistantMessageCompleted {
                    content: "已写入文件。".into(),
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
        "siw-react_{}_{}_{}",
        std::process::id(),
        seq,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ))
}

#[test]
fn react_loop_executes_tool_then_returns_final_answer() {
    let base = temp_dir();
    let workspace = base.join("workspace");
    std::fs::create_dir_all(&workspace).expect("workspace");
    let db = Arc::new(AppDatabase::open(base.join("app.sqlite3")).expect("db"));

    let store = SessionStore::open(db.clone()).expect("store");
    let session = store
        .create_session("s1", "react", "100", false)
        .expect("session");
    // write_file 是风险工具（requires_confirmation）：本测试聚焦 ReAct 执行/回灌路径，
    // 预先会话级授权以保留「直接执行」语义（权限暂停/续跑由 engine_permission 测试覆盖）。
    store
        .grant_tool(&session.id, "write_file", "100")
        .expect("grant");

    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(WriteFile {
        workspace: workspace.clone(),
    }));

    let events: Arc<Mutex<Vec<AgentStreamEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let events_for_emitter = events.clone();
    let engine = Engine::new(
        SessionStore::open(db.clone()).unwrap(),
        Arc::new(ReActClient {
            calls: AtomicUsize::new(0),
            target: "out.txt".into(),
        }),
    )
    .with_registry(registry)
    .with_emitter(Arc::new(move |e| {
        events_for_emitter.lock().unwrap().push(e)
    }));

    let (detail, _pending) = engine
        .submit_user_message(
            &session.id,
            "写一个 out.txt 内容为 hi",
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
        .expect("submit");

    // 文件被写入。
    let written = std::fs::read_to_string(workspace.join("out.txt")).expect("file written");
    assert_eq!(written, "hi");

    // 消息序列：user / assistant(tool_calls) / tool(result) / assistant(final)。
    let roles: Vec<&str> = detail.messages.iter().map(|m| m.role.as_str()).collect();
    assert_eq!(roles, vec!["user", "assistant", "tool", "assistant"]);

    let assistant_tool = &detail.messages[1];
    assert!(
        assistant_tool.tool_calls_json.is_some(),
        "assistant 消息应携带 tool_calls_json"
    );
    assert_eq!(
        assistant_tool.reasoning.as_deref(),
        Some("先想想"),
        "工具轮 assistant 消息应持久化 reasoning"
    );
    let tool_msg = &detail.messages[2];
    assert_eq!(tool_msg.tool_call_id.as_deref(), Some("call-1"));
    assert_eq!(tool_msg.tool_name.as_deref(), Some("write_file"));
    assert_eq!(
        tool_msg.tool_status.as_deref(),
        Some("done"),
        "成功工具的 tool 消息 tool_status 应为 done，实际: {:?}",
        tool_msg.tool_status
    );

    let final_msg = &detail.messages[3];
    assert_eq!(final_msg.content, "已写入文件。");
    assert!(final_msg.tool_calls_json.is_none());

    // 流式事件：发了 tool_call + tool_result + 最终 completed。
    let evts = events.lock().unwrap();

    // tool_call 事件：tool_name/tool_call_id 正确，text 含完整输入参数关键字（path 和 content）。
    let tool_call_evt = evts
        .iter()
        .find(|e| e.kind == "tool_call" && e.tool_name.as_deref() == Some("write_file"))
        .expect("应 emit tool_call 事件");
    assert_eq!(
        tool_call_evt.tool_call_id.as_deref(),
        Some("call-1"),
        "tool_call 事件 tool_call_id 应为 call-1"
    );
    let tool_call_text = tool_call_evt
        .text
        .as_deref()
        .expect("tool_call 事件应有 text");
    assert!(
        tool_call_text.contains("path"),
        "tool_call text 应含参数键 path，实际: {tool_call_text}"
    );
    assert!(
        tool_call_text.contains("content"),
        "tool_call text 应含参数键 content，实际: {tool_call_text}"
    );
    assert!(
        tool_call_text.contains("out.txt"),
        "tool_call text 应含 path 值 out.txt，实际: {tool_call_text}"
    );

    // tool_result 事件：tool_call_id 正确，text 含结果文本。
    let tool_result_evt = evts
        .iter()
        .find(|e| e.kind == "tool_result" && e.tool_call_id.as_deref() == Some("call-1"))
        .expect("应 emit tool_result 事件");
    assert!(tool_result_evt.text.is_some(), "tool_result 事件应有 text");

    assert!(
        evts.iter().any(|e| e.kind == "message_completed"),
        "应 emit message_completed"
    );
}

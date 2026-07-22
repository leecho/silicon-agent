use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use silicon_worker::context::prompt::system_prompt;
use silicon_worker::engine::Engine;
use silicon_worker::provider::client::{
    ModelCallRequest, ModelCallResult, ModelClient, ModelEvent, ProviderCallError,
};
use silicon_worker::session::SessionStore;
use silicon_worker::storage::AppDatabase;
use silicon_worker::tools::fs_tools::WriteFile;
use silicon_worker::tools::ToolRegistry;

fn temp_dir() -> std::path::PathBuf {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "siw-planmode_{}_{}_{}",
        std::process::id(),
        seq,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ))
}

/// ① plan 模式 system_prompt 含计划模式指引 + propose_plan；normal 模式不含。
#[test]
fn system_prompt_plan_mode_appends_guidance() {
    let plan = system_prompt(&[], &[], "", "plan", "", None, None, None, true, false, &[]);
    assert!(plan.contains("计划模式"), "plan 模式应含「计划模式」段");
    assert!(
        plan.contains("propose_plan"),
        "plan 模式应提到 propose_plan"
    );

    let normal = system_prompt(&[], &[], "", "normal", "", None, None, None, true, false, &[]);
    assert!(
        !normal.contains("计划模式"),
        "normal 模式不应含「计划模式」段"
    );
    assert!(
        !normal.contains("propose_plan"),
        "normal 模式不应提到 propose_plan"
    );
}

/// ② set_session_mode 写入 → get_session_mode 读回；非法 mode → Err；默认 normal。
#[test]
fn set_and_get_session_mode() {
    let base = temp_dir();
    std::fs::create_dir_all(&base).expect("base");
    let db = Arc::new(AppDatabase::open(base.join("app.sqlite3")).expect("db"));
    let store = SessionStore::open(db).expect("store");
    let session = store
        .create_session("s1", "m", "100", false)
        .expect("session");

    // 默认 normal（create_session 默认）。
    assert_eq!(store.get_session_mode(&session.id).unwrap(), "normal", "");
    assert_eq!(session.mode, "normal", "");

    // 切到 plan 并读回。
    store
        .set_session_mode(&session.id, "plan", "200")
        .expect("set plan");
    assert_eq!(store.get_session_mode(&session.id).unwrap(), "plan", "");

    // 切回 normal。
    store
        .set_session_mode(&session.id, "normal", "300")
        .expect("set normal");
    assert_eq!(store.get_session_mode(&session.id).unwrap(), "normal", "");

    // 非法 mode → Err，且模式不变。
    let err = store.set_session_mode(&session.id, "wild", "400");
    assert!(err.is_err(), "非法 mode 应 Err");
    assert_eq!(store.get_session_mode(&session.id).unwrap(), "normal", "");

    // 未知会话 → 默认 normal（不 panic）。
    assert_eq!(store.get_session_mode("nope").unwrap(), "normal", "");
}

/// 两轮 mock：第一轮请求 write_file（写工具），第二轮给最终答案。
struct WriteThenAnswerClient {
    calls: AtomicUsize,
}

impl ModelClient for WriteThenAnswerClient {
    fn stream_model_with_events(
        &self,
        _request: ModelCallRequest,
        _cancel: &std::sync::atomic::AtomicBool,
        on_event: &mut dyn FnMut(ModelEvent) -> bool,
    ) -> Result<ModelCallResult, ProviderCallError> {
        let turn = self.calls.fetch_add(1, Ordering::SeqCst);
        if turn == 0 {
            let args = serde_json::json!({ "path": "out.txt", "content": "hello" });
            on_event(ModelEvent::ToolCallCreated {
                id: "call-w".into(),
                name: "write_file".into(),
                arguments_json: String::new(),
            });
            Ok(ModelCallResult {
                events: vec![ModelEvent::ToolCallCreated {
                    id: "call-w".into(),
                    name: "write_file".into(),
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

/// ③ plan 模式下模型请求 write_file → 安全网拦截：落「计划模式下不可执行」结果、文件未写、不暂停、续跑收口。
#[test]
fn plan_mode_blocks_write_tool_without_pause() {
    let base = temp_dir();
    let workspace = base.join("workspace");
    std::fs::create_dir_all(&workspace).expect("workspace");
    let db = Arc::new(AppDatabase::open(base.join("app.sqlite3")).expect("db"));

    let store = SessionStore::open(db.clone()).expect("store");
    let session = store
        .create_session("s2", "plan", "100", false)
        .expect("session");
    store
        .set_session_mode(&session.id, "plan", "100")
        .expect("set plan");

    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(WriteFile {
        workspace: workspace.clone(),
    }));

    let engine = Engine::new(
        SessionStore::open(db).unwrap(),
        Arc::new(WriteThenAnswerClient {
            calls: AtomicUsize::new(0),
        }),
    )
    .with_registry(registry);

    let (detail, pending) = engine
        .submit_user_message(
            &session.id,
            "帮我写个文件",
            Arc::new(AtomicBool::new(false)),
        )
        .expect("submit");

    // 不暂停（安全网是 continue，非 pause），直接续跑到最终答案。
    assert!(pending.is_none(), "计划模式安全网不应暂停");

    // 消息序列：user / assistant(tool_calls) / tool(拦截结果) / assistant(final)。
    let roles: Vec<&str> = detail.messages.iter().map(|m| m.role.as_str()).collect();
    assert_eq!(roles, vec!["user", "assistant", "tool", "assistant"]);

    let tool_msg = &detail.messages[2];
    assert_eq!(tool_msg.tool_call_id.as_deref(), Some("call-w"));
    assert_eq!(tool_msg.tool_name.as_deref(), Some("write_file"));
    assert!(
        tool_msg.content.contains("计划模式下不可执行"),
        "tool 结果应含拦截提示，实际: {}",
        tool_msg.content
    );

    // 文件未写。
    assert!(
        !workspace.join("out.txt").exists(),
        "计划模式下文件不应被写入"
    );
}

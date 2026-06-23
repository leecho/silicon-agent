// Tests for the logic paths exercised by the `submit_permission_decision` Tauri command.
//
// `State<AppState>` cannot be constructed in integration tests, so we test at
// the SessionStore + Engine layer — the same code the command delegates to.
// The test structure mirrors what the command does:
//   1. submit_user_input → pending (暂停)
//   2a. Approve:  grant_tool + engine.resume
//   2b. Deny:     append_tool_result("用户拒绝了该操作。") + engine.resume

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use silicon_agent::engine::{Engine, PendingInteraction};
use silicon_agent::provider::client::{
    ModelCallRequest, ModelCallResult, ModelClient, ModelEvent, ProviderCallError,
};
use silicon_agent::session::PendingPermission;
use silicon_agent::session::{new_id, SessionStore};
use silicon_agent::storage::AppDatabase;
use silicon_agent::tools::command_tool::CommandExecute;
use silicon_agent::tools::ToolRegistry;

// ---------------------------------------------------------------------------
// Two-turn mock client: turn 0 requests run_command, turn 1 gives final answer.
// ---------------------------------------------------------------------------

struct TwoTurnClient {
    calls: AtomicUsize,
    /// Expected answer returned on the final (non-tool) turn.
    final_answer: String,
}

impl TwoTurnClient {
    fn new(final_answer: impl Into<String>) -> Self {
        Self {
            calls: AtomicUsize::new(0),
            final_answer: final_answer.into(),
        }
    }
}

impl ModelClient for TwoTurnClient {
    fn stream_model_with_events(
        &self,
        _request: ModelCallRequest,
        on_event: &mut dyn FnMut(ModelEvent) -> bool,
    ) -> Result<ModelCallResult, ProviderCallError> {
        let turn = self.calls.fetch_add(1, Ordering::SeqCst);
        if turn == 0 {
            // First turn: model requests run_command (a risk tool needing confirmation).
            let args = serde_json::json!({ "program": "echo", "args": ["hello"] });
            on_event(ModelEvent::ToolCallCreated {
                id: "cmd-1".into(),
                name: "run_command".into(),
                arguments_json: String::new(),
            });
            Ok(ModelCallResult {
                events: vec![ModelEvent::ToolCallCreated {
                    id: "cmd-1".into(),
                    name: "run_command".into(),
                    arguments_json: args.to_string(),
                }],
                usage: None,
                finish_reason: Some("tool_calls".into()),
            })
        } else {
            // Subsequent turns: final answer (after tool result or denial).
            let text = self.final_answer.clone();
            on_event(ModelEvent::Delta { text: text.clone() });
            Ok(ModelCallResult {
                events: vec![ModelEvent::AssistantMessageCompleted { content: text }],
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
        "siw-perm-cmd_{}_{}_{}",
        std::process::id(),
        seq,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ))
}

fn setup() -> (
    Arc<AppDatabase>,
    SessionStore,
    ToolRegistry,
    std::path::PathBuf,
) {
    let base = temp_dir();
    let workspace = base.join("workspace");
    std::fs::create_dir_all(&workspace).expect("workspace");
    let db = Arc::new(AppDatabase::open(base.join("app.sqlite3")).expect("db"));
    let store = SessionStore::open(db.clone()).expect("store");
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(CommandExecute {
        workspace: workspace.clone(),
    }));
    (db, store, registry, workspace)
}

/// 从泛化后的 `PendingInteraction` 取出权限暂停；其余形态视为断言失败。
fn expect_permission(pending: Option<PendingInteraction>) -> PendingPermission {
    match pending {
        Some(PendingInteraction::Permission(p)) => p,
        _ => panic!("应为权限暂停"),
    }
}

/// `now_string` mirror (same logic as engine/app_state).
fn now_string() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Test A: Approve path
//   submit → pending (run_command paused)
//   → find_pending_tool_name + grant_tool + engine.resume → executed + final answer
// ---------------------------------------------------------------------------

#[test]
fn approve_path_grants_and_resumes() {
    let (db, store, registry, _workspace) = setup();
    let session = store
        .create_session("s-approve", "approve", "100", false)
        .expect("session");

    let engine = Engine::new(
        SessionStore::open(db.clone()).unwrap(),
        Arc::new(TwoTurnClient::new("命令已执行完毕。")),
    )
    .with_registry(registry);

    // 1. submit → pending (暂停, run_command 需确认)
    let (mut detail, pending) = engine
        .submit_user_message(
            &session.id,
            "跑一下 echo hello",
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
        .expect("submit");
    let pending = expect_permission(pending);
    detail.pending_permission = Some(pending.clone());

    assert_eq!(pending.tool_name, "run_command");
    assert_eq!(pending.tool_call_id, "cmd-1");
    assert!(
        pending.input.contains("echo"),
        "input 应含 echo: {}",
        pending.input
    );

    // pending_permission 已塞进 detail（命令层行为）。
    assert!(detail.pending_permission.is_some());

    // 消息序列：user + assistant(tool_calls)，无 tool 结果消息（未执行）。
    let roles: Vec<&str> = detail.messages.iter().map(|m| m.role.as_str()).collect();
    assert_eq!(roles, vec!["user", "assistant"]);

    // 2. 命令层逻辑：find_pending_tool_name + grant_tool（Approve 路径）
    let tool_name = store
        .find_pending_tool_name(&session.id, &pending.tool_call_id)
        .expect("find ok")
        .expect("应找到 tool_name");
    assert_eq!(tool_name, "run_command");

    let now = now_string();
    store
        .grant_tool(&session.id, &tool_name, &now)
        .expect("grant");

    // 3. engine.resume → 执行 + 续跑 + 最终答案
    let (detail2, pending2) = engine
        .resume(
            &session.id,
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
        .expect("resume");

    assert!(pending2.is_none(), "续跑应完成，pending 应为 None");
    assert!(detail2.pending_permission.is_none());

    // 消息序列：user / assistant(tool_calls) / tool(result) / assistant(final)
    let roles2: Vec<&str> = detail2.messages.iter().map(|m| m.role.as_str()).collect();
    assert_eq!(roles2, vec!["user", "assistant", "tool", "assistant"]);

    let tool_msg = &detail2.messages[2];
    assert_eq!(tool_msg.role, "tool");
    assert_eq!(tool_msg.tool_call_id.as_deref(), Some("cmd-1"));
    assert_eq!(tool_msg.tool_name.as_deref(), Some("run_command"));
    assert!(
        tool_msg.content.contains("退出码"),
        "tool 结果应含命令退出码: {}",
        tool_msg.content
    );

    let final_msg = &detail2.messages[3];
    assert_eq!(final_msg.role, "assistant");
    assert_eq!(final_msg.content, "命令已执行完毕。");
    assert!(final_msg.tool_calls_json.is_none());
}

// ---------------------------------------------------------------------------
// Test B: Deny path
//   submit → pending (run_command paused)
//   → find_pending_tool_name + append_tool_result("用户拒绝了该操作。") + engine.resume
//   → 拒绝结果落库、模型续跑给最终答案
// ---------------------------------------------------------------------------

#[test]
fn deny_path_appends_rejection_and_resumes() {
    let (db, store, registry, _workspace) = setup();
    let session = store
        .create_session("s-deny", "deny", "100", false)
        .expect("session");

    let engine = Engine::new(
        SessionStore::open(db.clone()).unwrap(),
        Arc::new(TwoTurnClient::new("好的，我不执行该命令了。")),
    )
    .with_registry(registry);

    // 1. submit → pending
    let (mut detail, pending) = engine
        .submit_user_message(
            &session.id,
            "跑一下 echo hello",
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
        .expect("submit");
    let pending = expect_permission(pending);
    detail.pending_permission = Some(pending.clone());

    assert_eq!(pending.tool_name, "run_command");

    // 命令序列：仅 user + assistant(tool_calls)，无 tool 消息。
    assert_eq!(detail.messages.len(), 2);

    // 2. 命令层逻辑：find_pending_tool_name + append_tool_result（Deny 路径）
    let tool_name = store
        .find_pending_tool_name(&session.id, &pending.tool_call_id)
        .expect("find ok")
        .expect("tool_name");
    assert_eq!(tool_name, "run_command");

    let now = now_string();
    // 不授权，落拒绝结果（不再 pending）。
    store
        .append_tool_result(
            &new_id("msg"),
            &session.id,
            &pending.tool_call_id,
            &tool_name,
            "用户拒绝了该操作。",
            "done",
            &now,
        )
        .expect("append rejection");

    // 3. engine.resume → 无 pending 工具需执行，直接调模型基于拒绝结果续跑。
    let (detail2, pending2) = engine
        .resume(
            &session.id,
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
        .expect("resume");

    assert!(pending2.is_none(), "拒绝后续跑应无新暂停");

    // 消息序列：user / assistant(tool_calls) / tool(拒绝结果) / assistant(final)
    let roles2: Vec<&str> = detail2.messages.iter().map(|m| m.role.as_str()).collect();
    assert_eq!(roles2, vec!["user", "assistant", "tool", "assistant"]);

    // 拒绝 tool 消息内容应是"用户拒绝了该操作。"
    let rejection_msg = &detail2.messages[2];
    assert_eq!(rejection_msg.role, "tool");
    assert_eq!(rejection_msg.tool_call_id.as_deref(), Some("cmd-1"));
    assert_eq!(rejection_msg.tool_name.as_deref(), Some("run_command"));
    assert_eq!(rejection_msg.content, "用户拒绝了该操作。");

    // 模型续跑给了最终答案（基于拒绝结果改道）。
    let final_msg = &detail2.messages[3];
    assert_eq!(final_msg.role, "assistant");
    assert_eq!(final_msg.content, "好的，我不执行该命令了。");
    assert!(final_msg.tool_calls_json.is_none());
}

// ---------------------------------------------------------------------------
// Test C: find_pending_tool_name helper
//   验证 store.find_pending_tool_name 能正确从末条 assistant tool_calls 找到名字。
// ---------------------------------------------------------------------------

#[test]
fn find_pending_tool_name_returns_correct_name() {
    let (_db, store, _registry, _workspace) = setup();
    store
        .create_session("s-find", "find", "100", false)
        .expect("session");

    // 落一条带 tool_calls 的 assistant 消息。
    let tool_calls_json = serde_json::json!([
        { "id": "call-x", "name": "write_file", "arguments_json": "{}" },
        { "id": "call-y", "name": "read_file",  "arguments_json": "{}" }
    ])
    .to_string();
    store
        .append_assistant_tool_call(&new_id("msg"), "s-find", "", None, &tool_calls_json, "200")
        .expect("append");

    // 查 call-x → write_file
    let name = store
        .find_pending_tool_name("s-find", "call-x")
        .expect("ok")
        .expect("found");
    assert_eq!(name, "write_file");

    // 查 call-y → read_file
    let name2 = store
        .find_pending_tool_name("s-find", "call-y")
        .expect("ok")
        .expect("found");
    assert_eq!(name2, "read_file");

    // 查不存在的 id → None
    let none = store
        .find_pending_tool_name("s-find", "call-z")
        .expect("ok");
    assert!(none.is_none());
}

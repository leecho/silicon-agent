use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use silicon_worker::engine::event::AgentStreamEvent;
use silicon_worker::engine::{Engine, PendingInteraction};
use silicon_worker::provider::client::{
    ModelCallRequest, ModelCallResult, ModelClient, ModelEvent, ProviderCallError,
};
use silicon_worker::session::PendingPermission;
use silicon_worker::session::SessionStore;
use silicon_worker::storage::AppDatabase;
use silicon_worker::tools::command_tool::CommandExecute;
use silicon_worker::tools::ToolRegistry;

/// 两轮 mock：第一轮请求 run_command（风险工具，需确认），第二轮基于工具结果给最终答案。
/// 镜像真实 provider 流式：live ToolCallCreated 的 args 为空，完整 args 在最终 result.events。
struct PermissionClient {
    calls: AtomicUsize,
    program: String,
}

impl ModelClient for PermissionClient {
    fn stream_model_with_events(
        &self,
        _request: ModelCallRequest,
        _cancel: &std::sync::atomic::AtomicBool,
        on_event: &mut dyn FnMut(ModelEvent) -> bool,
    ) -> Result<ModelCallResult, ProviderCallError> {
        let turn = self.calls.fetch_add(1, Ordering::SeqCst);
        if turn == 0 {
            let args = serde_json::json!({ "program": self.program, "args": ["hello"] });
            on_event(ModelEvent::ToolCallCreated {
                id: "call-1".into(),
                name: "run_command".into(),
                arguments_json: String::new(),
            });
            Ok(ModelCallResult {
                events: vec![ModelEvent::ToolCallCreated {
                    id: "call-1".into(),
                    name: "run_command".into(),
                    arguments_json: args.to_string(),
                }],
                usage: None,
                finish_reason: Some("tool_calls".into()),
            })
        } else {
            on_event(ModelEvent::Delta {
                text: "命令已执行。".into(),
            });
            Ok(ModelCallResult {
                events: vec![ModelEvent::AssistantMessageCompleted {
                    content: "命令已执行。".into(),
                }],
                usage: None,
                finish_reason: Some("stop".into()),
            })
        }
    }
}

/// 从泛化后的 `PendingInteraction` 取出权限暂停；其余形态视为断言失败。
fn expect_permission(pending: Option<PendingInteraction>) -> PendingPermission {
    match pending {
        Some(PendingInteraction::Permission(p)) => p,
        _ => panic!("应为权限暂停"),
    }
}

fn temp_dir() -> std::path::PathBuf {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "siw-perm_{}_{}_{}",
        std::process::id(),
        seq,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ))
}

/// 未授权提交 → 暂停返回 PendingPermission(run_command)、命令未执行（无 tool 结果消息）、emit permission_required。
/// grant_tool + resume → 命令执行、tool 结果落库、第二轮给最终答案、返回 None。
#[test]
fn run_command_pauses_until_granted_then_resumes() {
    let base = temp_dir();
    let workspace = base.join("workspace");
    std::fs::create_dir_all(&workspace).expect("workspace");
    let db = Arc::new(AppDatabase::open(base.join("app.sqlite3")).expect("db"));

    let store = SessionStore::open(db.clone()).expect("store");
    let session = store
        .create_session("s1", "perm", "100", false)
        .expect("session");

    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(CommandExecute {
        workspace: workspace.clone(),
    }));

    let events: Arc<Mutex<Vec<AgentStreamEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let events_for_emitter = events.clone();
    let engine = Engine::new(
        SessionStore::open(db.clone()).unwrap(),
        Arc::new(PermissionClient {
            calls: AtomicUsize::new(0),
            program: "echo".into(),
        }),
    )
    .with_registry(registry)
    .with_emitter(Arc::new(move |e| {
        events_for_emitter.lock().unwrap().push(e)
    }));

    // 1. 未授权提交 → 暂停。
    let (detail, pending) = engine
        .submit_user_message(
            &session.id,
            "跑一下 echo hello",
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
        .expect("submit");

    let pending = expect_permission(pending);
    assert_eq!(pending.tool_name, "run_command");
    assert_eq!(pending.tool_call_id, "call-1");
    assert!(
        pending.input.contains("echo"),
        "pending.input 应含命令参数，实际: {}",
        pending.input
    );

    // 命令未执行：消息只有 user + assistant(tool_calls)，无 tool 结果消息。
    let roles: Vec<&str> = detail.messages.iter().map(|m| m.role.as_str()).collect();
    assert_eq!(roles, vec!["user", "assistant"]);
    assert!(
        detail.messages.iter().all(|m| m.role != "tool"),
        "暂停时不应有 tool 结果消息"
    );

    // emit 了 permission_required。
    {
        let evts = events.lock().unwrap();
        let req = evts
            .iter()
            .find(|e| e.kind == "permission_required")
            .expect("应 emit permission_required");
        assert_eq!(req.tool_name.as_deref(), Some("run_command"));
        assert_eq!(req.tool_call_id.as_deref(), Some("call-1"));
        // tool_call 事件也应在 permission_required 前 emit（让前端看到发起）。
        assert!(
            evts.iter()
                .any(|e| e.kind == "tool_call" && e.tool_call_id.as_deref() == Some("call-1")),
            "应 emit tool_call 事件"
        );
        // 未授权时不应执行 → 无 tool_result 事件。
        assert!(
            !evts.iter().any(|e| e.kind == "tool_result"),
            "暂停时不应 emit tool_result"
        );
    }

    // 2. 会话级授权 + resume → 执行 + 续跑 + 最终答案。
    store
        .grant_tool(&session.id, "run_command", "200")
        .expect("grant");

    let (detail2, pending2) = engine
        .resume(
            &session.id,
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
        .expect("resume");
    assert!(pending2.is_none(), "续跑应完成，不再暂停");

    // 命令执行：消息序列 user / assistant(tool_calls) / tool(result) / assistant(final)。
    let roles2: Vec<&str> = detail2.messages.iter().map(|m| m.role.as_str()).collect();
    assert_eq!(roles2, vec!["user", "assistant", "tool", "assistant"]);

    let tool_msg = &detail2.messages[2];
    assert_eq!(tool_msg.tool_call_id.as_deref(), Some("call-1"));
    assert_eq!(tool_msg.tool_name.as_deref(), Some("run_command"));
    assert!(
        tool_msg.content.contains("退出码"),
        "tool 结果应含命令退出码，实际: {}",
        tool_msg.content
    );

    let final_msg = &detail2.messages[3];
    assert_eq!(final_msg.content, "命令已执行。");
    assert!(final_msg.tool_calls_json.is_none());

    // 续跑期间 emit 了 tool_result + message_completed。
    let evts = events.lock().unwrap();
    assert!(
        evts.iter()
            .any(|e| e.kind == "tool_result" && e.tool_call_id.as_deref() == Some("call-1")),
        "续跑应 emit tool_result"
    );
    assert!(
        evts.iter().any(|e| e.kind == "message_completed"),
        "应 emit message_completed"
    );
}

/// 只读工具（mock 请求 read_file）：免确认，直接执行、不暂停。
struct ReadOnlyClient {
    calls: AtomicUsize,
}

impl ModelClient for ReadOnlyClient {
    fn stream_model_with_events(
        &self,
        _request: ModelCallRequest,
        _cancel: &std::sync::atomic::AtomicBool,
        on_event: &mut dyn FnMut(ModelEvent) -> bool,
    ) -> Result<ModelCallResult, ProviderCallError> {
        let turn = self.calls.fetch_add(1, Ordering::SeqCst);
        if turn == 0 {
            let args = serde_json::json!({ "path": "note.txt" });
            on_event(ModelEvent::ToolCallCreated {
                id: "call-r".into(),
                name: "read_file".into(),
                arguments_json: String::new(),
            });
            Ok(ModelCallResult {
                events: vec![ModelEvent::ToolCallCreated {
                    id: "call-r".into(),
                    name: "read_file".into(),
                    arguments_json: args.to_string(),
                }],
                usage: None,
                finish_reason: Some("tool_calls".into()),
            })
        } else {
            on_event(ModelEvent::Delta {
                text: "已读取。".into(),
            });
            Ok(ModelCallResult {
                events: vec![ModelEvent::AssistantMessageCompleted {
                    content: "已读取。".into(),
                }],
                usage: None,
                finish_reason: Some("stop".into()),
            })
        }
    }
}

#[test]
fn read_only_tool_runs_without_pause() {
    use silicon_worker::tools::fs_tools::ReadFile;

    let base = temp_dir();
    let workspace = base.join("workspace");
    std::fs::create_dir_all(&workspace).expect("workspace");
    std::fs::write(workspace.join("note.txt"), "hi there").expect("seed file");
    let db = Arc::new(AppDatabase::open(base.join("app.sqlite3")).expect("db"));

    let store = SessionStore::open(db.clone()).expect("store");
    let session = store
        .create_session("s2", "ro", "100", false)
        .expect("session");

    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(ReadFile {
        workspace: workspace.clone(),
    }));

    let engine = Engine::new(
        SessionStore::open(db.clone()).unwrap(),
        Arc::new(ReadOnlyClient {
            calls: AtomicUsize::new(0),
        }),
    )
    .with_registry(registry);

    let (detail, pending) = engine
        .submit_user_message(
            &session.id,
            "读一下 note.txt",
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
        .expect("submit");

    assert!(pending.is_none(), "只读工具不应暂停");
    let roles: Vec<&str> = detail.messages.iter().map(|m| m.role.as_str()).collect();
    assert_eq!(roles, vec!["user", "assistant", "tool", "assistant"]);
    let tool_msg = &detail.messages[2];
    assert_eq!(tool_msg.tool_name.as_deref(), Some("read_file"));
    assert!(
        tool_msg.content.contains("hi there"),
        "read_file 结果应含文件内容，实际: {}",
        tool_msg.content
    );
}

/// 两轮 mock：第一轮请求 write_file（低风险写工具），第二轮基于结果给最终答案。
struct WriteFileClient {
    calls: AtomicUsize,
}

impl ModelClient for WriteFileClient {
    fn stream_model_with_events(
        &self,
        _request: ModelCallRequest,
        _cancel: &std::sync::atomic::AtomicBool,
        on_event: &mut dyn FnMut(ModelEvent) -> bool,
    ) -> Result<ModelCallResult, ProviderCallError> {
        let turn = self.calls.fetch_add(1, Ordering::SeqCst);
        if turn == 0 {
            let args = serde_json::json!({ "path": "out.txt", "content": "hello world" });
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
                text: "已写入。".into(),
            });
            Ok(ModelCallResult {
                events: vec![ModelEvent::AssistantMessageCompleted {
                    content: "已写入。".into(),
                }],
                usage: None,
                finish_reason: Some("stop".into()),
            })
        }
    }
}

/// auto 模式：低风险 write_file 自动放行、不暂停、直接执行落结果。
#[test]
fn auto_mode_runs_low_risk_without_pause() {
    use silicon_worker::tools::fs_tools::WriteFile;

    let base = temp_dir();
    let workspace = base.join("workspace");
    std::fs::create_dir_all(&workspace).expect("workspace");
    let db = Arc::new(AppDatabase::open(base.join("app.sqlite3")).expect("db"));

    let store = SessionStore::open(db.clone()).expect("store");
    let session = store
        .create_session("s-auto-low", "auto-low", "100", false)
        .expect("session");
    store
        .set_session_permission_mode(&session.id, Some("auto"), "101")
        .expect("set mode");

    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(WriteFile {
        workspace: workspace.clone(),
    }));

    let engine = Engine::new(
        SessionStore::open(db.clone()).unwrap(),
        Arc::new(WriteFileClient {
            calls: AtomicUsize::new(0),
        }),
    )
    .with_registry(registry);

    let (detail, pending) = engine
        .submit_user_message(
            &session.id,
            "写一下 out.txt",
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
        .expect("submit");

    assert!(pending.is_none(), "auto 模式低风险工具不应暂停");
    let roles: Vec<&str> = detail.messages.iter().map(|m| m.role.as_str()).collect();
    assert_eq!(roles, vec!["user", "assistant", "tool", "assistant"]);
    let tool_msg = &detail.messages[2];
    assert_eq!(tool_msg.tool_name.as_deref(), Some("write_file"));
    assert!(
        workspace.join("out.txt").exists(),
        "write_file 应实际写入文件"
    );
}

/// auto 模式：高风险 run_command 仍需确认 → 暂停。
#[test]
fn auto_mode_pauses_high_risk_command() {
    let base = temp_dir();
    let workspace = base.join("workspace");
    std::fs::create_dir_all(&workspace).expect("workspace");
    let db = Arc::new(AppDatabase::open(base.join("app.sqlite3")).expect("db"));

    let store = SessionStore::open(db.clone()).expect("store");
    let session = store
        .create_session("s-auto-high", "auto-high", "100", false)
        .expect("session");
    store
        .set_session_permission_mode(&session.id, Some("auto"), "101")
        .expect("set mode");

    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(CommandExecute {
        workspace: workspace.clone(),
    }));

    let engine = Engine::new(
        SessionStore::open(db.clone()).unwrap(),
        Arc::new(PermissionClient {
            calls: AtomicUsize::new(0),
            program: "echo".into(),
        }),
    )
    .with_registry(registry);

    let (detail, pending) = engine
        .submit_user_message(
            &session.id,
            "跑一下 echo hello",
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
        .expect("submit");

    let pending = expect_permission(pending);
    assert_eq!(pending.tool_name, "run_command");
    let roles: Vec<&str> = detail.messages.iter().map(|m| m.role.as_str()).collect();
    assert_eq!(roles, vec!["user", "assistant"]);
    assert!(
        detail.messages.iter().all(|m| m.role != "tool"),
        "auto 模式高风险命令暂停时不应执行"
    );
}

/// full 模式：所有工具放行（含高风险 run_command）→ 不暂停、直接执行。
#[test]
fn full_mode_runs_everything_without_pause() {
    let base = temp_dir();
    let workspace = base.join("workspace");
    std::fs::create_dir_all(&workspace).expect("workspace");
    let db = Arc::new(AppDatabase::open(base.join("app.sqlite3")).expect("db"));

    let store = SessionStore::open(db.clone()).expect("store");
    let session = store
        .create_session("s-full", "full", "100", false)
        .expect("session");
    store
        .set_session_permission_mode(&session.id, Some("full"), "101")
        .expect("set mode");

    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(CommandExecute {
        workspace: workspace.clone(),
    }));

    let engine = Engine::new(
        SessionStore::open(db.clone()).unwrap(),
        Arc::new(PermissionClient {
            calls: AtomicUsize::new(0),
            program: "echo".into(),
        }),
    )
    .with_registry(registry);

    let (detail, pending) = engine
        .submit_user_message(
            &session.id,
            "跑一下 echo hello",
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
        .expect("submit");

    assert!(pending.is_none(), "full 模式不应暂停");
    let roles: Vec<&str> = detail.messages.iter().map(|m| m.role.as_str()).collect();
    assert_eq!(roles, vec!["user", "assistant", "tool", "assistant"]);
    let tool_msg = &detail.messages[2];
    assert_eq!(tool_msg.tool_name.as_deref(), Some("run_command"));
    assert!(
        tool_msg.content.contains("退出码"),
        "run_command 结果应含命令退出码，实际: {}",
        tool_msg.content
    );
}

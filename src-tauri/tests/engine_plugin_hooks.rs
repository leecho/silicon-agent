//! T66：plugin hooks 接入引擎工具生命周期。
//! - PreToolUse 返回 `{"decision":"block"}` → 工具不执行、落 `blocked` 结果。
//! - PostToolUse 在工具执行后触发（用写文件副作用验证被调用）。

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use silicon_worker::engine::event::AgentStreamEvent;
use silicon_worker::engine::Engine;
use silicon_worker::hook::{HookRule, HookService};
use silicon_worker::provider::client::{
    ModelCallRequest, ModelCallResult, ModelClient, ModelEvent, ProviderCallError,
};
use silicon_worker::provider::message::ModelMessageRole;
use silicon_worker::session::SessionStore;
use silicon_worker::storage::AppDatabase;
use silicon_worker::tools::registry::ToolRegistry;
use silicon_worker::tools::Tool;

/// 无副作用工具（Safe）：执行计数，便于断言「未执行」。
struct NoopTool {
    name: String,
    executed: Arc<AtomicUsize>,
}

impl Tool for NoopTool {
    fn name(&self) -> &str {
        &self.name
    }
    fn description(&self) -> &str {
        "noop"
    }
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({"type":"object","properties":{}})
    }
    fn execute(&self, _: &serde_json::Value) -> Result<String, String> {
        self.executed.fetch_add(1, Ordering::SeqCst);
        Ok("ok".into())
    }
}

/// 首次调用 emit 一个工具调用；其后（已有 tool 结果）以普通文本收口。
struct ToolThenStopClient {
    tool: String,
}

impl ModelClient for ToolThenStopClient {
    fn stream_model_with_events(
        &self,
        request: ModelCallRequest,
        _cancel: &std::sync::atomic::AtomicBool,
        _on_event: &mut dyn FnMut(ModelEvent) -> bool,
    ) -> Result<ModelCallResult, ProviderCallError> {
        // 历史里已有 tool 角色消息（含 blocked 结果）→ 收口；否则发起工具调用。
        let has_tool_result = request
            .messages
            .iter()
            .any(|m| m.role == ModelMessageRole::Tool);
        if has_tool_result {
            return Ok(ModelCallResult {
                events: vec![ModelEvent::AssistantMessageCompleted {
                    content: "完成".into(),
                }],
                usage: None,
                finish_reason: Some("stop".into()),
            });
        }
        Ok(ModelCallResult {
            events: vec![ModelEvent::ToolCallCreated {
                id: "call-1".into(),
                name: self.tool.clone(),
                arguments_json: "{}".into(),
            }],
            usage: None,
            finish_reason: Some("tool_calls".into()),
        })
    }
}

fn temp_db(tag: &str) -> (Arc<AppDatabase>, std::path::PathBuf) {
    static C: AtomicUsize = AtomicUsize::new(0);
    let seq = C.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "siw-hooks-{tag}_{}_{}_{}",
        std::process::id(),
        seq,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    (
        Arc::new(AppDatabase::open(dir.join("app.sqlite3")).expect("db")),
        dir,
    )
}

fn registry_with(tool: NoopTool) -> ToolRegistry {
    let mut r = ToolRegistry::new();
    r.register(Arc::new(tool));
    r
}

#[test]
fn pre_tool_use_block_prevents_execution() {
    let (db, dir) = temp_db("block");
    let store = SessionStore::open(db.clone()).unwrap();
    let s = store.create_session("s1", "t", "100", false).unwrap();

    let executed = Arc::new(AtomicUsize::new(0));
    let hooks = Arc::new(HookService::new());
    hooks.set_plugin(
        "p1",
        vec![HookRule {
            event: "PreToolUse".into(),
            matcher: Some("noop_tool".into()),
            command: r#"echo '{"decision":"block","reason":"被插件拦截"}'"#.into(),
            plugin_root: dir.clone(),
            plugin_data: dir.clone(),
        }],
    );

    let events: Arc<Mutex<Vec<AgentStreamEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();
    let engine = Engine::new(
        SessionStore::open(db.clone()).unwrap(),
        Arc::new(ToolThenStopClient {
            tool: "noop_tool".into(),
        }),
    )
    .with_workspace(dir.to_string_lossy().into_owned())
    .with_registry(registry_with(NoopTool {
        name: "noop_tool".into(),
        executed: executed.clone(),
    }))
    .with_hooks(hooks)
    .with_emitter(Arc::new(move |e| sink.lock().unwrap().push(e)));

    engine
        .submit_user_message(
            &s.id,
            "go",
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
        .expect("submit");

    assert_eq!(
        executed.load(Ordering::SeqCst),
        0,
        "被 block 的工具不应执行"
    );
    let evts = events.lock().unwrap();
    let blocked = evts
        .iter()
        .any(|e| e.kind == "tool_result" && e.status.as_deref() == Some("blocked"));
    assert!(blocked, "应 emit 一条 blocked tool_result");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn post_tool_use_runs_after_execution() {
    let (db, dir) = temp_db("post");
    let store = SessionStore::open(db.clone()).unwrap();
    let s = store.create_session("s1", "t", "100", false).unwrap();

    let executed = Arc::new(AtomicUsize::new(0));
    let hooks = Arc::new(HookService::new());
    // PostToolUse hook 在 cwd 写 marker 文件，验证被调用。
    hooks.set_plugin(
        "p1",
        vec![HookRule {
            event: "PostToolUse".into(),
            matcher: None,
            command: "touch post-marker".into(),
            plugin_root: dir.clone(),
            plugin_data: dir.clone(),
        }],
    );

    let engine = Engine::new(
        SessionStore::open(db.clone()).unwrap(),
        Arc::new(ToolThenStopClient {
            tool: "noop_tool".into(),
        }),
    )
    .with_workspace(dir.to_string_lossy().into_owned())
    .with_registry(registry_with(NoopTool {
        name: "noop_tool".into(),
        executed: executed.clone(),
    }))
    .with_hooks(hooks)
    .with_emitter(Arc::new(|_| {}));

    engine
        .submit_user_message(
            &s.id,
            "go",
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
        .expect("submit");

    assert_eq!(executed.load(Ordering::SeqCst), 1, "工具应正常执行");
    assert!(
        dir.join("post-marker").exists(),
        "PostToolUse hook 应在工具执行后触发"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn no_hooks_is_no_op() {
    let (db, dir) = temp_db("noop");
    let store = SessionStore::open(db.clone()).unwrap();
    let s = store.create_session("s1", "t", "100", false).unwrap();

    let executed = Arc::new(AtomicUsize::new(0));
    // 不注入 hooks → 全程短路，工具照常执行。
    let engine = Engine::new(
        SessionStore::open(db.clone()).unwrap(),
        Arc::new(ToolThenStopClient {
            tool: "noop_tool".into(),
        }),
    )
    .with_workspace(dir.to_string_lossy().into_owned())
    .with_registry(registry_with(NoopTool {
        name: "noop_tool".into(),
        executed: executed.clone(),
    }))
    .with_emitter(Arc::new(|_| {}));

    engine
        .submit_user_message(
            &s.id,
            "go",
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
        .expect("submit");

    assert_eq!(executed.load(Ordering::SeqCst), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

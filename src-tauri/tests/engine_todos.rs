// Tests for Slice Todos Task 1: update_todos 引擎拦截 + 持久化 + 事件。
//
// ① 第一轮请求 update_todos[2 项含 1 in_progress]、第二轮最终答案：
//    submit → todos 落库为这两项、emit todos_updated(todos 含步骤1)、tool 结果含「已更新待办」、
//    第二轮最终答案、pending None（不暂停）。
// ② 第一轮请求 update_todos[2 项 in_progress] → 结果含「同一时刻至多一项」、未覆写既有 todos。

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use silicon_worker::engine::event::AgentStreamEvent;
use silicon_worker::engine::Engine;
use silicon_worker::provider::client::{
    ModelCallRequest, ModelCallResult, ModelClient, ModelEvent, ProviderCallError,
};
use silicon_worker::session::{SessionStore, TodoItem};
use silicon_worker::storage::AppDatabase;
use silicon_worker::tools::update_todos::UpdateTodos;
use silicon_worker::tools::ToolRegistry;

fn temp_dir() -> std::path::PathBuf {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "siw-todos_{}_{}_{}",
        std::process::id(),
        seq,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ))
}

/// 两轮 mock：第一轮请求 update_todos{todos}，第二轮最终答案。
/// 镜像真实 provider 流式：live ToolCallCreated 的 args 为空，完整 args 在最终 result.events。
struct UpdateTodosClient {
    calls: AtomicUsize,
    todos_arg: serde_json::Value,
}

impl ModelClient for UpdateTodosClient {
    fn stream_model_with_events(
        &self,
        _request: ModelCallRequest,
        _cancel: &std::sync::atomic::AtomicBool,
        on_event: &mut dyn FnMut(ModelEvent) -> bool,
    ) -> Result<ModelCallResult, ProviderCallError> {
        let turn = self.calls.fetch_add(1, Ordering::SeqCst);
        if turn == 0 {
            let args = serde_json::json!({ "todos": self.todos_arg });
            on_event(ModelEvent::ToolCallCreated {
                id: "call-1".into(),
                name: "update_todos".into(),
                arguments_json: String::new(),
            });
            Ok(ModelCallResult {
                events: vec![ModelEvent::ToolCallCreated {
                    id: "call-1".into(),
                    name: "update_todos".into(),
                    arguments_json: args.to_string(),
                }],
                usage: None,
                finish_reason: Some("tool_calls".into()),
            })
        } else {
            on_event(ModelEvent::Delta {
                text: "已完成。".into(),
            });
            Ok(ModelCallResult {
                events: vec![ModelEvent::AssistantMessageCompleted {
                    content: "已完成。".into(),
                }],
                usage: None,
                finish_reason: Some("stop".into()),
            })
        }
    }
}

fn run(
    todos_arg: serde_json::Value,
) -> (
    silicon_worker::session::Session,
    Option<silicon_worker::engine::PendingInteraction>,
    Vec<AgentStreamEvent>,
    SessionStore,
) {
    let base = temp_dir();
    let db = Arc::new(AppDatabase::open(base.join("app.sqlite3")).expect("db"));

    let store = SessionStore::open(db.clone()).expect("store");
    let session = store
        .create_session("s1", "todos", "100", false)
        .expect("session");

    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(UpdateTodos));

    let events: Arc<Mutex<Vec<AgentStreamEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let events_for_emitter = events.clone();
    let engine = Engine::new(
        SessionStore::open(db.clone()).unwrap(),
        Arc::new(UpdateTodosClient {
            calls: AtomicUsize::new(0),
            todos_arg,
        }),
    )
    .with_registry(registry)
    .with_emitter(Arc::new(move |e| {
        events_for_emitter.lock().unwrap().push(e)
    }));

    let (detail, pending) = engine
        .submit_user_message(&session.id, "做多步任务", Arc::new(AtomicBool::new(false)))
        .expect("submit");

    let evts = events.lock().unwrap().clone();
    (detail, pending, evts, store)
}

/// ① 合法 update_todos → todos 落库、emit todos_updated、tool 结果含「已更新待办」、最终答案、不暂停。
#[test]
fn update_todos_is_intercepted_and_persisted() {
    let todos_arg = serde_json::json!([
        { "content": "步骤1", "status": "in_progress" },
        { "content": "步骤2", "status": "pending" },
    ]);
    let (detail, pending, evts, store) = run(todos_arg);

    // 未暂停。
    assert!(pending.is_none(), "update_todos 即时拦截，不应暂停");

    // todos 落库为这两项（id 1 基重排）。
    let stored = store.get_session_todos("s1").expect("get todos");
    assert_eq!(
        stored,
        vec![
            TodoItem {
                id: 1,
                content: "步骤1".into(),
                status: "in_progress".into()
            },
            TodoItem {
                id: 2,
                content: "步骤2".into(),
                status: "pending".into()
            },
        ]
    );
    // detail.todos 同步。
    assert_eq!(detail.todos, stored);

    // emit 了 todos_updated（带整组 todos，含步骤1 且为 in_progress）。
    let upd = evts
        .iter()
        .find(|e| e.kind == "todos_updated")
        .expect("应 emit todos_updated");
    let evt_todos = upd.todos.as_ref().expect("todos_updated 应带 todos");
    assert_eq!(evt_todos.len(), 2);
    assert_eq!(evt_todos[0].content, "步骤1");
    assert_eq!(evt_todos[0].status, "in_progress");

    // tool_result 含「已更新待办」汇总。
    let res = evts
        .iter()
        .find(|e| e.kind == "tool_result" && e.tool_name.as_deref() == Some("update_todos"))
        .expect("应 emit update_todos 的 tool_result");
    assert!(
        res.text.as_deref().unwrap_or("").contains("已更新待办"),
        "tool 结果应含「已更新待办」汇总，实际：{:?}",
        res.text
    );

    // 第二轮最终答案。
    let final_msg = detail.messages.last().expect("有消息");
    assert_eq!(final_msg.role, "assistant");
    assert_eq!(final_msg.content, "已完成。");
}

/// ② >1 in_progress → 结果含「同一时刻至多一项」、未覆写既有 todos。
#[test]
fn more_than_one_in_progress_is_rejected_without_overwrite() {
    let todos_arg = serde_json::json!([
        { "content": "步骤1", "status": "in_progress" },
        { "content": "步骤2", "status": "in_progress" },
    ]);
    let (detail, pending, evts, store) = run(todos_arg);

    assert!(pending.is_none(), "校验失败也是即时 continue，不暂停");

    // 未覆写：会话从未写过 todos → 仍为空。
    let stored = store.get_session_todos("s1").expect("get todos");
    assert!(stored.is_empty(), "校验失败不应覆写 todos");
    assert!(detail.todos.is_empty());

    // tool_result 含「同一时刻至多一项」错误。
    let res = evts
        .iter()
        .find(|e| e.kind == "tool_result" && e.tool_name.as_deref() == Some("update_todos"))
        .expect("应 emit update_todos 的 tool_result");
    assert!(
        res.text
            .as_deref()
            .unwrap_or("")
            .contains("同一时刻至多一项"),
        "结果应含「同一时刻至多一项」，实际：{:?}",
        res.text
    );

    // 未 emit todos_updated（未持久化）。
    assert!(
        !evts.iter().any(|e| e.kind == "todos_updated"),
        "校验失败不应 emit todos_updated"
    );
}

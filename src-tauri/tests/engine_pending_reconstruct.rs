//! reload 后从持久化消息重建 pending 交互（PermissionCard / AskCard 恢复）。
//!
//! 复现 bug：会话因风险工具（如 write_file）暂停等待授权时，pending 是运行期临时态、不持久化；
//! 重开 app / 刷新会话后 get_session_detail 必须能从"悬空 tool_call（无 tool 结果）"重建 pending，
//! 否则权限卡不再出现、用户卡住。

use std::sync::Arc;

use silicon_agent::engine::{Engine, PendingInteraction};
use silicon_agent::provider::client::ModelClient;
use silicon_agent::provider::message::ModelToolCall;
use silicon_agent::session::SessionStore;
use silicon_agent::storage::AppDatabase;
use silicon_agent::tools::{RiskLevel, Tool, ToolRegistry};

/// 不会被调用的占位 client：pending_interaction 只读持久化消息，不触模型。
struct NoClient;
impl ModelClient for NoClient {}

/// 需要确认的测试工具，名为 write_file（模拟风险工具）。
struct ConfirmTool;
impl Tool for ConfirmTool {
    fn name(&self) -> &'static str {
        "write_file"
    }
    fn description(&self) -> &'static str {
        "test confirm tool"
    }
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({"type":"object"})
    }
    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Low
    }
    fn execute(&self, _args: &serde_json::Value) -> Result<String, String> {
        Ok("ok".into())
    }
}

fn temp_db() -> Arc<AppDatabase> {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "siw-pending_{}_{}_{}",
        std::process::id(),
        seq,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    Arc::new(AppDatabase::open(dir.join("app.sqlite3")).expect("db"))
}

fn engine_with_registry(db: Arc<AppDatabase>) -> Engine {
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(ConfirmTool));
    Engine::new(SessionStore::open(db).unwrap(), Arc::new(NoClient)).with_registry(registry)
}

fn tool_calls_json(calls: &[ModelToolCall]) -> String {
    serde_json::to_string(calls).expect("serialize tool_calls")
}

/// 悬空的风险工具调用（无 tool 结果）→ 重建为 PendingPermission。
#[test]
fn reconstructs_permission_from_dangling_risky_tool_call() {
    let db = temp_db();
    let store = SessionStore::open(db.clone()).expect("store");
    let s = store
        .create_session("s1", "t", "100", false)
        .expect("session");
    store
        .append_message("u1", &s.id, "user", "写个报告", None, "101")
        .expect("user msg");
    let calls = vec![ModelToolCall {
        id: "call-1".into(),
        name: "write_file".into(),
        arguments_json: "{\"path\":\"r.md\"}".into(),
    }];
    store
        .append_assistant_tool_call("a1", &s.id, "", None, &tool_calls_json(&calls), "102")
        .expect("assistant tool_call");

    let engine = engine_with_registry(db);
    let pending = engine.pending_interaction(&s.id).expect("pending");
    match pending {
        Some(PendingInteraction::Permission(p)) => {
            assert_eq!(p.tool_call_id, "call-1");
            assert_eq!(p.tool_name, "write_file");
            assert_eq!(p.input, "{\"path\":\"r.md\"}");
        }
        other => panic!("expected Permission, got {other:?}"),
    }
}

/// 悬空的 ask_user 调用 → 重建为 PendingAsk（含 question/options）。
#[test]
fn reconstructs_ask_from_dangling_ask_user_call() {
    let db = temp_db();
    let store = SessionStore::open(db.clone()).expect("store");
    let s = store
        .create_session("s1", "t", "100", false)
        .expect("session");
    store
        .append_message("u1", &s.id, "user", "hi", None, "101")
        .expect("user msg");
    let calls = vec![ModelToolCall {
        id: "ask-1".into(),
        name: "ask_user".into(),
        arguments_json: "{\"questions\":[{\"question\":\"选哪个?\",\"options\":[\"A\",\"B\"]}]}"
            .into(),
    }];
    store
        .append_assistant_tool_call("a1", &s.id, "", None, &tool_calls_json(&calls), "102")
        .expect("assistant tool_call");

    let engine = engine_with_registry(db);
    match engine.pending_interaction(&s.id).expect("pending") {
        Some(PendingInteraction::Ask(a)) => {
            assert_eq!(a.tool_call_id, "ask-1");
            assert_eq!(a.questions[0].question, "选哪个?");
            assert_eq!(
                a.questions[0].options,
                vec!["A".to_string(), "B".to_string()]
            );
        }
        other => panic!("expected Ask, got {other:?}"),
    }
}

/// 风险工具已授权 → 不再重建 pending（应被续跑执行，不是等用户态）。
#[test]
fn granted_risky_tool_yields_no_pending() {
    let db = temp_db();
    let store = SessionStore::open(db.clone()).expect("store");
    let s = store
        .create_session("s1", "t", "100", false)
        .expect("session");
    store
        .append_message("u1", &s.id, "user", "写个报告", None, "101")
        .expect("user msg");
    let calls = vec![ModelToolCall {
        id: "call-1".into(),
        name: "write_file".into(),
        arguments_json: "{}".into(),
    }];
    store
        .append_assistant_tool_call("a1", &s.id, "", None, &tool_calls_json(&calls), "102")
        .expect("assistant tool_call");
    store.grant_tool(&s.id, "write_file", "103").expect("grant");

    let engine = engine_with_registry(db);
    assert!(engine
        .pending_interaction(&s.id)
        .expect("pending")
        .is_none());
}

/// tool_call 已有结果（已执行）→ 无 pending。
#[test]
fn executed_tool_call_yields_no_pending() {
    let db = temp_db();
    let store = SessionStore::open(db.clone()).expect("store");
    let s = store
        .create_session("s1", "t", "100", false)
        .expect("session");
    store
        .append_message("u1", &s.id, "user", "写个报告", None, "101")
        .expect("user msg");
    let calls = vec![ModelToolCall {
        id: "call-1".into(),
        name: "write_file".into(),
        arguments_json: "{}".into(),
    }];
    store
        .append_assistant_tool_call("a1", &s.id, "", None, &tool_calls_json(&calls), "102")
        .expect("assistant tool_call");
    store
        .append_tool_result("t1", &s.id, "call-1", "write_file", "已写入", "done", "103")
        .expect("tool result");

    let engine = engine_with_registry(db);
    assert!(engine
        .pending_interaction(&s.id)
        .expect("pending")
        .is_none());
}

//! 悬空 tool_call 自愈：历史里若存在「assistant 发起 tool_call 但无对应 tool 结果」的悬空调用
//! （多因上一次运行被中断/重开），续跑时构造给 provider 的消息序列必须为其补占位 tool 结果，
//! 否则 OpenAI-compatible API 会以 400 拒绝（每个 assistant tool_call 必须有 tool 响应），
//! 导致后续每次「继续」都静默失败、会话再也无法续跑。

use std::sync::{Arc, Mutex};

use silicon_worker::engine::Engine;
use silicon_worker::provider::client::{
    ModelCallRequest, ModelCallResult, ModelClient, ModelEvent, ProviderCallError,
};
use silicon_worker::provider::message::{ModelMessage, ModelMessageRole, ModelToolCall};
use silicon_worker::session::SessionStore;
use silicon_worker::storage::AppDatabase;

/// 捕获首次模型调用时收到的消息序列，便于断言其合法性；随后以普通文本收口结束循环。
struct CapturingClient {
    captured: Arc<Mutex<Vec<ModelMessage>>>,
}

impl ModelClient for CapturingClient {
    fn stream_model_with_events(
        &self,
        request: ModelCallRequest,
        _cancel: &std::sync::atomic::AtomicBool,
        _on_event: &mut dyn FnMut(ModelEvent) -> bool,
    ) -> Result<ModelCallResult, ProviderCallError> {
        {
            let mut c = self.captured.lock().unwrap();
            if c.is_empty() {
                *c = request.messages.clone();
            }
        }
        Ok(ModelCallResult {
            events: vec![ModelEvent::AssistantMessageCompleted {
                content: "好的".into(),
            }],
            usage: None,
            finish_reason: Some("stop".into()),
        })
    }
}

fn temp_db() -> Arc<AppDatabase> {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "siw-orphan_{}_{}_{}",
        std::process::id(),
        seq,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    Arc::new(AppDatabase::open(dir.join("app.sqlite3")).expect("db"))
}

fn tc_json(id: &str, name: &str) -> String {
    serde_json::to_string(&vec![ModelToolCall {
        id: id.into(),
        name: name.into(),
        arguments_json: "{}".into(),
    }])
    .unwrap()
}

#[test]
fn resume_synthesizes_tool_result_for_orphan_tool_call() {
    let db = temp_db();
    let store = SessionStore::open(db.clone()).expect("store");
    let s = store
        .create_session("s1", "t", "100", false)
        .expect("session");

    // 复现 616/617/618：先一个悬空 write_file（无结果），再一个已完成 write_file（有结果）。
    store
        .append_message("u1", &s.id, "user", "做任务", None, "101")
        .expect("user");
    store
        .append_assistant_tool_call(
            "a-orphan",
            &s.id,
            "",
            None,
            &tc_json("orphan-1", "write_file"),
            "102",
        )
        .expect("orphan assistant");
    store
        .append_assistant_tool_call(
            "a-real",
            &s.id,
            "",
            None,
            &tc_json("real-1", "write_file"),
            "103",
        )
        .expect("real assistant");
    store
        .append_tool_result(
            "t-real",
            &s.id,
            "real-1",
            "write_file",
            "已写入",
            "done",
            "104",
        )
        .expect("real result");

    let captured: Arc<Mutex<Vec<ModelMessage>>> = Arc::new(Mutex::new(Vec::new()));
    let engine = Engine::new(
        SessionStore::open(db.clone()).unwrap(),
        Arc::new(CapturingClient {
            captured: captured.clone(),
        }),
    );

    // 续跑（等价于用户发「继续」后引擎重入 run_loop）。
    engine
        .resume(&s.id, Arc::new(std::sync::atomic::AtomicBool::new(false)))
        .expect("resume should not error");

    let msgs = captured.lock().unwrap();
    assert!(!msgs.is_empty(), "应捕获到发给 provider 的消息");

    // 校验序列合法：每个带 tool_calls 的 assistant 之后，紧跟覆盖其全部 tool_call_id 的 tool 消息。
    for (i, m) in msgs.iter().enumerate() {
        if let Some(calls) = &m.tool_calls {
            for c in calls {
                let answered = msgs
                    .iter()
                    .skip(i + 1)
                    .take_while(|n| n.role == ModelMessageRole::Tool)
                    .any(|n| n.tool_call_id.as_deref() == Some(c.id.as_str()));
                assert!(
                    answered,
                    "tool_call {} 之后应紧跟其 tool 结果（含补的占位结果）",
                    c.id
                );
            }
        }
    }

    // 具体到悬空调用：orphan-1 必须有一条占位 tool 结果。
    let has_orphan_result = msgs
        .iter()
        .any(|m| m.role == ModelMessageRole::Tool && m.tool_call_id.as_deref() == Some("orphan-1"));
    assert!(
        has_orphan_result,
        "悬空 tool_call orphan-1 应被补占位 tool 结果"
    );
}

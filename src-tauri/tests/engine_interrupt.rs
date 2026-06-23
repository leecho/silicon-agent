//! Slice 6 / Task 1：取消标记 + 引擎中断检查。
//!
//! 覆盖两个中断检查点：
//!   1. cancel 预置 true → run_loop 第一轮开头即停（检查点①）：不调模型、emit stopped、
//!      不落「最终答案」assistant 消息。
//!   2. mock 发 2 个 Delta，第 1 个后由 mock 把 flag set true → 第 2 个 event 时引擎闭包
//!      返回 false 中止 token 流（检查点②）→ provider 返回 Err("model stream cancelled")
//!      → 引擎按 cancel 收口：落部分文本为 assistant 消息、emit stopped。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use silicon_agent::engine::event::AgentStreamEvent;
use silicon_agent::engine::Engine;
use silicon_agent::provider::client::{
    ModelCallRequest, ModelCallResult, ModelClient, ModelEvent, ProviderCallError,
};
use silicon_agent::session::SessionStore;
use silicon_agent::storage::AppDatabase;

fn temp_db() -> Arc<AppDatabase> {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "siw-interrupt_{}_{}_{}",
        std::process::id(),
        seq,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    Arc::new(AppDatabase::open(dir.join("app.sqlite3")).expect("db"))
}

/// 永不应被调用的 client：scenario 1 里 run_loop 第一轮开头即停，根本不到达 stream。
struct NeverCalledClient;

impl ModelClient for NeverCalledClient {
    fn stream_model_with_events(
        &self,
        _request: ModelCallRequest,
        _on_event: &mut dyn FnMut(ModelEvent) -> bool,
    ) -> Result<ModelCallResult, ProviderCallError> {
        panic!("模型不应被调用：cancel 预置 true，run_loop 第一轮开头即停");
    }
}

/// 发两个 Delta；第 1 个 delta 之后把共享 cancel 标记 set true，模拟用户中途点「停止」。
/// 沿用 provider 默认 `stream_model_with_events`：回调返回 false 时返回 Err。
struct MidStreamCancelClient {
    cancel: Arc<AtomicBool>,
}

impl ModelClient for MidStreamCancelClient {
    fn stream_model_with_events(
        &self,
        _request: ModelCallRequest,
        on_event: &mut dyn FnMut(ModelEvent) -> bool,
    ) -> Result<ModelCallResult, ProviderCallError> {
        // 第 1 个 delta：引擎闭包此刻 cancel 仍 false → 处理并累积「部分」。
        let cont = on_event(ModelEvent::Delta {
            text: "部分".into(),
        });
        assert!(cont, "第 1 个 delta 不应被取消");
        // 用户中途停止：set 标记。
        self.cancel.store(true, Ordering::Relaxed);
        // 第 2 个 delta：引擎闭包检查点②命中 → 返回 false → provider 据此返回 Err。
        let cont2 = on_event(ModelEvent::Delta {
            text: "不应累积".into(),
        });
        if !cont2 {
            return Err(ProviderCallError::new("model stream cancelled"));
        }
        Ok(ModelCallResult {
            events: vec![ModelEvent::AssistantMessageCompleted {
                content: "完整答案".into(),
            }],
            usage: None,
            finish_reason: Some("stop".into()),
        })
    }
}

#[test]
fn cancel_preset_true_stops_first_round_without_model_call() {
    let db = temp_db();
    let store = SessionStore::open(db.clone()).expect("store");
    let session = store
        .create_session("s_cancel", "t", "100", false)
        .expect("session");

    let events: Arc<Mutex<Vec<AgentStreamEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let events_for_emitter = events.clone();
    let engine = Engine::new(
        SessionStore::open(db.clone()).unwrap(),
        Arc::new(NeverCalledClient),
    )
    .with_emitter(Arc::new(move |e| {
        events_for_emitter.lock().unwrap().push(e)
    }));

    // cancel 预置 true。
    let cancel = Arc::new(AtomicBool::new(true));
    let (detail, pending) = engine
        .submit_user_message(&session.id, "在吗", cancel)
        .expect("submit");

    // pending 为 None（不是暂停，是停止收口）。
    assert!(pending.is_none());
    // 落库：user + stopped 标记两条；没有新增「最终答案」assistant 消息。
    // stopped 标记 role="stopped" + compacted=1：仅供 reload 在 feed 显示，不进模型上下文。
    assert_eq!(detail.messages.len(), 2, "应为 user + stopped 标记");
    assert_eq!(detail.messages[0].role, "user");
    assert_eq!(detail.messages[1].role, "stopped");
    assert!(detail.messages[1].compacted, "stopped 标记应 compacted");

    // emit 了 stopped 完成事件；没有 done 完成事件。
    let evts = events.lock().unwrap();
    assert!(
        evts.iter()
            .any(|e| e.kind == "message_completed" && e.status.as_deref() == Some("stopped")),
        "应 emit message_completed(stopped)"
    );
    assert!(
        !evts
            .iter()
            .any(|e| e.kind == "message_completed" && e.status.as_deref() == Some("done")),
        "不应有正常 done 收口"
    );
}

#[test]
fn mid_stream_cancel_persists_partial_text() {
    let db = temp_db();
    let store = SessionStore::open(db.clone()).expect("store");
    let session = store
        .create_session("s_partial", "t", "100", false)
        .expect("session");

    let events: Arc<Mutex<Vec<AgentStreamEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let events_for_emitter = events.clone();
    let cancel = Arc::new(AtomicBool::new(false));
    let engine = Engine::new(
        SessionStore::open(db.clone()).unwrap(),
        Arc::new(MidStreamCancelClient {
            cancel: cancel.clone(),
        }),
    )
    .with_emitter(Arc::new(move |e| {
        events_for_emitter.lock().unwrap().push(e)
    }));

    let (detail, pending) = engine
        .submit_user_message(&session.id, "讲个故事", cancel)
        .expect("submit");

    assert!(pending.is_none());
    // 落库：user + 部分 assistant + stopped 标记三条；assistant 内容为已累积的「部分」（不含第 2 个 delta）。
    assert_eq!(detail.messages.len(), 3);
    assert_eq!(detail.messages[0].role, "user");
    assert_eq!(detail.messages[1].role, "assistant");
    assert_eq!(detail.messages[1].content, "部分");
    // stopped 标记跟在部分 assistant 之后，role="stopped" + compacted=1。
    assert_eq!(detail.messages[2].role, "stopped");
    assert!(detail.messages[2].compacted, "stopped 标记应 compacted");

    // emit：至少 1 个 message_delta（第 1 个 token）+ 1 个 stopped 完成事件。
    let evts = events.lock().unwrap();
    let deltas = evts.iter().filter(|e| e.kind == "message_delta").count();
    assert_eq!(deltas, 1, "只应 emit 第 1 个 token 的 delta");
    assert!(
        evts.iter()
            .any(|e| e.kind == "message_completed" && e.status.as_deref() == Some("stopped")),
        "应 emit message_completed(stopped)"
    );
    assert!(
        !evts.iter().any(|e| e.kind == "message_failed"),
        "用户取消不应 emit message_failed"
    );
}

//! 工具调用参数 live 预览：provider 边生成边累积 arguments 并逐帧 emit ToolCallCreated；
//! 引擎在 arguments 非空时 emit `tool_call`(status=generating)，让前端实时显示大参数（如写报告）生成进度，
//! 修复"大 tool-call 生成期间界面长时间无反馈、看着像卡死"的问题。

use std::sync::{Arc, Mutex};

use silicon_agent::engine::event::AgentStreamEvent;
use silicon_agent::engine::Engine;
use silicon_agent::provider::client::{
    ModelCallRequest, ModelCallResult, ModelClient, ModelEvent, ProviderCallError,
};
use silicon_agent::session::SessionStore;
use silicon_agent::storage::AppDatabase;

/// 模拟真实 provider：流式逐帧 emit 累积 arguments 的 ToolCallCreated（先短后长），
/// 最终以一条普通文本收口（不触发后续工具执行，专注验证预览 emission）。
struct PreviewClient;

impl ModelClient for PreviewClient {
    fn stream_model_with_events(
        &self,
        _request: ModelCallRequest,
        on_event: &mut dyn FnMut(ModelEvent) -> bool,
    ) -> Result<ModelCallResult, ProviderCallError> {
        // 早帧：name 已到、args 仍空 → 引擎应跳过（不 emit 空预览）。
        on_event(ModelEvent::ToolCallCreated {
            id: "call-1".into(),
            name: "write_file".into(),
            arguments_json: String::new(),
        });
        // 累积中（部分参数）。
        on_event(ModelEvent::ToolCallCreated {
            id: "call-1".into(),
            name: "write_file".into(),
            arguments_json: "{\"path\":\"r.md\",\"content\":\"部分".into(),
        });
        // 累积完成（完整参数）。
        on_event(ModelEvent::ToolCallCreated {
            id: "call-1".into(),
            name: "write_file".into(),
            arguments_json: "{\"path\":\"r.md\",\"content\":\"部分报告内容\"}".into(),
        });
        Ok(ModelCallResult {
            events: vec![ModelEvent::AssistantMessageCompleted {
                content: "完成".into(),
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
        "siw-preview_{}_{}_{}",
        std::process::id(),
        seq,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    Arc::new(AppDatabase::open(dir.join("app.sqlite3")).expect("db"))
}

#[test]
fn streams_tool_call_argument_preview_during_generation() {
    let db = temp_db();
    let store = SessionStore::open(db.clone()).expect("store");
    let s = store
        .create_session("s1", "t", "100", false)
        .expect("session");

    let events: Arc<Mutex<Vec<AgentStreamEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();
    let engine = Engine::new(
        SessionStore::open(db.clone()).unwrap(),
        Arc::new(PreviewClient),
    )
    .with_emitter(Arc::new(move |e| sink.lock().unwrap().push(e)));

    engine
        .submit_user_message(
            &s.id,
            "写报告",
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
        .expect("submit");

    let evts = events.lock().unwrap();
    let previews: Vec<&AgentStreamEvent> = evts
        .iter()
        .filter(|e| e.kind == "tool_call" && e.tool_name.as_deref() == Some("write_file"))
        .collect();

    // 两帧非空 args → 两条预览（空 args 的早帧被跳过）。
    assert_eq!(
        previews.len(),
        2,
        "应只为非空 args 帧 emit 预览，实际 {}",
        previews.len()
    );
    for p in &previews {
        assert_eq!(p.tool_call_id.as_deref(), Some("call-1"));
        // 生成期状态为 generating（区别于实际执行 running），前端据此显示「正在生成…」。
        assert_eq!(p.status.as_deref(), Some("generating"));
    }
    // 参数随生成增长：末帧文本比首帧长，且含完整内容关键字。
    let first = previews[0].text.as_deref().unwrap_or("");
    let last = previews[1].text.as_deref().unwrap_or("");
    assert!(last.len() > first.len(), "末帧应含更完整的累积参数");
    assert!(
        last.contains("部分报告内容"),
        "末帧应含完整参数内容，实际: {last}"
    );
}

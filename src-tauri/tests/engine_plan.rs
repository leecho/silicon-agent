// Tests for the propose_plan control tool + engine interception (Slice 9 Task 2).
//
// 计划模式下模型第一轮调用 propose_plan（控制工具）→ 引擎按名拦截：不真执行、emit plan_required、
// 返回 PendingInteraction::Plan 暂停。命令层据用户裁定落该 propose_plan 调用的 tool 结果后 resume：
// 批准 → 切 normal 模式 + 落「已批准」结果 → 模型续跑给最终答案；评论 → 保持 plan 模式 + 落评论结果。

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use silicon_agent::engine::event::AgentStreamEvent;
use silicon_agent::engine::{Engine, PendingInteraction};
use silicon_agent::provider::client::{
    ModelCallRequest, ModelCallResult, ModelClient, ModelEvent, ProviderCallError,
};
use silicon_agent::session::{new_id, PendingPlan, SessionStore};
use silicon_agent::storage::AppDatabase;
use silicon_agent::tools::propose_plan::ProposePlan;
use silicon_agent::tools::ToolRegistry;

/// 两轮 mock：第一轮请求 propose_plan（带 title + plan_markdown），第二轮给最终答案。
/// 镜像真实 provider 流式：live ToolCallCreated 的 args 为空，完整 args 在最终 result.events。
struct PlanClient {
    calls: AtomicUsize,
}

impl ModelClient for PlanClient {
    fn stream_model_with_events(
        &self,
        _request: ModelCallRequest,
        on_event: &mut dyn FnMut(ModelEvent) -> bool,
    ) -> Result<ModelCallResult, ProviderCallError> {
        let turn = self.calls.fetch_add(1, Ordering::SeqCst);
        if turn == 0 {
            let args = serde_json::json!({
                "title": "重构 X",
                "plan_markdown": "步骤1: 读代码\n步骤2: 改造"
            });
            on_event(ModelEvent::ToolCallCreated {
                id: "call-p".into(),
                name: "propose_plan".into(),
                arguments_json: String::new(),
            });
            Ok(ModelCallResult {
                events: vec![ModelEvent::ToolCallCreated {
                    id: "call-p".into(),
                    name: "propose_plan".into(),
                    arguments_json: args.to_string(),
                }],
                usage: None,
                finish_reason: Some("tool_calls".into()),
            })
        } else {
            on_event(ModelEvent::Delta {
                text: "已按计划完成。".into(),
            });
            Ok(ModelCallResult {
                events: vec![ModelEvent::AssistantMessageCompleted {
                    content: "已按计划完成。".into(),
                }],
                usage: None,
                finish_reason: Some("stop".into()),
            })
        }
    }
}

fn temp_dir() -> std::path::PathBuf {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "siw-plan_{}_{}_{}",
        std::process::id(),
        seq,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ))
}

fn now_string() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_default()
}

fn engine_with(db: &Arc<AppDatabase>, events: Arc<Mutex<Vec<AgentStreamEvent>>>) -> Engine {
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(ProposePlan));
    Engine::new(
        SessionStore::open(db.clone()).unwrap(),
        Arc::new(PlanClient {
            calls: AtomicUsize::new(0),
        }),
    )
    .with_registry(registry)
    .with_emitter(Arc::new(move |e| events.lock().unwrap().push(e)))
}

/// submit（plan 模式）→ 引擎拦截 propose_plan：返回 PendingInteraction::Plan、propose_plan 未执行
/// （无 tool 结果）、emit plan_required。**批准**：切 normal + 落批准结果 + resume → 模型续跑给最终答案、
/// mode=normal、pending None、messages 含 propose_plan tool 结果（批准文案）。
#[test]
fn propose_plan_pauses_then_approve_switches_to_normal_and_resumes() {
    let base = temp_dir();
    std::fs::create_dir_all(&base).expect("base");
    let db = Arc::new(AppDatabase::open(base.join("app.sqlite3")).expect("db"));

    let store = SessionStore::open(db.clone()).expect("store");
    let session = store
        .create_session("s1", "plan", "100", false)
        .expect("session");
    store
        .set_session_mode(&session.id, "plan", "100")
        .expect("set plan");

    let events: Arc<Mutex<Vec<AgentStreamEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let engine = engine_with(&db, events.clone());

    // 1. submit → 暂停（propose_plan 拦截）。
    let (detail, pending) = engine
        .submit_user_message(&session.id, "帮我重构 X", Arc::new(AtomicBool::new(false)))
        .expect("submit");

    let plan: PendingPlan = match pending {
        Some(PendingInteraction::Plan(p)) => p,
        _ => panic!("应为 propose_plan 暂停"),
    };
    assert_eq!(plan.tool_call_id, "call-p");
    assert_eq!(plan.title, "重构 X");
    assert!(
        plan.plan_markdown.contains("步骤1"),
        "计划正文应含步骤，实际: {}",
        plan.plan_markdown
    );
    assert_eq!(plan.risk_level, "medium");

    // propose_plan 未执行：消息只有 user + assistant(tool_calls)，无 tool 结果。
    let roles: Vec<&str> = detail.messages.iter().map(|m| m.role.as_str()).collect();
    assert_eq!(roles, vec!["user", "assistant"]);
    assert!(
        detail.messages.iter().all(|m| m.role != "tool"),
        "暂停时不应有 propose_plan 的 tool 结果消息"
    );

    // emit 了 plan_required（带 plan_markdown + tool_call_id），无 tool_result。
    {
        let evts = events.lock().unwrap();
        let req = evts
            .iter()
            .find(|e| e.kind == "plan_required")
            .expect("应 emit plan_required");
        assert_eq!(req.tool_name.as_deref(), Some("propose_plan"));
        assert_eq!(req.tool_call_id.as_deref(), Some("call-p"));
        assert!(req.text.as_deref().unwrap_or("").contains("步骤1"));
        assert!(
            !evts.iter().any(|e| e.kind == "tool_result"),
            "拦截 propose_plan 时不应 emit tool_result"
        );
    }

    // 2. 批准：切 normal + 落「已批准」工具结果 + resume（模拟 submit_plan_decision 批准路径）。
    store
        .set_session_mode(&session.id, "normal", &now_string())
        .expect("set normal");
    store
        .append_tool_result(
            &new_id("msg"),
            &session.id,
            "call-p",
            "propose_plan",
            "[计划已批准] 用户已批准你的计划。现在已切换到执行模式，请按计划逐步实施。",
            "done",
            &now_string(),
        )
        .expect("append approval");

    let (detail2, pending2) = engine
        .resume(&session.id, Arc::new(AtomicBool::new(false)))
        .expect("resume");
    assert!(pending2.is_none(), "批准后续跑应完成，不再暂停");

    // mode 已切回 normal。
    assert_eq!(store.get_session_mode(&session.id).unwrap(), "normal");

    // 消息序列：user / assistant(tool_calls) / tool(批准结果) / assistant(final)。
    let roles2: Vec<&str> = detail2.messages.iter().map(|m| m.role.as_str()).collect();
    assert_eq!(roles2, vec!["user", "assistant", "tool", "assistant"]);

    let plan_result = &detail2.messages[2];
    assert_eq!(plan_result.tool_call_id.as_deref(), Some("call-p"));
    assert_eq!(plan_result.tool_name.as_deref(), Some("propose_plan"));
    assert!(
        plan_result.content.contains("计划已批准"),
        "tool 结果应含批准文案，实际: {}",
        plan_result.content
    );

    let final_msg = &detail2.messages[3];
    assert_eq!(final_msg.content, "已按计划完成。");
    assert!(final_msg.tool_calls_json.is_none());
}

/// **评论**路径：暂停后落评论结果 + resume → mode 仍为 plan（不切换），messages 含评论结果。
#[test]
fn propose_plan_comment_keeps_plan_mode() {
    let base = temp_dir();
    std::fs::create_dir_all(&base).expect("base");
    let db = Arc::new(AppDatabase::open(base.join("app.sqlite3")).expect("db"));

    let store = SessionStore::open(db.clone()).expect("store");
    let session = store
        .create_session("s2", "plan", "100", false)
        .expect("session");
    store
        .set_session_mode(&session.id, "plan", "100")
        .expect("set plan");

    let events: Arc<Mutex<Vec<AgentStreamEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let engine = engine_with(&db, events.clone());

    let (_detail, pending) = engine
        .submit_user_message(&session.id, "帮我重构 X", Arc::new(AtomicBool::new(false)))
        .expect("submit");
    assert!(
        matches!(pending, Some(PendingInteraction::Plan(_))),
        "应为 propose_plan 暂停"
    );

    // 评论：保持 plan 模式（不切 normal），落评论结果 + resume（模拟 submit_plan_decision 评论路径）。
    store
        .append_tool_result(
            &new_id("msg"),
            &session.id,
            "call-p",
            "propose_plan",
            "[用户评论] 步骤2 太粗\n请保持计划模式，据此修订计划并再次调用 propose_plan。",
            "done",
            &now_string(),
        )
        .expect("append comment");

    let (detail2, _pending2) = engine
        .resume(&session.id, Arc::new(AtomicBool::new(false)))
        .expect("resume");

    // mode 仍为 plan（评论路径不切换）。
    assert_eq!(store.get_session_mode(&session.id).unwrap(), "plan");

    // messages 含评论结果。
    let comment_msg = detail2
        .messages
        .iter()
        .find(|m| m.role == "tool" && m.tool_call_id.as_deref() == Some("call-p"))
        .expect("应有 propose_plan 评论结果");
    assert!(
        comment_msg.content.contains("用户评论"),
        "tool 结果应含评论文案，实际: {}",
        comment_msg.content
    );
}

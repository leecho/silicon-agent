// dispatch_agent 异步派生 — 引擎层测试（State<AppState> 无法在集成测试构造，故测 Engine+SessionStore）。
//   ① 模型派发已知智能体 → 建 child 会话（父子链 + origin=subagent）+ 父停泊 + 返回 Subagent 信号。
//   ② 模型派发未知智能体 → 落 failed tool 结果、不停泊为 Subagent（父继续到收口）。

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use silicon_worker::engine::{Engine, PendingInteraction};
use silicon_worker::expert::ExpertService;
use silicon_worker::provider::client::{
    ModelCallRequest, ModelCallResult, ModelClient, ModelEvent, ProviderCallError,
};
use silicon_worker::session::{new_id, SessionStore};
use silicon_worker::storage::AppDatabase;

/// 每次调用都派发同一个智能体（park 测试：模型只被调一次就停泊）。
struct DispatchClient {
    member: String,
}
impl ModelClient for DispatchClient {
    fn stream_model_with_events(
        &self,
        _req: ModelCallRequest,
        _cancel: &std::sync::atomic::AtomicBool,
        on_event: &mut dyn FnMut(ModelEvent) -> bool,
    ) -> Result<ModelCallResult, ProviderCallError> {
        let args = serde_json::json!({ "name": self.member, "task": "看看 src 结构" });
        on_event(ModelEvent::ToolCallCreated {
            id: "call-d".into(),
            name: "dispatch_agent".into(),
            arguments_json: String::new(),
        });
        Ok(ModelCallResult {
            events: vec![ModelEvent::ToolCallCreated {
                id: "call-d".into(),
                name: "dispatch_agent".into(),
                arguments_json: args.to_string(),
            }],
            usage: None,
            finish_reason: Some("tool_calls".into()),
        })
    }
}

/// 一轮内**并行**派发两个 ad-hoc 智能体（同一 ModelCallResult 含两个 dispatch tool_call）。
struct ParallelDispatchClient;
impl ModelClient for ParallelDispatchClient {
    fn stream_model_with_events(
        &self,
        _req: ModelCallRequest,
        _cancel: &std::sync::atomic::AtomicBool,
        on_event: &mut dyn FnMut(ModelEvent) -> bool,
    ) -> Result<ModelCallResult, ProviderCallError> {
        let mk = |id: &str, name: &str| ModelEvent::ToolCallCreated {
            id: id.into(),
            name: "dispatch_agent".into(),
            arguments_json: serde_json::json!({
                "name": name,
                "task": format!("{name} 的子任务"),
                "system_prompt": format!("你是{name}，按「结论/证据/风险/建议下一步」回禀。"),
                "tools": ["web_search"]
            })
            .to_string(),
        };
        on_event(ModelEvent::ToolCallCreated {
            id: "call-p1".into(),
            name: "dispatch_agent".into(),
            arguments_json: String::new(),
        });
        Ok(ModelCallResult {
            events: vec![mk("call-p1", "甲智能体"), mk("call-p2", "乙智能体")],
            usage: None,
            finish_reason: Some("tool_calls".into()),
        })
    }
}

/// 派发一个**未声明**的临时智能体，但带 inline spec（system_prompt + tools）——ad-hoc 路径。
struct AdHocDispatchClient;
impl ModelClient for AdHocDispatchClient {
    fn stream_model_with_events(
        &self,
        _req: ModelCallRequest,
        _cancel: &std::sync::atomic::AtomicBool,
        on_event: &mut dyn FnMut(ModelEvent) -> bool,
    ) -> Result<ModelCallResult, ProviderCallError> {
        let args = serde_json::json!({
            "name": "天气检索",
            "task": "查南昌今天天气",
            "system_prompt": "你是天气检索智能体，只用检索工具，按「结论/证据/风险/建议下一步」回禀。",
            "tools": ["web_search", "web_fetch"]
        });
        on_event(ModelEvent::ToolCallCreated {
            id: "call-h".into(),
            name: "dispatch_agent".into(),
            arguments_json: String::new(),
        });
        Ok(ModelCallResult {
            events: vec![ModelEvent::ToolCallCreated {
                id: "call-h".into(),
                name: "dispatch_agent".into(),
                arguments_json: args.to_string(),
            }],
            usage: None,
            finish_reason: Some("tool_calls".into()),
        })
    }
}

/// 第 0 轮派发未知智能体、第 1 轮给最终答复（验证 failed→继续→收口）。
struct UnknownThenAnswerClient {
    calls: AtomicUsize,
}
impl ModelClient for UnknownThenAnswerClient {
    fn stream_model_with_events(
        &self,
        _req: ModelCallRequest,
        _cancel: &std::sync::atomic::AtomicBool,
        on_event: &mut dyn FnMut(ModelEvent) -> bool,
    ) -> Result<ModelCallResult, ProviderCallError> {
        let turn = self.calls.fetch_add(1, Ordering::SeqCst);
        if turn == 0 {
            let args = serde_json::json!({ "name": "nobody", "task": "x" });
            on_event(ModelEvent::ToolCallCreated {
                id: "call-x".into(),
                name: "dispatch_agent".into(),
                arguments_json: String::new(),
            });
            Ok(ModelCallResult {
                events: vec![ModelEvent::ToolCallCreated {
                    id: "call-x".into(),
                    name: "dispatch_agent".into(),
                    arguments_json: args.to_string(),
                }],
                usage: None,
                finish_reason: Some("tool_calls".into()),
            })
        } else {
            on_event(ModelEvent::Delta {
                text: "已改用自己处理。".into(),
            });
            Ok(ModelCallResult {
                events: vec![ModelEvent::AssistantMessageCompleted {
                    content: "已改用自己处理。".into(),
                }],
                usage: None,
                finish_reason: Some("stop".into()),
            })
        }
    }
}

/// 第 0 轮派发 explorer；第 1 轮（父续跑、上下文已含 dispatch 结果）给最终答复。
struct DispatchThenAnswerClient {
    calls: AtomicUsize,
}
impl ModelClient for DispatchThenAnswerClient {
    fn stream_model_with_events(
        &self,
        _req: ModelCallRequest,
        _cancel: &std::sync::atomic::AtomicBool,
        on_event: &mut dyn FnMut(ModelEvent) -> bool,
    ) -> Result<ModelCallResult, ProviderCallError> {
        let turn = self.calls.fetch_add(1, Ordering::SeqCst);
        if turn == 0 {
            let args = serde_json::json!({ "name": "explorer", "task": "看看 src 结构" });
            on_event(ModelEvent::ToolCallCreated {
                id: "call-d".into(),
                name: "dispatch_agent".into(),
                arguments_json: String::new(),
            });
            Ok(ModelCallResult {
                events: vec![ModelEvent::ToolCallCreated {
                    id: "call-d".into(),
                    name: "dispatch_agent".into(),
                    arguments_json: args.to_string(),
                }],
                usage: None,
                finish_reason: Some("tool_calls".into()),
            })
        } else {
            on_event(ModelEvent::Delta {
                text: "根据智能体勘探，src 有 12 个模块。".into(),
            });
            Ok(ModelCallResult {
                events: vec![ModelEvent::AssistantMessageCompleted {
                    content: "根据智能体勘探，src 有 12 个模块。".into(),
                }],
                usage: None,
                finish_reason: Some("stop".into()),
            })
        }
    }
}

fn temp_dir(tag: &str) -> std::path::PathBuf {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "siw-agent-dispatch_{}_{}_{}_{}",
        tag,
        std::process::id(),
        seq,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ))
}

fn setup(tag: &str) -> (Arc<AppDatabase>, SessionStore, Arc<ExpertService>) {
    let base = temp_dir(tag);
    let db = Arc::new(AppDatabase::open(base.join("app.sqlite3")).expect("db"));
    let store = SessionStore::open(db.clone()).expect("store");
    // 方案B 无内置预置——写一个散装 explorer 智能体供"声明式派发"测试用。
    let agents_root = base.join("agents");
    std::fs::create_dir_all(&agents_root).unwrap();
    std::fs::write(
        agents_root.join("explorer.md"),
        "---\nname: explorer\ndescription: 只读勘探\ntools: [read_file, glob, grep]\nmodel: aux\nmax_turns: 8\nrole: member\n---\n你是勘探智能体，只读。\n",
    )
    .unwrap();
    let agents = Arc::new(ExpertService::new(db.clone(), agents_root));
    agents.sync().expect("agents sync");
    (db, store, agents)
}

#[test]
fn dispatch_known_member_creates_child_and_parks() {
    let (db, store, agents) = setup("known");
    let parent = store
        .create_session("s-parent", "p", "100", false)
        .expect("session");

    let engine = Engine::new(
        SessionStore::open(db.clone()).unwrap(),
        Arc::new(DispatchClient {
            member: "explorer".into(),
        }),
    )
    .with_experts(agents)
    .with_emitter(Arc::new(|_e| {}));

    let (_detail, pending) = engine
        .submit_user_message(
            &parent.id,
            "用勘探看看 src",
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
        .expect("submit");

    let child_id = match pending {
        Some(PendingInteraction::Subagent {
            mut child_session_ids,
        }) => {
            assert_eq!(child_session_ids.len(), 1, "单派发应只有一个 child");
            child_session_ids.remove(0)
        }
        _ => panic!("应为 Subagent 停泊信号"),
    };

    // child 会话父子链正确。
    let child = store
        .get_session(&child_id)
        .expect("get child")
        .expect("child exists");
    assert_eq!(child.parent_session_id.as_deref(), Some("s-parent"));
    assert_eq!(child.parent_tool_call_id.as_deref(), Some("call-d"));
    assert_eq!(child.expert_name.as_deref(), Some("explorer"));
    assert_eq!(child.origin, "subagent");

    // child 首条任务消息已落。
    let msgs = store.list_messages(&child_id).expect("msgs");
    assert!(msgs
        .iter()
        .any(|m| m.role == "user" && m.content.contains("看看 src 结构")));

    // 父已停泊。
    let p = store
        .get_session("s-parent")
        .expect("get parent")
        .expect("parent");
    assert_eq!(p.awaiting_subagent.as_deref(), Some(child_id.as_str()));
}

#[test]
fn dispatch_ad_hoc_member_creates_child_with_inline_spec() {
    // 方案B 核心：智能体名不在 ExpertService 里，但 dispatch 带了 system_prompt + tools，
    // 走 ad-hoc 路径 → 建 child 并把 inline spec 存到 child 行。
    let (db, store, agents) = setup("adhoc");
    let parent = store
        .create_session("s-parent-h", "ph", "100", false)
        .expect("session");

    let engine = Engine::new(
        SessionStore::open(db.clone()).unwrap(),
        Arc::new(AdHocDispatchClient),
    )
    .with_experts(agents)
    .with_emitter(Arc::new(|_e| {}));

    let (_detail, pending) = engine
        .submit_user_message(
            &parent.id,
            "查下南昌天气",
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
        .expect("submit");

    let child_id = match pending {
        Some(PendingInteraction::Subagent {
            mut child_session_ids,
        }) => {
            assert_eq!(child_session_ids.len(), 1, "单派发应只有一个 child");
            child_session_ids.remove(0)
        }
        _ => panic!("ad-hoc 智能体也应停泊为 Subagent"),
    };

    let child = store
        .get_session(&child_id)
        .expect("get child")
        .expect("child exists");
    assert_eq!(child.expert_name.as_deref(), Some("天气检索"));
    assert_eq!(child.parent_tool_call_id.as_deref(), Some("call-h"));
    assert_eq!(child.origin, "subagent");
    // inline spec 落到 child 行。
    assert!(child
        .expert_system_prompt
        .as_deref()
        .unwrap_or("")
        .contains("天气检索智能体"));
    assert_eq!(child.expert_tools.as_deref(), Some("web_search,web_fetch"));

    // 父已停泊。
    let p = store
        .get_session("s-parent-h")
        .expect("get parent")
        .expect("parent");
    assert_eq!(p.awaiting_subagent.as_deref(), Some(child_id.as_str()));
}

#[test]
fn parallel_dispatch_creates_all_children_and_parks_once() {
    // 一轮内派发两个智能体 → 信号带两个 child id，两个 child 会话都建好，父停泊一次。
    let (db, store, agents) = setup("parallel");
    let parent = store
        .create_session("s-parent-par", "ppar", "100", false)
        .expect("session");

    let engine = Engine::new(
        SessionStore::open(db.clone()).unwrap(),
        Arc::new(ParallelDispatchClient),
    )
    .with_experts(agents)
    .with_emitter(Arc::new(|_e| {}));

    let (_detail, pending) = engine
        .submit_user_message(
            &parent.id,
            "并行派两个智能体",
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
        .expect("submit");

    let child_ids = match pending {
        Some(PendingInteraction::Subagent { child_session_ids }) => child_session_ids,
        _ => panic!("并行派发应为 Subagent 停泊信号"),
    };
    assert_eq!(child_ids.len(), 2, "应攒齐两个 child 一并启动");

    // 两个 child 会话都建好，分别挂在两个 dispatch 调用上。
    let names: Vec<String> = child_ids
        .iter()
        .map(|id| {
            store
                .get_session(id)
                .expect("get")
                .expect("child")
                .expert_name
                .unwrap_or_default()
        })
        .collect();
    assert!(names.contains(&"甲智能体".to_string()));
    assert!(names.contains(&"乙智能体".to_string()));

    // pending_child_count = 2（两个 child 结果都未回填）。
    assert_eq!(store.pending_child_count("s-parent-par").expect("count"), 2);

    // 父停泊一次（busy 标记为首个 child）。
    let p = store
        .get_session("s-parent-par")
        .expect("get parent")
        .expect("parent");
    assert!(p.awaiting_subagent.is_some());
}

#[test]
fn dispatch_unknown_member_fails_and_continues() {
    let (db, store, agents) = setup("unknown");
    let parent = store
        .create_session("s-parent2", "p2", "100", false)
        .expect("session");

    let engine = Engine::new(
        SessionStore::open(db.clone()).unwrap(),
        Arc::new(UnknownThenAnswerClient {
            calls: AtomicUsize::new(0),
        }),
    )
    .with_experts(agents)
    .with_emitter(Arc::new(|_e| {}));

    let (detail, pending) = engine
        .submit_user_message(
            &parent.id,
            "派给不存在的人",
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
        .expect("submit");

    // 未知智能体不应产生 Subagent 停泊；run 继续到收口（None）。
    assert!(
        !matches!(pending, Some(PendingInteraction::Subagent { .. })),
        "未知智能体不应停泊为 Subagent"
    );
    // 落了一条 failed 的 dispatch_agent tool 结果。
    assert!(detail.messages.iter().any(|m| m.role == "tool"
        && m.tool_name.as_deref() == Some("dispatch_agent")
        && m.tool_status.as_deref() == Some("failed")));
    // 父未停泊。
    let p = store
        .get_session("s-parent2")
        .expect("get parent")
        .expect("parent");
    assert!(p.awaiting_subagent.is_none());
}

/// 全周期 crux：dispatch→park → 模拟 child 完成回填（= finish_child_into_parent 核心）→ 续跑父 →
/// 父见 dispatch tool 结果后继续到收口。验证"续跑父"机制（spawn_child_run 线程外的逻辑核心）。
#[test]
fn parent_resumes_after_child_summary_filled() {
    let (db, store, agents) = setup("cycle");
    let parent = store
        .create_session("s-cyc", "p", "100", false)
        .expect("session");
    let cancel = || Arc::new(std::sync::atomic::AtomicBool::new(false));

    let engine = Engine::new(
        SessionStore::open(db.clone()).unwrap(),
        Arc::new(DispatchThenAnswerClient {
            calls: AtomicUsize::new(0),
        }),
    )
    .with_experts(agents)
    .with_emitter(Arc::new(|_e| {}));

    // turn0：dispatch → park。
    let (_d, pending) = engine
        .submit_user_message(&parent.id, "用勘探看看 src", cancel())
        .expect("submit");
    assert!(matches!(pending, Some(PendingInteraction::Subagent { .. })));

    // 模拟 child 完成：回填父 dispatch tool 结果 + 清父停泊（finish_child_into_parent 的核心两步）。
    store
        .append_tool_result(
            &new_id("msg"),
            &parent.id,
            "call-d",
            "dispatch_agent",
            "勘探结论：src 有 12 个模块。",
            "done",
            "200",
        )
        .expect("fill");
    store
        .clear_awaiting_subagent(&parent.id, "200")
        .expect("clear");

    // 续跑父：父 run_loop 见 dispatch tool 结果（不再 pending）→ 调模型 turn1 → 最终答复、收口。
    let (detail, pending2) = engine.resume(&parent.id, cancel()).expect("resume");
    assert!(pending2.is_none(), "父续跑应收口（None）");

    // 父 messages 含 dispatch tool 结果。
    assert!(detail
        .messages
        .iter()
        .any(|m| m.role == "tool" && m.tool_name.as_deref() == Some("dispatch_agent")));
    // 续跑产出了据勘探结论的最终 assistant 答复（证明回填后模型被再调用）。
    // 注：不断言"末条"——本测试用了非真实时间戳，消息排序非确定；真实 finish_child_into_parent 用 now_string 顺序正确。
    assert!(
        detail
            .messages
            .iter()
            .any(|m| m.role == "assistant" && m.content.contains("模块")),
        "回填后续跑应产出最终答复"
    );
}

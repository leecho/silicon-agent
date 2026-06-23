// 子运行字段往返：建会话 → 读回 SessionInfo 的父子链/agent 字段（P0 数据模型）。
// 顶层会话四字段默认 None，schema/映射正确、向后兼容。
use silicon_agent::session::SessionStore;
use silicon_agent::storage::AppDatabase;
use std::sync::Arc;

fn store() -> SessionStore {
    use std::sync::atomic::{AtomicU64, Ordering};
    // 进程内自增序列保证每次 store() 的临时库文件唯一——不依赖时间戳，避免并行执行下纳秒撞名共享库。
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!(
        "siw-session-child-{}-{}.db",
        std::process::id(),
        seq
    ));
    let _ = std::fs::remove_file(&p);
    let db = Arc::new(AppDatabase::open(&p).expect("open"));
    SessionStore::open(db).expect("store")
}

#[test]
fn child_fields_default_none_then_roundtrip() {
    let s = store();
    let info = s
        .create_session("sess-child", "子运行", "1", false)
        .expect("create");
    // 新建默认顶层会话：四个子运行字段均 None
    assert!(info.parent_session_id.is_none());
    assert!(info.parent_tool_call_id.is_none());
    assert!(info.expert_name.is_none());
    assert!(info.agent_task.is_none());

    // 读回也应 None（schema 列存在、session_from_row 映射正确）
    let got = s.get_session("sess-child").expect("get").expect("some");
    assert!(got.parent_session_id.is_none());
    assert!(got.parent_tool_call_id.is_none());
    assert!(got.expert_name.is_none());
    assert!(got.agent_task.is_none());
    assert!(got.awaiting_subagent.is_none());
}

#[test]
fn create_child_then_read_links_and_awaiting() {
    let s = store();
    s.create_session("parent", "P", "1", false).expect("parent");
    s.create_child_session(
        "child",
        "parent",
        "tc-1",
        "explorer",
        "勘探任务",
        None,
        None,
        false,
        "1",
        None,
    )
    .expect("child");
    let c = s.get_session("child").expect("get").expect("some");
    assert_eq!(c.parent_session_id.as_deref(), Some("parent"));
    assert_eq!(c.parent_tool_call_id.as_deref(), Some("tc-1"));
    assert_eq!(c.expert_name.as_deref(), Some("explorer"));
    assert_eq!(c.agent_task.as_deref(), Some("勘探任务"));
    assert_eq!(c.origin, "subagent");

    s.set_awaiting_subagent("parent", "child", "2")
        .expect("set");
    assert_eq!(
        s.get_session("parent")
            .unwrap()
            .unwrap()
            .awaiting_subagent
            .as_deref(),
        Some("child")
    );
    s.clear_awaiting_subagent("parent", "3").expect("clear");
    assert!(s
        .get_session("parent")
        .unwrap()
        .unwrap()
        .awaiting_subagent
        .is_none());
}

#[test]
fn list_awaiting_parents_finds_only_parked() {
    // 启动恢复扫描的数据源：只列出 awaiting_subagent 非空的父会话。
    let s = store();
    s.create_session("p1", "P1", "1", false).expect("p1");
    s.create_session("p2", "P2", "1", false).expect("p2");
    s.create_session("p3", "P3", "1", false).expect("p3");
    // p1 停泊等 c1；p2、p3 未停泊。
    s.create_child_session(
        "c1", "p1", "tc-a", "explorer", "任务", None, None, false, "1", None,
    )
    .expect("c1");
    s.set_awaiting_subagent("p1", "c1", "2").expect("await");

    let parked = s.list_awaiting_parents().expect("list");
    assert_eq!(parked, vec![("p1".to_string(), "c1".to_string())]);

    // 清停泊后不再出现。
    s.clear_awaiting_subagent("p1", "3").expect("clear");
    assert!(s.list_awaiting_parents().expect("list2").is_empty());
}

#[test]
fn cancel_dangling_dispatches_backfills_and_unparks() {
    // 串行停止回归：父停泊在子代理 dispatch 上、子运行被取消而无 tool_result（悬空）。
    // 解冻应：把悬空 dispatch 回填为 failed 结果 + 清 awaiting_subagent，使父恢复可交互。
    let s = store();
    s.create_session("parent", "P", "1", false).expect("parent");
    s.create_child_session(
        "child", "parent", "tc-a", "explorer", "任务", None, None, false, "1", None,
    )
    .expect("child");
    s.set_awaiting_subagent("parent", "child", "2").expect("await");
    // 子运行有部分产出但被取消；父侧没有 tc-a 的 tool_result（悬空 dispatch）。
    s.append_message("m-c1", "child", "assistant", "我看了一半 src/", None, "2")
        .expect("partial");

    // 停泊前置：父停泊、tc-a 无结果。
    assert!(s
        .get_session("parent")
        .unwrap()
        .unwrap()
        .awaiting_subagent
        .is_some());
    assert!(s.tool_result_status("parent", "tc-a").unwrap().is_none());

    let backfilled = s
        .cancel_dangling_dispatches("parent", "dispatch_agent", "3")
        .expect("unfreeze");

    // tc-a 被回填、含此前进展；父停泊已清。
    let tcs: Vec<String> = backfilled.iter().map(|(tc, _)| tc.clone()).collect();
    assert_eq!(tcs, vec!["tc-a".to_string()]);
    assert!(backfilled[0].1.contains("我看了一半 src/"));
    assert_eq!(
        s.tool_result_status("parent", "tc-a").unwrap().as_deref(),
        Some("failed")
    );
    assert!(s
        .get_session("parent")
        .unwrap()
        .unwrap()
        .awaiting_subagent
        .is_none());

    // 幂等：非停泊态再调一次不重复回填。
    let again = s
        .cancel_dangling_dispatches("parent", "dispatch_agent", "4")
        .expect("again");
    assert!(again.is_empty());
}

#[test]
fn cancel_dangling_dispatches_skips_already_finished_child() {
    // 子运行已抢先回填结果（done）→ 不重复回填，但仍清父停泊。
    let s = store();
    s.create_session("parent", "P", "1", false).expect("parent");
    s.create_child_session(
        "child", "parent", "tc-a", "explorer", "任务", None, None, false, "1", None,
    )
    .expect("child");
    s.set_awaiting_subagent("parent", "child", "2").expect("await");
    s.append_tool_result("m-r", "parent", "tc-a", "dispatch_agent", "已回禀", "done", "2")
        .expect("result");

    let backfilled = s
        .cancel_dangling_dispatches("parent", "dispatch_agent", "3")
        .expect("unfreeze");
    assert!(backfilled.is_empty());
    // 既有结果未被覆盖。
    assert_eq!(
        s.tool_result_status("parent", "tc-a").unwrap().as_deref(),
        Some("done")
    );
    // 停泊仍被清。
    assert!(s
        .get_session("parent")
        .unwrap()
        .unwrap()
        .awaiting_subagent
        .is_none());
}

#[test]
fn settle_pending_tool_call_resolves_dangling_interaction() {
    // 中断架构：会话暂停在某工具（权限/ask/plan）上 = 该 tool_call 悬空无结果。
    // 停止收口应写一条 cancelled 结果解析掉它，使其不再 pending、reload 不复现、决策不复活。
    let s = store();
    s.create_session("sess", "S", "1", false).expect("sess");
    let calls = r#"[{"id":"call-x","name":"write_file","arguments_json":"{}"}]"#;
    s.append_assistant_tool_call("m-a", "sess", "", None, calls, "1")
        .expect("assistant");

    // 悬空：first_dangling 命中 (call-x, write_file)，且尚无结果。
    assert_eq!(
        s.first_dangling_tool_call("sess").unwrap(),
        Some(("call-x".to_string(), "write_file".to_string()))
    );
    assert!(s.tool_result_status("sess", "call-x").unwrap().is_none());

    // 收口：写 cancelled 结果。
    assert!(s
        .settle_pending_tool_call("sess", "call-x", "用户已停止会话，未执行该操作。", "2")
        .unwrap());
    assert_eq!(
        s.tool_result_status("sess", "call-x").unwrap().as_deref(),
        Some("cancelled")
    );
    // 收口后不再悬空、幂等。
    assert!(s.first_dangling_tool_call("sess").unwrap().is_none());
    assert!(!s
        .settle_pending_tool_call("sess", "call-x", "x", "3")
        .unwrap());
}

#[test]
fn append_stopped_marker_renders_as_divider_source() {
    // 停止反馈：停泊/停止时落一条 role="stopped" 分隔消息（前端渲染成「已手动停止」分隔线）。
    let s = store();
    s.create_session("sess", "S", "1", false).expect("sess");
    s.append_stopped_marker("sess", "2");
    let msgs = s.list_messages("sess").expect("msgs");
    let stopped: Vec<_> = msgs.iter().filter(|m| m.role == "stopped").collect();
    assert_eq!(stopped.len(), 1);
    assert!(stopped[0].compacted, "stopped 标记应 compacted=1 不进模型上下文");
}

#[test]
fn classify_drives_parked_converge_then_store_settles() {
    // 统一 reconcile：停泊 + 无活子 → 分类判定 ConvergeParked；其 DB 收敛由 cancel_dangling_dispatches 完成
    //（回填悬空 dispatch + 清 awaiting）。此处验证「分类决策 + store 收敛原语」两段拼起来的契约。
    use silicon_agent::run::reconcile::{classify_reconcile, ReconcileAction, ReconcileInputs};
    let parked_no_live = ReconcileInputs {
        is_live: false,
        any_child_live: false,
        awaiting_or_collect: true,
        stop_requested: false,
        pending_interaction: false,
        dangling_noninteraction: false,
        head_running: false,
    };
    assert_eq!(classify_reconcile(parked_no_live), ReconcileAction::ConvergeParked);
    // 子仍在跑 → 不动。
    let parked_live = ReconcileInputs { any_child_live: true, ..parked_no_live };
    assert_eq!(classify_reconcile(parked_live), ReconcileAction::Skip);

    // ConvergeParked 的 DB 收敛效果（既有原语，幂等）。
    let s = store();
    s.create_session("parent", "P", "1", false).unwrap();
    s.create_child_session(
        "child", "parent", "tc-a", "explorer", "任务", None, None, false, "1", None,
    )
    .unwrap();
    s.set_awaiting_subagent("parent", "child", "2").unwrap();
    let healed = s
        .cancel_dangling_dispatches("parent", "dispatch_agent", "3")
        .unwrap();
    assert_eq!(
        healed.iter().map(|(t, _)| t.clone()).collect::<Vec<_>>(),
        vec!["tc-a".to_string()]
    );
    assert!(s
        .get_session("parent")
        .unwrap()
        .unwrap()
        .awaiting_subagent
        .is_none());
}

#[test]
fn session_agent_and_role_roundtrip() {
    let s = store();
    s.create_session("sess", "S", "1", false).expect("create");
    // 默认没有实体归属，也没有运行角色。
    let got = s.get_session("sess").unwrap().unwrap();
    assert!(got.project_id.is_none());
    assert!(got.agent_id.is_none());
    assert!(got.role_kind.is_none());
    assert!(got.role_id.is_none());

    // 智能体是会话归属实体，不能再混在 role 字段里。
    s.set_agent_id("sess", Some("agent-dev"), "2")
        .expect("set agent");
    let got = s.get_session("sess").unwrap().unwrap();
    assert_eq!(got.agent_id.as_deref(), Some("agent-dev"));
    assert!(got.role_kind.is_none());
    assert!(got.role_id.is_none());

    // 专家/团队是运行角色定义，独立于项目/智能体归属实体。
    s.set_role("sess", Some("team"), Some("team-trade"), "3")
        .expect("set role");
    let got = s.get_session("sess").unwrap().unwrap();
    assert_eq!(got.agent_id.as_deref(), Some("agent-dev"));
    assert_eq!(got.role_kind.as_deref(), Some("team"));
    assert_eq!(got.role_id.as_deref(), Some("team-trade"));

    s.set_agent_id("sess", None, "4").expect("clear agent");
    s.set_role("sess", None, None, "5").expect("clear role");
    let got = s.get_session("sess").unwrap().unwrap();
    assert!(got.agent_id.is_none());
    assert!(got.role_kind.is_none());
    assert!(got.role_id.is_none());
}

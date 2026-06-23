// T70 会话任务队列：pending_tasks 字段往返 + 纯队列逻辑 + 排空判定。
use silicon_agent::session::task_queue::{
    cap_for, decide_enqueue, drain_decision, parse_queue, remove_queued, serialize_queue,
    DrainAction, EnqueueOutcome, SessionTaskItem, TaskKind, TaskStatus, MAIN_CAP, MEMBER_CAP,
};
use silicon_agent::session::SessionStore;
use silicon_agent::storage::AppDatabase;
use std::sync::Arc;

fn item(id: &str, kind: TaskKind, status: TaskStatus) -> SessionTaskItem {
    SessionTaskItem {
        item_id: id.into(),
        kind,
        payload: format!("payload-{id}"),
        tool_call_id: None,
        parent_session_id: None,
        status,
        enqueued_at: "1".into(),
    }
}

fn store() -> SessionStore {
    use std::time::{SystemTime, UNIX_EPOCH};
    // 用进程级原子计数器保证并行测试各得独立 DB 路径（避免 pid+nanos 在粗时钟下碰撞共享库）。
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let p = std::env::temp_dir().join(format!(
        "siw-task-queue-{}-{}-{}.db",
        std::process::id(),
        seq,
        nanos
    ));
    let _ = std::fs::remove_file(&p);
    let db = Arc::new(AppDatabase::open(&p).expect("open"));
    SessionStore::open(db).expect("store")
}

#[test]
fn pending_tasks_defaults_none_and_roundtrips() {
    let s = store();
    let info = s.create_session("sess", "S", "1", false).expect("create");
    assert!(
        info.pending_tasks.is_none(),
        "新建会话 pending_tasks 默认 None"
    );
    assert!(s.get_pending_tasks("sess").expect("get").is_none());

    s.set_pending_tasks("sess", Some("[]"), "2").expect("set");
    assert_eq!(s.get_pending_tasks("sess").unwrap().as_deref(), Some("[]"));
    assert_eq!(
        s.get_session("sess")
            .unwrap()
            .unwrap()
            .pending_tasks
            .as_deref(),
        Some("[]")
    );

    s.set_pending_tasks("sess", None, "3").expect("clear");
    assert!(s.get_pending_tasks("sess").unwrap().is_none());
}

#[test]
fn queue_serde_roundtrips() {
    let items = vec![
        item("a", TaskKind::UserMessage, TaskStatus::Running),
        item("b", TaskKind::UserMessage, TaskStatus::Queued),
    ];
    let json = serialize_queue(&items).expect("some");
    assert!(json.contains("\"kind\":\"user_message\""));
    assert!(json.contains("\"status\":\"running\""));
    let back = parse_queue(Some(&json));
    assert_eq!(back, items);
    // 空数组序列化为 None（清空语义），坏 JSON / None 解析为空 vec。
    assert!(serialize_queue(&[]).is_none());
    assert!(parse_queue(None).is_empty());
    assert!(parse_queue(Some("not json")).is_empty());
}

#[test]
fn capacity_is_by_session_type() {
    assert_eq!(cap_for(false), MAIN_CAP);
    assert_eq!(cap_for(true), MEMBER_CAP);
    assert_eq!((MAIN_CAP, MEMBER_CAP), (10, 3));
}

#[test]
fn enqueue_decision_idle_busy_overflow() {
    // 空闲（无 running 队头）→ 立即提升。
    assert_eq!(decide_enqueue(&[], MAIN_CAP), EnqueueOutcome::PromoteNow);
    // 有 running 队头、未满 → 排队。
    let one = vec![item("a", TaskKind::UserMessage, TaskStatus::Running)];
    assert_eq!(decide_enqueue(&one, MAIN_CAP), EnqueueOutcome::Queued);
    // 成员上限 3：running 队头 + 2 queued = 3，再来一个溢出。
    let full = vec![
        item("a", TaskKind::AgentTask, TaskStatus::Running),
        item("b", TaskKind::AgentTask, TaskStatus::Queued),
        item("c", TaskKind::AgentTask, TaskStatus::Queued),
    ];
    assert_eq!(decide_enqueue(&full, MEMBER_CAP), EnqueueOutcome::Overflow);
}

#[test]
fn drain_decision_is_polymorphic_by_kind_and_reason() {
    use TaskKind::*;
    assert_eq!(drain_decision(UserMessage, "paused"), DrainAction::Noop);
    assert_eq!(drain_decision(UserMessage, "parked"), DrainAction::Noop);
    assert_eq!(
        drain_decision(UserMessage, "completed"),
        DrainAction::PopAndPromote
    );
    assert_eq!(
        drain_decision(AgentTask, "completed"),
        DrainAction::PopAndPromote
    );
    // 失败按 kind 分流：agent_task 排空全回错；user_message halt-and-hold。
    assert_eq!(drain_decision(AgentTask, "failed"), DrainAction::DrainAll);
    assert_eq!(
        drain_decision(UserMessage, "failed"),
        DrainAction::HaltAndHold
    );
    // 取消两类同处理：排空。
    assert_eq!(
        drain_decision(UserMessage, "cancelled"),
        DrainAction::DrainAll
    );
    assert_eq!(
        drain_decision(AgentTask, "cancelled"),
        DrainAction::DrainAll
    );
}

#[test]
fn remove_queued_only_targets_queued_items() {
    let mut items = vec![
        item("a", TaskKind::UserMessage, TaskStatus::Running),
        item("b", TaskKind::UserMessage, TaskStatus::Queued),
    ];
    // 不能移除 running 队头。
    assert!(!remove_queued(&mut items, "a"));
    assert_eq!(items.len(), 2);
    // 可移除 queued 项。
    assert!(remove_queued(&mut items, "b"));
    assert_eq!(items.len(), 1);
    // 不存在的 id 返回 false。
    assert!(!remove_queued(&mut items, "zzz"));
}

use silicon_agent::session::task_queue::{
    drain_after_finish, enqueue_into_store, DrainNext, EnqueueResult,
};

fn new_item(id: &str, payload: &str) -> SessionTaskItem {
    SessionTaskItem {
        item_id: id.into(),
        kind: TaskKind::UserMessage,
        payload: payload.into(),
        tool_call_id: None,
        parent_session_id: None,
        status: TaskStatus::Queued,
        enqueued_at: "1".into(),
    }
}

#[test]
fn enqueue_promotes_when_idle_then_queues_when_busy() {
    let s = store();
    s.create_session("sess", "S", "1", false).expect("create");

    // 空闲：第一条立即提升（队头落库为 running）。
    let r = enqueue_into_store(&s, "sess", new_item("i1", "first"), false, "2").expect("enq1");
    match r {
        EnqueueResult::Promote(it) => assert_eq!(it.item_id, "i1"),
        _ => panic!("空闲应提升"),
    }
    let q = parse_queue(s.get_pending_tasks("sess").unwrap().as_deref());
    assert_eq!(q.len(), 1);
    assert_eq!(q[0].status, TaskStatus::Running);

    // 忙：第二条排队。
    let r = enqueue_into_store(&s, "sess", new_item("i2", "second"), false, "3").expect("enq2");
    assert!(matches!(r, EnqueueResult::Queued));
    let q = parse_queue(s.get_pending_tasks("sess").unwrap().as_deref());
    assert_eq!(q.len(), 2);
    assert_eq!(q[1].status, TaskStatus::Queued);
}

#[test]
fn enqueue_overflow_on_main_cap_plus_one() {
    let s = store();
    s.create_session("sess", "S", "1", false).expect("create");
    // 1 running + 9 queued = MAIN_CAP(10)；第 11 个溢出。
    for n in 0..MAIN_CAP {
        let r = enqueue_into_store(&s, "sess", new_item(&format!("i{n}"), "x"), false, "2")
            .expect("enq");
        if n == 0 {
            assert!(matches!(r, EnqueueResult::Promote(_)));
        } else {
            assert!(matches!(r, EnqueueResult::Queued));
        }
    }
    let r = enqueue_into_store(&s, "sess", new_item("overflow", "x"), false, "2").expect("enq");
    assert!(matches!(r, EnqueueResult::Overflow));
    assert_eq!(
        parse_queue(s.get_pending_tasks("sess").unwrap().as_deref()).len(),
        MAIN_CAP
    );
}

#[test]
fn drain_completed_pops_head_and_promotes_next() {
    let s = store();
    s.create_session("sess", "S", "1", false).expect("create");
    enqueue_into_store(&s, "sess", new_item("i1", "a"), false, "2").expect("e1");
    enqueue_into_store(&s, "sess", new_item("i2", "b"), false, "3").expect("e2");

    // 队头 i1 completed → 弹出，提升 i2。
    let next = drain_after_finish(&s, "sess", "completed", "4").expect("drain");
    match next {
        DrainNext::Promote(it) => assert_eq!(it.item_id, "i2"),
        _ => panic!("应提升 i2"),
    }
    let q = parse_queue(s.get_pending_tasks("sess").unwrap().as_deref());
    assert_eq!(q.len(), 1);
    assert_eq!(q[0].item_id, "i2");
    assert_eq!(q[0].status, TaskStatus::Running);

    // i2 completed 且队空 → 清空、Idle。
    let next = drain_after_finish(&s, "sess", "completed", "5").expect("drain2");
    assert!(matches!(next, DrainNext::Idle));
    assert!(s.get_pending_tasks("sess").unwrap().is_none());
}

#[test]
fn drain_failed_user_message_halts_and_holds_rest() {
    let s = store();
    s.create_session("sess", "S", "1", false).expect("create");
    enqueue_into_store(&s, "sess", new_item("i1", "a"), false, "2").expect("e1");
    enqueue_into_store(&s, "sess", new_item("i2", "b"), false, "3").expect("e2");

    // i1 失败（run 崩溃）→ 弹崩溃这条、保留 i2 为 queued、不自动提升。
    let next = drain_after_finish(&s, "sess", "failed", "4").expect("drain");
    assert!(matches!(next, DrainNext::Idle), "halt-and-hold 不自动续跑");
    let q = parse_queue(s.get_pending_tasks("sess").unwrap().as_deref());
    assert_eq!(q.len(), 1);
    assert_eq!(q[0].item_id, "i2");
    assert_eq!(q[0].status, TaskStatus::Queued, "保留为 queued，等用户决定");

    // 之后再有提交：无 running 队头 → 提升残留的更早项 i2（FIFO 恢复排空）。
    let r = enqueue_into_store(&s, "sess", new_item("i3", "c"), false, "5").expect("e3");
    match r {
        EnqueueResult::Promote(it) => assert_eq!(it.item_id, "i2", "恢复时先跑更早的 i2"),
        _ => panic!("应提升残留队头"),
    }
}

#[test]
fn drain_cancelled_clears_whole_queue() {
    let s = store();
    s.create_session("sess", "S", "1", false).expect("create");
    enqueue_into_store(&s, "sess", new_item("i1", "a"), false, "2").expect("e1");
    enqueue_into_store(&s, "sess", new_item("i2", "b"), false, "3").expect("e2");
    let next = drain_after_finish(&s, "sess", "cancelled", "4").expect("drain");
    assert!(matches!(next, DrainNext::Idle));
    assert!(
        s.get_pending_tasks("sess").unwrap().is_none(),
        "取消排空整个队列"
    );
}

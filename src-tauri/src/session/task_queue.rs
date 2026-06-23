//! 会话任务队列（session task queue，T70）：每个 session 背后的 FIFO 邮箱原语。
//!
//! 本模块只承载队列的**数据格式与纯状态迁移判定**：入队该排队/提升/溢出、收尾该弹头/续跑/
//! halt/清空。真正的 run 启动与 tool_call 回填由 `run::coordinator` 编排——把这里的判定结果
//! 翻译成引擎调用。这样"忙时入队、收尾排空"的核心逻辑可脱离 AppHandle/线程独立测试。
//!
//! 多态：`user_message`（人驱动，无 tool_call）与 `agent_task`（模型派发，带 tool_call 回填）。
//! 本期（T70）只完整接入 `user_message` 消费者；`agent_task` 的数据模型与排空判定在此就位，
//! 其落地回填由 T68 消费。

use serde::{Deserialize, Serialize};

use crate::session::SessionStore;

/// 主 session 队列上限：人驱动，宽上限避免拒收用户手打消息。
pub const MAIN_CAP: usize = 10;
/// 成员会话队列上限：模型驱动，紧上限防 runaway 连发。
pub const MEMBER_CAP: usize = 3;

/// 工作项类型。`user_message`=对话下一轮（无回填）；`agent_task`=派发任务（回填其 tool_call）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskKind {
    UserMessage,
    AgentTask,
}

/// 工作项状态。队头 `Running`=在飞；其余 `Queued`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Running,
    Queued,
}

/// 单个队列工作项；序列化为 `SessionInfo.pending_tasks` JSON 数组的一个元素。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionTaskItem {
    pub item_id: String,
    pub kind: TaskKind,
    /// `user_message`=消息内容；`agent_task`=任务描述（含 inputs 上游产物渲染）。
    pub payload: String,
    /// `agent_task` 回填用（成员侧，父=派发方）；`user_message` 无。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_session_id: Option<String>,
    pub status: TaskStatus,
    pub enqueued_at: String,
}

/// 入队判定：相对当前队列与容量，新工作应如何处置。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnqueueOutcome {
    /// 空闲（无在飞队头）→ 应立即提升运行队头。
    PromoteNow,
    /// 有在飞队头、未满 → 排到队尾等待。
    Queued,
    /// 有在飞队头且已达容量 → 溢出拒绝。
    Overflow,
}

/// 收尾排空判定（多态：按 kind × run 终态）。见 spec §4.3。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrainAction {
    /// completed：弹队头并提升下一个（drain 续）。
    PopAndPromote,
    /// failed(agent_task) / cancelled：排空整个队列（agent_task 逐条回错由 coordinator 处理）。
    DrainAll,
    /// failed(user_message)：弹崩溃这条、保留其余 queued、停止自动排空。
    HaltAndHold,
    /// paused / parked：不动队列，等就地续跑回到收尾。
    Noop,
}

/// 解析队列 JSON；None / 坏 JSON 视为空队列（不让损坏数据阻断会话）。
pub fn parse_queue(raw: Option<&str>) -> Vec<SessionTaskItem> {
    raw.and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default()
}

/// 序列化队列；空队列返回 None（对应清空 pending_tasks 列）。
pub fn serialize_queue(items: &[SessionTaskItem]) -> Option<String> {
    if items.is_empty() {
        None
    } else {
        serde_json::to_string(items).ok()
    }
}

/// 按 session 类型取容量：成员会话紧、主 session 宽。
/// 本期 `is_member` = `parent_session_id` 非空（子会话）；T68 再细化为 resident 成员。
pub fn cap_for(is_member: bool) -> usize {
    if is_member {
        MEMBER_CAP
    } else {
        MAIN_CAP
    }
}

/// 队头是否在飞。
pub fn head_running(items: &[SessionTaskItem]) -> bool {
    items
        .first()
        .map(|i| i.status == TaskStatus::Running)
        .unwrap_or(false)
}

/// 启动恢复：丢弃一个**幽灵 Running 队头**（进程被 kill 残留：队头标 Running 但其 run 线程随进程
/// 消亡、无在飞 run 来 `drain`，导致后续消息永久入队）。弹掉它后若仍有排队项，则把新队头标 Running
/// 返回供提升运行；否则返回 None（队列空）。**调用方须先确认**：队头确为 Running、该会话当前无在飞 run、
/// 且非合法停泊（`awaiting_subagent`/`pending_collect`，那由统一 `reconcile` 的停泊分支处理）。
/// 非 Running 队头时不动、返回 None。
pub fn reset_orphaned_head(items: &mut Vec<SessionTaskItem>) -> Option<SessionTaskItem> {
    if !head_running(items) {
        return None;
    }
    items.remove(0);
    if let Some(next) = items.first_mut() {
        next.status = TaskStatus::Running;
        Some(next.clone())
    } else {
        None
    }
}

/// 入队判定：仅当"有在飞队头且长度 ≥ cap"才溢出；无在飞队头一律提升（含 halt 残留的更早项）。
pub fn decide_enqueue(items: &[SessionTaskItem], cap: usize) -> EnqueueOutcome {
    if !head_running(items) {
        EnqueueOutcome::PromoteNow
    } else if items.len() >= cap {
        EnqueueOutcome::Overflow
    } else {
        EnqueueOutcome::Queued
    }
}

/// 收尾排空判定：按 kind × reason 多态收口。
pub fn drain_decision(kind: TaskKind, reason: &str) -> DrainAction {
    match reason {
        "paused" | "parked" => DrainAction::Noop,
        "completed" => DrainAction::PopAndPromote,
        "failed" => match kind {
            TaskKind::AgentTask => DrainAction::DrainAll,
            TaskKind::UserMessage => DrainAction::HaltAndHold,
        },
        "cancelled" => DrainAction::DrainAll,
        _ => DrainAction::Noop,
    }
}

/// 移除一个 `queued` 项（按 item_id）；running 队头与不存在的 id 一律拒绝（返回 false）。
pub fn remove_queued(items: &mut Vec<SessionTaskItem>, item_id: &str) -> bool {
    if let Some(pos) = items
        .iter()
        .position(|i| i.item_id == item_id && i.status == TaskStatus::Queued)
    {
        items.remove(pos);
        true
    } else {
        false
    }
}

/// 入队结果：coordinator 据此决定是否立刻起 run / 报溢出 / 仅入队。
pub enum EnqueueResult {
    /// 应立即提升运行该队头（已落库为 running）。
    Promote(SessionTaskItem),
    /// 已入队等待（队头在飞）。
    Queued,
    /// 容量溢出，未入队。
    Overflow,
}

/// 排空续跑结果。
pub enum DrainNext {
    /// 提升运行该项（已落库为 running）。
    Promote(SessionTaskItem),
    /// 队列已空 / halt-and-hold / 已排空，无后续自动续跑。
    Idle,
}

/// 入队一个工作项（读 pending_tasks → 判定 → 写回）。
/// 调用方须在 coordinator 的 `task_queue_lock` 下调用，保证 read-modify-write 原子。
pub fn enqueue_into_store(
    store: &SessionStore,
    session_id: &str,
    mut item: SessionTaskItem,
    is_member: bool,
    now: &str,
) -> Result<EnqueueResult, String> {
    let mut items = parse_queue(store.get_pending_tasks(session_id)?.as_deref());
    match decide_enqueue(&items, cap_for(is_member)) {
        EnqueueOutcome::Overflow => Ok(EnqueueResult::Overflow),
        EnqueueOutcome::Queued => {
            item.status = TaskStatus::Queued;
            items.push(item);
            store.set_pending_tasks(session_id, serialize_queue(&items).as_deref(), now)?;
            Ok(EnqueueResult::Queued)
        }
        EnqueueOutcome::PromoteNow => {
            // 入队后提升当前**队头**（可能是 halt 残留的更早项），保证 FIFO。
            item.status = TaskStatus::Queued;
            items.push(item);
            items[0].status = TaskStatus::Running;
            let head = items[0].clone();
            store.set_pending_tasks(session_id, serialize_queue(&items).as_deref(), now)?;
            Ok(EnqueueResult::Promote(head))
        }
    }
}

/// run 收尾排空：按队头 kind × reason 多态收口，返回下一个要跑的项（如有）。
/// `DrainAll` 在此清空 pending_tasks；agent_task 的逐条 tool_call 回错由 coordinator 在清空前完成。
pub fn drain_after_finish(
    store: &SessionStore,
    session_id: &str,
    reason: &str,
    now: &str,
) -> Result<DrainNext, String> {
    let mut items = parse_queue(store.get_pending_tasks(session_id)?.as_deref());
    let Some(head) = items.first().cloned() else {
        return Ok(DrainNext::Idle);
    };
    match drain_decision(head.kind, reason) {
        DrainAction::Noop => Ok(DrainNext::Idle),
        DrainAction::PopAndPromote => {
            items.remove(0);
            if let Some(next) = items.first_mut() {
                next.status = TaskStatus::Running;
                let n = next.clone();
                store.set_pending_tasks(session_id, serialize_queue(&items).as_deref(), now)?;
                Ok(DrainNext::Promote(n))
            } else {
                store.set_pending_tasks(session_id, None, now)?;
                Ok(DrainNext::Idle)
            }
        }
        DrainAction::HaltAndHold => {
            // 弹崩溃这条；其余保留为 queued（已是 queued），不自动提升。
            items.remove(0);
            store.set_pending_tasks(session_id, serialize_queue(&items).as_deref(), now)?;
            Ok(DrainNext::Idle)
        }
        DrainAction::DrainAll => {
            store.set_pending_tasks(session_id, None, now)?;
            Ok(DrainNext::Idle)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, status: TaskStatus) -> SessionTaskItem {
        SessionTaskItem {
            item_id: id.into(),
            kind: TaskKind::UserMessage,
            payload: id.into(),
            tool_call_id: None,
            parent_session_id: None,
            status,
            enqueued_at: "1".into(),
        }
    }

    #[test]
    fn reset_orphaned_head_pops_and_promotes_next() {
        // 幽灵 Running 队头（进程被杀残留）+ 后续排队项 → 弹掉队头、提升下一项为 Running。
        let mut items = vec![
            item("dead", TaskStatus::Running),
            item("next", TaskStatus::Queued),
            item("tail", TaskStatus::Queued),
        ];
        let promoted = reset_orphaned_head(&mut items);
        assert_eq!(promoted.as_ref().map(|i| i.item_id.as_str()), Some("next"));
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].item_id, "next");
        assert_eq!(items[0].status, TaskStatus::Running);
        assert_eq!(items[1].status, TaskStatus::Queued);
    }

    #[test]
    fn reset_orphaned_head_clears_when_only_head() {
        // 只有幽灵队头 → 弹掉后队列空、无提升项（截图场景：kill 时队列只有在飞那一条）。
        let mut items = vec![item("dead", TaskStatus::Running)];
        assert!(reset_orphaned_head(&mut items).is_none());
        assert!(items.is_empty());
    }

    #[test]
    fn reset_orphaned_head_noop_when_head_not_running() {
        // 队头非 Running（无幽灵）→ 不动、返回 None，避免误弹正常排队。
        let mut items = vec![item("q", TaskStatus::Queued)];
        assert!(reset_orphaned_head(&mut items).is_none());
        assert_eq!(items.len(), 1);
    }
}

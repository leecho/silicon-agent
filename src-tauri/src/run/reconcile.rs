//! 统一收敛器：把任一会话收敛到静止态（idle）。纯分类 + RunCoordinator 编排分离。
//!
//! 收敛到静止态语义：被中断的 run 一律收口为「已中断」、会话回到可交互 idle，由用户再发消息继续。
//! 绝不自动续跑、不启动下一个串行子。被四处复用：启动对账、看门狗、panic 收尾、停止。

/// reconcile 输入快照（由 coordinator 采集，便于纯函数判定）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReconcileInputs {
    /// 自身有活租约（心跳新鲜）。
    pub is_live: bool,
    /// 名下有活子租约。
    pub any_child_live: bool,
    /// `awaiting_subagent` 或 `pending_collect` 非空（停泊）。
    pub awaiting_or_collect: bool,
    pub stop_requested: bool,
    /// `engine.pending_interaction` 为 `Some(Ask/Plan/Permission)`（合法暂停）。
    pub pending_interaction: bool,
    /// 有悬空 tool_call 但非交互（崩溃中途）。
    pub dangling_noninteraction: bool,
    /// 任务队列队头 `Running`（幽灵队头）。
    pub head_running: bool,
}

/// reconcile 决策。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReconcileAction {
    /// 活 run / 合法停泊 / 合法暂停 / 已干净 → 不动。
    Skip,
    /// 分支 B：收子 + 解冻父。
    ConvergeParked,
    /// 分支 C：被停止的暂停态 → 收口交互。
    SettlePaused,
    /// 分支 A：本该在跑却无活线程（崩溃/panic/挂死/幽灵队头）。
    ConvergeOrphan,
}

/// 纯判定：给定快照决定收敛动作。详见 spec §3。
pub fn classify_reconcile(i: ReconcileInputs) -> ReconcileAction {
    if i.is_live {
        return ReconcileAction::Skip;
    }
    if i.awaiting_or_collect {
        return if i.any_child_live {
            ReconcileAction::Skip // 合法停泊：子确在跑
        } else {
            ReconcileAction::ConvergeParked
        };
    }
    if i.pending_interaction {
        return if i.stop_requested {
            ReconcileAction::SettlePaused
        } else {
            ReconcileAction::Skip // 合法暂停，等用户决策
        };
    }
    if i.head_running || i.dangling_noninteraction {
        return ReconcileAction::ConvergeOrphan;
    }
    ReconcileAction::Skip
}

use crate::app_state::now_string;
use crate::run::coordinator::{emit_run_event, RunCoordinator};
use crate::session::task_queue::{head_running, parse_queue, serialize_queue, settle_queue};

impl RunCoordinator {
    /// 采集快照 → 分类 → 执行收敛。租约即互斥：拿不到运行锁(有活 run / 别处在 reconcile)则跳过。
    /// 收敛到静止态：绝不续跑、不启动下一个串行子。幂等。
    pub(crate) fn reconcile(&self, sid: &str) {
        let Some(_guard) = self.run_registry.try_begin(sid) else {
            return; // 有活 run 或并发 reconcile 持锁 → 跳过
        };
        let Ok(Some(info)) = self.session.get_session(sid) else {
            return;
        };
        let Some(inputs) = self.collect_reconcile_inputs(sid, &info) else {
            return;
        };
        match classify_reconcile(inputs) {
            ReconcileAction::Skip => {}
            ReconcileAction::ConvergeParked => self.converge_parked(sid, &info),
            ReconcileAction::SettlePaused => self.converge_paused(sid),
            ReconcileAction::ConvergeOrphan => self.converge_orphan(sid),
        }
    }

    /// 统一停止（spec §5）：置 stop_requested + 级联子 + reconcile 收敛。
    /// - 非运行中的子（暂停/待跑）当场 reconcile；
    /// - 运行中的子由其取消退出时（见 spawn_child_run 的 cancelled 分支）收敛父；
    /// - 活动运行中的父由其 run 循环在检查点自停（落「已手动停止」标记）后收尾，这里 reconcile 会被 is_running 跳过。
    pub fn stop(&self, sid: &str) {
        self.cancel_flag(sid)
            .store(true, std::sync::atomic::Ordering::Relaxed);
        let children = self.session.list_children(sid).unwrap_or_default();
        for c in &children {
            if c.origin == "subagent" {
                self.cancel_flag(&c.id)
                    .store(true, std::sync::atomic::Ordering::Relaxed);
            }
        }
        for c in &children {
            if c.origin == "subagent" && !self.run_registry.is_running(&c.id) {
                self.reconcile(&c.id);
            }
        }
        // 任务台账：把本线程下未结束的任务（PM 自办「汇总」、未派发子任务、主任务）标为已取消（沿用旧 stop_children）。
        let _ = self.projects.cancel_pending_tasks(sid);
        emit_run_event(&self.app, "tasks_updated", sid, None);
        self.reconcile(sid);
    }

    /// 看门狗一拍：回收过期租约 + 找停泊孤儿 → 逐个 reconcile（spec §4 ②）。
    pub(crate) fn watchdog_tick(&self) {
        let stale = self
            .run_registry
            .stale_sessions(crate::run::watchdog::RUN_STALE_TIMEOUT_MS);
        for sid in &stale {
            self.run_registry.reclaim(sid); // 先回收 → reconcile 才能 try_begin；僵尸线程 token 失效
        }
        let parked_orphans = self.find_parked_orphans();
        let todo = crate::run::watchdog::sessions_to_reconcile(stale, parked_orphans);
        for sid in todo {
            self.reconcile(&sid);
        }
    }

    /// 停泊父中：名下无活子租约、又非自身在跑的 → 进程内孤儿（子线程死/未起）。
    fn find_parked_orphans(&self) -> Vec<String> {
        let Ok(sessions) = self.session.list_sessions() else {
            return Vec::new();
        };
        sessions
            .into_iter()
            .filter(|s| {
                s.parent_session_id.is_none()
                    && (s.awaiting_subagent.is_some() || s.pending_collect.is_some())
                    && !self.run_registry.is_running(&s.id)
                    && self
                        .session
                        .list_children(&s.id)
                        .map(|cs| {
                            cs.iter().all(|c| {
                                !self.run_registry.is_running(&c.id)
                                    && !(c.run_outcome.is_none()
                                        && self.child_has_pending_interaction(&c.id))
                            })
                        })
                        .unwrap_or(false)
            })
            .map(|s| s.id)
            .collect()
    }

    /// 启动对账：内存租约必空 = 无活线程。遍历所有顶层会话收敛到静止态。
    /// 取代 reconcile_orphaned_subagents + reconcile_orphaned_queue_heads。
    pub(crate) fn reconcile_all(&self) {
        let Ok(sessions) = self.session.list_sessions() else {
            return;
        };
        for s in sessions {
            if s.parent_session_id.is_some() {
                continue; // 子由父 ConvergeParked 递归收敛
            }
            self.reconcile(&s.id);
        }
    }

    /// 子会话是否正合法停在某个等待用户决策的交互（权限/Ask/计划）上。
    /// 这类「暂停」不是死孤儿：父须继续停泊等用户处理，reconcile 不得把它当孤儿收口。
    /// 崩溃中途的悬空非交互 tool_call 不算（pending_interaction 仅对需确认/Ask/计划返回 Some）。
    fn child_has_pending_interaction(&self, child_id: &str) -> bool {
        self.engine_builder
            .engine(child_id)
            .ok()
            .and_then(|e| e.pending_interaction(child_id).ok())
            .flatten()
            .map(|p| {
                matches!(
                    p,
                    crate::engine::PendingInteraction::Ask(_)
                        | crate::engine::PendingInteraction::Plan(_)
                        | crate::engine::PendingInteraction::Permission(_)
                )
            })
            .unwrap_or(false)
    }

    fn collect_reconcile_inputs(
        &self,
        sid: &str,
        info: &crate::session::SessionInfo,
    ) -> Option<ReconcileInputs> {
        let awaiting_or_collect =
            info.awaiting_subagent.is_some() || info.pending_collect.is_some();
        // 「活子」既包括有活租约的在跑子，也包括正合法停在权限/Ask/计划上等用户决策的子——
        // 后者不是死孤儿，父应继续停泊等其被处理，否则会被误收口（用户报的「playwright 暂停后被中断」）。
        let any_child_live = self.session.list_children(sid).ok()?.iter().any(|c| {
            self.run_registry.is_running(&c.id)
                || (c.run_outcome.is_none() && self.child_has_pending_interaction(&c.id))
        });
        let pend = self
            .engine_builder
            .engine(sid)
            .ok()
            .and_then(|e| e.pending_interaction(sid).ok())
            .flatten();
        let pending_interaction = matches!(
            pend,
            Some(crate::engine::PendingInteraction::Ask(_))
                | Some(crate::engine::PendingInteraction::Plan(_))
                | Some(crate::engine::PendingInteraction::Permission(_))
        );
        let dangling = self.session.first_dangling_tool_call(sid).ok().flatten();
        let dangling_noninteraction = dangling.is_some() && !pending_interaction;
        let head_run = head_running(&parse_queue(info.pending_tasks.as_deref()));
        Some(ReconcileInputs {
            is_live: false, // 已持 _guard，说明无其它活 run
            any_child_live,
            awaiting_or_collect,
            stop_requested: self
                .cancel_flag(sid)
                .load(std::sync::atomic::Ordering::Relaxed),
            pending_interaction,
            dangling_noninteraction,
            head_running: head_run,
        })
    }

    /// 分支 B：父停泊收敛——收子（并行=全部未终结；串行=在跑那个+待跑那些）+ 回填 + 清停泊 + 标记。
    fn converge_parked(&self, parent: &str, info: &crate::session::SessionInfo) {
        let now = now_string();
        // collect 停泊：交既有 advance（其检到 cancel_flag 会清 pending_collect+awaiting）。
        if info.pending_collect.is_some() {
            let _ = self.advance_pending_collect(parent);
        }
        // dispatch 停泊：回填悬空 dispatch（既有原语，幂等）。
        let _ = self.session.cancel_dangling_dispatches(
            parent,
            crate::tools::dispatch_agent::DISPATCH_AGENT_TOOL,
            &now,
        );
        // 收子：仍未终结的子（含串行待跑）→ 收口其自身悬空交互 + 标 cancelled + 通知前端清子冒泡卡。
        if let Ok(children) = self.session.list_children(parent) {
            for c in children {
                if c.origin != "subagent" || c.run_outcome.is_some() {
                    continue;
                }
                if !self.run_registry.is_running(&c.id) {
                    if let Ok(Some((tc, _))) = self.session.first_dangling_tool_call(&c.id) {
                        let _ = self.session.settle_pending_tool_call(
                            &c.id,
                            &tc,
                            "会话已停止，未执行该操作。",
                            &now,
                        );
                    }
                }
                let _ = self.session.set_run_outcome(&c.id, "cancelled", &now);
                emit_run_event(&self.app, "run_finished", &c.id, Some("cancelled"));
            }
        }
        self.session.append_interrupted_marker(parent, &now);
        // T91 P1-T4：队头复位走共享纯函数 settle_queue(Interrupted)（弹幽灵 Running 队头、不续跑），
        // 单一来源 = settle_session 的同一判定。converge_parked 的悬空 tool_call 用 dispatch 专用回填
        // （cancel_dangling_dispatches，含逐子摘要），与 settle_session 的通用收口语义不同，故只折叠队头部分，
        // 保留父/子收敛逻辑独立（不整体委派 settle_session，避免重复/冲突收口父的 dispatch tool_call）。
        {
            let _lock = self.task_queue_lock.lock().unwrap();
            let mut items = parse_queue(
                self.session
                    .get_pending_tasks(parent)
                    .ok()
                    .flatten()
                    .as_deref(),
            );
            let _ = settle_queue(&mut items, crate::session::task_queue::SettleOutcome::Interrupted);
            let payload = serialize_queue(&items);
            let payload = if items.is_empty() {
                None
            } else {
                payload.as_deref()
            };
            let _ = self.session.set_pending_tasks(parent, payload, &now);
        }
        emit_run_event(&self.app, "run_finished", parent, Some("cancelled"));
    }

    /// 分支 C：被停止的暂停态——收口悬空交互 tool_call + 标记。
    fn converge_paused(&self, sid: &str) {
        // T91：经唯一收口点（补上原先缺失的队头复位——停止一个暂停态的 run 也会留 Running 队头）。
        let _ = self.settle_session(sid, crate::session::task_queue::SettleOutcome::Cancelled);
    }

    /// 分支 A：本该在跑却无活线程——收口悬空非交互 tool_call + 解冻队头 + 标记。
    /// T91 P1-T4：委派唯一收口点。`settle_session(Interrupted)` 同义且更收敛——
    /// 收口悬空 tool_call("上一轮因进程退出未完成。") + interrupted 标记 + 弹幽灵 Running 队头
    /// + queued_tasks_updated + run_finished("cancelled")，与原 converge_orphan 逐项等价
    /// （唯一差异：原 reset_session_queue_head 会把下一项标 Running 但不续跑、留下幽灵队头；
    /// settle_queue(Interrupted) 只弹队头、余项保持 Queued，更符合「收敛到静止、绝不续跑」）。
    fn converge_orphan(&self, sid: &str) {
        let _ = self.settle_session(sid, crate::session::task_queue::SettleOutcome::Interrupted);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> ReconcileInputs {
        ReconcileInputs {
            is_live: false,
            any_child_live: false,
            awaiting_or_collect: false,
            stop_requested: false,
            pending_interaction: false,
            dangling_noninteraction: false,
            head_running: false,
        }
    }

    #[test]
    fn live_session_is_never_touched() {
        let i = ReconcileInputs {
            is_live: true,
            head_running: true,
            ..base()
        };
        assert_eq!(classify_reconcile(i), ReconcileAction::Skip);
    }

    #[test]
    fn parked_with_live_child_stays_parked() {
        let i = ReconcileInputs {
            awaiting_or_collect: true,
            any_child_live: true,
            ..base()
        };
        assert_eq!(classify_reconcile(i), ReconcileAction::Skip);
    }

    #[test]
    fn parked_with_no_live_child_converges() {
        let i = ReconcileInputs {
            awaiting_or_collect: true,
            any_child_live: false,
            ..base()
        };
        assert_eq!(classify_reconcile(i), ReconcileAction::ConvergeParked);
    }

    #[test]
    fn paused_waits_unless_stopped() {
        let i = ReconcileInputs {
            pending_interaction: true,
            ..base()
        };
        assert_eq!(classify_reconcile(i), ReconcileAction::Skip);
        let stopped = ReconcileInputs {
            pending_interaction: true,
            stop_requested: true,
            ..base()
        };
        assert_eq!(classify_reconcile(stopped), ReconcileAction::SettlePaused);
    }

    #[test]
    fn orphan_head_or_dangling_converges() {
        assert_eq!(
            classify_reconcile(ReconcileInputs {
                head_running: true,
                ..base()
            }),
            ReconcileAction::ConvergeOrphan
        );
        assert_eq!(
            classify_reconcile(ReconcileInputs {
                dangling_noninteraction: true,
                ..base()
            }),
            ReconcileAction::ConvergeOrphan
        );
    }

    #[test]
    fn clean_idle_is_noop() {
        assert_eq!(classify_reconcile(base()), ReconcileAction::Skip);
    }
}

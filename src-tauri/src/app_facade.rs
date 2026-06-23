use std::path::PathBuf;
use std::sync::Arc;

use crate::app_state::now_string;
use crate::engine::EngineBuilder;
use crate::run::RunCoordinator;
use crate::session::SessionStore;

/// 应用编排门面：把跨 session 的运行时编排聚合在一处，组合 `EngineBuilder`（纯构造）
/// 与 `RunCoordinator`（run 生命周期 + 子代理编排）。AppState 持有它作为命令层入口，自身退化为纯容器。
pub struct AppFacade {
    session: Arc<SessionStore>,
    engine_builder: Arc<EngineBuilder>,
    coordinator: Arc<RunCoordinator>,
    workspace_base: PathBuf,
}

impl AppFacade {
    pub fn new(
        session: Arc<SessionStore>,
        engine_builder: Arc<EngineBuilder>,
        coordinator: Arc<RunCoordinator>,
        workspace_base: PathBuf,
    ) -> Self {
        Self {
            session,
            engine_builder,
            coordinator,
            workspace_base,
        }
    }

    /// 删除会话的默认沙箱目录（含 attachments/）。仅当未显式设置 working_dir 时执行——
    /// 用户显式指定的工作目录是其自有内容，绝不删除。删除会话时调用，避免草稿/会话目录堆积。
    pub fn remove_default_workspace(&self, session_id: &str) -> Result<(), String> {
        let wd = self.session.get_working_dir(session_id)?;
        if wd.as_deref().map(|s| !s.trim().is_empty()).unwrap_or(false) {
            return Ok(());
        }
        let dir = self.workspace_base.join("sessions").join(session_id);
        if dir.exists() {
            std::fs::remove_dir_all(&dir).map_err(|e| format!("删除会话目录失败：{e}"))?;
        }
        Ok(())
    }

    /// 读取会话详情并据持久化重建 pending 交互（reload 后恢复权限卡 / Ask 卡）。
    ///
    /// pending 是运行期临时态、不持久化；刷新/重开 app 时由引擎从悬空 tool_call 重建，
    /// 否则暂停在风险工具或 ask_user 的会话刷新后会丢失确认卡、用户卡住。
    pub fn session_with_pending(
        &self,
        session_id: &str,
    ) -> Result<Option<crate::session::Session>, String> {
        let Some(mut detail) = self.session.get_session_detail(session_id)? else {
            return Ok(None);
        };
        match self
            .engine_builder
            .engine(session_id)?
            .pending_interaction(session_id)?
        {
            Some(crate::engine::PendingInteraction::Permission(p)) => {
                detail.pending_permission = Some(p)
            }
            Some(crate::engine::PendingInteraction::Ask(a)) => detail.pending_ask = Some(a),
            Some(crate::engine::PendingInteraction::Plan(p)) => detail.pending_plan = Some(p),
            None => {}
        }
        detail.resolved_working_dir = self
            .engine_builder
            .resolve_session_workspace(session_id)?
            .to_string_lossy()
            .into_owned();
        detail.is_running = self.coordinator.run_registry().is_running(session_id);
        detail.session.is_running = detail.is_running;
        detail.session.run_started_at = self.coordinator.run_registry().run_started_at(session_id);
        Ok(Some(detail))
    }

    /// 为定时任务构建引擎：在常规 engine 基础上标记 headless（权限/模型已写入会话）。
    pub fn engine_for_task(&self, session_id: &str) -> Result<crate::engine::Engine, String> {
        Ok(self.engine_builder.engine(session_id)?.with_headless(true))
    }

    /// 列某父会话的专家（child 子运行）+ 计算状态，供右侧面板展示。
    pub fn session_children(
        &self,
        parent_id: &str,
    ) -> Result<Vec<crate::session::ChildAgentSummary>, String> {
        let mut out = Vec::new();
        for c in self.session.list_children(parent_id)? {
            // running：child run 在跑；否则以 child 自身的 run_outcome 为准（done/failed/cancelled）——
            // **不**看父会话的 dispatch 结果：后台派发会即时写占位结果，否则会把仍在等权限/提问的 child
            // 误判为已完成。无终态且不在跑 → paused（等用户确认/回答/纠偏）。
            let status = if self.coordinator.run_registry().is_running(&c.id) {
                "running".to_string()
            } else {
                match c
                    .run_outcome
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                {
                    Some(o) => o.to_string(),
                    None => "paused".to_string(),
                }
            };
            // 轮次键：产出该专家 dispatch 调用的 assistant 消息 id；定位不到则回退 session_id（独占一轮）。
            let round_id = match &c.parent_tool_call_id {
                Some(tc) => self
                    .session
                    .find_dispatch_round(parent_id, tc)?
                    .unwrap_or_else(|| c.id.clone()),
                None => c.id.clone(),
            };
            let expert_name = c.expert_name.unwrap_or_default();
            out.push(crate::session::ChildAgentSummary {
                session_id: c.id,
                expert_name,
                task: c.agent_task.unwrap_or_default(),
                status,
                created_at: c.created_at,
                round_id,
                display_name: None,
                profession: None,
                avatar: None,
            });
        }
        Ok(out)
    }

    /// 新建一个远程会话（非草稿，来源标记 remote），返回会话 id。供远程接入 /new 与首条消息建会话。
    pub fn create_remote_session(&self) -> Result<String, String> {
        let id = crate::session::new_id("session");
        let now = now_string();
        self.session.create_session(&id, "远程会话", &now, false)?;
        let _ = self.session.set_session_origin(&id, "remote");
        Ok(id)
    }

}


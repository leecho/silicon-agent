use std::path::PathBuf;
use std::sync::Arc;

use tauri::Emitter;

use crate::app_settings::AppSettingsStore;
use crate::provider::{ProviderGateway, ProviderStore};
use crate::session::SessionStore;
use crate::storage::AppDatabase;
use crate::tools::add_artifact::AddArtifact;
use crate::tools::ask_user::AskUser;
use crate::tools::command_tool::CommandExecute;
use crate::tools::fs_tools::{EditFile, ReadFile, WriteFile};
use crate::tools::install_skill::InstallSkill;
use crate::tools::load_skill::LoadSkill;
use crate::tools::propose_plan::ProposePlan;
use crate::tools::search_tools::{Glob, Grep};
use crate::tools::update_todos::UpdateTodos;
use crate::tools::web_fetch::WebFetch;
use crate::tools::web_search::WebSearch;
use crate::tools::ToolRegistry;

/// 引擎构造器（纯构造，无可变状态）：根据 session 解析工作目录/技能，构造 `Engine`。
/// 所有依赖以 `Arc` / 不可变值持有，可安全跨线程克隆。
pub struct EngineBuilder {
    pub(crate) db: Arc<AppDatabase>,
    pub(crate) provider: Arc<ProviderStore>,
    pub(crate) gateway: Arc<ProviderGateway>,
    pub(crate) session: Arc<SessionStore>,
    pub(crate) workspace_base: PathBuf,
    pub(crate) app: tauri::AppHandle,
    pub(crate) skills: Arc<crate::skill::SkillService>,
    pub(crate) remote_hub: Arc<crate::remote::RemoteHub>,
}

impl EngineBuilder {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        db: Arc<AppDatabase>,
        provider: Arc<ProviderStore>,
        gateway: Arc<ProviderGateway>,
        session: Arc<SessionStore>,
        workspace_base: PathBuf,
        app: tauri::AppHandle,
        skills: Arc<crate::skill::SkillService>,
        remote_hub: Arc<crate::remote::RemoteHub>,
    ) -> Self {
        Self {
            db,
            provider,
            gateway,
            session,
            workspace_base,
            app,
            skills,
            remote_hub,
        }
    }

    /// 构建工具 registry（仅内置工具，沙箱根为传入的 workspace）。
    pub(crate) fn build_registry(&self, workspace: PathBuf) -> ToolRegistry {
        register_builtin_tools(workspace, self.skills.clone())
    }

    /// 解析某会话的工作目录（沙箱根），不创建目录（供展示/构建引擎）。
    pub(crate) fn resolve_session_workspace(&self, session_id: &str) -> Result<PathBuf, String> {
        let wd = self.session.get_working_dir(session_id)?;
        Ok(resolve_workspace(
            wd.as_deref(),
            &self.workspace_base,
            session_id,
        ))
    }

    /// 解析并确保会话工作目录存在（run 启动前调用，惰性创建）。
    pub(crate) fn ensure_session_workspace(&self, session_id: &str) -> Result<PathBuf, String> {
        let ws = self.resolve_session_workspace(session_id)?;
        std::fs::create_dir_all(&ws).map_err(|err| format!("create session workspace: {err}"))?;
        Ok(ws)
    }

    /// 构建带流式 emitter 与工具 registry 的引擎（沙箱根按 session 解析）。
    pub(crate) fn engine(&self, session_id: &str) -> Result<crate::engine::Engine, String> {
        let workspace = self.resolve_session_workspace(session_id)?;
        let workspace_str = workspace.to_string_lossy().into_owned();
        let registry = self.build_registry(workspace.clone());
        let app = self.app.clone();
        let hub = self.remote_hub.clone();
        let session = crate::session::SessionStore::open(self.db.clone())?;
        // 解析本会话模型选择：会话选过 → 用之（失效自动回退默认）；未选 → 默认。无可用模型则 None,
        // 调用时由 Gateway 报「未配置可用模型」。
        let selected_id = self.session.get_selected_model_id(session_id)?;
        let selection = self.provider.resolve_selection(selected_id.as_deref()).ok();
        Ok(crate::engine::Engine::new(session, self.gateway.clone())
            .with_app_settings(AppSettingsStore::open(self.db.clone())?)
            .with_workspace(workspace_str)
            .with_registry(registry)
            .with_skills(self.skills.clone())
            .with_usage(crate::usage::UsageStore::open(self.db.clone())?)
            .with_selection(selection)
            .with_emitter(std::sync::Arc::new(move |event| {
                // 多路分发：本地前端事件不变；远程按 session 绑定路由（无绑定零开销）。
                let _ = app.emit("agent_stream_event", event.clone());
                hub.on_event(event);
            })))
    }
}

/// 注册全部内置工具到一个新 registry（引擎工具白名单的唯一事实源）。
///
/// 唯一性：这是引擎暴露给模型的内置工具全集。新增/删除内置工具只改这里，
/// `tests::builtin_tool_whitelist_is_locked` 会据此守卫白名单不被意外扩张（如重新引入
/// dispatch_agent/create_expert/remember 等已裁剪的工具）。
fn register_builtin_tools(
    workspace: PathBuf,
    skills: Arc<crate::skill::SkillService>,
) -> ToolRegistry {
    let ws = workspace;
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(ReadFile {
        workspace: ws.clone(),
    }));
    registry.register(Arc::new(WriteFile {
        workspace: ws.clone(),
    }));
    registry.register(Arc::new(EditFile {
        workspace: ws.clone(),
    }));
    registry.register(Arc::new(Glob {
        workspace: ws.clone(),
    }));
    registry.register(Arc::new(Grep {
        workspace: ws.clone(),
    }));
    registry.register(Arc::new(CommandExecute {
        workspace: ws.clone(),
    }));
    registry.register(Arc::new(WebSearch::new()));
    registry.register(Arc::new(WebFetch));
    registry.register(Arc::new(AskUser));
    registry.register(Arc::new(LoadSkill));
    registry.register(Arc::new(crate::tools::read_skill_file::ReadSkillFile));
    registry.register(Arc::new(UpdateTodos));
    registry.register(Arc::new(AddArtifact));
    registry.register(Arc::new(ProposePlan));
    registry.register(Arc::new(InstallSkill {
        workspace: ws,
        skills,
    }));
    registry
}

/// 解析某会话的工作目录（沙箱根）：显式选过且非空白用其值；否则 base/sessions/{session_id}。
fn resolve_workspace(
    working_dir: Option<&str>,
    base: &std::path::Path,
    session_id: &str,
) -> std::path::PathBuf {
    match working_dir {
        Some(dir) if !dir.trim().is_empty() => std::path::PathBuf::from(dir),
        _ => base.join("sessions").join(session_id),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn resolve_workspace_uses_explicit_then_falls_back_to_base_session() {
        use std::path::Path;
        let base = Path::new("/home/u/.siliconagent");
        // 显式选择 → 原样。
        assert_eq!(
            super::resolve_workspace(Some("/work/proj"), base, "session-1"),
            std::path::PathBuf::from("/work/proj")
        );
        // 未选（None）→ base/sessions/{session_id}。
        assert_eq!(
            super::resolve_workspace(None, base, "session-1"),
            base.join("sessions").join("session-1")
        );
        // 空白串等同未选。
        assert_eq!(
            super::resolve_workspace(Some("   "), base, "session-1"),
            base.join("sessions").join("session-1")
        );
    }

    /// 守卫：内置工具白名单锁定。
    ///
    /// 用真实的 `register_builtin_tools` 注册全集，再从每个工具的 `spec().name`（即各工具
    /// `name()` 的真实返回值）取名集合，断言裁剪后应在的工具全在、已删除的工具一个不剩。
    /// 若有人重新注册 dispatch_agent / create_expert / remember 等已裁剪工具，此测试即失败。
    #[test]
    fn builtin_tool_whitelist_is_locked() {
        use std::collections::BTreeSet;
        use std::sync::Arc;

        // 真实 SkillService（InstallSkill 需要它），用临时 DB + 临时根目录。
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let dir = std::env::temp_dir().join(format!("sa-whitelist-{suffix}"));
        let db = Arc::new(
            crate::storage::AppDatabase::open(dir.join("test.db")).expect("open test db"),
        );
        let skills = Arc::new(crate::skill::SkillService::new(db, dir.join("skills")));

        let registry =
            super::register_builtin_tools(std::path::PathBuf::from("/tmp/ws"), skills);
        // 从真实 spec().name 取名集合（非硬编码字符串）。
        let names: BTreeSet<String> = registry.specs().into_iter().map(|s| s.name).collect();

        const PRESENT: &[&str] = &[
            "read_file",
            "write_file",
            "edit_file",
            "glob",
            "grep",
            // 工具实例 CommandExecute 的真实 name() 为 run_command。
            "run_command",
            "web_search",
            "web_fetch",
            "ask_user",
            "load_skill",
            "read_skill_file",
            "install_skill",
            "update_todos",
            "add_artifact",
            "propose_plan",
        ];
        const ABSENT: &[&str] = &[
            "dispatch_agent",
            "collect_agents",
            "create_expert",
            "create_team",
            "install_plugin",
            "propose_soul_update",
            "remember",
            "update_tasks",
        ];

        for want in PRESENT {
            assert!(
                names.contains(*want),
                "内置工具白名单缺少 {want}（当前: {names:?}）"
            );
        }
        for banned in ABSENT {
            assert!(
                !names.contains(*banned),
                "已裁剪工具 {banned} 被重新注册（当前: {names:?}）"
            );
        }
        // 精确锁定：白名单恰为 PRESENT，未来新增工具需显式更新此测试。
        assert_eq!(
            names.len(),
            PRESENT.len(),
            "内置工具数量与白名单不符（当前: {names:?}）"
        );
    }
}

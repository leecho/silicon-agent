use std::path::PathBuf;
use std::sync::Arc;

use tauri::{Emitter, Manager};

use crate::app_settings::AppSettingsStore;
use crate::memory::MemoryStore;
use crate::provider::{ProviderGateway, ProviderStore};
use crate::session::SessionStore;
use crate::storage::AppDatabase;
use crate::tools::add_artifact::AddArtifact;
use crate::tools::ask_user::AskUser;
use crate::tools::command_tool::CommandExecute;
use crate::tools::dispatch_agent::DispatchAgent;
use crate::tools::fs_tools::{EditFile, ReadFile, WriteFile};
use crate::tools::install_expert::InstallExpert;
use crate::tools::install_plugin::InstallPlugin;
use crate::tools::install_skill::InstallSkill;
use crate::tools::install_team::InstallTeam;
use crate::tools::load_skill::LoadSkill;
use crate::tools::propose_plan::ProposePlan;
use crate::tools::remember::Remember;
use crate::tools::fs_search::{Glob, Grep};
use crate::tools::update_todos::UpdateTodos;
use crate::tools::web_fetch::WebFetch;
use crate::tools::web_search::WebSearch;
use crate::tools::ToolRegistry;

use crate::app_state::AppState;

/// 引擎构造器（纯构造，无可变状态）：根据 session 解析角色/工作目录/技能，构造 `Engine`
/// （主运行 + child 子运行）。所有依赖以 `Arc` / 不可变值持有，可安全跨线程克隆。
pub struct EngineBuilder {
    pub(crate) db: Arc<AppDatabase>,
    pub(crate) provider: Arc<ProviderStore>,
    pub(crate) gateway: Arc<ProviderGateway>,
    pub(crate) session: Arc<SessionStore>,
    pub(crate) app_settings: Arc<AppSettingsStore>,
    pub(crate) workspace_base: PathBuf,
    pub(crate) app: tauri::AppHandle,
    pub(crate) skills: Arc<crate::skill::SkillService>,
    pub(crate) experts: Arc<crate::expert::ExpertService>,
    pub(crate) agents: Arc<crate::agent::AgentService>,
    pub(crate) plugins: Arc<crate::plugin::PluginService>,
    pub(crate) teams: Arc<crate::team::TeamService>,
    pub(crate) projects: Arc<crate::project::ProjectService>,
    pub(crate) mcp: Arc<crate::mcp::McpService>,
    pub(crate) remote_hub: Arc<crate::remote::RemoteHub>,
    /// T66：plugin hooks 注册表（注入引擎，工具/会话生命周期点触发）。
    pub(crate) hooks: Arc<crate::hook::HookService>,
    /// T92：app 级常驻浏览器（跨 run/跨会话复用同一 Chrome）。注入 Browser 工具，取代每 run 新建 CdpController。
    pub(crate) shared_browser: Arc<crate::browser::shared::SharedBrowser>,
}

impl EngineBuilder {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        db: Arc<AppDatabase>,
        provider: Arc<ProviderStore>,
        gateway: Arc<ProviderGateway>,
        session: Arc<SessionStore>,
        app_settings: Arc<AppSettingsStore>,
        workspace_base: PathBuf,
        app: tauri::AppHandle,
        skills: Arc<crate::skill::SkillService>,
        experts: Arc<crate::expert::ExpertService>,
        agents: Arc<crate::agent::AgentService>,
        plugins: Arc<crate::plugin::PluginService>,
        teams: Arc<crate::team::TeamService>,
        projects: Arc<crate::project::ProjectService>,
        mcp: Arc<crate::mcp::McpService>,
        remote_hub: Arc<crate::remote::RemoteHub>,
        hooks: Arc<crate::hook::HookService>,
        shared_browser: Arc<crate::browser::shared::SharedBrowser>,
    ) -> Self {
        Self {
            db,
            provider,
            gateway,
            session,
            app_settings,
            workspace_base,
            app,
            skills,
            experts,
            agents,
            plugins,
            teams,
            projects,
            mcp,
            remote_hub,
            hooks,
            shared_browser,
        }
    }

    /// 构建工具 registry（内置工具 + MCP 代理，沙箱根为传入的 workspace）。
    pub(crate) fn build_registry(&self, workspace: PathBuf, session_id: &str) -> ToolRegistry {
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
        registry.register(Arc::new(crate::tools::find_tools::FindTools));
        registry.register(Arc::new(crate::tools::read_skill_file::ReadSkillFile));
        registry.register(Arc::new(UpdateTodos));
        registry.register(Arc::new(crate::tools::update_tasks::UpdateTasks));
        registry.register(Arc::new(AddArtifact));
        registry.register(Arc::new(Remember));
        registry.register(Arc::new(crate::tools::search_knowledge::SearchKnowledge));
        registry.register(Arc::new(ProposePlan));
        registry.register(Arc::new(
            crate::tools::propose_soul_update::ProposeSoulUpdate,
        ));
        // 派生专家（控制工具，引擎拦截）。child registry 会过滤掉它，禁递归（§6.6）。
        registry.register(Arc::new(DispatchAgent));
        registry.register(Arc::new(crate::tools::collect_agents::CollectAgents));
        registry.register(Arc::new(InstallSkill {
            workspace: ws.clone(),
            skills: self.skills.clone(),
        }));
        registry.register(Arc::new(InstallPlugin {
            workspace: ws.clone(),
            plugins: self.plugins.clone(),
        }));
        // AI 创建：登记散装专家 / 团队（写持久状态，需用户确认）。
        registry.register(Arc::new(InstallExpert {
            agents: self.experts.clone(),
        }));
        registry.register(Arc::new(InstallTeam {
            teams: self.teams.clone(),
        }));
        // MCP 代理工具：来自已连接 server 的动态工具集（registry 每次构建，无需注销机制）。
        for proxy in self.mcp.tool_proxies() {
            registry.register(proxy);
        }
        // 桌面操作工具（T84）：macOS / Windows 注册（控制器为各自平台专属；
        // 其余平台如 Linux 上该工具缺席）。
        // 披露 Deferred → 只在桌面会话激活；risk Safe → 不触发逐动作确认（授权门是桌面会话本身）。
        #[cfg(target_os = "macos")]
        let computer_backend: Arc<dyn crate::desktop::DesktopController> =
            Arc::new(crate::desktop::macos::MacosController);
        #[cfg(target_os = "windows")]
        let computer_backend: Arc<dyn crate::desktop::DesktopController> =
            Arc::new(crate::desktop::windows::WindowsController);
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        registry.register(Arc::new(crate::tools::computer::Computer::new(
            computer_backend,
        )));
        // T85 浏览器操作工具：macOS + Windows（CDP 跨平台，OS 差异仅 Chrome 探测；Linux 见 P3）。
        // 比 computer 多覆盖面：CDP 连 Windows 也只换 Chrome 路径，无需 OS 专属后端。
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            // 旧：CdpController::new(...) 每 run 新建（run 结束关窗）。
            // 新：注入 app 级常驻 SharedBrowser（跨 run/跨会话复用同一 Chrome）。
            // headless 现在在 SharedBrowser 工厂内读取；download_dir 由 P1-T3 命令覆盖到会话 workspace。
            registry.register(Arc::new(crate::tools::browser::Browser::new(
                self.shared_browser.clone(),
                ws.clone(),
                session_id.to_string(),
            )));
        }
        // T90 macOS 应用工具：日历/提醒（EventKit）+ 备忘录（osascript）。仅 macOS 注册；
        // 披露 Deferred → 经 find_tools 激活；risk 按 action 动态判定（delete 跟随权限模式）。
        #[cfg(target_os = "macos")]
        {
            registry.register(Arc::new(crate::tools::calendar::Calendar::new(Arc::new(
                crate::apple::calendar::EkCalendar::new(),
            ))));
            registry.register(Arc::new(crate::tools::reminders::Reminders::new(Arc::new(
                crate::apple::reminders::EkReminders::new(),
            ))));
            registry.register(Arc::new(crate::tools::notes::Notes::new(Arc::new(
                crate::apple::notes::OsaNotes::new(),
            ))));
        }
        registry
    }

    /// 解析某会话的工作目录（沙箱根），不创建目录（供展示/构建引擎）。
    pub(crate) fn resolve_session_workspace(&self, session_id: &str) -> Result<PathBuf, String> {
        // 子代理（origin=subagent）会话：自身未显式设工作目录时，继承父会话工作目录——
        // 与运行时（engine_for_child 用父 workspace）保持一致，使其展示/产物落在项目（父）目录，
        // 而非 base/sessions/{childId} 的空目录。
        if let Some(s) = self.session.get_session(session_id)? {
            let own_empty = s
                .working_dir
                .as_deref()
                .map(str::trim)
                .filter(|d| !d.is_empty())
                .is_none();
            if own_empty && s.origin == "subagent" {
                if let Some(parent) = s.parent_session_id.as_deref() {
                    return self.resolve_session_workspace(parent);
                }
            }
            // T69+：所属智能体且自身未设目录 → 默认用该智能体的专属工作目录（设了才用，否则落下方默认）。
            if own_empty {
                if let Some(rid) = s.agent_id.as_deref().filter(|x| !x.is_empty()) {
                    if let Ok(Some(a)) = self.agents.get_by_id(rid) {
                        if let Some(wd) = a
                            .working_dir
                            .as_deref()
                            .map(str::trim)
                            .filter(|d| !d.is_empty())
                        {
                            return Ok(resolve_workspace(
                                Some(wd),
                                &self.workspace_base,
                                session_id,
                            ));
                        }
                    }
                }
            }
        }
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
        // T61：编排线程（项目/团队）用任务台账 update_tasks，**不**给通用 update_todos（否则模型会
        // 默认用熟悉的 update_todos、计划落不进台账）；普通会话反之只给 update_todos。
        let orchestration = self
            .session
            .get_session(session_id)
            .ok()
            .flatten()
            .map(|s| s.project_id.is_some() || s.role_kind.as_deref() == Some("team"))
            .unwrap_or(false);
        // 编排线程是"在固定名册上做编排"：去掉 builder 工具（install_team/install_expert）——
        // 否则 PM 会去新建团队/专家污染流程；计划用 update_tasks，故也去掉 update_todos。
        let registry = if orchestration {
            self.build_registry(workspace.clone(), session_id)
                .without_name(crate::tools::update_todos::UPDATE_TODOS_TOOL)
                .without_name("install_team")
                .without_name("install_expert")
        } else {
            self.build_registry(workspace.clone(), session_id)
                .without_name(crate::tools::update_tasks::UPDATE_TASKS_TOOL)
        };
        let app = self.app.clone();
        let hub = self.remote_hub.clone();
        let session = crate::session::SessionStore::open(self.db.clone())?;
        // 解析本会话模型选择：会话选过 → 用之（失效自动回退默认）；未选 → 默认。无可用模型则 None,
        // 调用时由 Gateway 报「未配置可用模型」。
        let selected_id = self.session.get_selected_model_id(session_id)?;
        let selection = self.provider.resolve_selection(selected_id.as_deref()).ok();
        // 解析所选模型的 vision 能力（每模型覆盖 ∨ 内置查表）；无可用模型则 false。
        let supports_vision = selection
            .as_ref()
            .map(|r| self.provider.supports_vision_for(&r.model))
            .unwrap_or(false);
        // T57：后台 child 即时启动回调（捕获 AppHandle → spawn_child_run）。
        let spawn_app = self.app.clone();
        Ok(crate::engine::Engine::new(session, self.gateway.clone())
            .with_app_settings(AppSettingsStore::open(self.db.clone())?)
            .with_workspace(workspace_str)
            .with_registry(registry)
            .with_memory(MemoryStore::open(self.db.clone())?)
            .with_knowledge(std::sync::Arc::new(crate::knowledge::KnowledgeStore::open(self.db.clone())?))
            .with_embedder(std::sync::Arc::new(crate::knowledge::embed_gateway::GatewayEmbedder {
                gateway: self.gateway.clone(),
                model_id: self.app_settings.get_knowledge_embedding_model().unwrap_or_default(),
            }))
            .with_skills(self.skills.clone())
            .with_experts(self.experts.clone())
            .with_agents(self.agents.clone())
            .with_plugins(self.plugins.clone())
            .with_teams(self.teams.clone())
            .with_projects(self.projects.clone())
            .with_hooks(self.hooks.clone())
            .with_usage(crate::usage::UsageStore::open(self.db.clone())?)
            .with_selection(selection)
            .with_supports_vision(supports_vision)
            .with_child_spawner(std::sync::Arc::new(move |child_id: &str| {
                let st = spawn_app.state::<AppState>();
                if let Err(e) = st.coordinator.request_start_child_run(child_id) {
                    eprintln!("[agent] 后台 child 启动失败 child={child_id}：{e}");
                }
            }))
            .with_emitter(std::sync::Arc::new(move |event| {
                // 多路分发：本地前端事件不变；远程按 session 绑定路由（无绑定零开销）。
                let _ = app.emit("agent_stream_event", event.clone());
                hub.on_event(event);
            })))
    }

    /// 按会话角色槽解析一个声明式专家的展示摘要（team→成员；agent/none→散装）。不存在返回 None。
    pub(crate) fn resolve_role_summary(
        &self,
        role_kind: &str,
        role_id: &str,
        name: &str,
    ) -> Option<crate::expert::ExpertSummary> {
        match role_kind {
            "team" if !role_id.is_empty() => self
                .teams
                .detail(role_id)
                .ok()
                .and_then(|d| d.members.into_iter().find(|m| m.name == name)),
            // 项目成员：项目私有快照（从团队导入）或散装引用，统一经 ProjectService 解析。
            "project" if !role_id.is_empty() => self
                .projects
                .get_member_by_name(role_id, name)
                .ok()
                .flatten()
                .and_then(|m| self.projects.member_summary(&m)),
            // 散装：先按散装解析，查不到再全局按名兜底。
            _ => self
                .experts
                .summary_by_owner("", "", name)
                .or_else(|| self.experts.summary_by_name(name)),
        }
    }

    /// 按会话角色槽解析一个声明式专家的角色定义（team→roster 成员；agent/none→散装）。
    pub(crate) fn resolve_role_spec(
        &self,
        role_kind: &str,
        role_id: &str,
        name: &str,
    ) -> Result<Option<crate::expert::ExpertSpec>, String> {
        match role_kind {
            "team" if !role_id.is_empty() => {
                let (_lead, roster) = self.teams.resolve_for_run(role_id)?;
                Ok(roster.into_iter().find(|s| s.name == name))
            }
            // 项目成员：项目私有快照（从团队导入）或散装引用，统一经 ProjectService 解析。
            "project" if !role_id.is_empty() => Ok(self
                .projects
                .get_member_by_name(role_id, name)?
                .and_then(|m| self.projects.member_spec(&m))),
            // 自由模式：只解析**公开**专家（散装 + plugin 提供）。
            //
            // 这里过去兜底走 `load_spec_by_name`（裸 get_by_name、**无 owner 过滤**），
            // 意味着加载器本身能捞到 **team 私有专家** —— 今天只是被 control_tools 的闸门
            // （只认散装）挡着才没出事。闸门一旦按标准放开让 plugin agent 可派发，
            // 这里若不同步收紧，team 私有专家会立刻变成自由模式可派发，捅穿团队隔离。
            // 闸门与加载器必须同口径：`resolve_public_spec_by_name`（T108 P1）。
            _ => self.experts.resolve_public_spec_by_name(name),
        }
    }

    /// 为 child（origin="subagent"）会话构建受限引擎：按 agent.tools 过滤 registry（剔 dispatch_agent
    /// 防递归）、注入 agent system prompt、按 model_tier 选模型、共享父 workspace（§6.7）。
    pub(crate) fn engine_for_child(
        &self,
        child_session_id: &str,
    ) -> Result<crate::engine::Engine, String> {
        let info = self
            .session
            .get_session(child_session_id)?
            .ok_or("child 会话不存在")?;
        let expert_name = info.expert_name.clone().ok_or("child 缺 expert_name")?;
        let parent = info.parent_session_id.clone().ok_or("child 缺 parent")?;
        // ad-hoc（动态生成）专家：child 行直接带 inline spec，不查 ExpertService；
        // 否则回退到声明式专家（散装 .md / 未来 plugin）。
        let spec = match info
            .expert_system_prompt
            .clone()
            .filter(|s| !s.trim().is_empty())
        {
            Some(system_prompt) => {
                let tools = info
                    .expert_tools
                    .as_deref()
                    .unwrap_or("")
                    .split(',')
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>();
                crate::expert::ExpertSpec {
                    name: expert_name.clone(),
                    description: String::new(),
                    system_prompt,
                    tools,
                    model_tier: "main".to_string(),
                    max_turns: None,
                    role: "subagent".to_string(),
                    plugin_id: String::new(),
                    team_id: String::new(),
                }
            }
            None => {
                // 声明式：按父会话上下文解析（project/team→名册成员；其余→散装）。
                let (role_kind, role_id) = self
                    .session
                    .get_session(&parent)
                    .ok()
                    .flatten()
                    .map(|s| {
                        if let Some(project_id) = s.project_id {
                            ("project".to_string(), project_id)
                        } else {
                            (
                                s.role_kind.unwrap_or_default(),
                                s.role_id.unwrap_or_default(),
                            )
                        }
                    })
                    .unwrap_or_default();
                self.resolve_role_spec(&role_kind, &role_id, &expert_name)?
                    .ok_or_else(|| format!("agent 不存在：{expert_name}"))?
            }
        };
        // 共享父 workspace（§6.7）。
        let workspace = self.resolve_session_workspace(&parent)?;
        let workspace_str = workspace.to_string_lossy().into_owned();
        // 受限 registry：剔除 dispatch_agent（禁递归，§6.6）。
        // 未声明 tools（散装/AI 建 agent 常见）→ **全部工具开放**（仅去 dispatch）；声明了则按白名单过滤。
        let registry = if spec.tools.is_empty() {
            self.build_registry(workspace, child_session_id)
                .without_name(crate::tools::dispatch_agent::DISPATCH_AGENT_TOOL)
                .without_name(crate::tools::collect_agents::COLLECT_AGENTS_TOOL)
                .without_name(crate::tools::update_tasks::UPDATE_TASKS_TOOL)
                .without_name("install_team")
                .without_name("install_expert")
        } else {
            let tools = child_tool_whitelist(&spec.tools);
            self.build_registry(workspace, child_session_id)
                .filter_by_names(&tools)
        };
        // 模型分层：aux→辅助模型，否则父会话模型。
        let selection = match spec.model_tier.as_str() {
            "aux" => {
                let aux = self.app_settings.get_aux_model_id().ok().flatten();
                self.provider.resolve_selection(aux.as_deref()).ok()
            }
            _ => {
                let pid = self.session.get_selected_model_id(&parent)?;
                self.provider.resolve_selection(pid.as_deref()).ok()
            }
        };
        // child 子运行同样按所选模型 vision 能力展开/降级附件图片。
        let supports_vision = selection
            .as_ref()
            .map(|r| self.provider.supports_vision_for(&r.model))
            .unwrap_or(false);
        let app = self.app.clone();
        let hub = self.remote_hub.clone();
        let session = crate::session::SessionStore::open(self.db.clone())?;
        // 子运行来源标记：child 的每条事件注入 parent/agent，供前端路由到专家面板。
        let marker_parent = parent.clone();
        let marker_ptc = info.parent_tool_call_id.clone();
        let marker_agent = expert_name.clone();
        // 项目成员（快照）子运行：继承其源团队的私有 skill（owner=team_id），与 PM/lead 路径对称。
        let child_team_ids: Vec<String> = self
            .session
            .get_session(&parent)
            .ok()
            .flatten()
            .and_then(|p| p.project_id)
            .and_then(|pid| {
                self.projects
                    .member_origin_team_id(&pid, &expert_name)
                    .ok()
                    .flatten()
            })
            .into_iter()
            .collect();
        Ok(crate::engine::Engine::new(session, self.gateway.clone())
            .with_app_settings(AppSettingsStore::open(self.db.clone())?)
            .with_workspace(workspace_str)
            .with_registry(registry)
            .with_memory(MemoryStore::open(self.db.clone())?)
            .with_knowledge(std::sync::Arc::new(crate::knowledge::KnowledgeStore::open(self.db.clone())?))
            .with_embedder(std::sync::Arc::new(crate::knowledge::embed_gateway::GatewayEmbedder {
                gateway: self.gateway.clone(),
                model_id: self.app_settings.get_knowledge_embedding_model().unwrap_or_default(),
            }))
            .with_skills(self.skills.clone())
            .with_system_prompt_override(spec.system_prompt)
            .with_private_skills_expert(expert_name.clone())
            .with_private_skills_team_ids(child_team_ids)
            .with_hooks(self.hooks.clone())
            .with_selection(selection)
            .with_supports_vision(supports_vision)
            .with_usage(crate::usage::UsageStore::open(self.db.clone())?)
            .with_emitter(std::sync::Arc::new(move |mut event| {
                event.parent_session_id = Some(marker_parent.clone());
                event.parent_tool_call_id = marker_ptc.clone();
                event.expert_name = Some(marker_agent.clone());
                let _ = app.emit("agent_stream_event", event.clone());
                hub.on_event(event);
            })))
    }
}

/// 声明式子代理的工具白名单：去掉禁递归的派发/收集工具，并始终补上 find_tools，
/// 使白名单内的 Deferred 工具（web/MCP 等）能被子代理激活（T83 §5.2）。
fn child_tool_whitelist(declared: &[String]) -> Vec<String> {
    let mut tools: Vec<String> = declared
        .iter()
        .filter(|t| {
            t.as_str() != crate::tools::dispatch_agent::DISPATCH_AGENT_TOOL
                && t.as_str() != crate::tools::collect_agents::COLLECT_AGENTS_TOOL
        })
        .cloned()
        .collect();
    let find = crate::tools::find_tools::FIND_TOOLS_TOOL.to_string();
    if !tools.contains(&find) {
        tools.push(find);
    }
    tools
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
mod child_whitelist_tests {
    use super::*;

    #[test]
    fn whitelist_always_includes_find_tools() {
        let out = child_tool_whitelist(&["web_search".to_string()]);
        assert!(out.contains(&"web_search".to_string()));
        assert!(out.contains(&"find_tools".to_string()));
    }

    #[test]
    fn whitelist_strips_dispatch_and_collect_but_keeps_find_tools() {
        let out = child_tool_whitelist(&[
            "read_file".to_string(),
            "dispatch_agent".to_string(),
            "collect_agents".to_string(),
        ]);
        assert!(out.contains(&"read_file".to_string()));
        assert!(!out.contains(&"dispatch_agent".to_string()));
        assert!(!out.contains(&"collect_agents".to_string()));
        assert!(out.contains(&"find_tools".to_string()));
    }

    #[test]
    fn whitelist_does_not_duplicate_find_tools() {
        let out = child_tool_whitelist(&["find_tools".to_string()]);
        assert_eq!(out.iter().filter(|t| *t == "find_tools").count(), 1);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn resolve_workspace_uses_explicit_then_falls_back_to_base_session() {
        use std::path::Path;
        let base = Path::new("/home/u/.siliconworker");
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
}

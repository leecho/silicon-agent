use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use tauri::Manager;

use crate::app_settings::AppSettingsStore;
use crate::memory::MemoryStore;
use crate::provider::{ProviderGateway, ProviderStore};
use crate::session::SessionStore;
use crate::storage::AppDatabase;
use crate::usage::UsageStore;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RunOrigin {
    Local,
    Remote,
}

/// 全局应用服务容器。本切片持有数据库、Provider Store/Gateway 与 AppHandle；
/// 引擎按需经 `engine()` builder 构建并注入流式 emitter 与工具 registry。
pub struct AppState {
    pub db: Arc<AppDatabase>,
    /// 厂商/模型持久化 owner（CRUD、设置、解析）。
    pub provider: Arc<ProviderStore>,
    /// 模型调用网关（`dyn ModelClient`）：store 之上的调用行为。
    pub gateway: Arc<ProviderGateway>,
    pub session: Arc<SessionStore>,
    /// 长期记忆 owner（全局、跨会话）。与 session 正交，独立 `memories` 表与 schema。
    pub memory: Arc<MemoryStore>,
    /// 知识库 owner（用户导入的文档/分块/检索）。与 memory 正交，独立四表 schema。
    pub knowledge: Arc<crate::knowledge::KnowledgeStore>,
    /// app 全局配置存储（owner = `app_settings` 表）。
    pub app_settings: Arc<AppSettingsStore>,
    pub usage: Arc<UsageStore>,
    /// 模型调用日志 store（owner = model_call_log 表）。供命令层查询调用明细。
    pub call_log: Arc<crate::call_log::CallLogStore>,
    /// 默认工作目录基址（{home}/.siliconworker）。未显式选目录的会话沙箱根 = base/sessions/{session_id}。
    pub workspace_base: PathBuf,
    pub app: tauri::AppHandle,
    /// 引擎构造器（纯构造，无可变状态）：解析角色/工作目录/技能并构造 `Engine`。
    pub engine_builder: Arc<crate::engine::EngineBuilder>,
    /// 运行时编排：run 生命周期 + 子代理编排，拥有 cancel_flags / run_registry / child_retries。
    pub coordinator: Arc<crate::run::RunCoordinator>,
    /// 应用编排门面：聚合跨 session / project 的运行时编排，组合 engine_builder + coordinator。
    pub facade: crate::app_facade::AppFacade,
    /// 技能服务（文件型技能：索引 + 磁盘根 {workspace_base}/skills）。
    pub skills: std::sync::Arc<crate::skill::SkillService>,
    /// 专家服务（expert 角色模板：索引 + 磁盘根 {workspace_base}/experts）。
    /// 用户面「专家」。与 plugin/skill 正交。
    pub experts: std::sync::Arc<crate::expert::ExpertService>,
    /// 伴随体服务（agent 实例：软复制 expert 指令 + 引用其技能 + 私有记忆 + 跨会话身份）。用户面「智能体」。
    /// 表 `agents`（复用 T67 腾出的名）——**构造须在 `experts`(ExpertService) 之后**，详见 agent/service.rs。
    pub agents: std::sync::Arc<crate::agent::AgentService>,
    /// 插件服务（Claude 式能力包：索引 + 用户磁盘根 {workspace_base}/plugins，内置 {workspace_base}/builtin-plugins）。
    pub plugins: std::sync::Arc<crate::plugin::PluginService>,
    /// 团队服务（会话级编排：lead+members 引用 + 私有组件）。引用 agent/skill 货币，与 plugin 正交。
    pub teams: std::sync::Arc<crate::team::TeamService>,
    /// 市场服务（精选预置 agent/team/skill 来源 + 加入我的 + 市场增删启停）。
    pub markets: std::sync::Arc<crate::market::Markets>,
    /// 「我的」用户自定义分组服务。
    pub groups: std::sync::Arc<crate::group::GroupService>,
    /// 项目多专家协作空间服务（群聊 + 成员 + 任务板）。
    pub projects: std::sync::Arc<crate::project::ProjectService>,
    /// MCP client 子系统：连接外部 MCP server，把其 tools 映射为代理工具。
    pub mcp: std::sync::Arc<crate::mcp::McpService>,
    /// 定时任务存储。
    pub tasks: std::sync::Arc<crate::scheduler::TaskStore>,
    /// 保持系统唤醒：持有 guard 时阻止休眠。None = 关闭。
    pub keep_awake: std::sync::Mutex<Option<crate::scheduler::keepawake::KeepAwakeGuard>>,
    /// 远程接入持久化（白名单/绑定/channel 配置）。
    pub remote_store: std::sync::Arc<crate::remote::RemoteStore>,
    /// 远程枢纽：connector 注册 + 出站发送线程 + 引擎事件分发。
    pub remote_hub: std::sync::Arc<crate::remote::RemoteHub>,
    /// T92：app 级常驻浏览器（跨 run/跨会话复用同一 Chrome）。供 P1-T3 命令显式 close/状态/下载目录。
    pub shared_browser: std::sync::Arc<crate::browser::shared::SharedBrowser>,
}

impl AppState {
    pub fn open(handle: &tauri::AppHandle) -> Result<Self, String> {
        let dir = handle
            .path()
            .app_data_dir()
            .map_err(|err| format!("resolve app data dir: {err}"))?;
        let db = Arc::new(
            AppDatabase::open(dir.join("silicon-worker.sqlite3")).map_err(|err| err.to_string())?,
        );
        let provider = Arc::new(ProviderStore::open(db.clone(), dir.clone())?);
        let session = Arc::new(SessionStore::open(db.clone())?);
        let memory = Arc::new(MemoryStore::open(db.clone())?);
        let knowledge = Arc::new(crate::knowledge::KnowledgeStore::open(db.clone())?);
        let app_settings = Arc::new(AppSettingsStore::open(db.clone())?);
        let usage = Arc::new(UsageStore::open(db.clone())?);
        // 调用日志：store + 观察者，注入 gateway，使所有模型调用（主/子代理/标题/建议/压缩/策展）
        // 共享同一咽喉被记录（默认关，开关读 app_settings）。
        let call_log = Arc::new(crate::call_log::CallLogStore::open(db.clone())?);
        let call_log_observer = Arc::new(crate::call_log::CallLogObserver::new(
            call_log.clone(),
            app_settings.clone(),
        ));
        let gateway = Arc::new(ProviderGateway::with_observer(
            provider.clone(),
            call_log_observer,
        ));
        let workspace_base = default_workspace_base();
        std::fs::create_dir_all(&workspace_base)
            .map_err(|err| format!("create workspace base dir: {err}"))?;
        let skills_root = workspace_base.join("skills");
        let skills = std::sync::Arc::new(crate::skill::SkillService::new(db.clone(), skills_root));
        // 启动同步：物化内置 + 扫描 upsert + 清孤儿。失败不阻断启动，仅记录。
        if let Err(e) = skills.sync() {
            eprintln!("[skill] 启动同步失败：{e}");
        }
        // 插件服务：plugins（用户）+ builtin-plugins（内置，本期无内容）。在 skills.sync 之后同步，
        // 以便插件内 skill 写入同一 skills 表（带 plugin_id）。
        let plugins = std::sync::Arc::new(crate::plugin::PluginService::new(
            db.clone(),
            workspace_base.join("plugins"),
            workspace_base.join("builtin-plugins"),
        ));
        if let Err(e) = plugins.sync() {
            eprintln!("[plugin] 启动同步失败：{e}");
        }
        let experts_root = migrate_legacy_experts_root(&workspace_base)?;
        // 专家服务：独立目录 {workspace_base}/experts，独立索引表。与 plugin/skill 正交。
        let experts =
            std::sync::Arc::new(crate::expert::ExpertService::new(db.clone(), experts_root));
        if let Err(e) = experts.sync() {
            eprintln!("[expert] 启动同步失败：{e}");
        }
        // 伴随体服务（agent 实例）：表 `agents` 复用腾出的名，**必须在上面 ExpertService 之后构造**
        // ——先让 expert 的 ensure_schema 完成旧 agents→experts 改名，再建伴随体新 agents 表（T67/T69 顺序护栏）。
        let agents = std::sync::Arc::new(crate::agent::AgentService::new_with_workspace_base(
            db.clone(),
            workspace_base.clone(),
        ));
        // 团队服务：会话级编排，引用 agent/skill 货币。须在 experts 之后（解析成员引用）。
        // 导入的团队包复制进 {workspace_base}/teams 受管。
        let teams = std::sync::Arc::new(crate::team::TeamService::new(
            db.clone(),
            experts.clone(),
            workspace_base.join("teams"),
        ));
        // 市场（T109）：四个各自独立的市场 —— 插件/专家/团队来自官方静态仓，技能来自 SkillHub。
        let markets = std::sync::Arc::new(crate::market::Markets::new(
            db.clone(),
            workspace_base.join("market-cache"),
        ));
        // 「我的」用户自定义分组。
        let groups = std::sync::Arc::new(crate::group::GroupService::new(db.clone()));
        // 项目协作空间（成员引用 agent 货币）。
        let projects = std::sync::Arc::new(crate::project::ProjectService::new(
            db.clone(),
            experts.clone(),
        ));
        // MCP 子系统：表 + 密钥文件 + 连接管理。启动连接在拿到 AppHandle 后触发（见 lib.rs setup）。
        let mcp_store = crate::mcp::store::McpStore::new(db.clone(), dir.join("mcp.secrets.json"))?;
        let mcp = crate::mcp::McpService::new(mcp_store);
        // 把 agent/team 类型 plugin 内的 experts 索引进 experts 表（带 plugin_id 命名空间）。
        // 须在 plugins.sync（plugin 行就绪）+ experts.sync（散装就绪）之后；失败不阻断启动。
        {
            let now = now_string();
            for (plugin_id, plugin_dir, manifest) in
                plugins.list_agent_plugins().unwrap_or_default()
            {
                let mut names = Vec::new();
                for rel in &manifest.agents {
                    let abs = plugin_dir.join(rel);
                    match experts.index_plugin_expert(&plugin_id, &abs, &now) {
                        Ok(name) => names.push(name),
                        Err(e) => eprintln!("[plugin->agent] 索引失败 {}：{e}", abs.display()),
                    }
                }
                if let Err(e) = experts.clear_plugin_experts_except(&plugin_id, &names) {
                    eprintln!("[plugin->agent] 清理孤儿失败 plugin={plugin_id}：{e}");
                }
            }
        }
        let tasks = std::sync::Arc::new(crate::scheduler::TaskStore::open(db.clone())?);
        // T66：plugin hooks 注册表（进程内，随插件启停刷新）。注入引擎与门面。
        let hooks = std::sync::Arc::new(crate::hook::HookService::new());
        let remote_store = std::sync::Arc::new(crate::remote::RemoteStore::open(db.clone())?);
        let remote_hub = std::sync::Arc::new(crate::remote::RemoteHub::new(remote_store.clone()));
        // T92：app 级常驻浏览器，建一次、共享给 EngineBuilder（注入 Browser 工具）与 AppState（P1-T3 命令）。
        // 工厂懒建 CdpController，headless 每次按设置读取；默认下载基目录由 P1-T3 命令覆盖到会话 workspace。
        let shared_browser = {
            let app_settings_for_browser = app_settings.clone();
            // 默认下载基目录（per-run 工具会用 set_download_dir 覆盖到会话 workspace；P1-T3）。
            let default_browser_ws = workspace_base.join("browser-downloads");
            Arc::new(crate::browser::shared::SharedBrowser::new(move || {
                let headless = app_settings_for_browser
                    .get_browser_headless()
                    .unwrap_or(false);
                Ok(std::sync::Arc::new(crate::browser::cdp::CdpController::new(
                    default_browser_ws.clone(),
                    headless,
                ))
                    as std::sync::Arc<dyn crate::browser::BrowserController>)
            }))
        };
        // 引擎构造器（纯构造）：装配完 service 后建，注入 RunCoordinator。
        let engine_builder = Arc::new(crate::engine::EngineBuilder::new(
            db.clone(),
            provider.clone(),
            gateway.clone(),
            session.clone(),
            app_settings.clone(),
            workspace_base.clone(),
            handle.clone(),
            skills.clone(),
            experts.clone(),
            agents.clone(),
            plugins.clone(),
            teams.clone(),
            projects.clone(),
            mcp.clone(),
            remote_hub.clone(),
            hooks.clone(),
            shared_browser.clone(),
        ));
        // 运行时编排：持运行时状态（cancel_flags / run_registry / child_retries）+ run/子代理方法。
        let coordinator = Arc::new(crate::run::RunCoordinator::new(
            engine_builder.clone(),
            session.clone(),
            projects.clone(),
            handle.clone(),
            gateway.clone(),
            db.clone(),
            remote_hub.clone(),
            app_settings.clone(),
        ));
        // 应用编排门面：组合 engine_builder + coordinator，承载跨 session / project 的运行时编排。
        let facade = crate::app_facade::AppFacade::new(
            session.clone(),
            projects.clone(),
            engine_builder.clone(),
            coordinator.clone(),
            workspace_base.clone(),
            plugins.clone(),
            mcp.clone(),
            hooks.clone(),
        );
        // T66：plugin → MCP server + hooks 启动摄取。把启用插件声明的 MCP server upsert 进 McpStore
        // （带 plugin_id 标记、变量已解析）；连接交由 lib.rs setup 的 startup_connect_all 统一拉起。
        // hooks 同步进 HookService（随插件启停）。失败不阻断启动，仅记录。
        for (plugin_id, plugin_dir, manifest) in plugins.list_all_with_dir().unwrap_or_default() {
            if !manifest.mcp_servers.is_empty() {
                if let Err(e) = facade.sync_plugin_mcp(&plugin_id, &plugin_dir, &manifest, true) {
                    eprintln!("[plugin->mcp] 启动摄取失败 plugin={plugin_id}：{e}");
                }
            }
            if !manifest.hooks.is_empty() {
                facade.sync_plugin_hooks(&plugin_id, &plugin_dir, &manifest, true);
            }
        }
        Ok(Self {
            db,
            provider,
            gateway,
            session,
            memory,
            knowledge,
            app_settings,
            usage,
            call_log,
            workspace_base,
            app: handle.clone(),
            engine_builder,
            coordinator,
            facade,
            skills,
            experts,
            agents,
            plugins,
            teams,
            markets,
            groups,
            projects,
            mcp,
            tasks,
            keep_awake: std::sync::Mutex::new(None),
            remote_store,
            remote_hub,
            shared_browser,
        })
    }
}

/// 默认工作目录基址：{home}/.siliconworker。HOME 缺失时回退相对 .siliconworker。
fn default_workspace_base() -> std::path::PathBuf {
    match std::env::var_os("HOME") {
        Some(home) => std::path::PathBuf::from(home).join(".siliconworker"),
        None => std::path::PathBuf::from(".siliconworker"),
    }
}

/// T69+：旧版本把用户专家定义放在 `{workspace}/agents`。现在该目录留给智能体工作目录，
/// 仅迁移旧根里的顶层 `.md` 专家文件以及同名资产目录，避免误搬新智能体工作目录。
fn migrate_legacy_experts_root(workspace_base: &Path) -> Result<PathBuf, String> {
    let legacy = workspace_base.join("agents");
    let experts = workspace_base.join("experts");
    std::fs::create_dir_all(&experts)
        .map_err(|err| format!("create experts dir {}: {err}", experts.display()))?;
    if !legacy.is_dir() {
        return Ok(experts);
    }

    let mut moved_stems = Vec::new();
    let entries = std::fs::read_dir(&legacy)
        .map_err(|err| format!("read legacy experts dir {}: {err}", legacy.display()))?;
    for entry in entries {
        let entry = entry.map_err(|err| format!("read legacy experts entry: {err}"))?;
        let path = entry.path();
        let is_md = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.eq_ignore_ascii_case("md"))
            .unwrap_or(false);
        if !path.is_file() || !is_md {
            continue;
        }
        let Some(file_name) = path.file_name() else {
            continue;
        };
        let target = experts.join(file_name);
        if !target.exists() {
            std::fs::rename(&path, &target).map_err(|err| {
                format!(
                    "move legacy expert {} to {}: {err}",
                    path.display(),
                    target.display()
                )
            })?;
        }
        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            moved_stems.push(stem.to_string());
        }
    }

    for stem in moved_stems {
        let source = legacy.join(&stem);
        if !source.is_dir() {
            continue;
        }
        let target = experts.join(&stem);
        if target.exists() {
            continue;
        }
        std::fs::rename(&source, &target).map_err(|err| {
            format!(
                "move legacy expert assets {} to {}: {err}",
                source.display(),
                target.display()
            )
        })?;
    }
    let _ = std::fs::remove_dir(&legacy);
    Ok(experts)
}

/// 生成 now 时间戳（Unix epoch 秒，字符串），用于配置写入审计列。
pub(crate) fn now_string() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();
    seconds.to_string()
}

#[cfg(test)]
mod tests {
    use super::migrate_legacy_experts_root;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn migrates_legacy_expert_files_without_moving_agent_workdirs() {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!(
            "siw-app-state-migrate-{}-{}",
            std::process::id(),
            n
        ));
        let legacy = base.join("agents");
        std::fs::create_dir_all(legacy.join("writer").join("skills")).unwrap();
        std::fs::create_dir_all(legacy.join("agent-id-1")).unwrap();
        std::fs::write(legacy.join("writer.md"), "---\nname: writer\n---\nprompt").unwrap();
        std::fs::write(legacy.join("writer").join("skills").join("a.md"), "skill").unwrap();
        std::fs::write(legacy.join("agent-id-1").join("note.txt"), "agent work").unwrap();

        let experts = migrate_legacy_experts_root(&base).unwrap();

        assert_eq!(experts, base.join("experts"));
        assert!(base.join("experts").join("writer.md").is_file());
        assert!(base
            .join("experts")
            .join("writer")
            .join("skills")
            .join("a.md")
            .is_file());
        assert!(legacy.join("agent-id-1").join("note.txt").is_file());
        assert!(!legacy.join("writer.md").exists());

        let _ = std::fs::remove_dir_all(&base);
    }
}

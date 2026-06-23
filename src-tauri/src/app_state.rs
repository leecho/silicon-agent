use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use tauri::Manager;

use crate::app_settings::AppSettingsStore;
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
    /// app 全局配置存储（owner = `app_settings` 表）。
    pub app_settings: Arc<AppSettingsStore>,
    pub usage: Arc<UsageStore>,
    /// 模型调用日志 store（owner = model_call_log 表）。供命令层查询调用明细。
    pub call_log: Arc<crate::call_log::CallLogStore>,
    /// 默认工作目录基址（{home}/.siliconagent）。未显式选目录的会话沙箱根 = base/sessions/{session_id}。
    pub workspace_base: PathBuf,
    pub app: tauri::AppHandle,
    /// 引擎构造器（纯构造，无可变状态）：解析角色/工作目录/技能并构造 `Engine`。
    pub engine_builder: Arc<crate::engine::EngineBuilder>,
    /// 运行时编排：run 生命周期 + 子代理编排，拥有 cancel_flags / run_registry / child_retries。
    pub coordinator: Arc<crate::run::RunCoordinator>,
    /// 应用编排门面：聚合跨 session 的运行时编排，组合 engine_builder + coordinator。
    pub facade: crate::app_facade::AppFacade,
    /// 技能服务（文件型技能：索引 + 磁盘根 {workspace_base}/skills）。
    pub skills: std::sync::Arc<crate::skill::SkillService>,
    /// 远程接入持久化（白名单/绑定/channel 配置）。
    pub remote_store: std::sync::Arc<crate::remote::RemoteStore>,
    /// 远程枢纽：connector 注册 + 出站发送线程 + 引擎事件分发。
    pub remote_hub: std::sync::Arc<crate::remote::RemoteHub>,
}

impl AppState {
    pub fn open(handle: &tauri::AppHandle) -> Result<Self, String> {
        let dir = handle
            .path()
            .app_data_dir()
            .map_err(|err| format!("resolve app data dir: {err}"))?;
        let db = Arc::new(
            AppDatabase::open(dir.join("silicon-agent.sqlite3")).map_err(|err| err.to_string())?,
        );
        let provider = Arc::new(ProviderStore::open(db.clone(), dir.clone())?);
        let session = Arc::new(SessionStore::open(db.clone())?);
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
        let remote_store = std::sync::Arc::new(crate::remote::RemoteStore::open(db.clone())?);
        let remote_hub = std::sync::Arc::new(crate::remote::RemoteHub::new(remote_store.clone()));
        // 引擎构造器（纯构造）：装配完 service 后建，注入 RunCoordinator。
        let engine_builder = Arc::new(crate::engine::EngineBuilder::new(
            db.clone(),
            provider.clone(),
            gateway.clone(),
            session.clone(),
            workspace_base.clone(),
            handle.clone(),
            skills.clone(),
            remote_hub.clone(),
        ));
        // 运行时编排：持运行时状态（cancel_flags / run_registry）+ run 生命周期方法。
        let coordinator = Arc::new(crate::run::RunCoordinator::new(
            engine_builder.clone(),
            session.clone(),
            handle.clone(),
            gateway.clone(),
            db.clone(),
            remote_hub.clone(),
        ));
        // 应用编排门面：组合 engine_builder + coordinator，承载跨 session / project 的运行时编排。
        let facade = crate::app_facade::AppFacade::new(
            session.clone(),
            engine_builder.clone(),
            coordinator.clone(),
            workspace_base.clone(),
        );
        Ok(Self {
            db,
            provider,
            gateway,
            session,
            app_settings,
            usage,
            call_log,
            workspace_base,
            app: handle.clone(),
            engine_builder,
            coordinator,
            facade,
            skills,
            remote_store,
            remote_hub,
        })
    }
}

/// 默认工作目录基址：{home}/.siliconagent。HOME 缺失时回退相对 .siliconagent。
fn default_workspace_base() -> std::path::PathBuf {
    match std::env::var_os("HOME") {
        Some(home) => std::path::PathBuf::from(home).join(".siliconagent"),
        None => std::path::PathBuf::from(".siliconagent"),
    }
}

/// 生成 now 时间戳（Unix epoch 秒，字符串），用于配置写入审计列。
pub(crate) fn now_string() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();
    seconds.to_string()
}


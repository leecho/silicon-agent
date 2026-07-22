use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::app_state::now_string;
use crate::engine::EngineBuilder;
use crate::hook::{HookRule, HookService};
use crate::mcp::types::{McpServerConfig, McpTransportConfig};
use crate::mcp::McpService;
use crate::plugin::manifest::{ParsedMcpKind, PluginManifest};
use crate::plugin::vars::resolve_plugin_vars;
use crate::plugin::PluginService;
use crate::project::ProjectService;
use crate::run::RunCoordinator;
use crate::session::SessionStore;

/// 应用编排门面：把跨 session / project 的运行时编排聚合在一处，组合 `EngineBuilder`（纯构造）
/// 与 `RunCoordinator`（run 生命周期 + 子代理编排）。AppState 持有它作为命令层入口，自身退化为纯容器。
pub struct AppFacade {
    session: Arc<SessionStore>,
    projects: Arc<ProjectService>,
    engine_builder: Arc<EngineBuilder>,
    coordinator: Arc<RunCoordinator>,
    workspace_base: PathBuf,
    plugins: Arc<PluginService>,
    mcp: Arc<McpService>,
    hooks: Arc<HookService>,
}

impl AppFacade {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        session: Arc<SessionStore>,
        projects: Arc<ProjectService>,
        engine_builder: Arc<EngineBuilder>,
        coordinator: Arc<RunCoordinator>,
        workspace_base: PathBuf,
        plugins: Arc<PluginService>,
        mcp: Arc<McpService>,
        hooks: Arc<HookService>,
    ) -> Self {
        Self {
            session,
            projects,
            engine_builder,
            coordinator,
            workspace_base,
            plugins,
            mcp,
            hooks,
        }
    }

    /// T59：项目共享工作目录——缺省则派生稳定路径 `{base}/projects/{id}` 并落库。
    pub fn ensure_project_workspace(&self, project_id: &str) -> Result<String, String> {
        let p = self.projects.get(project_id)?.ok_or("项目不存在")?;
        if let Some(dir) = p.workspace_dir.filter(|d| !d.trim().is_empty()) {
            std::fs::create_dir_all(&dir).map_err(|e| format!("create project workspace: {e}"))?;
            return Ok(dir);
        }
        let base = self.workspace_base.join("projects").join(project_id);
        std::fs::create_dir_all(&base).map_err(|e| format!("create project workspace: {e}"))?;
        let dir = base.to_string_lossy().into_owned();
        self.projects.set_workspace(project_id, &dir)?;
        Ok(dir)
    }

    /// 发送项目草稿时创建项目线程：origin=project + project_id + 项目 workspace + 继承权限模式。
    fn create_project_session_for_message(
        &self,
        project_id: &str,
        mode: Option<&str>,
        permission_mode: Option<&str>,
        selected_model_id: Option<&str>,
    ) -> Result<String, String> {
        let p = self.projects.get(project_id)?.ok_or("项目不存在")?;
        let now = now_string();
        let id = crate::session::new_id("session");
        self.session.create_session(&id, "新会话", &now, false)?;
        self.session.set_session_origin(&id, "project")?;
        self.session.set_project_id(&id, project_id, &now)?;
        let ws = self.ensure_project_workspace(project_id)?;
        self.session.set_working_dir(&id, &ws, &now)?;
        // 线程继承项目权限模式（成员 ask/plan/permission 据此上浮）。
        let perm = permission_mode.unwrap_or(&p.permission_mode);
        self.session
            .set_session_permission_mode(&id, Some(perm), &now)?;
        if let Some(m) = mode.filter(|m| !m.trim().is_empty()) {
            self.session.set_session_mode(&id, m, &now)?;
        }
        if let Some(model) = selected_model_id.filter(|m| !m.trim().is_empty()) {
            self.session.set_selected_model_id(&id, Some(model), &now)?;
        }
        Ok(id)
    }

    pub fn submit_project_draft_message(
        &self,
        project_id: &str,
        content: &str,
        source_draft_session_id: Option<&str>,
        mode: Option<&str>,
        permission_mode: Option<&str>,
        selected_model_id: Option<&str>,
    ) -> Result<String, String> {
        if content.trim().is_empty() {
            return Err("消息内容不能为空".to_string());
        }
        if source_draft_session_id.is_some() {
            return Err("项目草稿包含附件，当前版本不能提交".to_string());
        }
        let id = self.create_project_session_for_message(
            project_id,
            mode,
            permission_mode,
            selected_model_id,
        )?;
        self.coordinator.spawn_user_message(&id, content)?;
        Ok(id)
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
            // 子运行派发信号是内部态，不作为用户 pending 呈现（child 运行由 awaiting_subagent + 事件体现）。
            Some(crate::engine::PendingInteraction::Subagent { .. }) | None => {}
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
        // 父会话运行上下文 → 解析成员展示身份。
        let (role_kind, role_id) = self
            .session
            .get_session(parent_id)
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
            // 展示身份：按角色槽解析（team→成员；agent/none→散装）；ad-hoc/缺省为空，前端回退 expert_name。
            let ident =
                self.engine_builder
                    .resolve_role_summary(&role_kind, &role_id, &expert_name);
            out.push(crate::session::ChildAgentSummary {
                session_id: c.id,
                expert_name,
                task: c.agent_task.unwrap_or_default(),
                status,
                created_at: c.created_at,
                round_id,
                display_name: ident.as_ref().and_then(|s| s.display_name.clone()),
                profession: ident.as_ref().and_then(|s| s.profession.clone()),
                avatar: ident.as_ref().and_then(|s| s.avatar.clone()),
            });
        }
        Ok(out)
    }

    /// T59：项目级任务看板投影——聚合本项目所有线程下的成员 child 运行。复用 session_children 的状态（paused→blocked）。
    pub fn list_project_child_runs(
        &self,
        project_id: &str,
    ) -> Result<Vec<crate::project::ProjectChildRun>, String> {
        let mut out = Vec::new();
        for thread in self.session.list_project_threads(project_id)? {
            for c in self.session_children(&thread.id)? {
                let status = if c.status == "paused" {
                    "blocked".to_string()
                } else {
                    c.status.clone()
                };
                let artifact_count = self
                    .session
                    .list_artifacts(&c.session_id)
                    .map(|a| a.len())
                    .unwrap_or(0);
                out.push(crate::project::ProjectChildRun {
                    session_id: c.session_id,
                    thread_id: thread.id.clone(),
                    thread_title: thread.title.clone(),
                    expert_name: c.expert_name,
                    display_name: c.display_name,
                    task: c.task,
                    status,
                    artifact_count,
                });
            }
        }
        Ok(out)
    }

    /// T59：项目级产物投影——聚合本项目所有线程下成员 child 已登记的 artifacts。
    pub fn list_project_artifacts(
        &self,
        project_id: &str,
    ) -> Result<Vec<crate::project::ProjectArtifact>, String> {
        let mut out = Vec::new();
        for thread in self.session.list_project_threads(project_id)? {
            for c in self.session_children(&thread.id)? {
                for a in self
                    .session
                    .list_artifacts(&c.session_id)
                    .unwrap_or_default()
                {
                    out.push(crate::project::ProjectArtifact {
                        path: a.path,
                        title: a.title,
                        session_id: c.session_id.clone(),
                        expert_name: c.expert_name.clone(),
                        display_name: c.display_name.clone(),
                        task: c.task.clone(),
                    });
                }
            }
        }
        Ok(out)
    }

    /// 智能体级产物投影：聚合该智能体直接激活的顶层会话已登记 artifacts。
    pub fn list_agent_artifacts(
        &self,
        agent_id: &str,
    ) -> Result<Vec<crate::project::ProjectArtifact>, String> {
        let mut out = Vec::new();
        for thread in self.session.list_agent_threads(agent_id)? {
            for a in self.session.list_artifacts(&thread.id).unwrap_or_default() {
                out.push(crate::project::ProjectArtifact {
                    path: a.path,
                    title: a.title,
                    session_id: thread.id.clone(),
                    expert_name: agent_id.to_string(),
                    display_name: None,
                    task: thread.title.clone(),
                });
            }
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

    // ── T66：plugin → MCP server 桥接 ───────────────────────────────────────────

    /// 插件私有数据目录：`{workspace_base}/plugin-data/<plugin_id>`（用时建）。
    fn plugin_data_dir(&self, plugin_id: &str) -> PathBuf {
        self.workspace_base.join("plugin-data").join(plugin_id)
    }

    /// 把一个插件声明的 MCP server 摄取（upsert）进 McpStore，并清理本次未声明的孤儿。
    /// 仅写 store，不发起连接（连接交由 `connect_plugin_mcp` 或启动 `startup_connect_all`）。
    /// `enabled` 通常跟随插件启用态：插件禁用时把其 server 一并置 disabled。
    pub fn sync_plugin_mcp(
        &self,
        plugin_id: &str,
        plugin_dir: &Path,
        manifest: &PluginManifest,
        enabled: bool,
    ) -> Result<(), String> {
        let plugin_data = self.plugin_data_dir(plugin_id);
        ingest_plugin_mcp(
            &self.mcp,
            plugin_id,
            plugin_dir,
            &plugin_data,
            manifest,
            enabled,
        )
    }

    /// 卸载插件时：断开并删除其全部 MCP server。
    pub fn remove_plugin_mcp(&self, plugin_id: &str) -> Result<(), String> {
        for s in self.mcp.store.list_by_plugin(plugin_id)? {
            self.mcp.disconnect(&s.id);
        }
        self.mcp.store.delete_by_plugin(plugin_id)
    }

    // ── T66：plugin → hooks 摄取（镜像 MCP）────────────────────────────────────

    /// 把一个插件声明的 hooks 摄取（整组替换）进 HookService。
    /// `enabled=false` 时清除该插件 hook（禁用插件即不触发其 hooks）。
    pub fn sync_plugin_hooks(
        &self,
        plugin_id: &str,
        plugin_dir: &Path,
        manifest: &PluginManifest,
        enabled: bool,
    ) {
        if !enabled || manifest.hooks.is_empty() {
            self.hooks.remove_plugin(plugin_id);
            return;
        }
        let plugin_data = self.plugin_data_dir(plugin_id);
        let rules: Vec<HookRule> = manifest
            .hooks
            .iter()
            .map(|h| HookRule {
                event: h.event.clone(),
                matcher: h.matcher.clone(),
                command: h.command.clone(),
                plugin_root: plugin_dir.to_path_buf(),
                plugin_data: plugin_data.clone(),
            })
            .collect();
        self.hooks.set_plugin(plugin_id, rules);
    }

    /// 卸载插件时清除其全部 hooks。
    pub fn remove_plugin_hooks(&self, plugin_id: &str) {
        self.hooks.remove_plugin(plugin_id);
    }

    /// 运行时连接某插件已启用的 MCP server（装/启用后即连）。
    pub fn connect_plugin_mcp(&self, plugin_id: &str) -> Result<(), String> {
        for s in self.mcp.store.list_by_plugin(plugin_id)? {
            if s.enabled {
                if let Err(e) = self.mcp.connect_one(&s) {
                    eprintln!("[plugin->mcp] 连接 {} 失败：{e}", s.name);
                }
            }
        }
        Ok(())
    }

    /// 装/启用/禁用插件后，重摄取其 MCP 声明并按 enabled 连接或断开。供命令层调用。
    pub fn refresh_plugin_mcp(&self, plugin_id: &str, enabled: bool) -> Result<(), String> {
        // 从 store 定位该插件目录与 manifest。
        let found = self
            .plugins
            .list_all_with_dir()?
            .into_iter()
            .find(|(id, _, _)| id == plugin_id);
        match found {
            Some((_, dir, manifest)) => {
                self.sync_plugin_mcp(plugin_id, &dir, &manifest, enabled)?;
                self.sync_plugin_hooks(plugin_id, &dir, &manifest, enabled);
                if enabled {
                    self.connect_plugin_mcp(plugin_id)?;
                } else {
                    for s in self.mcp.store.list_by_plugin(plugin_id)? {
                        self.mcp.disconnect(&s.id);
                    }
                }
                Ok(())
            }
            // 插件被禁用时 list_all_with_dir 不返回它：仅把其 server 断开并置 disabled，hooks 清除。
            None => {
                self.hooks.remove_plugin(plugin_id);
                for s in self.mcp.store.list_by_plugin(plugin_id)? {
                    self.mcp.disconnect(&s.id);
                    self.mcp.store.set_enabled(&s.id, false)?;
                }
                Ok(())
            }
        }
    }
}

/// plugin → MCP 摄取核心（无 facade 依赖，便于单测）：把 manifest 声明的 server upsert 进 store
/// （变量解析、命名空间化 name、稳定 id），并清理本次未声明的孤儿（断开 + 删除）。
fn ingest_plugin_mcp(
    mcp: &McpService,
    plugin_id: &str,
    plugin_dir: &Path,
    plugin_data: &Path,
    manifest: &PluginManifest,
    enabled: bool,
) -> Result<(), String> {
    let plugin_root = plugin_dir.to_string_lossy().into_owned();
    let plugin_data_str = plugin_data.to_string_lossy().into_owned();
    let plugin_name = if manifest.name.trim().is_empty() {
        plugin_id
    } else {
        manifest.name.trim()
    };

    let resolve = |s: &str| resolve_plugin_vars(s, &plugin_root, &plugin_data_str);

    let mut kept_ids: Vec<String> = Vec::new();
    let mut needs_data_dir = false;
    for parsed in &manifest.mcp_servers {
        let id = format!("mcpp-{}-{}", plugin_id, slug(&parsed.name));
        // 命名空间化展示名，避开 mcp_servers.name 的 UNIQUE 约束与用户 server 撞名。
        let name = format!("{plugin_name}:{}", parsed.name);
        // OAuth client_id 只对 HTTP 传输有意义（stdio 无 OAuth）。
        //
        // **清单没声明时必须保留库里已有的值**：摄取在插件启停/安装/**每次 app 启动**都会跑，
        // 若无脑写 None，用户为「不支持 DCR 的服务」手填的 client_id 会在下次重启被抹掉
        // （填了也留不住）。清单显式声明时以清单为准（包自己升级了 client_id）。
        let declared = match &parsed.kind {
            ParsedMcpKind::Http {
                oauth_client_id, ..
            } => oauth_client_id.clone(),
            ParsedMcpKind::Stdio { .. } => None,
        };
        let oauth_client_id = declared.or_else(|| {
            mcp.store
                .get(&id)
                .ok()
                .flatten()
                .and_then(|existing| existing.oauth_client_id)
        });
        // resource 覆盖来自清单，不是用户填的，直接以清单为准。
        let oauth_resource = match &parsed.kind {
            ParsedMcpKind::Http { oauth_resource, .. } => oauth_resource.clone(),
            ParsedMcpKind::Stdio { .. } => None,
        };
        let transport = match &parsed.kind {
            ParsedMcpKind::Stdio {
                command,
                args,
                env,
                cwd,
            } => {
                if cwd
                    .as_deref()
                    .map(|c| c.contains("CLAUDE_PLUGIN_DATA"))
                    .unwrap_or(false)
                    || env.values().any(|v| v.contains("CLAUDE_PLUGIN_DATA"))
                    || args.iter().any(|a| a.contains("CLAUDE_PLUGIN_DATA"))
                    || command.contains("CLAUDE_PLUGIN_DATA")
                {
                    needs_data_dir = true;
                }
                McpTransportConfig::Stdio {
                    command: resolve(command),
                    args: args.iter().map(|a| resolve(a)).collect(),
                    env: env.iter().map(|(k, v)| (k.clone(), resolve(v))).collect(),
                    cwd: cwd.as_deref().map(&resolve),
                }
            }
            ParsedMcpKind::Http { url, headers, .. } => McpTransportConfig::Http {
                url: resolve(url),
                headers: headers
                    .iter()
                    .map(|(k, v)| (k.clone(), resolve(v)))
                    .collect(),
            },
        };
        let cfg = McpServerConfig {
            id: id.clone(),
            name,
            preset_id: None,
            plugin_id: plugin_id.to_string(),
            oauth_client_id,
            oauth_resource,
            transport,
            auto_approve: false,
            enabled,
        };
        mcp.store.upsert(cfg)?;
        kept_ids.push(id);
    }

    if needs_data_dir {
        let _ = std::fs::create_dir_all(plugin_data);
    }

    // clear-except：本插件名下、本次未声明的 server → 断开 + 删除（镜像 clear_plugin_experts_except）。
    for s in mcp.store.list_by_plugin(plugin_id)? {
        if !kept_ids.contains(&s.id) {
            mcp.disconnect(&s.id);
            mcp.store.delete(&s.id)?;
        }
    }
    Ok(())
}

/// 把任意 server 名归一化为 id 片段：小写、非 [a-z0-9] 转 `-`、折叠连续 `-`。
fn slug(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_dash = false;
    for ch in s.trim().to_ascii_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        "server".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod mcp_bridge_tests {
    use super::*;
    use crate::mcp::store::McpStore;
    use crate::mcp::McpService;
    use crate::plugin::manifest::{ParsedMcpKind, ParsedMcpServer};
    use crate::storage::AppDatabase;
    use std::sync::Arc;

    fn temp_mcp(tag: &str) -> (Arc<McpService>, std::path::PathBuf) {
        static C: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let seq = C.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "siw-facade-mcp-{tag}_{}_{}_{nanos}",
            std::process::id(),
            seq
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let db = Arc::new(AppDatabase::open(dir.join("app.sqlite3")).unwrap());
        let store = McpStore::new(db, dir.join("mcp.secrets.json")).unwrap();
        (McpService::new(store), dir)
    }

    fn manifest_with(name: &str, servers: Vec<ParsedMcpServer>) -> PluginManifest {
        PluginManifest {
            name: name.into(),
            display_name: name.into(),
            version: String::new(),
            description: String::new(),
            description_zh: None,
            category: None,
            customized_from: None,
            skills: vec![],
            commands: vec![],
            agents: vec![],
            author: None,
            homepage: None,
            repository: None,
            license: None,
            keywords: vec![],
            mcp_servers: servers,
            hooks: vec![],
        }
    }

    #[test]
    fn slug_normalizes() {
        assert_eq!(slug("Files Server"), "files-server");
        assert_eq!(slug("a__b//c"), "a-b-c");
        assert_eq!(slug("  ---  "), "server");
    }

    #[test]
    fn ingest_namespaces_resolves_and_clears_orphans() {
        let (mcp, dir) = temp_mcp("ingest");
        let root = dir.join("plugins/p");
        let data = dir.join("plugin-data/plg-1");

        // 首轮：两个 server，stdio cwd 用 ${CLAUDE_PLUGIN_ROOT}。
        let m1 = manifest_with(
            "MyPlugin",
            vec![
                ParsedMcpServer {
                    name: "Files".into(),
                    kind: ParsedMcpKind::Stdio {
                        command: "node".into(),
                        args: vec!["${CLAUDE_PLUGIN_ROOT}/s.js".into()],
                        env: Default::default(),
                        cwd: Some("${CLAUDE_PLUGIN_ROOT}".into()),
                    },
                },
                ParsedMcpServer {
                    name: "remote".into(),
                    kind: ParsedMcpKind::Http {
                        url: "https://e/mcp".into(),
                        headers: Default::default(),
                        oauth_client_id: None,
                        oauth_resource: None,
                    },
                },
            ],
        );
        ingest_plugin_mcp(&mcp, "plg-1", &root, &data, &m1, true).unwrap();

        let listed = mcp.store.list_by_plugin("plg-1").unwrap();
        assert_eq!(listed.len(), 2);
        let files = listed.iter().find(|s| s.id == "mcpp-plg-1-files").unwrap();
        assert_eq!(files.name, "MyPlugin:Files", "命名空间化 name");
        assert_eq!(files.plugin_id, "plg-1");
        match &files.transport {
            McpTransportConfig::Stdio { cwd, args, .. } => {
                assert_eq!(
                    cwd.as_deref(),
                    Some(root.to_string_lossy().as_ref()),
                    "变量已解析"
                );
                assert!(
                    args[0].ends_with("/s.js")
                        && args[0].starts_with(root.to_string_lossy().as_ref())
                );
            }
            _ => panic!("expected stdio"),
        }

        // 次轮：仅声明 remote → Files 成为孤儿被删除。
        let m2 = manifest_with(
            "MyPlugin",
            vec![ParsedMcpServer {
                name: "remote".into(),
                kind: ParsedMcpKind::Http {
                    url: "https://e/mcp".into(),
                    headers: Default::default(),
                    oauth_client_id: None,
                    oauth_resource: None,
                },
            }],
        );
        ingest_plugin_mcp(&mcp, "plg-1", &root, &data, &m2, true).unwrap();
        let after = mcp.store.list_by_plugin("plg-1").unwrap();
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].id, "mcpp-plg-1-remote");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ingest_preserves_user_client_id_override() {
        // 摄取在插件启停/安装/**每次 app 启动**都会跑。若清单没声明 clientId 就写 None，
        // 用户为「不支持 DCR 的服务」手填的 client_id 会在下次重启被抹掉——填了也留不住。
        let (mcp, dir) = temp_mcp("ingest-preserve");
        let root = dir.join("plugins/p");
        let data = dir.join("plugin-data/plg-x");

        // 清单**不**声明 clientId。
        let m = manifest_with(
            "P",
            vec![ParsedMcpServer {
                name: "remote".into(),
                kind: ParsedMcpKind::Http {
                    url: "https://e/mcp".into(),
                    headers: Default::default(),
                    oauth_client_id: None,
                    oauth_resource: None,
                },
            }],
        );
        ingest_plugin_mcp(&mcp, "plg-x", &root, &data, &m, true).unwrap();

        // 用户手填 client_id（模拟「该服务不支持 DCR」时的救急通路）。
        let id = "mcpp-plg-x-remote";
        let mut cfg = mcp.store.get(id).unwrap().expect("已摄取");
        cfg.oauth_client_id = Some("user-typed".into());
        mcp.store.upsert(cfg).unwrap();

        // 再次摄取（= 重启 app / 启停插件）：不得抹掉用户的值。
        ingest_plugin_mcp(&mcp, "plg-x", &root, &data, &m, true).unwrap();
        let after = mcp.store.get(id).unwrap().expect("仍在");
        assert_eq!(
            after.oauth_client_id.as_deref(),
            Some("user-typed"),
            "清单未声明时，用户手填的 client_id 必须保留"
        );

        // 但清单显式声明时以清单为准（包自己升级了 client_id）。
        let m2 = manifest_with(
            "P",
            vec![ParsedMcpServer {
                name: "remote".into(),
                kind: ParsedMcpKind::Http {
                    url: "https://e/mcp".into(),
                    headers: Default::default(),
                    oauth_client_id: Some("from-manifest".into()),
                    oauth_resource: None,
                },
            }],
        );
        ingest_plugin_mcp(&mcp, "plg-x", &root, &data, &m2, true).unwrap();
        let after2 = mcp.store.get(id).unwrap().expect("仍在");
        assert_eq!(after2.oauth_client_id.as_deref(), Some("from-manifest"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ingest_passes_through_oauth_client_id() {
        let (mcp, dir) = temp_mcp("ingest-oauth");
        let root = dir.join("plugins/figma");
        let data = dir.join("plugin-data/plg-fig");

        let m = manifest_with(
            "Figma",
            vec![
                ParsedMcpServer {
                    name: "figma".into(),
                    kind: ParsedMcpKind::Http {
                        url: "https://mcp.figma.com/mcp".into(),
                        headers: Default::default(),
                        oauth_client_id: Some("cid-123".into()),
                        oauth_resource: None,
                    },
                },
                ParsedMcpServer {
                    name: "plain".into(),
                    kind: ParsedMcpKind::Http {
                        url: "https://e/mcp".into(),
                        headers: Default::default(),
                        oauth_client_id: None,
                        oauth_resource: None,
                    },
                },
            ],
        );
        ingest_plugin_mcp(&mcp, "plg-fig", &root, &data, &m, true).unwrap();

        let listed = mcp.store.list_by_plugin("plg-fig").unwrap();
        let figma = listed
            .iter()
            .find(|s| s.id == "mcpp-plg-fig-figma")
            .expect("figma server 已摄取");
        assert_eq!(
            figma.oauth_client_id.as_deref(),
            Some("cid-123"),
            "插件声明的 clientId 必须落库（否则只能走 DCR，T104 阻断项）"
        );

        let plain = listed
            .iter()
            .find(|s| s.id == "mcpp-plg-fig-plain")
            .expect("plain server 已摄取");
        assert!(
            plain.oauth_client_id.is_none(),
            "未声明 clientId → None（走 DCR）"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}

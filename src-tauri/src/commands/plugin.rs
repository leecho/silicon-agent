//! 插件命令（薄入口）。
use crate::app_state::AppState;
use crate::mcp::types::McpTransportConfig;
use crate::plugin::types::{InstalledExtension, PluginDetail, PluginMcpSummary, PluginSummary};
use tauri::State;

/// 列出全部插件（按 name 升序，含各自 skill 数）。
#[tauri::command]
pub fn list_plugins(services: State<'_, AppState>) -> Result<Vec<PluginSummary>, String> {
    services.plugins.list()
}

/// **装载入口内核**（T108 §5.1）：按**清单文件名**把包分发到三条装载器之一。
///
/// | 清单 | 体系 | 组件归属 |
/// |---|---|---|
/// | `team.json` | silicon 团队 | 成员 expert 与 skill 均 `team_id` → **私有**，激活团队才载入 |
/// | `expert.json` | silicon 专家 | 专家散装 + 其 skill `expert_id` → **私有**，选中专家才载入 |
/// | `plugin.json` | 标准 plugin | 全部 `plugin_id` → **公开**（照 Claude/Codex 规范） |
///
/// 三条装载器**本就存在**，此前只是没接对：入口只做 team/plugin 二分（靠探 `teamInfo`），
/// **根本不认 expert 包** —— 市场装的 expert 包会被当成普通 plugin 装，导致它本应私有的
/// 技能变成公开的、污染全局技能目录（T108 §8 缺陷④）。
///
/// 本地装载与**从市场装载**共用此函数（市场把包物化到临时目录后走同一条路）。
pub fn install_extension_from_path(
    services: &AppState,
    path: &str,
) -> Result<InstalledExtension, String> {
    use crate::team::import::PackageKind;
    match crate::team::import::detect_package_kind(path)? {
        PackageKind::Team => {
            let team = services
                .teams
                .import_from_path(path, crate::team::model::TeamSource::Imported)?;
            Ok(InstalledExtension::Team(team))
        }
        PackageKind::Expert => {
            let expert = crate::expert::expert::import_expert(
                &services.experts,
                &services.skills,
                &services.workspace_base,
                path,
            )?;
            Ok(InstalledExtension::Expert(expert))
        }
        PackageKind::Plugin => {
            let summary = services.plugins.install_from_path(path)?;
            // MCP 联动失败不回滚安装，仅记录（插件本体已就绪）。
            if let Err(e) = services.facade.refresh_plugin_mcp(&summary.id, true) {
                eprintln!("[plugin->mcp] 安装后摄取失败 plugin={}：{e}", summary.id);
            }
            Ok(InstalledExtension::Plugin(summary))
        }
    }
}

/// **统一装载入口**（T106）：从本地路径装载一个扩展包（目录或 zip）。
#[tauri::command]
pub fn install_plugin_from_path(
    services: State<'_, AppState>,
    path: String,
) -> Result<InstalledExtension, String> {
    install_extension_from_path(&services, &path)
}

/// 切换插件启用开关（其下 skill 的可见性随之级联；其 MCP server 随之连接/断开）。
#[tauri::command]
pub fn toggle_plugin(
    services: State<'_, AppState>,
    id: String,
    enabled: bool,
) -> Result<PluginSummary, String> {
    let summary = services.plugins.toggle(&id, enabled)?;
    if let Err(e) = services.facade.refresh_plugin_mcp(&id, enabled) {
        eprintln!("[plugin->mcp] 切换后联动失败 plugin={id}：{e}");
    }
    Ok(summary)
}

/// 卸载用户插件（内置不可卸载），级联清掉它带来的**全部**组件。
///
/// 清理分工（缺一样就留孤儿）：
/// - 这里：**MCP server**（要先断连）、**hooks**（要热卸载）—— `PluginService` 够不到这两个子系统；
/// - `plugins.uninstall`：**skill**、**expert**、插件目录与行。
///
/// 此前 **expert 与 hooks 都在泄漏**（T108 §8 缺陷②）：expert 侧压根没有 `delete_by_plugin`；
/// `remove_plugin_hooks` 函数写了、却**从没有任何调用点** —— 摆在那儿看着像已经接好了。
///
/// 前置清理失败**只记日志、继续卸载**（沿用 MCP 既有做法）：半清理好过卡住不让用户卸载。
#[tauri::command]
pub fn uninstall_plugin(services: State<'_, AppState>, id: String) -> Result<(), String> {
    if let Err(e) = services.facade.remove_plugin_mcp(&id) {
        eprintln!("[plugin->mcp] 卸载前清理 MCP 失败 plugin={id}：{e}");
    }
    services.facade.remove_plugin_hooks(&id);
    services.plugins.uninstall(&id)
}

/// 插件详情：能力包元数据 + 其下 skill 列表 + 提供的 agents。
#[tauri::command]
pub fn plugin_detail(services: State<'_, AppState>, id: String) -> Result<PluginDetail, String> {
    let mut detail = services.plugins.detail(&id)?;
    // 补上该插件提供的专家（来自 agents 表，按 plugin_id）。
    detail.agents = services.experts.list_by_plugin(&id).unwrap_or_default();
    // 补上该插件提供的 MCP server（来自 mcp store，按 plugin_id；连接状态来自 statuses）。
    let statuses = services.mcp.statuses();
    detail.mcp_servers = services
        .mcp
        .list_by_plugin(&id)
        .into_iter()
        .map(|cfg| {
            let (transport, target) = match &cfg.transport {
                McpTransportConfig::Stdio { command, .. } => ("stdio", command.clone()),
                McpTransportConfig::Http { url, .. } => ("http", url.clone()),
                McpTransportConfig::Sse { url, .. } => ("sse", url.clone()),
            };
            let state = statuses
                .iter()
                .find(|s| s.server_id == cfg.id)
                .map(|s| s.state.clone())
                .unwrap_or_else(|| "disconnected".into());
            PluginMcpSummary {
                name: cfg.name,
                transport: transport.into(),
                target,
                state,
            }
        })
        .collect();
    Ok(detail)
}

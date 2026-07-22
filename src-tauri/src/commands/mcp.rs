//! MCP 设置页命令（薄入口，业务在 mcp::manager / mcp::store / mcp::json）。

use tauri::State;

use crate::app_state::AppState;
use crate::mcp::types::{McpServerConfig, McpServerStatus, McpToolDef};

#[tauri::command]
pub fn mcp_list_servers(services: State<'_, AppState>) -> Result<Vec<McpServerConfig>, String> {
    services.mcp.store.list()
}

#[tauri::command]
pub fn mcp_server_statuses(services: State<'_, AppState>) -> Result<Vec<McpServerStatus>, String> {
    Ok(services.mcp.statuses())
}

/// 保存单个实例（详情页编辑用）。保存后异步重连。
#[tauri::command]
pub fn mcp_upsert_server(
    services: State<'_, AppState>,
    config: McpServerConfig,
) -> Result<McpServerConfig, String> {
    let saved = services.mcp.store.upsert(config)?;
    let mcp = services.mcp.clone();
    let cfg = saved.clone();
    std::thread::spawn(move || {
        if cfg.enabled {
            let _ = mcp.connect_one(&cfg);
        } else {
            mcp.disconnect(&cfg.id);
        }
    });
    Ok(saved)
}

/// 导入标准 mcpServers JSON：合并 upsert（新增/更新，不删除已有），触发受影响服务重连。
#[tauri::command]
pub fn mcp_import_json(
    services: State<'_, AppState>,
    json: String,
) -> Result<Vec<McpServerConfig>, String> {
    let parsed = crate::mcp::json::parse_mcp_servers(&json)?;
    let saved = services.mcp.store.import_json(parsed)?;
    let mcp = services.mcp.clone();
    let saved_clone = saved.clone();
    std::thread::spawn(move || {
        for s in &saved_clone {
            if s.enabled {
                let _ = mcp.connect_one(s);
            } else {
                mcp.disconnect(&s.id);
            }
        }
    });
    Ok(saved)
}

/// 导出当前手动服务为标准 mcpServers JSON。
#[tauri::command]
pub fn mcp_export_json(services: State<'_, AppState>) -> Result<String, String> {
    let servers: Vec<_> = services
        .mcp
        .store
        .list()?
        .into_iter()
        .filter(|s| s.plugin_id.is_empty())
        .collect();
    Ok(crate::mcp::json::to_mcp_servers_json(&servers))
}

/// 某 server 的工具清单（详情页）。
#[tauri::command]
pub fn mcp_list_tools(
    services: State<'_, AppState>,
    id: String,
) -> Result<Vec<McpToolDef>, String> {
    Ok(services.mcp.tools_for(&id))
}

#[tauri::command]
pub fn mcp_set_enabled(
    services: State<'_, AppState>,
    id: String,
    enabled: bool,
) -> Result<(), String> {
    services.mcp.store.set_enabled(&id, enabled)?;
    let mcp = services.mcp.clone();
    std::thread::spawn(move || {
        if enabled {
            if let Ok(Some(cfg)) = mcp.store.get(&id) {
                let _ = mcp.connect_one(&cfg);
            }
        } else {
            mcp.disconnect(&id);
        }
    });
    Ok(())
}

#[tauri::command]
pub fn mcp_set_auto_approve(
    services: State<'_, AppState>,
    id: String,
    auto_approve: bool,
) -> Result<(), String> {
    services.mcp.store.set_auto_approve(&id, auto_approve)
}

#[tauri::command]
pub fn mcp_delete_server(services: State<'_, AppState>, id: String) -> Result<(), String> {
    services.mcp.disconnect(&id);
    services.mcp.store.delete(&id)
}

/// 重新连接一个已保存的 server：断开 → 重连，状态经 `mcp_status_event` 回流。
///
/// 用户面「重试」按钮的落点。`mcp_test_connection` 校验的是一份**离线配置**（可未保存），
/// 不会改变这个 server 的实际连接状态——连不上时用它「测试成功」反而更让人困惑。
#[tauri::command]
pub fn mcp_reconnect(services: State<'_, AppState>, id: String) -> Result<(), String> {
    let mcp = services.mcp.clone();
    std::thread::spawn(move || {
        mcp.disconnect(&id);
        match mcp.store.get(&id) {
            Ok(Some(cfg)) if cfg.enabled => {
                let _ = mcp.connect_one(&cfg);
            }
            _ => {}
        }
    });
    Ok(())
}

/// 即时测试一份配置（可未保存，凭证内联在 config）。
///
/// **必须 async + spawn_blocking**：握手要跑一次阻塞 HTTP，同步命令会占住主线程冻住 UI。
#[tauri::command]
pub async fn mcp_test_connection(
    services: State<'_, AppState>,
    config: McpServerConfig,
) -> Result<Vec<McpToolDef>, String> {
    let mcp = services.mcp.clone();
    tauri::async_runtime::spawn_blocking(move || mcp.test_connection(&config))
        .await
        .map_err(|e| format!("测试连接任务失败：{e}"))?
}

/// 发起 OAuth 授权，返回授权 URL（前端展示/复制）。授权在后台完成，结果经 mcp_status_event 回流。
///
/// **必须 async + spawn_blocking**：返回 auth_url 之前要同步跑完 OAuth 发现
/// （PRM → AS 元数据 → 可能还有动态注册），全是阻塞 HTTP。做成同步命令会占住主线程，
/// 点「登录」后整个窗口卡死直到发现完成或超时。
#[tauri::command]
pub async fn mcp_oauth_authorize(
    services: State<'_, AppState>,
    id: String,
) -> Result<String, String> {
    let mcp = services.mcp.clone();
    tauri::async_runtime::spawn_blocking(move || mcp.oauth_authorize(id))
        .await
        .map_err(|e| format!("授权任务失败：{e}"))?
}

/// 撤销某 server 的 OAuth 授权（清 token + 断开）。
/// 设置/清除某 MCP server 的 OAuth client_id（不支持动态注册的服务需手填）。
/// 插件提供的 server 也允许——client_id 是凭证，不是包的构成（见 McpService::set_oauth_client_id）。
#[tauri::command]
pub fn mcp_set_oauth_client_id(
    services: State<'_, AppState>,
    id: String,
    client_id: Option<String>,
) -> Result<(), String> {
    services.mcp.set_oauth_client_id(&id, client_id)
}

#[tauri::command]
pub fn mcp_oauth_revoke(services: State<'_, AppState>, id: String) -> Result<(), String> {
    services.mcp.oauth_revoke(&id);
    Ok(())
}

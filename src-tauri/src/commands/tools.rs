//! 工具叙事标签命令：把后端 `Tool::label()` 暴露给前端（单一真相源）。
use crate::app_state::AppState;
use std::collections::HashMap;
use tauri::State;

/// 内置工具的 name→中文标签映射（单一真相源 = 后端 `Tool::label()`）。
/// 前端据此叙事化展示，取代前端硬编码标签表。MCP 动态工具（`mcp__` 前缀）前端单独处理，此处排除。
#[tauri::command]
pub fn get_tool_labels(services: State<'_, AppState>) -> Result<HashMap<String, String>, String> {
    // 仅取工具标签（无会话上下文）：传占位 session_id，浏览器工具不会真的绑定动作。
    let registry = services
        .engine_builder
        .build_registry(std::env::temp_dir(), "");
    Ok(registry
        .specs()
        .into_iter()
        .filter(|s| !s.name.starts_with("mcp__"))
        .map(|s| (s.name, s.label))
        .collect())
}

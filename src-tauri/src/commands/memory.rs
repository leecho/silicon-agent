//! 长期记忆命令（薄入口）。
use crate::app_state::{now_string, AppState};
use crate::memory::{Memory, MemoryScope};
use tauri::State;

/// 列出全部长期记忆（按创建时间升序）。
#[tauri::command]
pub fn list_memories(services: State<'_, AppState>) -> Result<Vec<Memory>, String> {
    services.memory.list_memories()
}

/// 新增一条长期记忆。
#[tauri::command]
pub fn add_memory(services: State<'_, AppState>, content: String) -> Result<Memory, String> {
    services
        .memory
        .add_memory(&content, &now_string(), MemoryScope::Global)
}

/// 更新一条长期记忆的内容。
#[tauri::command]
pub fn update_memory(
    services: State<'_, AppState>,
    id: String,
    content: String,
) -> Result<(), String> {
    services.memory.update_memory(&id, &content)
}

/// 删除一条长期记忆。
#[tauri::command]
pub fn delete_memory(services: State<'_, AppState>, id: String) -> Result<(), String> {
    services.memory.delete_memory(&id)
}

/// 清空全部长期记忆。
#[tauri::command]
pub fn clear_memories(services: State<'_, AppState>) -> Result<(), String> {
    services.memory.clear_memories()
}

/// 读取用户画像整段文本（无则返回空串）。
#[tauri::command]
pub fn get_memory_profile(services: State<'_, AppState>) -> Result<String, String> {
    Ok(services.memory.get_profile()?.unwrap_or_default())
}

/// 写入/覆盖用户画像（空内容等同清空）。
#[tauri::command]
pub fn set_memory_profile(services: State<'_, AppState>, content: String) -> Result<(), String> {
    services.memory.set_profile(&content, &now_string())
}

/// 置顶/取消置顶一条记忆（置顶进 Tier1，始终注入）。
#[tauri::command]
pub fn set_memory_pinned(
    services: State<'_, AppState>,
    id: String,
    pinned: bool,
) -> Result<(), String> {
    services.memory.set_pinned(&id, pinned)
}

/// 由 (scope_kind, scope_id) 构造 MemoryScope：project/agent 各取其 id，其余视为 global。
fn scope_from<'a>(scope_kind: &str, scope_id: &'a str) -> MemoryScope<'a> {
    match scope_kind {
        "project" if !scope_id.is_empty() => MemoryScope::Project(scope_id),
        "agent" if !scope_id.is_empty() => MemoryScope::Agent(scope_id),
        _ => MemoryScope::Global,
    }
}

/// 列出指定作用域内的 fact（精确作用域，不并入全局）。供项目/智能体记忆管理界面。
#[tauri::command]
pub fn list_scoped_memories(
    services: State<'_, AppState>,
    scope_kind: String,
    scope_id: String,
) -> Result<Vec<Memory>, String> {
    services
        .memory
        .list_scoped_facts(scope_from(&scope_kind, &scope_id))
}

/// 统计指定作用域内的 fact 条数。供首页记忆卡片。
#[tauri::command]
pub fn count_scoped_memories(
    services: State<'_, AppState>,
    scope_kind: String,
    scope_id: String,
) -> Result<i64, String> {
    services
        .memory
        .count_scoped_facts(scope_from(&scope_kind, &scope_id))
}

/// 在指定作用域内新增一条 fact。
#[tauri::command]
pub fn add_scoped_memory(
    services: State<'_, AppState>,
    scope_kind: String,
    scope_id: String,
    content: String,
) -> Result<Memory, String> {
    services
        .memory
        .add_memory(&content, &now_string(), scope_from(&scope_kind, &scope_id))
}

/// 主动整理：模型驱动地对事实去重/合并、并抽取/更新用户画像。
/// 用默认模型（会话无关），事实不足阈值则跳过（ran=false）。
#[tauri::command]
pub fn curate_memories(
    services: State<'_, AppState>,
) -> Result<crate::memory::curation::CurationOutcome, String> {
    let selection = services.provider.resolve_selection(None).ok();
    crate::memory::curation::curate(
        &services.memory,
        services.gateway.as_ref(),
        selection.as_ref(),
    )
}

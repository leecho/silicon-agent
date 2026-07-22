//! 「我的」分组命令（薄入口）：建/列/改名/删分组 + 把专家/团队移入分组。
use crate::app_state::AppState;
use crate::group::Group;
use tauri::State;

/// 列出某类型（agent|team）的分组。
#[tauri::command]
pub fn list_groups(services: State<'_, AppState>, kind: String) -> Result<Vec<Group>, String> {
    services.groups.list(&kind)
}

/// 新建分组。
#[tauri::command]
pub fn create_group(
    services: State<'_, AppState>,
    kind: String,
    name: String,
) -> Result<Group, String> {
    services.groups.create(&kind, &name)
}

/// 重命名分组。
#[tauri::command]
pub fn rename_group(services: State<'_, AppState>, id: String, name: String) -> Result<(), String> {
    services.groups.rename(&id, &name)
}

/// 删除分组（组内项归零；kind 决定清哪张表）。
#[tauri::command]
pub fn delete_group(services: State<'_, AppState>, id: String, kind: String) -> Result<(), String> {
    match kind.as_str() {
        "team" => services.teams.clear_group(&id)?,
        "skill" => services.skills.clear_group(&id)?,
        _ => services.experts.clear_group(&id)?,
    }
    services.groups.delete(&id)
}

/// 把散装专家移入分组（group_id 为空字符串/None=移出）。
#[tauri::command]
pub fn set_expert_group(
    services: State<'_, AppState>,
    expert_id: String,
    group_id: Option<String>,
) -> Result<(), String> {
    let gid = group_id.filter(|s| !s.is_empty());
    services.experts.set_group(&expert_id, gid.as_deref())
}

/// 把团队移入分组（group_id 为空字符串/None=移出）。
#[tauri::command]
pub fn set_team_group(
    services: State<'_, AppState>,
    team_id: String,
    group_id: Option<String>,
) -> Result<(), String> {
    let gid = group_id.filter(|s| !s.is_empty());
    services.teams.set_group(&team_id, gid.as_deref())
}

/// 把技能移入分组（group_id 为空字符串/None=移出）。
#[tauri::command]
pub fn set_skill_group(
    services: State<'_, AppState>,
    skill_id: String,
    group_id: Option<String>,
) -> Result<(), String> {
    let gid = group_id.filter(|s| !s.is_empty());
    services.skills.set_group(&skill_id, gid.as_deref())
}

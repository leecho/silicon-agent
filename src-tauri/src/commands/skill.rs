//! 技能命令（薄入口）。
use crate::app_state::AppState;
use crate::skill::types::{SkillDetail, SkillFilePreview, SkillSummary};
use tauri::State;

/// 列出全部技能（内置 + 用户，按 name 升序）。
#[tauri::command]
pub fn list_skills(services: State<'_, AppState>) -> Result<Vec<SkillSummary>, String> {
    services.skills.list()
}

/// 切换技能启用开关。
#[tauri::command]
pub fn toggle_skill(
    services: State<'_, AppState>,
    id: String,
    enabled: bool,
) -> Result<SkillSummary, String> {
    services.skills.toggle(&id, enabled)
}

/// 从本地路径安装技能（.zip 文件或技能目录）。
#[tauri::command]
pub fn install_skill_from_path(
    services: State<'_, AppState>,
    path: String,
) -> Result<SkillSummary, String> {
    services.skills.install_from_path(&path)
}

/// 卸载用户技能（内置不可卸载）。
#[tauri::command]
pub fn uninstall_skill(services: State<'_, AppState>, id: String) -> Result<(), String> {
    services.skills.uninstall(&id)
}

/// 技能详情：元数据 + SKILL.md 原文 + 目录文件列表。
#[tauri::command]
pub fn get_skill_detail(services: State<'_, AppState>, id: String) -> Result<SkillDetail, String> {
    services.skills.detail(&id)
}

/// 读取技能目录内单文件用于预览。
#[tauri::command]
pub fn read_skill_file(
    services: State<'_, AppState>,
    id: String,
    rel_path: String,
) -> Result<SkillFilePreview, String> {
    services.skills.read_file(&id, &rel_path)
}

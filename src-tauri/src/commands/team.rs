//! 团队命令（薄入口）。会话级编排：lead + members 引用 + 私有组件。
use crate::app_state::AppState;
use crate::team::model::TeamMember;
use crate::team::{TeamDetail, TeamSummary};
use tauri::State;

/// 列出全部团队（按 name 升序）。
#[tauri::command]
pub fn list_teams(services: State<'_, AppState>) -> Result<Vec<TeamSummary>, String> {
    services.teams.list()
}

/// 列出启用团队（供 Composer 角色槽）。
#[tauri::command]
pub fn list_active_teams(services: State<'_, AppState>) -> Result<Vec<TeamSummary>, String> {
    services.teams.list_enabled()
}

/// 团队详情：元数据 + 解析后的 lead/成员 + 开场引导语。
#[tauri::command]
pub fn team_detail(services: State<'_, AppState>, id: String) -> Result<TeamDetail, String> {
    let mut detail = services.teams.detail(&id)?;
    detail.skills = services.skills.list_by_team(&id).unwrap_or_default();
    Ok(detail)
}

/// 新建用户团队：lead 可空；members 为对 agent 的引用。
#[tauri::command]
pub fn create_team(
    services: State<'_, AppState>,
    name: String,
    display_name: String,
    description: Option<String>,
    lead: Option<TeamMember>,
    members: Vec<TeamMember>,
) -> Result<TeamSummary, String> {
    services.teams.create(
        &name,
        &display_name,
        description.as_deref().unwrap_or(""),
        lead,
        members,
    )
}

/// 切换团队启用状态。
#[tauri::command]
pub fn toggle_team(
    services: State<'_, AppState>,
    id: String,
    enabled: bool,
) -> Result<TeamSummary, String> {
    services.teams.toggle(&id, enabled)
}

/// 删除团队（级联其私有组件；内置不可删）。
#[tauri::command]
pub fn delete_team(services: State<'_, AppState>, id: String) -> Result<(), String> {
    services.teams.delete(&id)
}

/// 列出可作团队成员/角色的启用 agent（散装 + plugin 提供；带 owner 供前端构造引用）。
#[tauri::command]
pub fn list_experts(
    services: State<'_, AppState>,
) -> Result<Vec<crate::expert::ExpertSummary>, String> {
    services.experts.list_enabled()
}

/// 从本地目录导入团队结构的包（原生 / codebuddy / Claude 方言）→ 落成 source=imported 的团队。
#[tauri::command]
pub fn import_team_from_path(
    services: State<'_, AppState>,
    path: String,
) -> Result<TeamSummary, String> {
    services
        .teams
        .import_from_path(&path, crate::team::model::TeamSource::Imported)
}

/// 导入「带技能的专家」expert 包（原生 / codebuddy / Claude 方言）→ 散装 agent + 其 skill 作该 agent 私有。
#[tauri::command]
pub fn import_expert_from_path(
    services: State<'_, AppState>,
    path: String,
) -> Result<crate::expert::ExpertSummary, String> {
    crate::expert::expert::import_expert(
        &services.experts,
        &services.skills,
        &services.workspace_base,
        &path,
    )
}

/// 列出**散装** agent（含未启用），供「专家」管理页。
#[tauri::command]
pub fn list_standalone_experts(
    services: State<'_, AppState>,
) -> Result<Vec<crate::expert::ExpertSummary>, String> {
    services.experts.list_standalone()
}

/// 列出「扩展 → 专家」Tab 的全部 agent：**散装 + plugin 提供**（含未启用，排除 team 私有）。
/// T106 §5.2：扩展页每个类型 Tab 显示全部，按 owner 分「我的 / 来自插件」两组。
#[tauri::command]
pub fn list_manageable_experts(
    services: State<'_, AppState>,
) -> Result<Vec<crate::expert::ExpertSummary>, String> {
    services.experts.list_manageable()
}

/// 新建散装 agent。
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn create_expert(
    services: State<'_, AppState>,
    name: String,
    description: String,
    system_prompt: String,
    tools: Vec<String>,
    model_tier: String,
    display_name: Option<String>,
    profession: Option<String>,
    avatar: Option<String>,
    quick_prompts: Option<Vec<String>>,
) -> Result<crate::expert::ExpertSummary, String> {
    services.experts.create_standalone(
        &name,
        &description,
        &system_prompt,
        tools,
        &model_tier,
        display_name,
        profession,
        avatar,
        quick_prompts.unwrap_or_default(),
        None,
    )
}

/// 切换 agent 启用状态。
#[tauri::command]
pub fn toggle_expert(
    services: State<'_, AppState>,
    id: String,
    enabled: bool,
) -> Result<crate::expert::ExpertSummary, String> {
    services.experts.toggle(&id, enabled)
}

/// 删除散装 user agent（内置仅可禁用；套件/团队拥有的不在此删）。
#[tauri::command]
pub fn delete_expert(services: State<'_, AppState>, id: String) -> Result<(), String> {
    services.experts.delete_standalone(&id)
}

/// 专家详情：摘要 + 角色设定正文。
#[tauri::command]
pub fn expert_detail(
    services: State<'_, AppState>,
    id: String,
) -> Result<crate::expert::ExpertDetail, String> {
    let mut detail = services.experts.detail(&id)?;
    // 私有技能 owner = agent name；含未启用，供详情展示。
    detail.skills = services
        .skills
        .list_by_expert(&detail.agent.name)
        .unwrap_or_default();
    Ok(detail)
}

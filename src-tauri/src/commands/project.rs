//! 项目协作命令（薄入口）：项目 CRUD + 成员 + 线程（群聊/任务由 session/engine 体系驱动）。
use crate::app_state::AppState;
use crate::project::runtime_skills::ProjectSkillSummary;
use crate::project::{Project, ProjectMember};
use tauri::State;
use tauri_plugin_opener::OpenerExt;

#[tauri::command]
pub fn list_projects(services: State<'_, AppState>) -> Result<Vec<Project>, String> {
    services.projects.list()
}

#[tauri::command]
pub fn create_project(
    services: State<'_, AppState>,
    name: String,
    description: Option<String>,
    instructions: Option<String>,
    workspace_dir: Option<String>,
) -> Result<Project, String> {
    services.projects.create(
        &name,
        description.as_deref().unwrap_or(""),
        instructions.as_deref().unwrap_or(""),
        workspace_dir.as_deref(),
    )
}

/// T59：设项目指令（章程/PM 指令）。
#[tauri::command]
pub fn set_project_instructions(
    services: State<'_, AppState>,
    project_id: String,
    instructions: String,
) -> Result<(), String> {
    services
        .projects
        .set_instructions(&project_id, &instructions)
}

/// T59：更新项目名称与描述。
#[tauri::command]
pub fn update_project(
    services: State<'_, AppState>,
    id: String,
    name: String,
    description: Option<String>,
) -> Result<(), String> {
    services
        .projects
        .update(&id, &name, description.as_deref().unwrap_or(""))
}

/// T59：设项目工作目录（编辑时改用指定目录）。
#[tauri::command]
pub fn set_project_workspace(
    services: State<'_, AppState>,
    project_id: String,
    workspace_dir: String,
) -> Result<(), String> {
    services.projects.set_workspace(&project_id, &workspace_dir)
}

#[tauri::command]
pub fn get_project(services: State<'_, AppState>, id: String) -> Result<Option<Project>, String> {
    services.projects.get(&id)
}

/// 删除项目。**连带删除该项目的项目层记忆**（spec §3.6：记忆是项目从属内容，
/// 项目不存在则其记忆无意义保留）。属破坏性操作，前端删除入口须提示并确认（见 ProjectHome 删除弹窗）。
#[tauri::command]
pub fn delete_project(services: State<'_, AppState>, id: String) -> Result<(), String> {
    services.projects.delete(&id)?;
    // 级联清理该项目记忆（fact/episode），含 FTS 同步。失败不回滚项目删除，仅返回错误供上层提示。
    services.memory.delete_by_project(&id)
}

#[tauri::command]
pub fn list_project_members(
    services: State<'_, AppState>,
    project_id: String,
) -> Result<Vec<ProjectMember>, String> {
    services.projects.list_members(&project_id)
}

/// 项目运行时真实可用的专属技能：项目成员专家私有技能 + 导入团队来源私有技能。
#[tauri::command]
pub fn list_project_skills(
    services: State<'_, AppState>,
    project_id: String,
) -> Result<Vec<ProjectSkillSummary>, String> {
    crate::project::runtime_skills::list_project_runtime_skill_items(
        &services.projects,
        &services.skills,
        |team_id| {
            services
                .teams
                .detail(team_id)
                .ok()
                .map(|detail| detail.team.display_name)
        },
        &project_id,
    )
}

#[tauri::command]
pub fn add_project_member(
    services: State<'_, AppState>,
    project_id: String,
    expert_name: String,
    role_label: Option<String>,
    responsibilities: Option<String>,
    is_coordinator: Option<bool>,
) -> Result<ProjectMember, String> {
    services.projects.add_member(
        &project_id,
        &expert_name,
        role_label.as_deref(),
        responsibilities.as_deref(),
        is_coordinator.unwrap_or(false),
    )
}

#[tauri::command]
pub fn remove_project_member(
    services: State<'_, AppState>,
    member_id: String,
) -> Result<(), String> {
    services.projects.remove_member(&member_id)
}

/// 方案C：从团队导入一名成员到项目——把团队成员定义快照复制成「项目私有副本」，
/// 与源团队解耦（源团队删除/改名不影响项目）。返回新成员。
#[tauri::command]
pub fn import_team_member(
    services: State<'_, AppState>,
    project_id: String,
    team_id: String,
    expert_name: String,
) -> Result<ProjectMember, String> {
    // 角色定义（含正文/工具/模型档位）。
    let (_lead, roster) = services.teams.resolve_for_run(&team_id)?;
    let spec = roster
        .into_iter()
        .find(|s| s.name == expert_name)
        .ok_or_else(|| format!("团队成员不存在：{expert_name}"))?;
    // 展示信息（名称/职业/头像）。
    let summary = services
        .teams
        .detail(&team_id)
        .ok()
        .and_then(|d| d.members.into_iter().find(|m| m.name == expert_name));
    let (display_name, profession, avatar) = match summary {
        Some(s) => (s.display_name, s.profession, s.avatar),
        None => (None, None, None),
    };
    services.projects.add_member_snapshot(
        &project_id,
        &expert_name,
        display_name.as_deref(),
        profession.as_deref(),
        avatar.as_deref(),
        Some(spec.description.as_str()),
        &spec.system_prompt,
        spec.tools.clone(),
        &spec.model_tier,
        Some(team_id.as_str()),
    )
}

/// T62：发送项目草稿时创建项目线程、写入首条消息并启动运行。
#[tauri::command]
pub fn submit_project_draft_message(
    services: State<'_, AppState>,
    project_id: String,
    content: String,
    source_draft_session_id: Option<String>,
    mode: Option<String>,
    permission_mode: Option<String>,
    selected_model_id: Option<String>,
) -> Result<String, String> {
    services.facade.submit_project_draft_message(
        &project_id,
        &content,
        source_draft_session_id.as_deref(),
        mode.as_deref(),
        permission_mode.as_deref(),
        selected_model_id.as_deref(),
    )
}

/// T59：列项目顶层线程。
#[tauri::command]
pub fn list_project_threads(
    services: State<'_, AppState>,
    project_id: String,
) -> Result<Vec<crate::session::SessionInfo>, String> {
    services.session.list_project_threads(&project_id)
}

/// T59：设项目权限模式（manual|auto|full）。
#[tauri::command]
pub fn set_project_permission_mode(
    services: State<'_, AppState>,
    project_id: String,
    mode: String,
) -> Result<(), String> {
    services.projects.set_permission_mode(&project_id, &mode)
}

/// T59：项目级任务看板投影（跨线程聚合成员 child 运行）。
#[tauri::command]
pub fn list_project_child_runs(
    services: State<'_, AppState>,
    project_id: String,
) -> Result<Vec<crate::project::ProjectChildRun>, String> {
    services.facade.list_project_child_runs(&project_id)
}

/// T59：项目级产物投影（跨线程聚合成员 child 已登记 artifacts）。
#[tauri::command]
pub fn list_project_artifacts(
    services: State<'_, AppState>,
    project_id: String,
) -> Result<Vec<crate::project::ProjectArtifact>, String> {
    services.facade.list_project_artifacts(&project_id)
}

/// T61：项目级任务台账（跨该项目所有线程聚合）。
#[tauri::command]
pub fn list_project_tasks(
    services: State<'_, AppState>,
    project_id: String,
) -> Result<Vec<crate::project::ProjectTask>, String> {
    services.projects.tasks_by_project(&project_id)
}

/// T61：某编排线程的任务台账。
#[tauri::command]
pub fn list_thread_tasks(
    services: State<'_, AppState>,
    thread_session_id: String,
) -> Result<Vec<crate::project::ProjectTask>, String> {
    services.projects.list_tasks(&thread_session_id)
}

/// 打开项目共享工作目录（成员任务产出落此处）。
#[tauri::command]
pub fn open_project_workspace(
    app: tauri::AppHandle,
    services: State<'_, AppState>,
    project_id: String,
) -> Result<(), String> {
    let workspace = services.facade.ensure_project_workspace(&project_id)?;
    app.opener()
        .open_path(workspace, None::<String>)
        .map_err(|err| format!("打开工作目录失败：{err}"))
}

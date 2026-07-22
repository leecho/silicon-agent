//! 伴随体（agent 实例）命令（薄入口）。用户面「智能体」。
//! agent = expert 软复制指令播种的持久实例 + 私有记忆 + 跨会话身份。委托 `AgentService`。
use crate::agent::AgentRecord;
use crate::app_state::AppState;
use crate::project::runtime_skills::ProjectSkillSummary;
use tauri::State;
use tauri_plugin_opener::OpenerExt;

/// 由源 expert 播种一个伴随体（软复制其指令、引用其技能）。
/// `name`（唯一标识）由后端从 `display_name` 派生；前端只传显示名。
#[tauri::command]
pub fn create_agent(
    services: State<'_, AppState>,
    source_expert: String,
    display_name: String,
) -> Result<AgentRecord, String> {
    services
        .agents
        .create_from_expert(&services.experts, &source_expert, &display_name)
}

/// 列出全部伴随体（按 name 升序）。
#[tauri::command]
pub fn list_agents(services: State<'_, AppState>) -> Result<Vec<AgentRecord>, String> {
    services.agents.list()
}

/// 伴随体详情。
#[tauri::command]
pub fn agent_detail(services: State<'_, AppState>, id: String) -> Result<AgentRecord, String> {
    services
        .agents
        .get_by_id(&id)?
        .ok_or_else(|| format!("伴随体不存在：{id}"))
}

/// 智能体会话列表：只返回直接使用该持久智能体的顶层会话。
#[tauri::command]
pub fn list_agent_sessions(
    services: State<'_, AppState>,
    agent_id: String,
) -> Result<Vec<crate::session::SessionInfo>, String> {
    services.session.list_agent_threads(&agent_id)
}

/// 智能体任务台账：跨该智能体会话聚合任务事实。
#[tauri::command]
pub fn list_agent_tasks(
    services: State<'_, AppState>,
    agent_id: String,
) -> Result<Vec<crate::project::ProjectTask>, String> {
    services.projects.tasks_by_agent(&agent_id)
}

/// 智能体产物列表：跨该智能体会话聚合已登记产物。
#[tauri::command]
pub fn list_agent_artifacts(
    services: State<'_, AppState>,
    agent_id: String,
) -> Result<Vec<crate::project::ProjectArtifact>, String> {
    services.facade.list_agent_artifacts(&agent_id)
}

/// 智能体运行时可用的专属技能：由其源 expert 私有技能继承而来。
#[tauri::command]
pub fn list_agent_skills(
    services: State<'_, AppState>,
    agent_id: String,
) -> Result<Vec<ProjectSkillSummary>, String> {
    let agent = services
        .agents
        .get_by_id(&agent_id)?
        .ok_or_else(|| format!("伴随体不存在：{agent_id}"))?;
    let Some(source_expert_id) = agent
        .source_expert_id
        .clone()
        .filter(|value| !value.trim().is_empty())
    else {
        return Ok(Vec::new());
    };
    let source_name = agent
        .display_name
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| source_expert_id.clone());
    Ok(services
        .skills
        .list_enabled_by_expert(&source_expert_id)?
        .into_iter()
        .map(|skill| ProjectSkillSummary {
            skill,
            source_kind: "expert".into(),
            source_id: source_expert_id.clone(),
            source_name: source_name.clone(),
        })
        .collect())
}

/// 打开智能体专属工作目录。旧数据若缺工作目录，会按默认目录创建并回填。
#[tauri::command]
pub fn open_agent_workspace(
    app: tauri::AppHandle,
    services: State<'_, AppState>,
    agent_id: String,
) -> Result<(), String> {
    let workspace = services.agents.ensure_workspace(&agent_id)?;
    app.opener()
        .open_path(workspace.to_string_lossy().into_owned(), None::<String>)
        .map_err(|err| format!("打开工作目录失败：{err}"))
}

/// 保存（编辑）伴随体：前端回传整条记录，刷新 updated_at 后落库。
#[tauri::command]
pub fn update_agent(
    services: State<'_, AppState>,
    record: AgentRecord,
) -> Result<AgentRecord, String> {
    services.agents.save(record)
}

/// 切换启用状态。
#[tauri::command]
pub fn toggle_agent(
    services: State<'_, AppState>,
    id: String,
    enabled: bool,
) -> Result<(), String> {
    services.agents.toggle(&id, enabled)
}

/// 设置「我的」分组（None=移出）。
#[tauri::command]
pub fn set_agent_group(
    services: State<'_, AppState>,
    id: String,
    group_id: Option<String>,
) -> Result<(), String> {
    services.agents.set_group(&id, group_id.as_deref())
}

/// 删除伴随体：级联删其私有记忆（agent_id=id），历史会话保留（agent_id 指向已删伴随体时 UI 降级）。
#[tauri::command]
pub fn delete_agent(services: State<'_, AppState>, id: String) -> Result<(), String> {
    services.memory.delete_by_agent(&id)?;
    services.agents.delete(&id)
}

// ---- T73 自我演化 ----

/// 设置「允许自我演化」开关。
#[tauri::command]
pub fn set_evolution_enabled(
    services: State<'_, AppState>,
    id: String,
    enabled: bool,
) -> Result<(), String> {
    services.agents.set_evolution_enabled(&id, enabled)
}

/// 列出某伴随体的 SOUL 版本史（新在前）。
#[tauri::command]
pub fn list_soul_versions(
    services: State<'_, AppState>,
    id: String,
) -> Result<Vec<crate::agent::SoulVersion>, String> {
    services.agents.list_soul_versions(&id)
}

/// 批准一个待批准的 SOUL 提案：设为活跃并同步注入用人格。
#[tauri::command]
pub fn approve_soul_proposal(
    services: State<'_, AppState>,
    id: String,
    version_id: String,
) -> Result<(), String> {
    services.agents.approve_soul(&id, &version_id)
}

/// 拒绝一个待批准的 SOUL 提案。
#[tauri::command]
pub fn reject_soul_proposal(
    services: State<'_, AppState>,
    version_id: String,
) -> Result<(), String> {
    services.agents.reject_soul(&version_id)
}

/// 回滚到某历史 SOUL 版本。
#[tauri::command]
pub fn rollback_soul_version(
    services: State<'_, AppState>,
    id: String,
    version_id: String,
) -> Result<(), String> {
    services.agents.rollback_soul(&id, &version_id)
}

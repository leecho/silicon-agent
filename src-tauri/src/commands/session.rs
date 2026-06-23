//! 会话基础 CRUD 命令（薄入口）。
use crate::app_state::AppState;
use crate::session::{new_id, Session, SessionInfo};
use tauri::State;

#[tauri::command]
pub fn list_sessions(services: State<'_, AppState>) -> Result<Vec<SessionInfo>, String> {
    let mut sessions = services.session.list_sessions()?;
    for session in &mut sessions {
        session.is_running = services.coordinator.run_registry().is_running(&session.id);
        session.run_started_at = services
            .coordinator
            .run_registry()
            .run_started_at(&session.id);
    }
    Ok(sessions)
}

#[tauri::command]
pub fn get_default_session(services: State<'_, AppState>) -> Result<Session, String> {
    let now = crate::engine::now_string();
    let session = services.session.get_or_create_default(&now)?;
    services
        .facade
        .session_with_pending(&session.id)?
        .ok_or_else(|| "session not found".into())
}

#[tauri::command]
pub fn create_session(
    services: State<'_, AppState>,
    is_draft: Option<bool>,
) -> Result<SessionInfo, String> {
    let now = crate::engine::now_string();
    let mut session = services.session.create_session(
        &new_id("session"),
        "新会话",
        &now,
        is_draft.unwrap_or(false),
    )?;
    session.is_running = services.coordinator.run_registry().is_running(&session.id);
    session.run_started_at = services
        .coordinator
        .run_registry()
        .run_started_at(&session.id);
    Ok(session)
}

/// 保存草稿内容（前端防抖调用）。
#[tauri::command]
pub fn set_draft_content(
    services: State<'_, AppState>,
    session_id: String,
    content: String,
) -> Result<(), String> {
    let now = crate::engine::now_string();
    services
        .session
        .set_draft_content(&session_id, &content, &now)
}

/// 清理空草稿（app 启动时调用一次）。返回删除条数。
#[tauri::command]
pub fn cleanup_empty_drafts(services: State<'_, AppState>) -> Result<usize, String> {
    services.session.cleanup_empty_drafts()
}

#[tauri::command]
pub fn get_session(
    services: State<'_, AppState>,
    session_id: String,
) -> Result<Option<Session>, String> {
    services.facade.session_with_pending(&session_id)
}

/// 设/清会话运行角色（kind 为空串 = 自由模式；否则 kind∈{"expert","team"} + id）。
#[tauri::command]
pub fn set_session_role(
    services: State<'_, AppState>,
    session_id: String,
    kind: String,
    id: String,
) -> Result<(), String> {
    let now = crate::engine::now_string();
    let kind = if kind.trim().is_empty() {
        None
    } else {
        Some(kind.as_str())
    };
    let id = if id.trim().is_empty() {
        None
    } else {
        Some(id.as_str())
    };
    services.session.set_role(&session_id, kind, id, &now)
}

/// 设/清会话所属持久智能体。智能体是实体归属，独立于专家/团队角色定义。
#[tauri::command]
pub fn set_session_agent(
    services: State<'_, AppState>,
    session_id: String,
    agent_id: Option<String>,
) -> Result<(), String> {
    let now = crate::engine::now_string();
    services
        .session
        .set_agent_id(&session_id, agent_id.as_deref(), &now)
}

#[tauri::command]
pub fn delete_session(services: State<'_, AppState>, session_id: String) -> Result<(), String> {
    // 尽力清理默认沙箱目录（含附件）；失败不阻断删除。显式 working_dir 不删。
    let _ = services.facade.remove_default_workspace(&session_id);
    services.session.delete_session(&session_id)?;
    services.coordinator.clear_cancel_flag(&session_id); // 顺带清取消标记，避免 cancel_flags 泄漏。
    Ok(())
}

#[tauri::command]
pub fn rename_session(
    services: State<'_, AppState>,
    session_id: String,
    title: String,
) -> Result<SessionInfo, String> {
    let now = crate::engine::now_string();
    services
        .session
        .update_session_title(&session_id, &title, &now)?;
    let mut session = services
        .session
        .get_session(&session_id)?
        .ok_or_else(|| format!("session not found: {session_id}"))?;
    session.is_running = services.coordinator.run_registry().is_running(&session.id);
    session.run_started_at = services
        .coordinator
        .run_registry()
        .run_started_at(&session.id);
    Ok(session)
}

//! 会话运行/交互命令：发送消息、权限/提问/计划决定、运行期相关设置（薄入口）。
use crate::app_state::AppState;
use crate::session::Session;
use tauri::State;

/// 异步命令：前台落用户消息，后台 OS 线程跑引擎，立即返回当前详情（含 is_running=true）。
/// run 生命周期通过 run_started/run_finished 事件通知前端；引擎流式事件在 run_loop 内 emit。
/// 后台线程与 WebView 生命周期解耦——刷新/重开不终止 run，reload 后前端可从事件恢复态。
#[tauri::command]
pub async fn submit_user_message(
    services: State<'_, AppState>,
    session_id: String,
    content: String,
) -> Result<Session, String> {
    if content.trim().is_empty() {
        return Err("请输入消息".into());
    }
    eprintln!(
        "[cmd] submit_user_message 会话={session_id} 内容长度={}",
        content.chars().count()
    );
    // 落消息/标题/运行锁/后台跑引擎全部走 AppState::spawn_user_message（与远程接入共用同一路径）。
    services
        .coordinator
        .spawn_user_message(&session_id, &content)?;
    // 立即返回当前 detail（含已落用户消息 + is_running=true）。终态后续走事件。
    services
        .facade
        .session_with_pending(&session_id)?
        .ok_or_else(|| "session not found".into())
}

/// T70：列会话任务队列（在飞队头 + 排队项）。前端排队条数据源。
#[tauri::command]
pub async fn list_session_queue(
    services: State<'_, AppState>,
    session_id: String,
) -> Result<Vec<crate::session::task_queue::SessionTaskItem>, String> {
    services.coordinator.list_queue(&session_id)
}

/// T70：取消一个排队中的任务项（不影响在飞队头），返回取消后的队列。
#[tauri::command]
pub async fn cancel_queued_task(
    services: State<'_, AppState>,
    session_id: String,
    item_id: String,
) -> Result<Vec<crate::session::task_queue::SessionTaskItem>, String> {
    services
        .coordinator
        .cancel_queued_item(&session_id, &item_id)
}

/// 异步命令：处理权限决定（批准/拒绝）后续跑引擎，返回最新会话详情。
///
/// - `approved=true`：会话级授权该工具（同工具后续自动放行），然后重入引擎续跑。
/// - `approved=false`：落一条"用户拒绝"工具结果（使其不再 pending），然后重入引擎续跑
///   （模型见到拒绝结果后改道）。
///
/// 前端传 `{ sessionId, toolCallId, approved }`（camelCase）。
#[tauri::command]
pub async fn submit_permission_decision(
    services: State<'_, AppState>,
    session_id: String,
    tool_call_id: String,
    approved: bool,
) -> Result<Session, String> {
    eprintln!("[cmd] submit_permission_decision 会话={session_id} tool_call={tool_call_id} approved={approved}");
    services
        .coordinator
        .spawn_permission_decision(&session_id, &tool_call_id, approved)?;
    services
        .facade
        .session_with_pending(&session_id)?
        .ok_or_else(|| "session not found".into())
}

/// 处理 ask_user 的用户回答：把每题答案格式化后作为该 ask_user 调用的 tool 结果落库，然后续跑引擎。
#[tauri::command]
pub async fn submit_ask_response(
    services: State<'_, AppState>,
    session_id: String,
    tool_call_id: String,
    answers: Vec<Vec<String>>,
) -> Result<Session, String> {
    eprintln!("[cmd] submit_ask_response 会话={session_id} tool_call={tool_call_id}");
    services
        .coordinator
        .spawn_ask_response(&session_id, &tool_call_id, answers)?;
    services
        .facade
        .session_with_pending(&session_id)?
        .ok_or_else(|| "session not found".into())
}

/// 取消 ask_user 的回答并停止本轮：落一条「已取消」工具结果解析掉 pending，不续跑引擎。
#[tauri::command]
pub async fn cancel_ask_response(
    services: State<'_, AppState>,
    session_id: String,
    tool_call_id: String,
) -> Result<Session, String> {
    eprintln!("[cmd] cancel_ask_response 会话={session_id} tool_call={tool_call_id}");
    services
        .coordinator
        .cancel_pending_ask(&session_id, &tool_call_id)?;
    services
        .facade
        .session_with_pending(&session_id)?
        .ok_or_else(|| "session not found".into())
}

/// 设/清会话级权限模式覆盖。`mode=None` 清除覆盖回归全局默认。
#[tauri::command]
pub async fn set_session_permission_mode(
    services: State<'_, AppState>,
    session_id: String,
    mode: Option<String>,
) -> Result<Session, String> {
    if let Some(m) = &mode {
        if !matches!(m.as_str(), "manual" | "auto" | "full") {
            return Err(format!("非法权限模式: {m}"));
        }
    }
    let now = crate::engine::now_string();
    services
        .session
        .set_session_permission_mode(&session_id, mode.as_deref(), &now)?;
    services
        .facade
        .session_with_pending(&session_id)?
        .ok_or_else(|| "session not found".into())
}

/// 读全局默认权限模式。
#[tauri::command]
pub async fn get_global_permission_mode(services: State<'_, AppState>) -> Result<String, String> {
    services.app_settings.get_global_permission_mode()
}

/// 设全局默认权限模式。
#[tauri::command]
pub async fn set_global_permission_mode(
    services: State<'_, AppState>,
    mode: String,
) -> Result<(), String> {
    if !matches!(mode.as_str(), "manual" | "auto" | "full") {
        return Err(format!("非法权限模式: {mode}"));
    }
    services.app_settings.set_global_permission_mode(&mode)
}

/// 读「每轮结束后生成快捷建议」开关（默认开）。
#[tauri::command]
pub fn get_suggestions_enabled(services: State<'_, AppState>) -> Result<bool, String> {
    services.app_settings.get_suggestions_enabled()
}

/// 写「每轮结束后生成快捷建议」开关。
#[tauri::command]
pub fn set_suggestions_enabled(services: State<'_, AppState>, enabled: bool) -> Result<(), String> {
    services.app_settings.set_suggestions_enabled(enabled)
}

/// 读辅助模型 id（标题/建议生成用）。None = 跟随会话模型。
#[tauri::command]
pub fn get_aux_model_id(services: State<'_, AppState>) -> Result<Option<String>, String> {
    services.app_settings.get_aux_model_id()
}

/// 写辅助模型 id；null/空 表示清除（回退会话模型）。
#[tauri::command]
pub fn set_aux_model_id(
    services: State<'_, AppState>,
    model_id: Option<String>,
) -> Result<(), String> {
    services.app_settings.set_aux_model_id(model_id.as_deref())
}

/// 处理 propose_plan 的用户裁定（批准 / 评论），落为该 propose_plan 调用的 tool 结果后续跑引擎。
///
/// - `approved=true`：切回执行模式（mode=normal），落一条「计划已批准」工具结果，引擎续跑后
///   模型按计划在普通模式逐步实施。
/// - `approved=false`：保持计划模式，落一条带用户评论的工具结果，引擎续跑后模型据评论修订计划
///   并再次调用 propose_plan（计划卡重弹）。
///
/// 前端传 `{ sessionId, toolCallId, approved, comment? }`（camelCase）。
#[tauri::command]
pub async fn submit_plan_decision(
    services: State<'_, AppState>,
    session_id: String,
    tool_call_id: String,
    approved: bool,
    comment: Option<String>,
) -> Result<Session, String> {
    eprintln!(
        "[cmd] submit_plan_decision 会话={session_id} tool_call={tool_call_id} approved={approved}"
    );
    services
        .coordinator
        .spawn_plan_decision(&session_id, &tool_call_id, approved, comment)?;
    services
        .facade
        .session_with_pending(&session_id)?
        .ok_or_else(|| "session not found".into())
}

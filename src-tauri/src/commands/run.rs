//! 会话运行/交互命令：发送消息、权限/提问/计划决定、运行期相关设置（薄入口）。
use crate::app_state::AppState;
use crate::session::Session;
use tauri::State;

/// `submit_user_message` 结果：当前会话详情 + 本条消息是「入队」还是「即时起跑」。
///
/// T70：忙时消息入队（不进 feed），空闲时起跑（落 feed）。后端的这一判定**必须**回传前端，
/// 否则前端的乐观气泡无从对账——当前端 `busy` 与后端实际繁忙态不一致（队列排空边界/重连那一拍）时，
/// 乐观气泡会变成与排队条并存的「孤儿已送达消息」。
#[derive(serde::Serialize)]
pub struct SubmitOutcome {
    pub session: Session,
    /// true=已入队（未起跑、不在 feed）；false=已即时起跑（落 feed）。
    pub queued: bool,
}

/// 异步命令：前台落用户消息或入队，后台 OS 线程跑引擎，立即返回当前详情 + 入队/起跑标记。
/// run 生命周期通过 run_started/run_finished 事件通知前端；引擎流式事件在 run_loop 内 emit。
/// 后台线程与 WebView 生命周期解耦——刷新/重开不终止 run，reload 后前端可从事件恢复态。
#[tauri::command]
pub async fn submit_user_message(
    services: State<'_, AppState>,
    session_id: String,
    content: String,
) -> Result<SubmitOutcome, String> {
    if content.trim().is_empty() {
        return Err("请输入消息".into());
    }
    eprintln!(
        "[cmd] submit_user_message 会话={session_id} 内容长度={}",
        content.chars().count()
    );
    // 落消息/标题/运行锁/后台跑引擎全部走 AppState::spawn_user_message（与远程接入共用同一路径）。
    // 返回值：true=入队、false=起跑。前端据此对账乐观气泡。
    let queued = services
        .coordinator
        .spawn_user_message(&session_id, &content)?;
    // 立即返回当前 detail（is_running=true）。终态后续走事件。
    let session = services
        .facade
        .session_with_pending(&session_id)?
        .ok_or_else(|| "session not found".to_string())?;
    Ok(SubmitOutcome { session, queued })
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

/// 异步命令：处理权限决定（批准/拒绝），返回最新会话详情。
///
/// - `approved=true`：会话级授权该工具（同工具后续自动放行），然后重入引擎续跑。
/// - `approved=false`：收口该 tool_call（落"用户拒绝"结果使其不再 pending）并**立即停止会话**，
///   不再续跑——用户拒绝即视为「停手」，不让 agent 改道继续。
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

/// 增强消息：把输入框草稿润色 + 补全为结构清晰、指令明确的提示词（一次性辅助 LLM 调用）。
///
/// 返回改写后的正文供前端直接回填；空草稿原样返回（不调模型）。模型选择复用
/// `resolve_aux_selection`（辅助模型 → 会话所选模型 → 全局默认），草稿会话传空 session_id 即可。
///
/// **必须 async + `spawn_blocking`**：`complete_model` 是阻塞调用（最长 30s 超时），直接放在同步命令里
/// 会卡死 UI 主线程（与 title/suggestions 一律用后台线程跑 LLM 同理）。这里把阻塞工作丢到
/// 阻塞线程池、await 其结果，命令本身不占用异步运行时线程。
#[tauri::command]
pub async fn enhance_message(
    services: State<'_, AppState>,
    session_id: String,
    text: String,
) -> Result<String, String> {
    if text.trim().is_empty() {
        return Ok(text);
    }
    // 克隆所需 Arc 句柄移入阻塞线程（State 借用不跨 await）。
    let gateway = services.gateway.clone();
    let session = services.session.clone();
    let app_settings = services.app_settings.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let selection = crate::aux_gen::shared::resolve_aux_selection(
            &gateway,
            &session,
            &app_settings,
            &session_id,
        );
        crate::aux_gen::enhance::enhance_message(&gateway, selection, &session_id, &text)
    })
    .await
    .map_err(|e| format!("增强任务执行失败：{e}"))?
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

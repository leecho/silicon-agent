//! 会话组织命令：停止、置顶/分组/模式、分组 CRUD（薄入口）。
use crate::app_state::{now_string, AppState};
use crate::session::SessionGroup;
use tauri::State;

/// 请求停止指定 session 正在进行的运行（设置取消标记，引擎在检查点处停止并保留已产出）。
/// 同步命令即可——只 set 一个原子标记、瞬时返回。
#[tauri::command]
pub fn stop_session(services: State<'_, AppState>, session_id: String) -> Result<(), String> {
    // 统一停止：置 stop 意图 + 级联子 + reconcile 收敛到静止态（活动运行的父/子由其 run 循环自停后收尾）。
    services.coordinator.stop(&session_id);
    Ok(())
}

/// 置顶 / 取消置顶某会话。
#[tauri::command]
pub fn set_session_pinned(
    services: State<'_, AppState>,
    session_id: String,
    pinned: bool,
) -> Result<(), String> {
    services
        .session
        .set_session_pinned(&session_id, pinned, &now_string())
}

/// 把会话归入某分组（`Some`）或移出分组（`None`）。
#[tauri::command]
pub fn set_session_group(
    services: State<'_, AppState>,
    session_id: String,
    group_id: Option<String>,
) -> Result<(), String> {
    services
        .session
        .set_session_group(&session_id, group_id.as_deref(), &now_string())
}

/// 设置会话工作模式（"normal" | "plan"）。非法模式 → Err。
#[tauri::command]
pub fn set_session_mode(
    services: State<'_, AppState>,
    session_id: String,
    mode: String,
) -> Result<(), String> {
    services
        .session
        .set_session_mode(&session_id, &mode, &now_string())
}

/// 新建分组。color 为用户取色器选定的十六进制色（#RRGGBB）；缺省回退 gray。
#[tauri::command]
pub fn create_session_group(
    services: State<'_, AppState>,
    label: String,
    color: Option<String>,
) -> Result<SessionGroup, String> {
    let color_key = color
        .filter(|c| !c.trim().is_empty())
        .unwrap_or_else(|| "gray".into());
    services
        .session
        .create_session_group(&label, &color_key, &now_string())
}

/// 编辑分组名称与颜色（内建分组不可编辑）。color 为十六进制色（#RRGGBB）；缺省回退 gray。
#[tauri::command]
pub fn update_session_group(
    services: State<'_, AppState>,
    id: String,
    label: String,
    color: Option<String>,
) -> Result<SessionGroup, String> {
    let color_key = color
        .filter(|c| !c.trim().is_empty())
        .unwrap_or_else(|| "gray".into());
    services
        .session
        .update_session_group(&id, &label, &color_key)
}

/// 列出全部分组（按创建时间升序）。
#[tauri::command]
pub fn list_session_groups(services: State<'_, AppState>) -> Result<Vec<SessionGroup>, String> {
    services.session.list_session_groups()
}

/// 删除分组（专家会话回到「最近」）。
#[tauri::command]
pub fn delete_session_group(services: State<'_, AppState>, group_id: String) -> Result<(), String> {
    services.session.delete_session_group(&group_id)
}

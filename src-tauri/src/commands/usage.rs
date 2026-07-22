//! 用量分析命令（薄入口）。
use crate::app_state::{now_string, AppState};
use crate::usage::{
    ContextUsageView, ScopedUsageView, UsageAnalyticsView, UsageMessageRow, UsageTotals,
};
use tauri::State;

/// 读取用量分析聚合。range ∈ {"all","30d","7d"}；非法值按 "all" 处理（resolve_cutoff 兜底）。
#[tauri::command]
pub fn get_usage_analytics(
    services: State<'_, AppState>,
    range: String,
) -> Result<UsageAnalyticsView, String> {
    let now_epoch: i64 = now_string().parse().unwrap_or(0);
    services.usage.analytics(&range, now_epoch)
}

/// 单会话上下文窗口占用，供 composer 的 context meter 展示。
///
/// 已用量取该会话最近一次主体调用的真实用量（provider 统计，无本地分词）；上限按生效模型
/// 查表得出。首条消息发出前无用量记录，返回 used=0、percent=0（语义即「尚未占用上下文」）。
#[tauri::command]
pub fn get_session_context_usage(
    services: State<'_, AppState>,
    session_id: String,
) -> Result<ContextUsageView, String> {
    let latest = services.usage.latest_session_usage(&session_id)?;
    // 模型名：优先用最近一次调用实际使用的模型；无记录则取会话生效模型（失效回退默认）。
    let model = match &latest {
        Some((m, _)) => m.clone(),
        None => {
            let selected = services.session.get_selected_model_id(&session_id)?;
            match services.provider.resolve_selection(selected.as_deref()) {
                Ok(r) => r.model,
                Err(_) => String::new(),
            }
        }
    };
    let used_tokens = latest.map(|(_, u)| u).unwrap_or(0);
    // 上限：优先该模型在设置页配置的覆盖值，否则内置查表兜底。
    let max_tokens = services.provider.context_limit_for(&model).max(1) as u64;
    let percent = ((used_tokens.saturating_mul(100)) / max_tokens).min(100) as u8;
    Ok(ContextUsageView {
        used_tokens,
        max_tokens,
        percent,
        model,
    })
}

/// 单会话累计 token 用量（供 composer 的累计 chip 展示）。
#[tauri::command]
pub fn get_session_usage(
    services: State<'_, AppState>,
    session_id: String,
) -> Result<UsageTotals, String> {
    services.usage.session_totals(&session_id)
}

/// 项目维度用量详情（总计 + 按会话）。range ∈ {"all","30d","7d"}。
#[tauri::command]
pub fn get_project_usage(
    services: State<'_, AppState>,
    project_id: String,
    range: String,
) -> Result<ScopedUsageView, String> {
    let now_epoch: i64 = now_string().parse().unwrap_or(0);
    services.usage.project_usage(&project_id, &range, now_epoch)
}

/// 智能体维度用量详情（总计 + 按会话，递归归属）。range ∈ {"all","30d","7d"}。
#[tauri::command]
pub fn get_agent_usage(
    services: State<'_, AppState>,
    agent_id: String,
    range: String,
) -> Result<ScopedUsageView, String> {
    let now_epoch: i64 = now_string().parse().unwrap_or(0);
    services.usage.agent_usage(&agent_id, &range, now_epoch)
}

/// 单会话的按消息用量（会话→消息二层展开用）。
#[tauri::command]
pub fn get_session_message_usage(
    services: State<'_, AppState>,
    session_id: String,
) -> Result<Vec<UsageMessageRow>, String> {
    services.usage.session_message_usage(&session_id)
}

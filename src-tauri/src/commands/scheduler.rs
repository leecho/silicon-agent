use tauri::State;

use crate::app_state::AppState;
use crate::scheduler::now_secs;
use crate::scheduler::schedule::{normalize_to_cron, ScheduleInput};
use crate::scheduler::timing::next_after_local;
use crate::scheduler::types::{ScheduledTask, TaskExecution, TaskInput};
use crate::session::new_id;

/// 前端创建/更新入参。schedule 为预设或 cron；后端归一化为 6 字段 cron。
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduledTaskInput {
    pub name: String,
    pub prompt: String,
    pub schedule: ScheduleInput,
    pub schedule_display: Option<String>,
    pub working_dir: Option<String>,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub role_kind: Option<String>,
    #[serde(default)]
    pub role_id: Option<String>,
    /// 会话权限模式（manual/auto/full）；缺省时后端兜底为 full（新建默认 full）。
    #[serde(default)]
    pub permission_mode: Option<String>,
    /// 运行模型 id；None=全局默认。
    #[serde(default)]
    pub model_id: Option<String>,
}

fn build_task_input(input: &ScheduledTaskInput) -> Result<(TaskInput, String), String> {
    if input.name.trim().is_empty() {
        return Err("任务名不能为空".into());
    }
    if input.prompt.trim().is_empty() {
        return Err("需求内容不能为空".into());
    }
    // 校验权限模式取值；缺省兜底 full（定时任务默认全自主）。
    let permission_mode = match input.permission_mode.as_deref() {
        None => Some("full".to_string()),
        Some("manual") | Some("auto") | Some("full") => input.permission_mode.clone(),
        Some(other) => return Err(format!("非法权限模式：{other}")),
    };
    let project_id = normalize_non_empty(input.project_id.as_deref());
    let agent_id = normalize_non_empty(input.agent_id.as_deref());
    if project_id.is_some() && agent_id.is_some() {
        return Err("定时任务不能同时绑定项目和智能体".into());
    }
    let role_kind = normalize_non_empty(input.role_kind.as_deref());
    let role_id = normalize_non_empty(input.role_id.as_deref());
    let (role_kind, role_id) = match (role_kind, role_id) {
        (Some(kind @ ("expert" | "team")), Some(id)) => {
            (Some(kind.to_string()), Some(id.to_string()))
        }
        (None, None) => (None, None),
        (Some(other), Some(_)) => return Err(format!("非法角色类型：{other}")),
        _ => return Err("角色类型和角色 id 必须同时提供".into()),
    };
    let spec = normalize_to_cron(&input.schedule)?;
    Ok((
        TaskInput {
            name: input.name.trim().into(),
            prompt: input.prompt.clone(),
            schedule_spec: spec.clone(),
            schedule_display: input.schedule_display.clone(),
            working_dir: input.working_dir.clone(),
            project_id: project_id.map(str::to_string),
            agent_id: agent_id.map(str::to_string),
            role_kind,
            role_id,
            permission_mode,
            model_id: input.model_id.clone(),
        },
        spec,
    ))
}

fn normalize_non_empty(value: Option<&str>) -> Option<&str> {
    value.and_then(|v| {
        let trimmed = v.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

#[tauri::command]
pub fn create_scheduled_task(
    services: State<'_, AppState>,
    input: ScheduledTaskInput,
) -> Result<ScheduledTask, String> {
    let (task_input, spec) = build_task_input(&input)?;
    let now = now_secs();
    let next = next_after_local(&spec, now)?;
    services
        .tasks
        .create_task(&new_id("task"), &task_input, now, Some(next))
}

#[tauri::command]
pub fn update_scheduled_task(
    services: State<'_, AppState>,
    id: String,
    input: ScheduledTaskInput,
) -> Result<ScheduledTask, String> {
    let (task_input, spec) = build_task_input(&input)?;
    let now = now_secs();
    let next = next_after_local(&spec, now)?;
    services
        .tasks
        .update_task(&id, &task_input, Some(next), now)?;
    services
        .tasks
        .get_task(&id)?
        .ok_or_else(|| "任务不存在".into())
}

#[tauri::command]
pub fn delete_scheduled_task(
    services: State<'_, AppState>,
    id: String,
    delete_sessions: bool,
) -> Result<(), String> {
    if delete_sessions {
        // 删除该任务产生的所有执行会话（尽力，失败不中断）。
        for exec in services.tasks.list_executions(Some(&id), None)? {
            if !exec.session_id.is_empty() {
                let _ = services.session.delete_session(&exec.session_id);
            }
        }
    }
    services.tasks.delete_task(&id)
}

#[tauri::command]
pub fn set_task_enabled(
    services: State<'_, AppState>,
    id: String,
    enabled: bool,
) -> Result<ScheduledTask, String> {
    let now = now_secs();
    let next = if enabled {
        let task = services.tasks.get_task(&id)?.ok_or("任务不存在")?;
        Some(next_after_local(&task.schedule_spec, now)?)
    } else {
        None
    };
    services.tasks.set_enabled(&id, enabled, next, now)?;
    services
        .tasks
        .get_task(&id)?
        .ok_or_else(|| "任务不存在".into())
}

#[tauri::command]
pub fn list_scheduled_tasks(services: State<'_, AppState>) -> Result<Vec<ScheduledTask>, String> {
    services.tasks.list_tasks()
}

#[tauri::command]
pub fn get_scheduled_task(
    services: State<'_, AppState>,
    id: String,
) -> Result<Option<ScheduledTask>, String> {
    services.tasks.get_task(&id)
}

#[tauri::command]
pub fn list_task_executions(
    services: State<'_, AppState>,
    task_id: Option<String>,
    status: Option<String>,
) -> Result<Vec<TaskExecution>, String> {
    services
        .tasks
        .list_executions(task_id.as_deref(), status.as_deref())
}

/// 手动立即触发一次。复用 runner::fire_run_public（trigger=manual）。
/// 返回本次新建的 session id（前端据此跳转到会话页）；任务已在运行/触发失败时返回 None。
#[tauri::command]
pub fn run_task_now(
    services: State<'_, AppState>,
    app: tauri::AppHandle,
    id: String,
) -> Result<Option<String>, String> {
    let task = services.tasks.get_task(&id)?.ok_or("任务不存在")?;
    Ok(crate::scheduler::runner::fire_run_public(
        &app, &task, "manual",
    ))
}

/// 读取当前 runtime 是否持有系统唤醒 guard；这是定时任务页开关的事实源。
#[tauri::command]
pub fn get_keep_system_awake(services: State<'_, AppState>) -> Result<bool, String> {
    let slot = services.keep_awake.lock().map_err(|_| "锁中毒")?;
    Ok(slot.is_some())
}

#[tauri::command]
pub fn set_keep_system_awake(services: State<'_, AppState>, enabled: bool) -> Result<(), String> {
    use crate::scheduler::keepawake::KeepAwakeGuard;
    let mut slot = services.keep_awake.lock().map_err(|_| "锁中毒")?;
    if enabled {
        if slot.is_none() {
            *slot = Some(KeepAwakeGuard::acquire()?);
        }
    } else {
        *slot = None; // drop guard → 释放
    }
    Ok(())
}

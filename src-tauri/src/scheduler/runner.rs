use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Manager};

use crate::app_state::AppState;
use crate::scheduler::types::ScheduledTask;
use crate::scheduler::{now_secs, plan_startup_one_local, timing};
use crate::session::new_id;

const TICK: Duration = Duration::from_secs(30);

/// 调度器句柄：drop 时通知线程退出。
pub struct Scheduler {
    stop: Arc<AtomicBool>,
}

impl Scheduler {
    /// 启动后台调度线程（detached）。在 lib.rs setup 中 app.manage 之后调用。
    pub fn start(app: AppHandle) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = stop.clone();
        std::thread::spawn(move || {
            // 启动 catch-up：错过的槽位各补一次。
            run_startup(&app);
            loop {
                if stop_thread.load(Ordering::Relaxed) {
                    break;
                }
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run_tick(&app);
                }));
                std::thread::sleep(TICK);
            }
        });
        Self { stop }
    }
}

impl Drop for Scheduler {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn run_startup(app: &AppHandle) {
    let state = app.state::<AppState>();
    let now = now_secs();
    let tasks = match state.tasks.enabled_tasks() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("[sched] 启动读取任务失败：{e}");
            return;
        }
    };
    for task in tasks {
        match plan_startup_one_local(&task.schedule_spec, task.next_run_at, now) {
            Ok(plan) => {
                if plan.fire {
                    fire_run(app, &state, &task, "catchup");
                    // 任务实际触发：同时更新 next_run_at 和 last_run_at。
                    let _ = state.tasks.set_next_run(&task.id, plan.new_next, now);
                } else {
                    // 任务未触发，仅重排 next_run_at，不误盖 last_run_at。
                    let _ = state.tasks.set_next_run_only(&task.id, plan.new_next, now);
                }
            }
            Err(e) => eprintln!("[sched] 启动规划失败 task={}：{e}", task.id),
        }
    }
}

fn run_tick(app: &AppHandle) {
    let state = app.state::<AppState>();
    let now = now_secs();
    let due = match state.tasks.due_tasks(now) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("[sched] due_tasks 失败：{e}");
            return;
        }
    };
    for task in due {
        fire_run(app, &state, &task, "schedule");
        match timing::next_after_local(&task.schedule_spec, now) {
            Ok(next) => {
                let _ = state.tasks.set_next_run(&task.id, next, now);
            }
            Err(e) => eprintln!("[sched] 重算 next 失败 task={}：{e}", task.id),
        }
    }
}

/// 触发任务一次：TOCTOU 原子声明 → 解析会话 → 后台跑引擎 → 收尾更新。
/// 返回本次运行新建的 session id；被跳过（已在运行）或建会话/引擎失败时返回 None。
/// 手动「立即执行」据此跳转到该会话页。
fn fire_run(
    app: &AppHandle,
    state: &AppState,
    task: &ScheduledTask,
    trigger: &str,
) -> Option<String> {
    let now = now_secs();

    // 解析目标会话（每次运行都新建）。必须在 try_begin_execution 之前建好 session_id，
    // 因为 try_begin_execution 需要将 session_id 写入执行记录。
    let session_id = match resolve_session(state, task) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[sched] 解析会话失败 task={}：{e}", task.id);
            // 无法建会话：写一条 failed 执行（session_id 空串）。
            let exec_id = new_id("exec");
            let _ = state
                .tasks
                .create_execution(&exec_id, &task.id, "", trigger, now);
            let _ = state
                .tasks
                .finish_execution(&exec_id, "failed", Some(&e), now_secs());
            return None;
        }
    };

    let exec_id = new_id("exec");
    // TOCTOU 原子守护：在单个事务内检查 running 执行并声明，防止 tick 与 run_task_now 双重触发。
    match state
        .tasks
        .try_begin_execution(&exec_id, &task.id, &session_id, trigger, now)
    {
        Ok(true) => {} // 成功声明，继续
        Ok(false) => {
            // 已有 running 执行：删除刚建的空会话（避免孤儿），写 skipped 记录并返回。
            let _ = state.session.delete_session(&session_id);
            let _ = state
                .tasks
                .create_skipped_execution(&new_id("exec"), &task.id, trigger, now);
            return None;
        }
        Err(e) => {
            eprintln!("[sched] 声明执行失败 task={}：{e}", task.id);
            return None;
        }
    }

    // RunRegistry 占锁（同会话不并发）。
    let guard = match state.coordinator.run_registry().try_begin(&session_id) {
        Some(g) => g,
        None => {
            // 会话已声明执行记录但无法占 RunRegistry 锁：标记为 skipped。
            let _ = state.tasks.finish_execution(&exec_id, "skipped", None, now);
            return None;
        }
    };

    // 把任务的权限模式 + 模型写入新建会话，引擎按会话级系统生效（headless 仅影响 ask_user）。
    let _ = state.session.set_session_permission_mode(
        &session_id,
        task.permission_mode.as_deref(),
        &crate::engine::now_string(),
    );
    let _ = state.session.set_selected_model_id(
        &session_id,
        task.model_id.as_deref(),
        &crate::engine::now_string(),
    );

    // 构建 headless 引擎（会话已带权限模式/模型）。
    let engine = match state.facade.engine_for_task(&session_id) {
        Ok(e) => e,
        Err(e) => {
            let _ = state
                .tasks
                .finish_execution(&exec_id, "failed", Some(&e), now_secs());
            return None;
        }
    };
    let cancel = state.coordinator.cancel_flag(&session_id);
    cancel.store(false, Ordering::Relaxed);

    let prompt = task.prompt.clone();
    let sid = session_id.clone();
    let app2 = app.clone();
    let task_id = task.id.clone();
    let task_name = task.name.clone();
    let exec2 = exec_id.clone();

    std::thread::spawn(move || {
        let outcome = {
            let _guard = guard;
            let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                engine.submit_user_message(&sid, &prompt, cancel)
            }));
            match res {
                Ok(Ok((_, Some(_)))) => ("needs_attention", None),
                Ok(Ok((_, None))) => ("completed", None),
                Ok(Err(err)) => ("failed", Some(err)),
                Err(_) => ("failed", Some("引擎线程 panic".to_string())),
            }
        };
        let (status, err) = outcome;
        if let Some(st) = app2.try_state::<AppState>() {
            let _ = st
                .tasks
                .finish_execution(&exec2, status, err.as_deref(), now_secs());
            st.coordinator.clear_cancel_flag(&sid);
        }
        // 通知 + 前端事件（见 Task 7）。
        notify_finish(&app2, &task_id, &task_name, &exec2, status);
    });

    Some(session_id)
}

/// 公开入口，供 run_task_now 命令调用。返回本次新建的 session id（跳过/失败时 None）。
pub fn fire_run_public(app: &AppHandle, task: &ScheduledTask, trigger: &str) -> Option<String> {
    let state = app.state::<AppState>();
    fire_run(app, &state, task, trigger)
}

/// 每次运行都新建会话，并标记来源为 scheduled（不归入任何分组）。
/// SessionManager 据 origin 白名单过滤，这些会话只在 TaskTree 三层树里出现。
fn resolve_session(state: &AppState, task: &ScheduledTask) -> Result<String, String> {
    let id = new_id("session");
    // 默认标题带运行时刻，便于在 TaskTree 里区分同一任务的多次运行（可被重命名覆盖）。
    let title = format!(
        "{} · {}",
        task.name,
        chrono::Local::now().format("%m-%d %H:%M")
    );
    state
        .session
        .create_session(&id, &title, &crate::engine::now_string(), false)?;
    // 标记来源（尽力；失败仅意味着会话短暂被当作 user 来源，不影响功能）。
    let _ = state.session.set_session_origin(&id, "scheduled");
    let now = crate::engine::now_string();
    if let Some(project_id) = task
        .project_id
        .as_deref()
        .filter(|id| !id.trim().is_empty())
    {
        state.session.set_project_id(&id, project_id, &now)?;
        let ws = state.facade.ensure_project_workspace(project_id)?;
        state.session.set_working_dir(&id, &ws, &now)?;
    } else if let Some(agent_id) = task.agent_id.as_deref().filter(|id| !id.trim().is_empty()) {
        state.session.set_agent_id(&id, Some(agent_id), &now)?;
    }
    if let Some(kind) = task
        .role_kind
        .as_deref()
        .filter(|kind| !kind.trim().is_empty())
    {
        state
            .session
            .set_role(&id, Some(kind), task.role_id.as_deref(), &now)?;
    }
    // 应用任务配置的工作目录（沙箱根）。项目任务固定使用项目工作目录；非项目任务目录存在才设置。
    if task.project_id.is_none() {
        if let Some(dir) = task.working_dir.as_deref() {
            if std::path::Path::new(dir).is_dir() {
                let _ = state.session.set_working_dir(&id, dir, &now);
            } else {
                eprintln!(
                    "[sched] 任务 {} 配置的工作目录不存在，退回默认沙箱：{dir}",
                    task.id
                );
            }
        }
    }
    Ok(id)
}

/// 执行收尾：发 OS 通知 + scheduled_task_event（驱动前端刷新）。
pub fn notify_finish(app: &AppHandle, task_id: &str, task_name: &str, exec_id: &str, status: &str) {
    use tauri::Emitter;
    // 1) OS 通知
    let (title, body) = match status {
        "completed" => ("定时任务完成", format!("✓ {task_name} 已完成")),
        "needs_attention" => ("定时任务需确认", format!("⏸ {task_name} 需要你的确认")),
        "failed" => ("定时任务失败", format!("✗ {task_name} 运行失败")),
        _ => ("定时任务", format!("{task_name}：{status}")),
    };
    {
        use tauri_plugin_notification::NotificationExt;
        let _ = app.notification().builder().title(title).body(&body).show();
    }
    // 2) 前端事件
    let _ = app.emit(
        "scheduled_task_event",
        serde_json::json!({
            "taskId": task_id,
            "executionId": exec_id,
            "status": status,
        }),
    );
}

pub mod keepawake;
pub mod runner;
pub mod schedule;
pub mod store;
pub mod timing;
pub mod types;

pub use store::TaskStore;
pub use types::{ScheduledTask, TaskExecution, TaskInput};

/// 当前 epoch 秒（数值，用于时间比较）。与 engine::now_string() 同源（秒）。
pub fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_secs()).unwrap_or(0))
        .unwrap_or(0)
}

/// 启动规划（生产版，本地时区）。语义同 timing::plan_startup_one。
pub fn plan_startup_one_local(
    spec: &str,
    current_next: Option<i64>,
    now: i64,
) -> Result<timing::StartupPlan, String> {
    use timing::{next_after_local, StartupPlan};
    match current_next {
        None => Ok(StartupPlan {
            fire: false,
            new_next: next_after_local(spec, now)?,
        }),
        Some(next) if next <= now => Ok(StartupPlan {
            fire: true,
            new_next: next_after_local(spec, now)?,
        }),
        Some(next) => Ok(StartupPlan {
            fire: false,
            new_next: next,
        }),
    }
}

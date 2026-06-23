//! 进程内看门狗：周期扫描过期租约与停泊孤儿，交 `reconcile` 收敛到静止态。
//!
//! 覆盖「运行超时/挂死」与「进程内停泊孤儿」：心跳过期 → 判定死 run → 回收租约（bump token，使僵尸
//! 线程的后续写入失效）→ reconcile。随进程生灭的 detached 线程。

use std::time::Duration;

use tauri::{AppHandle, Manager};

use crate::app_state::AppState;

/// 心跳过期阈值（spec §4）：远超最长合法单步（模型调用），又能在合理时间兜住真挂死。
pub const RUN_STALE_TIMEOUT_MS: u64 = 5 * 60 * 1000; // 5 分钟
/// 看门狗扫描间隔。
pub const WATCHDOG_TICK: Duration = Duration::from_secs(30);

/// 纯判定：给定过期会话与停泊孤儿集合，返回需 reconcile 的去重列表（保序）。
/// 把 IO 留给调用方，保证可单测。
pub fn sessions_to_reconcile(stale: Vec<String>, parked_orphans: Vec<String>) -> Vec<String> {
    let mut out = stale;
    for p in parked_orphans {
        if !out.contains(&p) {
            out.push(p);
        }
    }
    out
}

/// 启动进程内看门狗线程（detached，随进程生灭）。须在 `app.manage(state)` 之后调用。
pub fn start(app: AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(WATCHDOG_TICK);
        let st = app.state::<AppState>();
        st.coordinator.watchdog_tick();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedups_stale_and_parked() {
        let r = sessions_to_reconcile(
            vec!["a".into(), "b".into()],
            vec!["b".into(), "c".into()],
        );
        assert_eq!(r, vec!["a".to_string(), "b".to_string(), "c".to_string()]);
    }

    #[test]
    fn empty_inputs_empty_output() {
        assert!(sessions_to_reconcile(vec![], vec![]).is_empty());
    }
}

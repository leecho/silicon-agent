//! per-session 运行锁 + 租约：保证同一会话同一时刻只有一个引擎 run 在跑，并携带 token(代次) 与
//! 心跳，供看门狗检测挂死/死 run 并回收。
//!
//! 这是「引擎 run 生命周期」的并发控制，由命令层/调度器在启动 run 前占锁、run 结束析构解锁；
//! Engine 自身不依赖它（它是 run 编排的护栏，而非 ReAct 循环内部逻辑）。

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

/// 单个会话的运行租约。`token` 唯一递增代次；`heartbeat` 由 run 循环刷新（epoch 毫秒，lock-free）。
#[derive(Clone)]
struct Lease {
    token: u64,
    heartbeat: Arc<AtomicU64>,
}

/// per-session「运行中」登记表：保证同一会话同一时刻只有一个引擎 run 在跑。
///
/// 背景：长任务运行期间刷新/重开 app 会再触发一次 submit_user_input；运行锁拦截重复提交，
/// 同时其 `is_running` 让刷新后前端恢复「运行中」。Arc 内核使 `RunGuard` 可 move 进后台线程。
#[derive(Clone, Default)]
pub struct RunRegistry {
    active: Arc<Mutex<HashMap<String, Lease>>>,
    next_token: Arc<AtomicU64>,
}

/// RAII 运行锁：持 Arc（`'static`，可跨线程），在所有退出路径（含 panic 展开）析构时解锁。
/// 析构仅在 token 仍属自己时移除——防止被看门狗回收后误删新租约。
pub struct RunGuard {
    active: Arc<Mutex<HashMap<String, Lease>>>,
    session_id: String,
    token: u64,
    heartbeat: Arc<AtomicU64>,
}

impl RunRegistry {
    /// 尝试为 session 占用运行锁；已被占用则返回 None。
    pub fn try_begin(&self, session_id: &str) -> Option<RunGuard> {
        let mut active = self.active.lock().unwrap();
        if active.contains_key(session_id) {
            // 已在运行：可能是用户重复提交（调用方会返回「处理中」提示），也可能是派发/collect
            // 路径对「已在跑的后台 child」做的预期去重——两者都不必告警，静默返回 None。
            return None;
        }
        let token = self.next_token.fetch_add(1, Ordering::Relaxed) + 1;
        let heartbeat = Arc::new(AtomicU64::new(now_ms()));
        active.insert(session_id.to_string(), Lease { token, heartbeat: heartbeat.clone() });
        eprintln!(
            "[run] 开始：会话 {session_id}（当前运行中会话数={}）",
            active.len()
        );
        Some(RunGuard {
            active: self.active.clone(),
            session_id: session_id.to_string(),
            token,
            heartbeat,
        })
    }

    /// 该会话是否有 run 正在跑（不看心跳新鲜度）。
    pub fn is_running(&self, session_id: &str) -> bool {
        self.active.lock().unwrap().contains_key(session_id)
    }

    /// 该会话是否有**心跳新鲜**的 run（在 timeout_ms 内）。无租约视为不 live。
    pub fn is_live(&self, session_id: &str, timeout_ms: u64) -> bool {
        let active = self.active.lock().unwrap();
        match active.get(session_id) {
            Some(l) => now_ms().saturating_sub(l.heartbeat.load(Ordering::Relaxed)) <= timeout_ms,
            None => false,
        }
    }

    /// 看门狗回收一个（挂死/死）租约：移除，使其 token 失效、可被重新 begin。
    pub fn reclaim(&self, session_id: &str) {
        self.active.lock().unwrap().remove(session_id);
    }

    /// 列出心跳过期的运行中会话（看门狗用）。
    pub fn stale_sessions(&self, timeout_ms: u64) -> Vec<String> {
        let active = self.active.lock().unwrap();
        let now = now_ms();
        active
            .iter()
            .filter(|(_, l)| now.saturating_sub(l.heartbeat.load(Ordering::Relaxed)) > timeout_ms)
            .map(|(k, _)| k.clone())
            .collect()
    }

    /// 该会话当前 run 的开始/最近心跳时间（Unix epoch 秒字符串）。未运行则返回 None。
    pub fn run_started_at(&self, session_id: &str) -> Option<String> {
        self.active
            .lock()
            .unwrap()
            .get(session_id)
            .map(|l| (l.heartbeat.load(Ordering::Relaxed) / 1000).to_string())
    }
}

impl RunGuard {
    /// 本租约代次（看门狗回收后会变）。
    pub fn token(&self) -> u64 {
        self.token
    }

    /// run 循环在检查点调用，刷新心跳（lock-free）。
    pub fn beat(&self) {
        self.heartbeat.store(now_ms(), Ordering::Relaxed);
    }

    /// 心跳句柄，可 clone 进引擎以在循环检查点刷新。
    pub fn heartbeat_handle(&self) -> Arc<AtomicU64> {
        self.heartbeat.clone()
    }
}

impl Drop for RunGuard {
    fn drop(&mut self) {
        let mut active = self.active.lock().unwrap();
        // 仅当当前租约 token 仍是自己时移除（防回收后误删新租约）。
        if active.get(&self.session_id).map(|l| l.token) == Some(self.token) {
            active.remove(&self.session_id);
            eprintln!("[run] 结束：会话 {}", self.session_id);
        }
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::RunRegistry;

    #[test]
    fn run_lock_blocks_concurrent_then_frees_on_drop() {
        let reg = RunRegistry::default();
        assert!(!reg.is_running("s1"));
        let g1 = reg.try_begin("s1").expect("first acquire succeeds");
        assert!(reg.is_running("s1"), "占锁后应 is_running");
        assert!(reg.try_begin("s1").is_none(), "并发同会话应被拒");
        let g2 = reg.try_begin("s2").expect("其它会话不受影响");
        assert!(reg.is_running("s2"));
        drop(g1);
        assert!(!reg.is_running("s1"), "解锁后 is_running 应为 false");
        assert!(reg.try_begin("s1").is_some(), "解锁后应能再次占用");
        drop(g2);
    }

    #[test]
    fn run_started_at_exists_only_while_running() {
        let reg = RunRegistry::default();
        assert_eq!(reg.run_started_at("s1"), None);

        let guard = reg.try_begin("s1").expect("run starts");
        let started_at = reg.run_started_at("s1").expect("started_at is recorded");
        assert!(!started_at.is_empty());

        drop(guard);
        assert_eq!(reg.run_started_at("s1"), None);
    }

    #[test]
    fn lease_carries_token_and_heartbeat() {
        let reg = RunRegistry::default();
        let g = reg.try_begin("s1").expect("begin");
        assert!(reg.is_live("s1", 60_000), "刚 begin 心跳新鲜");
        let g2 = reg.try_begin("s2").expect("begin2");
        assert_ne!(g.token(), g2.token(), "token 唯一");
        drop(g);
        drop(g2);
    }

    #[test]
    fn reclaim_invalidates_old_guard_drop() {
        let reg = RunRegistry::default();
        let g = reg.try_begin("s1").expect("begin");
        let old = g.token();
        reg.reclaim("s1"); // 看门狗回收
        let g2 = reg.try_begin("s1").expect("rebegin");
        assert_ne!(old, g2.token(), "回收后新 token");
        drop(g); // 老 guard drop 不应误删新租约
        assert!(reg.is_running("s1"), "老 guard drop 不应移除新租约");
        drop(g2);
        assert!(!reg.is_running("s1"));
    }

    #[test]
    fn stale_heartbeat_is_detected() {
        use std::sync::atomic::Ordering;
        let reg = RunRegistry::default();
        let g = reg.try_begin("s1").expect("begin");
        // 强制把心跳设为远古（确定性，不依赖真实时间差）。
        g.heartbeat_handle().store(1, Ordering::Relaxed);
        assert!(!reg.is_live("s1", 60_000), "陈旧心跳 → 不 live");
        assert_eq!(reg.stale_sessions(60_000), vec!["s1".to_string()]);
        // 刷新后转新鲜。
        g.beat();
        assert!(reg.is_live("s1", 60_000), "刷新后 live");
        assert!(reg.stale_sessions(60_000).is_empty());
        drop(g);
    }
}

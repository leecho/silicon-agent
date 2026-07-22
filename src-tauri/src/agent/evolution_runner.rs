//! 演化扫描线程（T73）：常驻后台线程，定期扫描「允许演化」的伴随体，按「频率上限 ∧ 记忆阈值」
//! 触发一次自我反思运行（镜像 `scheduler::runner` 形态 + `fire_run` 的会话/引擎接法）。
//!
//! 状态驱动：触发条件从持久数据（`agents.last_reflection_at` + `memories`）现算，故重启后补跑天然免费。
//! **不依赖调度器**。反思运行复用引擎现有对外接口（`engine_for_task` + `submit_user_message`），不改引擎内部。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Manager};

use crate::agent::model::{should_reflect, AgentRecord};
use crate::app_state::AppState;
use crate::scheduler::now_secs;
use crate::session::new_id;

/// 扫描节奏（粗粒度，演化不赶时间）。
const SCAN_INTERVAL: Duration = Duration::from_secs(600);
/// 频率上限：两次反思最快间隔（防抖）。
const MIN_INTERVAL_SECS: i64 = 7 * 24 * 3600;
/// 记忆阈值：自上次反思以来需新增的私有记忆条数。
const MEMORY_THRESHOLD: i64 = 20;

/// 系统固定反思 prompt（演化的「大脑」）：克制、只提炼稳定模式、禁碰 IDENTITY。
const REFLECTION_PROMPT: &str = "现在进行一次自我反思。\n\n\
你当前的人格（SOUL）与近期与用户相处的经历（私有记忆）已注入到你的上下文中。\n\
请回顾这些近期经历，找出其中**反复出现、稳定**的风格或偏好变化（例如：用户偏好更简洁的回答、\
更喜欢中文术语、对某类话题更谨慎等），把它们**最小必要地**整合进你的人格。\n\n\
要求：\n\
- 只提炼稳定、反复出现的模式；一次性、偶发的事不要写进人格。\n\
- 做最小改动，保留原有人格的稳定部分。\n\
- **绝不改动你的身份锚（IDENTITY）与任何硬性边界**。\n\
- 通过调用 `propose_soul_update(new_soul, summary)` 提交：`new_soul` 是改写后的完整 SOUL 人格正文，\
`summary` 是一句话变更摘要。提案将等待用户批准后才生效。\n\
- 如果近期经历不足以支撑任何稳定的人格调整，就不要调用工具，直接说明「暂无需要固化的变化」。";

/// 演化扫描句柄：drop 时通知线程退出。
pub struct EvolutionScanner {
    stop: Arc<AtomicBool>,
}

impl EvolutionScanner {
    /// 启动后台扫描线程（detached）。在 lib.rs setup 中 app.manage 之后调用。
    pub fn start(app: AppHandle) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = stop.clone();
        std::thread::spawn(move || loop {
            if stop_thread.load(Ordering::Relaxed) {
                break;
            }
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                scan_once(&app);
            }));
            std::thread::sleep(SCAN_INTERVAL);
        });
        Self { stop }
    }
}

impl Drop for EvolutionScanner {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn scan_once(app: &AppHandle) {
    let state = app.state::<AppState>();
    let now = now_secs();
    let agents = match state.agents.list() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("[evolve] 读取伴随体失败：{e}");
            return;
        }
    };
    for agent in agents {
        if !agent.evolution_enabled {
            continue;
        }
        let count = state
            .memory
            .count_since(&agent.id, agent.last_reflection_at.unwrap_or(0))
            .unwrap_or(0);
        if !should_reflect(
            now,
            agent.last_reflection_at,
            count,
            MIN_INTERVAL_SECS,
            MEMORY_THRESHOLD,
        ) {
            continue;
        }
        // 触发即落锚：防同轮重复、防重启后立刻重触发（即便提案后续被拒）。
        let _ = state.agents.mark_reflected(&agent.id, now);
        fire_reflection(app, &state, &agent);
    }
}

/// 触发一次反思运行（镜像 `scheduler::runner::fire_run` 精简版）：新建 agent 绑定会话 →
/// headless 引擎跑系统反思 prompt → 模型调 `propose_soul_update` 产待批准提案。
fn fire_reflection(app: &AppHandle, state: &AppState, agent: &AgentRecord) -> Option<String> {
    let now = crate::engine::now_string();
    let sid = new_id("session");
    let who = agent
        .display_name
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| agent.name.clone());
    let title = format!("{who} · 自我反思");
    if let Err(e) = state.session.create_session(&sid, &title, &now, false) {
        eprintln!("[evolve] 建反思会话失败 agent={}：{e}", agent.id);
        return None;
    }
    // 会话双绑定：set_role 驱动人格注入（引擎按 role_kind="agent" 注入 IDENTITY⧺SOUL）；
    // set_agent_id 驱动私有记忆作用域 + propose_soul_update 解析当前伴随体。
    let _ = state
        .session
        .set_role(&sid, Some("agent"), Some(&agent.id), &now);
    let _ = state.session.set_agent_id(&sid, Some(&agent.id), &now);

    // RunRegistry 占锁（同会话不并发）。反思每次新建会话，正常不会冲突。
    let guard = match state.coordinator.run_registry().try_begin(&sid) {
        Some(g) => g,
        None => {
            eprintln!("[evolve] 反思会话占锁失败 agent={}", agent.id);
            return None;
        }
    };
    let engine = match state.facade.engine_for_task(&sid) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("[evolve] 构建反思引擎失败 agent={}：{e}", agent.id);
            return None;
        }
    };
    let cancel = state.coordinator.cancel_flag(&sid);
    cancel.store(false, Ordering::Relaxed);

    let sid2 = sid.clone();
    let app2 = app.clone();
    std::thread::spawn(move || {
        {
            let _guard = guard;
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                engine.submit_user_message(&sid2, REFLECTION_PROMPT, cancel)
            }));
        }
        if let Some(st) = app2.try_state::<AppState>() {
            st.coordinator.clear_cancel_flag(&sid2);
        }
    });
    Some(sid)
}

//! 伴随体（agent 实例）模块：T67 腾出的 `agent` 落为**持久运行实例**。
//!
//! agent = expert 的持久实例 = 软复制指令(身份) + 引用源 expert 技能(能力) + 私有记忆 + 跨会话身份。
//! 与 `expert/`（无状态模板层）并列、正交：独立表 `agents`、独立 store/service。
//! 运行实例层既有标识符（`AgentStreamEvent`/`agent_run_id`/`subagent`）语义为「一次运行」，与本实体正交共存。

pub mod evolution_runner;
pub mod model;
pub mod service;
pub mod soul_store;
pub mod store;

pub use model::{AgentRecord, SoulVersion};
pub use service::AgentService;

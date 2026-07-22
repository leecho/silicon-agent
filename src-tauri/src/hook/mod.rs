//! Plugin hooks（T66 P1）：插件声明的 hooks 在工具/会话生命周期点确定性触发，随插件启停。
//!
//! v1 范围：事件 `PreToolUse`/`PostToolUse`/`SessionStart`/`Stop`，仅 `command` 类型。
//! - 解析见 `plugin::manifest`（中性 `ParsedHook`）。
//! - 注册表 [`HookService`]：进程内 `plugin_id -> Vec<HookRule>`，随插件启停 set/remove，非持久化。
//! - 执行 [`runner::run_command_hook`]：在会话工作目录起子进程、超时、stdin 收事件 JSON、
//!   stdout 读控制 JSON（仅 PreToolUse 可 `{"decision":"block","reason":...}` 阻止该工具）。
//!
//! 安全：hook 子进程仅在会话工作目录、超时、错误非致命；PreToolUse 只能拦不能放。

mod runner;
mod service;

pub use runner::{run_command_hook, HookOutcome};
pub use service::{HookRule, HookService};

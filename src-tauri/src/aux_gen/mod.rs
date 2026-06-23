//! 会话级辅助模型生成：标题、快捷建议。
//!
//! 这些是对会话的「二次 LLM 调用」（首条消息 → 简短标题、对话片段 → 下一步建议），
//! 独立于主 ReAct 引擎 run。收在本模块，使 Tauri command 回归薄入口、模型调用细节
//! 不外泄到命令层。后续若新建 `agent/**`（模型驱动决策）可再并入。
//!
//! 本模块不持久化跨域事实，只做：构造模型请求 → 调用 provider → 解析输出 → 写回各 owner store / emit。
//!
//! 命名用 `aux_gen` 而非 `aux`：`aux` 是 Windows 保留设备名，目录名为 `aux` 会破坏 Windows 检出/构建。

pub mod shared;
pub mod suggestions;
pub mod title;

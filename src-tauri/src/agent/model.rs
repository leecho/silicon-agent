//! 伴随体（agent 实例）领域类型。
//!
//! agent = expert 的**持久运行实例**：软复制其指令(身份/人格，含 identity) + 按 `source_expert_id`
//! 引用源 expert 的技能(能力) + per-agent 私有记忆 + 跨会话身份。区别于 `expert/`（无状态模板层）。
//! 边界：本文件只定义类型，不含 SQL / 文件系统 / 业务流程（镜像 expert/model.rs）。

use serde::{Deserialize, Serialize};

/// `agents` 索引表的一行：一个伴随体实例。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRecord {
    pub id: String,
    pub name: String,
    /// 软复制自源 expert 的 system_prompt，此后伴随体自有、可编辑（= **SOUL** 可演化人格层）。
    pub instructions: String,
    /// **IDENTITY 稳定锚**（T74）：名字/角色/硬边界；只人工编辑，自我演化（T73）绝不改。
    /// 创建时由 display_name 种；存量伴随体为空（注入时空则退化为仅 SOUL，与拆分前逐字等价）。
    pub identity: String,
    /// T73：per-伴随体「允许自我演化」开关，默认 false。
    pub evolution_enabled: bool,
    /// T73：上次**触发反思运行**的 epoch 秒；None=从未。演化扫描线程据此做频率上限 + 补跑判定。
    pub last_reflection_at: Option<i64>,
    /// 工具白名单（软复制自源 expert.tools，可增删）。
    pub tools: Vec<String>,
    /// 模型档位："main" | "aux"。
    pub model_tier: String,
    /// **运行时技能引用解析键 + 溯源**：源 expert 的唯一名（= skills 表 expert owner 键）。
    /// None / 源 expert 已删 → 运行降级为仅全局技能池（不崩）。
    pub source_expert_id: Option<String>,
    /// 专属工作目录（T69+）：绑定该智能体的会话未显式设目录时默认用它，使产出跨会话落同一文件夹。
    /// None/空 → 回退会话级默认（`{workspace_base}/sessions/{id}`）。
    pub working_dir: Option<String>,
    pub display_name: Option<String>,
    pub profession: Option<String>,
    pub avatar: Option<String>,
    pub color: Option<String>,
    pub enabled: bool,
    pub group_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// 组合注入正文：IDENTITY（锚）在前、SOUL（人格）在后。
/// identity 为空 → 退化为仅 soul（与 T74 拆分前的 `instructions` 注入逐字等价）。
pub fn compose_persona(identity: &str, soul: &str) -> String {
    let id = identity.trim();
    if id.is_empty() {
        soul.to_string()
    } else {
        format!("{id}\n\n{soul}")
    }
}

/// SOUL 版本（T73）：活跃版 / 待批准提案 / 历史归档。
/// 活跃版 `soul` 与该伴随体 `agents.instructions`（T74 的 SOUL 别名）保持一致。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SoulVersion {
    pub id: String,
    pub agent_id: String,
    pub soul: String,
    /// "active"（当前生效，每伴随体至多一条）/ "pending"（待批准提案）/ "archived"（历史/已回滚）。
    pub status: String,
    /// 人类可读的变更摘要（反思生成，供审阅 diff）。
    pub summary: String,
    /// "seed"（创建初版）/ "reflection"（反思提案）/ "manual"（人工编辑）。
    pub source: String,
    pub created_at: String,
}

/// T73 演化触发判定：频率上限（防抖）∧ 记忆阈值（真正的闸）。
/// `last_reflection_at = None`（从未反思）视为间隔已满足。状态驱动，故重启后现算即补跑。
pub fn should_reflect(
    now: i64,
    last_reflection_at: Option<i64>,
    new_memory_count: i64,
    min_interval_secs: i64,
    memory_threshold: i64,
) -> bool {
    let interval_ok = match last_reflection_at {
        None => true,
        Some(t) => now - t >= min_interval_secs,
    };
    interval_ok && new_memory_count >= memory_threshold
}

#[cfg(test)]
mod tests {
    use super::{compose_persona, should_reflect};

    #[test]
    fn empty_identity_is_soul_verbatim() {
        assert_eq!(compose_persona("", "灵魂正文"), "灵魂正文");
        assert_eq!(compose_persona("   ", "灵魂正文"), "灵魂正文");
    }

    #[test]
    fn identity_prepended_with_blank_line() {
        assert_eq!(compose_persona("你是小研。", "灵魂正文"), "你是小研。\n\n灵魂正文");
    }

    #[test]
    fn reflect_when_never_reflected_and_enough_memory() {
        assert!(should_reflect(1_000, None, 20, 600, 20));
    }

    #[test]
    fn no_reflect_when_interval_not_elapsed() {
        // 距上次仅 100s < 600s 上限 → 不触发，即便记忆够多。
        assert!(!should_reflect(1_000, Some(900), 50, 600, 20));
    }

    #[test]
    fn no_reflect_when_memory_below_threshold() {
        assert!(!should_reflect(10_000, Some(0), 19, 600, 20));
    }

    #[test]
    fn reflect_when_interval_and_memory_both_ok() {
        assert!(should_reflect(10_000, Some(0), 20, 600, 20));
    }
}

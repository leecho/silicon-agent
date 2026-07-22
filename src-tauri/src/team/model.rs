//! team 模块领域类型：团队来源、成员引用、持久化行。
//!
//! 边界：本文件只定义类型，不含 SQL / 文件系统 / 业务流程（镜像 agent/skill 的 model.rs）。
//!
//! team = `lead`（其正文作主助手 SOP）+ `members`（对 agent 的有序**引用**）+ 可选私有组件
//! （私有 skill/agent 落 skills/agents 表的 `team_id`，不在本表内）。

use serde::{Deserialize, Serialize};

/// 团队来源。`User`=用户自建；`Imported`=由团队包导入；`Builtin`=随 app 内嵌（不可删）；
/// `Plugin`=经统一装载入口由一个 plugin 包运送进来（T106「运送≠合并」）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TeamSource {
    User,
    Imported,
    Builtin,
    // 注：曾有 `Plugin` 变体（T106「team 由 plugin 运送」）。T108 确立三体系分立后
    // team 不再由 plugin 运送，该变体已移除；遗留值在 `from_str` 里映射为 `Imported`。
}

impl TeamSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            TeamSource::User => "user",
            TeamSource::Imported => "imported",
            TeamSource::Builtin => "builtin",
        }
    }
    /// 从字符串还原；未知按 `User` 兜底。
    ///
    /// 遗留 `"plugin"`（T106 期间由 plugin 运送落库的 team）→ **`Imported`**：
    /// 它们确实是从一个包导入的，落到 `User`（用户自建）兜底会是错的。
    pub fn from_str(s: &str) -> Self {
        match s {
            "imported" | "plugin" => TeamSource::Imported,
            "builtin" => TeamSource::Builtin,
            _ => TeamSource::User,
        }
    }
}

/// 对一个 agent 的**引用**（lead 或 member）。引用对象由 owner 命名空间定位：
/// `plugin_id` 非空=引用某 plugin 提供的全局 agent；`team_id` 非空=引用本 team 的私有 agent；
/// 都空=引用散装 agent。`plugin_id` 与 `team_id` 至多一个非空。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamMember {
    #[serde(default)]
    pub plugin_id: String,
    #[serde(default)]
    pub team_id: String,
    /// 被引用 agent 的 name（在其 owner 命名空间内唯一）。
    pub name: String,
    /// "lead" | "member"。
    pub role: String,
    /// 可选展示覆盖（团队层优先，回退被引用 agent 记录）。
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub profession: Option<String>,
    #[serde(default)]
    pub avatar: Option<String>,
}

/// `teams` 表一行：团队编排定义。私有组件不在此（在 skills/agents 表以 `team_id` 关联）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamRecord {
    pub id: String,
    pub source: TeamSource,
    pub name: String,
    pub display_name: String,
    pub description: String,
    /// lead 引用（其正文作主助手工作 SOP）；可空（无 lead 则主助手用默认 SOP）。
    pub lead: Option<TeamMember>,
    /// 派发 roster（不含 lead）。
    pub members: Vec<TeamMember>,
    pub avatar: Option<String>,
    pub category: Option<String>,
    /// 选中该 team 时的开场引导语（composer 可一键填入）。
    pub quick_prompts: Vec<String>,
    pub enabled: bool,
    pub installed_at: String,
    pub updated_at: String,
    /// 来自广场目录的稳定标识（「加入我的」的副本带；其余 None）。
    pub catalog_id: Option<String>,
    /// 「我的」用户自定义分组 id（未分组为 None）。
    pub group_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::TeamSource;

    /// 遗留 `"plugin"`（T106 期间由 plugin 运送落库的 team）必须映射为 `Imported`，
    /// 不能落到 `User` 兜底 —— 它们确实是从一个包导入的，标成「用户自建」是错的。
    #[test]
    fn legacy_plugin_source_maps_to_imported() {
        assert_eq!(TeamSource::from_str("plugin"), TeamSource::Imported);
        assert_eq!(TeamSource::from_str("imported"), TeamSource::Imported);
        assert_eq!(TeamSource::from_str("builtin"), TeamSource::Builtin);
        assert_eq!(TeamSource::from_str("whatever"), TeamSource::User);
    }

    #[test]
    fn unknown_source_falls_back_to_user() {
        assert_eq!(TeamSource::from_str("nonsense"), TeamSource::User);
    }
}

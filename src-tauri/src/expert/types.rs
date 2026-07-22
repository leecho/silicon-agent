//! agent 模块对 command/前端暴露的 DTO（camelCase 序列化）。

use serde::Serialize;

use crate::expert::model::{ExpertRecord, ExpertSource};

/// 专家详情：摘要 + 角色设定正文（system prompt）。供详情页展示。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpertDetail {
    pub agent: ExpertSummary,
    /// 角色设定正文（运行时读盘）；读不到则空。
    pub system_prompt: String,
    /// 用户引导语（使用该专家的提示词列表）。
    pub quick_prompts: Vec<String>,
    /// 该 agent 的私有技能（owner=agent name，含未启用）；由命令层填充。
    pub skills: Vec<crate::skill::SkillSummary>,
}

/// 专家列表项 / 启停返回。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpertSummary {
    pub id: String,
    pub source: ExpertSource,
    pub name: String,
    pub description: String,
    pub tools: Vec<String>,
    pub model_tier: String,
    pub max_turns: Option<u32>,
    pub role: String,
    /// 归属 plugin id（plugin 提供则非空，否则空）。
    pub plugin_id: String,
    /// 归属 team id（team 私有则非空，否则空）。owner = plugin_id XOR team_id。
    pub team_id: String,
    /// 可选展示身份（纯 UI）。
    pub display_name: Option<String>,
    pub profession: Option<String>,
    pub avatar: Option<String>,
    pub color: Option<String>,
    pub enabled: bool,
    pub installed_at: String,
    /// 来自广场目录的稳定标识（「加入我的」的副本带；其余 None）。
    pub catalog_id: Option<String>,
    /// 「我的」用户自定义分组 id（未分组为 None）。
    pub group_id: Option<String>,
}

impl From<ExpertRecord> for ExpertSummary {
    fn from(r: ExpertRecord) -> Self {
        ExpertSummary {
            id: r.id,
            source: r.source,
            name: r.name,
            description: r.description,
            tools: r.tools,
            model_tier: r.model_tier,
            max_turns: r.max_turns,
            role: r.role,
            plugin_id: r.plugin_id,
            team_id: r.team_id,
            display_name: r.display_name,
            profession: r.profession,
            avatar: r.avatar,
            color: r.color,
            enabled: r.enabled,
            installed_at: r.installed_at,
            catalog_id: r.catalog_id,
            group_id: r.group_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expert::model::{ExpertRecord, ExpertSource};

    #[test]
    fn from_record_maps_fields() {
        let r = ExpertRecord {
            id: "i".into(),
            source: ExpertSource::Builtin,
            name: "explorer".into(),
            description: "只读勘探".into(),
            tools: vec!["grep".into()],
            model_tier: "aux".into(),
            max_turns: Some(8),
            role: "member".into(),
            plugin_id: String::new(),
            team_id: String::new(),
            display_name: None,
            profession: None,
            avatar: None,
            color: None,
            file_name: "explorer.md".into(),
            enabled: true,
            installed_at: "1".into(),
            updated_at: "1".into(),
            catalog_id: None,
            group_id: None,
        };
        let s: ExpertSummary = r.into();
        assert_eq!(s.name, "explorer");
        assert_eq!(s.tools, vec!["grep"]);
        assert_eq!(s.model_tier, "aux");
        assert!(s.enabled);
    }
}

//! team 模块对 command/前端暴露的 DTO（camelCase 序列化）。

use serde::Serialize;

use crate::expert::ExpertSummary;
use crate::team::model::{TeamRecord, TeamSource};

/// 团队列表项 / 启停返回。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamSummary {
    pub id: String,
    pub source: TeamSource,
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub avatar: Option<String>,
    pub category: Option<String>,
    pub enabled: bool,
    pub installed_at: String,
    /// 成员数（不含 lead）。
    pub member_count: usize,
    /// 「我的」用户自定义分组 id（未分组为 None）。
    pub group_id: Option<String>,
    /// 来自广场目录的稳定标识（「加入我的」的副本带；其余 None）。供广场标注「已加入」。
    pub catalog_id: Option<String>,
}

impl TeamSummary {
    pub fn from_record(r: &TeamRecord) -> Self {
        TeamSummary {
            id: r.id.clone(),
            source: r.source,
            name: r.name.clone(),
            display_name: r.display_name.clone(),
            description: r.description.clone(),
            avatar: r.avatar.clone(),
            category: r.category.clone(),
            enabled: r.enabled,
            installed_at: r.installed_at.clone(),
            member_count: r.members.len(),
            group_id: r.group_id.clone(),
            catalog_id: r.catalog_id.clone(),
        }
    }
}

/// 团队详情：元数据 + 解析后的 lead/成员（含展示身份）+ 开场引导语。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamDetail {
    pub team: TeamSummary,
    /// lead 解析后的展示摘要（其正文运行时作 SOP）；引用解析不到则 None。
    pub lead: Option<ExpertSummary>,
    /// 成员解析后的展示摘要（按 members 顺序；解析不到的项跳过）。
    pub members: Vec<ExpertSummary>,
    pub quick_prompts: Vec<String>,
    /// 该团队的私有技能（owner=team id，含未启用）；由命令层填充。
    pub skills: Vec<crate::skill::SkillSummary>,
}

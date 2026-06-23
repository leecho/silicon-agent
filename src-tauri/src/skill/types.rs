//! skill 模块对 command 暴露的 DTO（camelCase 序列化，与前端 TS 对齐）。

use serde::Serialize;

use crate::skill::model::{SkillRecord, SkillSource};

/// 技能列表项 / 启停返回。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillSummary {
    pub id: String,
    pub source: SkillSource,
    pub name: String,
    pub description: String,
    pub enabled: bool,
    pub installed_at: String,
    /// 归属插件 id；None = 非 plugin 拥有。
    pub plugin_id: Option<String>,
    /// 归属 team id；None = 非 team 私有。owner = plugin_id XOR team_id。
    pub team_id: Option<String>,
    /// 是否对用户可见/可调。
    pub user_invocable: bool,
    /// mention 菜单输入提示。
    pub argument_hint: Option<String>,
    /// 「我的」用户自定义分组 id（未分组为 None）。
    pub group_id: Option<String>,
}

impl From<SkillRecord> for SkillSummary {
    fn from(r: SkillRecord) -> Self {
        SkillSummary {
            id: r.id,
            source: r.source,
            name: r.name,
            description: r.description,
            enabled: r.enabled,
            installed_at: r.installed_at,
            plugin_id: r.plugin_id,
            team_id: r.team_id,
            user_invocable: r.user_invocable,
            argument_hint: r.argument_hint,
            group_id: r.group_id,
        }
    }
}

/// 详情抽屉「文件」子 Tab 的一个条目（相对技能目录的路径）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillFile {
    pub rel_path: String,
    pub is_dir: bool,
}

/// 技能详情：元数据 + SKILL.md 原文 + 目录全部文件列表。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillDetail {
    pub skill: SkillSummary,
    pub skill_md: String,
    pub files: Vec<SkillFile>,
}

/// 单文件预览结果。`kind` ∈ markdown|text|image|binary。
/// markdown/text 用 `text`；image 用 `data_url`（后端读字节 base64，前端不直接访问 FS）；binary 两者皆空。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillFilePreview {
    pub kind: String,
    pub text: Option<String>,
    pub data_url: Option<String>,
    pub name: String,
}

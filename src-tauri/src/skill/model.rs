//! skill 模块领域类型：技能来源与持久化行。
//!
//! 边界：本文件只定义类型，不含 SQL、文件系统或业务流程。

use serde::Serialize;

/// 技能来源。`Builtin` 随 app 内嵌、不可卸载、只能禁用；`User` 由安装写入、可卸载。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SkillSource {
    Builtin,
    User,
}

impl SkillSource {
    /// 持久化到 SQLite 的字符串表示。
    pub fn as_str(&self) -> &'static str {
        match self {
            SkillSource::Builtin => "builtin",
            SkillSource::User => "user",
        }
    }
    /// 从 SQLite 字符串还原；未知值按 `User` 兜底（不致命）。
    pub fn from_str(s: &str) -> Self {
        match s {
            "builtin" => SkillSource::Builtin,
            _ => SkillSource::User,
        }
    }
}

/// skills 索引表的一行：磁盘技能在 SQLite 的元数据缓存（正文不入库，运行时读盘）。
/// `dir_name` 是相对技能根目录的目录名（= 技能 name），运行时用 `root.join(dir_name)` 解析。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillRecord {
    pub id: String,
    pub source: SkillSource,
    pub name: String,
    pub description: String,
    pub dir_name: String,
    pub enabled: bool,
    pub installed_at: String,
    pub updated_at: String,
    /// owner 三态之一：归属插件 id（plugin 提供，全局）；`None` = 非 plugin 拥有（持久化哨兵 `''`）。
    pub plugin_id: Option<String>,
    /// owner 三态之一：归属 team id（team 私有，选中才入池）；`None` = 非 team 私有（持久化哨兵 `''`）。
    pub team_id: Option<String>,
    /// owner 之一：归属 agent name（agent 私有，激活/派发该 agent 时才入池）；`None`（哨兵 `''`）= 非 agent 私有。
    /// 不变式：`plugin_id` / `team_id` / `expert_id` 至多一个为 Some。
    pub expert_id: Option<String>,
    /// 对用户可见/可调（默认 true）；`false` 为内部知识库 skill，不进菜单与 system prompt。
    pub user_invocable: bool,
    /// mention 菜单输入提示。
    pub argument_hint: Option<String>,
    /// 「我的」用户自定义分组 id（未分组为 None）。不随 sync 覆盖。
    pub group_id: Option<String>,
}

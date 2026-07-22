//! agent 模块领域类型：专家来源与持久化行。
//!
//! 边界：本文件只定义类型，不含 SQL、文件系统或业务流程（镜像 skill/model.rs）。

use serde::Serialize;

/// 专家来源。`Builtin` 随 app 内嵌、不可卸载、只能禁用；`User` 由安装写入、可卸载。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ExpertSource {
    Builtin,
    User,
    /// 来自某 plugin 的 `agents/` 组件（归属 plugin 由 `ExpertRecord::plugin_id` 记）。
    Plugin,
}

impl ExpertSource {
    /// 持久化到 SQLite 的字符串表示。
    pub fn as_str(&self) -> &'static str {
        match self {
            ExpertSource::Builtin => "builtin",
            ExpertSource::User => "user",
            ExpertSource::Plugin => "plugin",
        }
    }
    /// 从 SQLite 字符串还原；未知值按 `User` 兜底（不致命）。
    pub fn from_str(s: &str) -> Self {
        match s {
            "builtin" => ExpertSource::Builtin,
            "plugin" => ExpertSource::Plugin,
            _ => ExpertSource::User,
        }
    }
}

/// team_members 索引表的一行：磁盘专家角色定义在 SQLite 的元数据缓存（正文不入库，运行时读盘）。
/// `file_name` 为相对 agent 根目录的文件名（= `<name>.md`），运行时 `root.join(file_name)` 解析。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpertRecord {
    pub id: String,
    pub source: ExpertSource,
    pub name: String,
    pub description: String,
    /// 工具白名单（运行时构造受限 Registry 用；本计划仅持久化，不消费）。
    pub tools: Vec<String>,
    /// 模型档位："main"（会话主模型）| "aux"（辅助模型）。
    pub model_tier: String,
    /// 轮数上限；None = 取引擎默认。
    pub max_turns: Option<u32>,
    /// 团队角色："lead" | "member"。散装默认 member。
    pub role: String,
    /// owner 三态之一：归属 plugin id（plugin 提供，全局）；散装/内置/team 私有时为空串 ""。
    pub plugin_id: String,
    /// owner 三态之一：归属 team id（team 私有，选中才生效）；非私有时为空串 ""。
    /// 不变式：`plugin_id` 与 `team_id` 至多一个非空。唯一键 `(plugin_id, team_id, name)`。
    pub team_id: String,
    /// 以下为可选展示身份（纯 UI，不入 ExpertSpec、不影响运行）。
    pub display_name: Option<String>,
    pub profession: Option<String>,
    pub avatar: Option<String>,
    pub color: Option<String>,
    /// 正文文件定位：散装/内置 = 相对 agent 根目录的 `<name>.md`；plugin = 绝对路径。
    pub file_name: String,
    pub enabled: bool,
    pub installed_at: String,
    pub updated_at: String,
    /// 来自广场目录的稳定标识（「加入我的」时写入；自建/导入/内置为 None）。
    pub catalog_id: Option<String>,
    /// 「我的」用户自定义分组 id（未分组为 None）。不随 sync 覆盖。
    pub group_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_roundtrips_through_str() {
        assert_eq!(ExpertSource::from_str("builtin"), ExpertSource::Builtin);
        assert_eq!(ExpertSource::from_str("user"), ExpertSource::User);
        assert_eq!(ExpertSource::from_str("???"), ExpertSource::User); // 未知兜底 User
        assert_eq!(ExpertSource::from_str("plugin"), ExpertSource::Plugin);
        assert_eq!(ExpertSource::Builtin.as_str(), "builtin");
        assert_eq!(ExpertSource::User.as_str(), "user");
        assert_eq!(ExpertSource::Plugin.as_str(), "plugin");
    }
}

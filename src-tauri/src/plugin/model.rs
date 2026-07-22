//! plugin 模块领域类型：插件来源与持久化行。
//!
//! 边界：本文件只定义类型，不含 SQL、文件系统或业务流程。

use serde::Serialize;

/// 插件来源。`Builtin` 随 app 内嵌（本期无）；`User` 由安装写入，可卸载。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PluginSource {
    Builtin,
    User,
}

impl PluginSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            PluginSource::Builtin => "builtin",
            PluginSource::User => "user",
        }
    }
    pub fn from_str(s: &str) -> Self {
        match s {
            "builtin" => PluginSource::Builtin,
            _ => PluginSource::User,
        }
    }
}

/// plugins 索引表的一行：插件元数据缓存。其下 skill 在 skills 表中以 `plugin_id` 关联。
/// `dir_name` 为插件目录相对所在根（`plugins/` 用户或 `builtin-plugins/` 内置）的目录名（= 插件 name）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginRecord {
    pub id: String,
    pub source: PluginSource,
    pub name: String,
    pub display_name: String,
    pub version: String,
    pub description: String,
    pub description_zh: Option<String>,
    pub category: Option<String>,
    pub customized_from: Option<String>,
    pub dir_name: String,
    pub enabled: bool,
    pub installed_at: String,
    pub updated_at: String,
}

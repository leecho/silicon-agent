//! plugin owner 模块：角色工具箱（多 skill 打包）的发现、索引、安装与管理。
//!
//! 边界：插件目录以磁盘 `plugins/<name>/`（用户）与 `builtin-plugins/<name>/`（内置）为准，
//! SQLite `plugins` 表缓存元数据；插件内 skill 写入 skills 表并带 `plugin_id`。
//! 规范来源见 docs/04-specs/2026-06-09-plugin-subsystem-design.md。

pub mod manifest;
pub mod model;
pub mod namespace;
pub mod service;
pub mod store;
pub mod types;
pub mod vars;

pub use model::{PluginRecord, PluginSource};
pub use service::PluginService;
pub use types::{PluginDetail, PluginSummary};

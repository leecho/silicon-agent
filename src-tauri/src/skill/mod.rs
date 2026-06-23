//! skill owner 模块：文件型技能的发现、索引、安装与运行期读取。
//!
//! 边界：技能正文以磁盘 `{skills_root}/<name>/SKILL.md` 为准，SQLite 仅缓存元数据与启用状态。
//! 不在此模块处理 UI 或 provider 调用。

/// 数据目录名（`{home}/.siliconagent`），与 app_state 的 default_workspace_base 对齐。
/// 内置 skill 正文里用 `{{.DataDirName}}` 占位，加载时替换为此值。
pub const DATA_DIR_NAME: &str = ".siliconagent";

pub mod builtin;
pub mod frontmatter;
pub mod model;
pub mod service;
pub mod store;
pub mod types;

pub use model::{SkillRecord, SkillSource};
pub use service::SkillService;
pub use types::{SkillDetail, SkillFile, SkillFilePreview, SkillSummary};

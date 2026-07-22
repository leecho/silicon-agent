//! agent owner 模块：agent 角色**定义**的发现、索引、安装与运行期读取。
//!
//! 分层：本模块是 agent **定义/注册表层**。一份定义既可跑成**主 agent**、也可被 dispatch 成**子运行**
//! （"主/子"是运行时关系，由 `parent_session_id` 体现，不是实体类型）。其上的「团队/专家」是用户面
//! 组合概念（团队可由模型动态编组、或由 plugin 声明派生）。详见设计 §0「分层与来源」。
//!
//! 边界：角色正文以磁盘 `{agents_root}/<name>.md` 为准，SQLite 仅缓存元数据与启用状态。
//! 与 plugin/skill 正交：独立目录、独立表、独立 store/service。
//!
//! 命名：代码层用 `Agent*`（实体）；用户面展示用「团队 / 专家」。

pub mod builtin;
pub mod expert;
pub mod frontmatter;
pub mod model;
pub mod service;
pub mod store;
pub mod types;

pub use model::{ExpertRecord, ExpertSource};
pub use service::{ExpertService, ExpertSpec};
pub use types::{ExpertDetail, ExpertSummary};

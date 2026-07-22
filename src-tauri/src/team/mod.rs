//! team 聚合：silicon-worker 特有的会话级编排单元（lead + members 引用 + 私有组件）。
//! 与 plugin（Claude 式全局能力包）正交：team 引用 agent/skill 货币，plugin 提供其中一部分。

pub mod import;
pub mod model;
pub mod service;
pub mod store;
pub mod types;

pub use model::{TeamMember, TeamRecord, TeamSource};
pub use service::{InlineExpert, TeamService};
pub use types::{TeamDetail, TeamSummary};

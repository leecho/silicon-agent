//! 市场（T109）：**四个各自独立的市场** —— 插件 / 技能 / 专家 / 团队。
//!
//! # 为什么不共用一套「通用市场」
//!
//! 曾经有过一个 `MarketSource` trait + 通用 `MarketItem`，四类货共用。它有两个病：
//!
//! 1. **类型在说谎**：通用详情里塞着 `skills/agents/mcpServers/commands/hooks/teamLead/
//!    teamMembers`，而一个技能会把其中六个填成空数组。
//! 2. **接口在说谎**：静态仓（分片 + 逐文件 GET）和 SkillHub（REST + 服务端分页 + zip）
//!    根本不是一种东西。硬套一个 trait，只能靠默认实现和空返回打补丁。
//!
//! 现在四个市场各写各的：各自的条目类型、各自的服务、各自的命令。冗余一点，
//! 但**加一个新市场 = 新写一个文件，不动任何既有代码**。
//!
//! 顺带消掉一整类 bug：「第三方来源不得上架专家/团队」那道运行时闸门**不用写了** ——
//! 专家/团队市场结构上就没有「来源」这个入口。
//!
//! # 共用的只有传输
//!
//! - [`fetch`]：HTTP + ETag + 磁盘缓存。进去是 URL，出来是字节，不含领域概念。
//! - [`repo`]：静态市场仓的读取器（`market.json` → 货架分片 → 包内文件）。
//!   插件/专家/团队都读这种仓，但**各自映射成自己的类型**。
//! - [`wire`]：静态仓的 JSON 契约与路径安全校验。

pub mod expert_market;
pub mod fetch;
pub mod plugin_market;
pub mod repo;
pub mod skill_market;
pub mod team_market;
pub mod wire;

pub use expert_market::ExpertMarket;
pub use plugin_market::PluginMarket;
pub use skill_market::SkillMarket;
pub use team_market::TeamMarket;

/// silicon 官方静态市场仓：**专家 / 团队**的来源（插件货架目前为空）。
pub const OFFICIAL_MARKET_URL: &str = "https://market.silicower.com";

/// 一个可物化的 skill：元数据 + 目录内全部文件（相对 skill 目录的路径 → 字节）。
/// 落盘时把 `files` 写进受管目录并按 owner（plugin/expert/team）索引。
#[derive(Debug, Clone)]
pub struct MaterializedSkill {
    pub name: String,
    pub description: String,
    pub user_invocable: bool,
    pub argument_hint: Option<String>,
    /// (相对 skill 目录的路径, 字节内容)。
    pub files: Vec<(String, Vec<u8>)>,
}

/// 四个市场的持有者。装进 `AppState`，命令层按货架取用。
pub struct Markets {
    pub plugin: PluginMarket,
    pub skill: SkillMarket,
    pub expert: ExpertMarket,
    pub team: TeamMarket,
}

impl Markets {
    pub fn new(
        db: std::sync::Arc<crate::storage::AppDatabase>,
        cache_dir: std::path::PathBuf,
    ) -> Self {
        Self {
            plugin: PluginMarket::new(db.clone(), OFFICIAL_MARKET_URL, cache_dir.join("plugin")),
            skill: SkillMarket::new(db.clone()),
            expert: ExpertMarket::new(db.clone(), OFFICIAL_MARKET_URL, cache_dir.join("expert")),
            team: TeamMarket::new(db, OFFICIAL_MARKET_URL, cache_dir.join("team")),
        }
    }
}

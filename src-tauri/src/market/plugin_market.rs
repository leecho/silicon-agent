//! **插件市场**：标准 plugin 生态（Claude / Codex）的静态仓货架。
//!
//! **单来源**，和另外三个市场一样。不做「用户订阅任意源」的通用机制 ——
//! 真要接 Claude / Codex 的 marketplace 时，它们的接口形态跟这里未必一样
//! （SkillHub 就是个现成例子：REST + zip，跟静态仓完全两回事），
//! 到时候新写一个市场即可，而不是先造一套注定不合身的通用抽象。
//!
//! 官方仓目前**没有 plugin 货架**（只上架专家/团队），所以这个货架现在是空的 ——
//! 插件仍可从本地目录/zip 安装。

use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;

use crate::market::expert_market::{leaf, write_package};
use crate::market::repo::{provides_labels, StaticRepo};
use crate::storage::AppDatabase;

const DIR: &str = "plugins";
const MANIFEST: &str = "plugin.json";
const SHELF: &str = "plugin";

/// 市场里的一个插件包。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginMarketItem {
    pub name: String,
    pub display_name: String,
    pub version: String,
    pub description: String,
    /// 能力概览标签（如 ["3 技能", "1 MCP"]）。插件的内容是**异质**的 ——
    /// 技能/专家/MCP/命令/钩子都可能有，故用标签而不是某个单一计数。
    pub provides: Vec<String>,
    pub installed: bool,
}

/// 插件详情（安装前预览「这个插件里到底有什么」）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginMarketDetail {
    pub name: String,
    pub display_name: String,
    pub version: String,
    pub description: String,
    pub skills: Vec<String>,
    pub agents: Vec<String>,
    pub mcp_servers: Vec<String>,
    pub commands: Vec<String>,
    pub hooks: usize,
    pub author: Option<String>,
    pub homepage: Option<String>,
    pub installed: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginMarketPage {
    pub items: Vec<PluginMarketItem>,
    pub total: u64,
}

pub struct PluginMarket {
    repo: StaticRepo,
    db: Arc<AppDatabase>,
}

impl PluginMarket {
    pub fn new(db: Arc<AppDatabase>, base_url: &str, cache_dir: PathBuf) -> Self {
        Self {
            repo: StaticRepo::new(base_url, cache_dir),
            db,
        }
    }

    #[cfg(test)]
    pub fn with_repo(db: Arc<AppDatabase>, repo: StaticRepo) -> Self {
        Self { repo, db }
    }

    fn installed_names(&self) -> Result<std::collections::HashSet<String>, String> {
        Ok(crate::plugin::store::list(&self.db)?
            .into_iter()
            .map(|r| r.name)
            .collect())
    }

    pub fn browse(
        &self,
        page: u32,
        page_size: u32,
        keyword: Option<&str>,
    ) -> Result<PluginMarketPage, String> {
        let installed = self.installed_names()?;
        let kw = keyword.map(str::trim).filter(|s| !s.is_empty());

        let matched: Vec<PluginMarketItem> = self
            .repo
            .shelf(SHELF)?
            .into_iter()
            .filter(|e| match kw {
                None => true,
                Some(k) => {
                    let hay = format!(
                        "{} {} {}",
                        e.display_name,
                        e.name,
                        e.description.as_deref().unwrap_or_default()
                    );
                    hay.to_lowercase().contains(&k.to_lowercase())
                }
            })
            .map(|e| PluginMarketItem {
                provides: provides_labels(e.provides.as_ref()),
                installed: installed.contains(&e.name),
                name: e.name,
                display_name: e.display_name,
                version: e.version,
                description: e.description.unwrap_or_default(),
            })
            .collect();

        let total = matched.len() as u64;
        let skip = (page.saturating_sub(1) as usize).saturating_mul(page_size as usize);
        Ok(PluginMarketPage {
            items: matched
                .into_iter()
                .skip(skip)
                .take(page_size as usize)
                .collect(),
            total,
        })
    }

    /// 详情：只拉 `plugin.json` 一个文件——**不**拉整包。
    pub fn detail(&self, name: &str) -> Result<PluginMarketDetail, String> {
        let raw = self.repo.manifest(DIR, name, MANIFEST)?;
        let m = crate::plugin::manifest::parse_manifest(&raw)?;

        Ok(PluginMarketDetail {
            display_name: if m.display_name.trim().is_empty() {
                name.to_string()
            } else {
                m.display_name.clone()
            },
            version: m.version.clone(),
            description: m.description.clone(),
            skills: m.skills.iter().map(|s| leaf(s)).collect(),
            agents: m.agents.iter().map(|a| leaf(a)).collect(),
            mcp_servers: m.mcp_servers.iter().map(|s| s.name.clone()).collect(),
            commands: m.commands.iter().map(|c| leaf(c)).collect(),
            hooks: m.hooks.len(),
            author: m.author.clone(),
            homepage: m.homepage.clone(),
            installed: self.installed_names()?.contains(name),
            name: name.to_string(),
        })
    }

    pub fn materialize(&self, name: &str, dest: &std::path::Path) -> Result<(), String> {
        write_package(&self.repo, DIR, name, dest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::market::fetch::testing::MapFetcher;

    const BASE: &str = "https://market.test";

    fn market(entries: Vec<(&str, &str)>) -> PluginMarket {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dbp =
            std::env::temp_dir().join(format!("siw-plgmkt-{}-{nanos}.db", std::process::id()));
        let db = Arc::new(AppDatabase::open(&dbp).unwrap());
        crate::plugin::store::ensure_schema(&db).unwrap();
        let map = entries
            .into_iter()
            .map(|(p, b)| (format!("{BASE}/{p}"), b.as_bytes().to_vec()))
            .collect();
        let repo = StaticRepo::with_fetcher(BASE, None, Box::new(MapFetcher::new(map)));
        PluginMarket::with_repo(db, repo)
    }

    /// 官方仓**没有 plugin 货架** —— 插件页应显示空货架，而不是加载失败。
    #[test]
    fn missing_plugin_shelf_is_empty_not_error() {
        let m = market(vec![(
            "market.json",
            r#"{"name":"官方","version":4,"shelves":{"expert":"market_expert.json"}}"#,
        )]);
        let page = m.browse(1, 20, None).expect("缺 plugin 货架不应报错");
        assert_eq!(page.total, 0);
    }

    #[test]
    fn browse_and_detail_list_plugin_contents() {
        let m = market(vec![
            (
                "market.json",
                r#"{"name":"官方","version":4,"shelves":{"plugin":"market_plugin.json"}}"#,
            ),
            (
                "market_plugin.json",
                r#"[{"name":"figma","displayName":"Figma","version":"2.0.14",
                     "description":"设计连接器","provides":{"skills":2,"mcpServers":1}}]"#,
            ),
            (
                "plugins/figma/plugin.json",
                r#"{"name":"figma","displayName":"Figma","version":"2.0.14",
                    "description":"设计连接器","skills":["skills/get-context","skills/create-frame"],
                    "mcpServers":{"figma":{"url":"https://mcp.figma.com/mcp"}}}"#,
            ),
        ]);

        let page = m.browse(1, 20, None).expect("列货");
        assert_eq!(page.items[0].display_name, "Figma");
        assert_eq!(page.items[0].provides, vec!["2 技能", "1 MCP"]);

        let d = m.detail("figma").expect("详情");
        assert_eq!(d.skills, vec!["get-context", "create-frame"]);
        assert_eq!(d.mcp_servers, vec!["figma"]);
    }
}

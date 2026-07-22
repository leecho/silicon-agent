//! **团队市场**：silicon 官方静态仓的 `team` 货架（`teams/<name>/team.json`）。
//!
//! 团队同样是 **silicon 自有体系**，只有官方源，故这里也没有「来源」的概念。
//!
//! 团队条目的字段和专家**不一样**：它有主理人与成员，没有「自带技能数」那种单值 ——
//! 这正是四个市场各写各的类型、而不是共用一个通用条目的原因。

use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;

use crate::market::expert_market::{leaf, write_package};
use crate::market::repo::StaticRepo;
use crate::storage::AppDatabase;

const DIR: &str = "teams";
const MANIFEST: &str = "team.json";
const SHELF: &str = "team";

/// 市场里的一个团队包。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamMarketItem {
    pub name: String,
    pub display_name: String,
    pub version: String,
    pub description: String,
    /// 成员数（含主理人）。团队的规模是用户选包时最先看的东西。
    pub member_count: usize,
    pub installed: bool,
}

/// 团队包详情（安装前预览「这个团队里有谁」）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamMarketDetail {
    pub name: String,
    pub display_name: String,
    pub version: String,
    pub description: String,
    /// 主理人（团队的入口成员）。
    pub lead: Option<String>,
    pub members: Vec<String>,
    pub installed: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamMarketPage {
    pub items: Vec<TeamMarketItem>,
    pub total: u64,
}

pub struct TeamMarket {
    repo: StaticRepo,
    db: Arc<AppDatabase>,
}

impl TeamMarket {
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
        Ok(crate::team::store::list(&self.db)?
            .into_iter()
            .map(|r| r.name)
            .collect())
    }

    pub fn browse(
        &self,
        page: u32,
        page_size: u32,
        keyword: Option<&str>,
    ) -> Result<TeamMarketPage, String> {
        let installed = self.installed_names()?;
        let kw = keyword.map(str::trim).filter(|s| !s.is_empty());

        let matched: Vec<TeamMarketItem> = self
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
            .map(|e| TeamMarketItem {
                member_count: e
                    .provides
                    .as_ref()
                    .and_then(|p| p.get("agents"))
                    .and_then(|x| x.as_u64())
                    .unwrap_or(0) as usize,
                installed: installed.contains(&e.name),
                name: e.name,
                display_name: e.display_name,
                version: e.version,
                description: e.description.unwrap_or_default(),
            })
            .collect();

        let total = matched.len() as u64;
        let skip = (page.saturating_sub(1) as usize).saturating_mul(page_size as usize);
        Ok(TeamMarketPage {
            items: matched
                .into_iter()
                .skip(skip)
                .take(page_size as usize)
                .collect(),
            total,
        })
    }

    /// 详情：只拉 `team.json`。主理人/成员取自 `teamInfo`。
    pub fn detail(&self, name: &str) -> Result<TeamMarketDetail, String> {
        let raw = self.repo.manifest(DIR, name, MANIFEST)?;
        let v: serde_json::Value =
            serde_json::from_str(&raw).map_err(|e| format!("team.json 不是合法 JSON：{e}"))?;
        let info = v.get("teamInfo");

        Ok(TeamMarketDetail {
            display_name: v
                .get("displayName")
                .and_then(|x| x.as_str())
                .filter(|s| !s.trim().is_empty())
                .unwrap_or(name)
                .to_string(),
            version: v
                .get("version")
                .and_then(|x| x.as_str())
                .unwrap_or_default()
                .to_string(),
            description: v
                .get("description")
                .and_then(|x| x.as_str())
                .unwrap_or_default()
                .to_string(),
            lead: info
                .and_then(|t| t.get("leadAgent"))
                .and_then(|x| x.as_str())
                .map(leaf),
            members: info
                .and_then(|t| t.get("memberAgents"))
                .and_then(|x| x.as_array())
                .map(|arr| arr.iter().filter_map(|s| s.as_str()).map(leaf).collect())
                .unwrap_or_default(),
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

    fn seeded() -> TeamMarket {
        let dbp = std::env::temp_dir().join(format!(
            "siw-teammkt-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Arc::new(AppDatabase::open(&dbp).unwrap());
        crate::team::store::ensure_schema(&db).unwrap();

        let entries = vec![
            (
                "market.json",
                r#"{"name":"官方","version":4,"shelves":{"team":"market_team.json"}}"#,
            ),
            (
                "market_team.json",
                r#"[{"name":"ai-content-creator-team","displayName":"内容创作专家团",
                     "version":"1.0.0","description":"多模态内容生产","provides":{"agents":7}}]"#,
            ),
            (
                "teams/ai-content-creator-team/team.json",
                r#"{"name":"ai-content-creator-team","displayName":"内容创作专家团",
                    "version":"1.0.0","description":"多模态内容生产",
                    "teamInfo":{"leadAgent":"ai-content-creator-team-lead",
                                "memberAgents":["copywriter","video-editor"]}}"#,
            ),
        ];
        let map = entries
            .into_iter()
            .map(|(p, b)| (format!("{BASE}/{p}"), b.as_bytes().to_vec()))
            .collect();
        let repo = StaticRepo::with_fetcher(BASE, None, Box::new(MapFetcher::new(map)));
        TeamMarket::with_repo(db, repo)
    }

    #[test]
    fn browse_lists_teams_with_member_count() {
        let page = seeded().browse(1, 20, None).expect("列货");
        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].display_name, "内容创作专家团");
        assert_eq!(page.items[0].member_count, 7);
    }

    /// 团队详情要认出主理人与成员 —— 前端据此提示「装完会进团队页」。
    #[test]
    fn detail_reads_lead_and_members() {
        let d = seeded().detail("ai-content-creator-team").expect("详情");
        assert_eq!(d.lead.as_deref(), Some("ai-content-creator-team-lead"));
        assert_eq!(d.members, vec!["copywriter", "video-editor"]);
        assert!(!d.installed);
    }
}

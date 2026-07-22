//! **专家市场**：silicon 官方静态仓的 `expert` 货架（`experts/<name>/expert.json`）。
//!
//! 专家是 **silicon 自有体系**，只有官方源。这里**没有「来源」的概念** ——
//! 不是靠运行时闸门挡住第三方，而是结构上就没有那条路。
//! （旧设计把四类货挤在一个 `MarketSource` 里，就得写一道
//! 「第三方源不许上架 expert/team」的检查，还得在列货与取包两层各挡一次。）

use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;

use crate::market::repo::StaticRepo;
use crate::storage::AppDatabase;

/// 货架目录与清单名（T108：**清单文件名即类型标记**）。
const DIR: &str = "experts";
const MANIFEST: &str = "expert.json";
const SHELF: &str = "expert";

/// 市场里的一个专家包。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpertMarketItem {
    pub name: String,
    pub display_name: String,
    pub version: String,
    pub description: String,
    /// 自带技能数。这些技能是**私有的**（只在选中该专家时载入），故值得在卡片上标出来。
    pub skill_count: usize,
    pub installed: bool,
}

/// 专家包详情（安装前预览）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpertMarketDetail {
    pub name: String,
    pub display_name: String,
    pub version: String,
    pub description: String,
    /// 自带的私有技能名。
    pub skills: Vec<String>,
    pub installed: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpertMarketPage {
    pub items: Vec<ExpertMarketItem>,
    pub total: u64,
}

pub struct ExpertMarket {
    repo: StaticRepo,
    db: Arc<AppDatabase>,
}

impl ExpertMarket {
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
        Ok(crate::expert::store::list(&self.db)?
            .into_iter()
            .map(|r| r.name)
            .collect())
    }

    /// 浏览专家货架。
    ///
    /// 静态仓是**一份分片列全**（silicon-market 只有十几个专家），所以分页/搜索在内存里做。
    /// 这和技能市场（7 万条，必须服务端分页）本就是两种东西 —— 分开写，各自最合适。
    pub fn browse(
        &self,
        page: u32,
        page_size: u32,
        keyword: Option<&str>,
    ) -> Result<ExpertMarketPage, String> {
        let installed = self.installed_names()?;
        let kw = keyword.map(str::trim).filter(|s| !s.is_empty());

        let matched: Vec<ExpertMarketItem> = self
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
            .map(|e| ExpertMarketItem {
                skill_count: e
                    .provides
                    .as_ref()
                    .and_then(|p| p.get("skills"))
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
        Ok(ExpertMarketPage {
            items: matched
                .into_iter()
                .skip(skip)
                .take(page_size as usize)
                .collect(),
            total,
        })
    }

    /// 详情：只拉 `expert.json` 一个文件——**不**拉整包，否则点开详情就等于下载全包。
    pub fn detail(&self, name: &str) -> Result<ExpertMarketDetail, String> {
        let raw = self.repo.manifest(DIR, name, MANIFEST)?;
        let v: serde_json::Value =
            serde_json::from_str(&raw).map_err(|e| format!("expert.json 不是合法 JSON：{e}"))?;

        Ok(ExpertMarketDetail {
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
            skills: v
                .get("skills")
                .and_then(|x| x.as_array())
                .map(|arr| arr.iter().filter_map(|s| s.as_str()).map(leaf).collect())
                .unwrap_or_default(),
            installed: self.installed_names()?.contains(name),
            name: name.to_string(),
        })
    }

    /// 物化整包到 `dest`（含 `expert.json`），装载由命令层交给专家导入器。
    pub fn materialize(&self, name: &str, dest: &std::path::Path) -> Result<(), String> {
        write_package(&self.repo, DIR, name, dest)
    }
}

/// 取路径末段作展示名（`skills/foo` → `foo`、`agents/a.md` → `a`）。
pub(crate) fn leaf(p: &str) -> String {
    let last = p.rsplit(['/', '\\']).next().unwrap_or(p);
    last.strip_suffix(".md").unwrap_or(last).to_string()
}

/// 把静态仓里的一个包逐文件落到 `dest`。专家/团队/插件市场共用（纯落盘，无领域语义）。
pub(crate) fn write_package(
    repo: &StaticRepo,
    dir: &str,
    name: &str,
    dest: &std::path::Path,
) -> Result<(), String> {
    let files = repo.files(dir, name)?;
    if files.is_empty() {
        return Err(format!("该市场未提供包文件：{name}"));
    }
    for (rel, bytes) in files {
        // 纵深防御：repo 已校验过，落盘前再挡一次。
        if !crate::market::wire::is_safe_relative_path(&rel) {
            return Err(format!("包内文件路径非法（疑路径穿越）：{rel}"));
        }
        let path = dest.join(&rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("建目录失败 {}：{e}", parent.display()))?;
        }
        std::fs::write(&path, &bytes).map_err(|e| format!("写文件失败 {}：{e}", path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::market::fetch::testing::MapFetcher;

    const BASE: &str = "https://market.test";

    fn market(entries: Vec<(&str, &str)>) -> ExpertMarket {
        let dbp = std::env::temp_dir().join(format!(
            "siw-expmkt-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Arc::new(AppDatabase::open(&dbp).unwrap());
        crate::expert::store::ensure_schema(&db).unwrap();
        let map = entries
            .into_iter()
            .map(|(p, b)| (format!("{BASE}/{p}"), b.as_bytes().to_vec()))
            .collect();
        let repo = StaticRepo::with_fetcher(BASE, None, Box::new(MapFetcher::new(map)));
        ExpertMarket::with_repo(db, repo)
    }

    fn seeded() -> ExpertMarket {
        market(vec![
            (
                "market.json",
                r#"{"name":"官方","version":4,"shelves":{"expert":"market_expert.json"}}"#,
            ),
            (
                "market_expert.json",
                r#"[{"name":"ai-engineer","displayName":"深网网","version":"1.0.0",
                     "description":"全栈 AI 工程师","provides":{"skills":4}},
                    {"name":"aihot","displayName":"卡兹克","version":"1.0.0",
                     "description":"AI 日报","provides":{"skills":1}}]"#,
            ),
            (
                "experts/ai-engineer/expert.json",
                r#"{"name":"ai-engineer","displayName":"深网网","version":"1.0.0",
                    "description":"全栈 AI 工程师","agents":["agents/ai-engineer.md"],
                    "skills":["skills/deep-research","skills/fullstack-dev"]}"#,
            ),
        ])
    }

    #[test]
    fn browse_lists_experts_with_private_skill_count() {
        let page = seeded().browse(1, 20, None).expect("列货");
        assert_eq!(page.total, 2);
        assert_eq!(page.items[0].name, "ai-engineer");
        assert_eq!(page.items[0].display_name, "深网网");
        // 自带技能是**私有**的（只在选中该专家时载入）—— 卡片上得让用户看见有几个。
        assert_eq!(page.items[0].skill_count, 4);
    }

    #[test]
    fn browse_filters_by_keyword_and_paginates() {
        let m = seeded();
        let hit = m.browse(1, 20, Some("日报")).expect("搜索");
        assert_eq!(hit.total, 1);
        assert_eq!(hit.items[0].name, "aihot");

        // 第二页（每页 1 条）应是第二个专家，而不是重复第一个。
        let p2 = m.browse(2, 1, None).expect("翻页");
        assert_eq!(p2.total, 2, "total 是全量，不随分页变");
        assert_eq!(p2.items.len(), 1);
        assert_eq!(p2.items[0].name, "aihot");
    }

    /// 详情只该拉 `expert.json` —— **不能**把整包拖下来，否则点开详情等于下载全包。
    #[test]
    fn detail_reads_only_the_manifest() {
        let d = seeded().detail("ai-engineer").expect("详情");
        assert_eq!(d.display_name, "深网网");
        assert_eq!(
            d.skills,
            vec!["deep-research", "fullstack-dev"],
            "技能取末段名"
        );
        assert!(!d.installed);
    }
}

//! **技能市场**（T109）：接入 SkillHub（`https://api.skillhub.cn`）。
//!
//! 它和另外三个市场（插件 / 专家 / 团队）是**各自独立**的一套，不共用条目类型 ——
//! 技能有下载量、作者、上游仓库；专家有私有技能；团队有主理人与成员。硬塞进一个
//! 「通用条目」只会让每种货各填一半字段、另一半留空。
//!
//! 两处与静态市场仓的根本差异，决定了它必须自成一套：
//!
//! 1. **规模**：7 万+ 技能。「拉一份分片列全再前端过滤」在这里直接不成立 ——
//!    分页与关键词搜索必须**下推到服务端**。
//! 2. **取包**：静态仓是逐文件 GET（靠 `plugin.manifest.json` 列清单）；
//!    SkillHub 是 `GET /api/v1/download?slug=` 返回一个 **zip**。

use std::sync::Arc;

use serde::Serialize;

use crate::market::fetch::{urlencode, CachedHttp, Fetcher};
use crate::storage::AppDatabase;

pub const SKILLHUB_BASE: &str = "https://api.skillhub.cn";

/// 技能市场里的一个技能。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillMarketItem {
    /// SkillHub 的 slug —— 取包与「已安装」比对都用它（下载包里 `SKILL.md` 的 `name` 即此值）。
    pub slug: String,
    pub display_name: String,
    pub version: String,
    pub description: String,
    /// 下载量（已收成人话，如 "18.2 万"）。空串 = 不显示。
    pub downloads: String,
    pub installed: bool,
}

/// 技能详情（安装前预览）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillMarketDetail {
    pub slug: String,
    pub display_name: String,
    pub version: String,
    pub description: String,
    pub author: Option<String>,
    /// 上游仓库地址（SkillHub 的技能大多来自 GitHub / ClawHub）。
    pub homepage: Option<String>,
    pub installed: bool,
}

/// 一页技能。`total` 是**当前筛选条件下的总数**（无筛选时 7 万+），不是本页条数。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillMarketPage {
    pub items: Vec<SkillMarketItem>,
    pub total: u64,
}

/// SkillHub 的技能分类（一级）。
///
/// **分类是 SkillHub 自己的**，不是 silicon 定义的 —— 所以从它的接口取，不硬编码。
/// 硬编码一份的话，上游加一个分类，我们这边就永远看不见那批技能。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillCategory {
    /// 传回 `/api/skills?category=` 的键。
    pub key: String,
    pub name: String,
}

pub struct SkillMarket {
    base_url: String,
    http: CachedHttp,
    db: Arc<AppDatabase>,
}

impl SkillMarket {
    pub fn new(db: Arc<AppDatabase>) -> Self {
        Self {
            base_url: SKILLHUB_BASE.to_string(),
            // 不缓存：列表按页/按关键词变化，下载是一次性的 —— 磁盘缓存没有收益。
            http: CachedHttp::direct(),
            db,
        }
    }

    /// 注入构造（测试：内存 Fetcher 替换真实网络）。
    pub fn with_fetcher(
        db: Arc<AppDatabase>,
        base_url: impl Into<String>,
        fetcher: Box<dyn Fetcher>,
    ) -> Self {
        Self {
            base_url: base_url.into(),
            http: CachedHttp::with_fetcher(None, fetcher),
            db,
        }
    }

    fn get(&self, path_and_query: &str) -> Result<Vec<u8>, String> {
        let url = format!(
            "{}/{}",
            self.base_url.trim_end_matches('/'),
            path_and_query.trim_start_matches('/')
        );
        self.http.get(&url)
    }

    fn get_json(&self, path_and_query: &str) -> Result<serde_json::Value, String> {
        serde_json::from_slice(&self.get(path_and_query)?)
            .map_err(|e| format!("SkillHub 返回的不是合法 JSON（{path_and_query}）：{e}"))
    }

    /// 本地已装技能名（`SKILL.md` 的 `name` == SkillHub 的 slug）。
    fn installed_names(&self) -> Result<std::collections::HashSet<String>, String> {
        Ok(crate::skill::store::list(&self.db)?
            .into_iter()
            .map(|r| r.name)
            .collect())
    }

    /// SkillHub 的一级分类（按其 `sortOrder` 排，已下架的不要）。
    pub fn categories(&self) -> Result<Vec<SkillCategory>, String> {
        let v = self.get_json("/api/v1/categories")?;
        let items = v
            .get("items")
            .and_then(|i| i.as_array())
            .ok_or("SkillHub 未返回分类列表")?;

        let mut out: Vec<(i64, SkillCategory)> = items
            .iter()
            .filter(|c| c.get("active").and_then(|a| a.as_bool()).unwrap_or(true))
            // 只要一级分类：二级分类有几十个，平铺成 chip 会把整个市场页挤没。
            .filter(|c| c.get("level").and_then(|l| l.as_i64()).unwrap_or(1) == 1)
            .filter_map(|c| {
                let key = c.get("key").and_then(|x| x.as_str())?.to_string();
                let name = c
                    .get("name")
                    .and_then(|x| x.as_str())
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or(&key)
                    .to_string();
                let sort = c.get("sortOrder").and_then(|x| x.as_i64()).unwrap_or(0);
                Some((sort, SkillCategory { key, name }))
            })
            .collect();
        out.sort_by_key(|(sort, _)| *sort);
        Ok(out.into_iter().map(|(_, c)| c).collect())
    }

    /// 分页浏览 + 关键词搜索 + 分类筛选。**三者都下推到服务端** ——
    /// 7 万条不可能先拉全再过滤。
    pub fn browse(
        &self,
        page: u32,
        page_size: u32,
        keyword: Option<&str>,
        category: Option<&str>,
    ) -> Result<SkillMarketPage, String> {
        let mut path = format!(
            "/api/skills?page={}&pageSize={}&sortBy=score&order=desc",
            page.max(1),
            page_size.clamp(1, 100),
        );
        if let Some(kw) = keyword.map(str::trim).filter(|s| !s.is_empty()) {
            path.push_str(&format!("&keyword={}", urlencode(kw)));
        }
        if let Some(cat) = category.map(str::trim).filter(|s| !s.is_empty()) {
            path.push_str(&format!("&category={}", urlencode(cat)));
        }

        let v = self.get_json(&path)?;
        // 形态：{code:0, message, data:{skills:[…], total}}
        if v.get("code").and_then(|c| c.as_i64()).unwrap_or(0) != 0 {
            let msg = v
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("未知错误");
            return Err(format!("SkillHub 接口报错：{msg}"));
        }
        let data = v.get("data").ok_or("SkillHub 响应缺 data")?;
        let installed = self.installed_names()?;

        let items = data
            .get("skills")
            .and_then(|s| s.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|e| item_from_entry(e, &installed))
                    .collect()
            })
            .unwrap_or_default();

        Ok(SkillMarketPage {
            items,
            total: data.get("total").and_then(|t| t.as_u64()).unwrap_or(0),
        })
    }

    /// 技能详情。技能**没有 JSON 清单** —— 元数据只存在于 SkillHub 的 API 里。
    pub fn detail(&self, slug: &str) -> Result<SkillMarketDetail, String> {
        guard_slug(slug)?;
        let v = self.get_json(&format!("/api/v1/skills/{}", urlencode(slug)))?;
        let skill = v.get("skill").ok_or("SkillHub 技能详情缺 skill 字段")?;

        Ok(SkillMarketDetail {
            slug: slug.to_string(),
            display_name: skill
                .get("displayName")
                .and_then(|x| x.as_str())
                .filter(|s| !s.trim().is_empty())
                .unwrap_or(slug)
                .to_string(),
            version: v
                .get("latestVersion")
                .and_then(|l| l.get("version"))
                .and_then(|x| x.as_str())
                .unwrap_or_default()
                .to_string(),
            description: pick_zh(
                skill.get("summary_zh").and_then(|x| x.as_str()),
                skill.get("summary").and_then(|x| x.as_str()),
            ),
            author: v
                .get("owner")
                .and_then(|o| o.get("displayName").or_else(|| o.get("handle")))
                .and_then(|x| x.as_str())
                .map(str::to_string),
            homepage: skill
                .get("sourceUrl")
                .or_else(|| skill.get("upstream_url"))
                .and_then(|x| x.as_str())
                .filter(|s| !s.trim().is_empty())
                .map(str::to_string),
            installed: self.installed_names()?.contains(slug),
        })
    }

    /// **装之前先看正文**：拉技能的 `SKILL.md` 原文（markdown）。
    ///
    /// 技能的说明文字往往只有一句，光看描述判断不了它到底会让模型做什么 ——
    /// 而技能是**会改变模型行为**的东西，装进来之前该看得见。
    ///
    /// 两步：先列文件（`/files`），挑出正文文件，再取它（`/file?path=`）。
    /// 不直接猜 `SKILL.md`：上游允许 `README.md` 之类的变体，猜错就是一个 404。
    pub fn preview(&self, slug: &str) -> Result<String, String> {
        guard_slug(slug)?;
        let v = self.get_json(&format!("/api/v1/skills/{}/files", urlencode(slug)))?;
        let files = v
            .get("files")
            .and_then(|f| f.as_array())
            .ok_or("SkillHub 未返回文件列表")?;

        let path = pick_readme(files).ok_or("这个技能没有可预览的正文文件")?;
        // path 来自上游（不可信），会拼进 URL —— 先挡穿越。
        if !crate::market::wire::is_safe_relative_path(&path) {
            return Err(format!("技能文件路径非法：{path}"));
        }

        let bytes = self.get(&format!(
            "/api/v1/skills/{}/file?path={}",
            urlencode(slug),
            urlencode(&path)
        ))?;
        Ok(strip_frontmatter(&String::from_utf8_lossy(&bytes)))
    }

    /// 下载技能并物化到 `dest`（一个含 `SKILL.md` 的目录）。装载由命令层交给 `SkillService`。
    pub fn materialize(&self, slug: &str, dest: &std::path::Path) -> Result<(), String> {
        guard_slug(slug)?;
        let zip = self.get(&format!("/api/v1/download?slug={}", urlencode(slug)))?;
        for (rel, bytes) in unzip(&zip)? {
            let path = dest.join(&rel);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("建目录失败 {}：{e}", parent.display()))?;
            }
            std::fs::write(&path, &bytes)
                .map_err(|e| format!("写文件失败 {}：{e}", path.display()))?;
        }
        Ok(())
    }
}

/// 解压技能包。
///
/// 两处刻意的取舍：
/// - **`enclosed_name()` 而非 `name()`**：前者会拒绝 `../` 与绝对路径。zip 是不可信输入，
///   拿 `name()` 直接落盘就是标准的 zip-slip 洞。
/// - **剥掉 `_meta.json`**：那是 SkillHub 自己的发布元数据（ownerId/publishedAt），
///   不属于技能内容，落进技能目录只是噪音。
fn unzip(bytes: &[u8]) -> Result<Vec<(String, Vec<u8>)>, String> {
    use std::io::Read;
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|e| format!("技能包不是合法 zip：{e}"))?;

    let mut out = Vec::new();
    for i in 0..zip.len() {
        let mut f = zip
            .by_index(i)
            .map_err(|e| format!("读取技能包第 {i} 项失败：{e}"))?;
        if f.is_dir() {
            continue;
        }
        let Some(path) = f.enclosed_name() else {
            return Err("技能包内含非法路径（疑路径穿越）".to_string());
        };
        let rel = path.to_string_lossy().replace('\\', "/");
        if rel == "_meta.json" {
            continue;
        }
        let mut buf = Vec::new();
        f.read_to_end(&mut buf)
            .map_err(|e| format!("解压 {rel} 失败：{e}"))?;
        out.push((rel, buf));
    }
    if out.is_empty() {
        return Err("技能包是空的".to_string());
    }
    Ok(out)
}

/// 剥掉 YAML frontmatter，只留正文。
///
/// frontmatter 里的 name / description / version 详情页**已经单独显示过**了，
/// 再当 markdown 渲一遍就是一大坨 `name: x description: … metadata: {"openclaw":…}`
/// 糊在正文最前面 —— 那不是给人看的。
///
/// 只在**开头**且首行是 `---` 时剥。正文中间的 `---` 是分隔线，动不得。
fn strip_frontmatter(s: &str) -> String {
    let t = s.trim_start_matches('\u{feff}');
    let Some(rest) = t.strip_prefix("---") else {
        return s.to_string();
    };
    // 第一行必须只有 `---`（可带 \r）。
    let Some((first_line_rest, after_first)) = rest.split_once('\n') else {
        return s.to_string();
    };
    if !first_line_rest.trim().is_empty() {
        return s.to_string();
    }

    // 找闭合的 `---` 行，返回其后的内容。
    let mut offset = 0usize;
    for line in after_first.split_inclusive('\n') {
        if line.trim_end() == "---" {
            return after_first[offset + line.len()..].trim_start().to_string();
        }
        offset += line.len();
    }
    // 没有闭合标记 —— 不是合法 frontmatter。原样返回：宁可多显示，也不能吞掉正文。
    s.to_string()
}

/// 从文件列表里挑出「正文」：`SKILL.md` 优先，其次 `README.md`，再不然第一个 `.md`。
///
/// 大小上限 1 MB：`file?path=` 能取包里**任意**文件，别让一个巨大的附件把内存和 UI 拖垮。
fn pick_readme(files: &[serde_json::Value]) -> Option<String> {
    const MAX: u64 = 1024 * 1024;

    let usable: Vec<&str> = files
        .iter()
        .filter(|f| f.get("size").and_then(|s| s.as_u64()).unwrap_or(0) <= MAX)
        .filter_map(|f| f.get("path").and_then(|p| p.as_str()))
        .collect();

    let by_name = |want: &str| {
        usable
            .iter()
            .find(|p| p.eq_ignore_ascii_case(want))
            .map(|p| p.to_string())
    };
    by_name("SKILL.md")
        .or_else(|| by_name("README.md"))
        .or_else(|| {
            usable
                .iter()
                .find(|p| p.to_lowercase().ends_with(".md"))
                .map(|p| p.to_string())
        })
}

fn item_from_entry(
    v: &serde_json::Value,
    installed: &std::collections::HashSet<String>,
) -> Option<SkillMarketItem> {
    let slug = v.get("slug").and_then(|x| x.as_str())?.trim().to_string();
    if slug.is_empty() {
        return None;
    }
    Some(SkillMarketItem {
        display_name: v
            .get("name")
            .and_then(|x| x.as_str())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(&slug)
            .to_string(),
        version: v
            .get("version")
            .and_then(|x| x.as_str())
            .unwrap_or_default()
            .to_string(),
        description: pick_zh(
            v.get("description_zh").and_then(|x| x.as_str()),
            v.get("description").and_then(|x| x.as_str()),
        ),
        downloads: v
            .get("downloads")
            .and_then(|x| x.as_u64())
            .filter(|n| *n > 0)
            .map(compact_count)
            .unwrap_or_default(),
        installed: installed.contains(&slug),
        slug,
    })
}

/// 优先中文。SkillHub 的空字段是**空串而非 null**，所以空串也必须落到备选，
/// 否则一堆技能的描述会变成空白。
fn pick_zh(zh: Option<&str>, fallback: Option<&str>) -> String {
    zh.map(str::trim)
        .filter(|s| !s.is_empty())
        .or_else(|| fallback.map(str::trim).filter(|s| !s.is_empty()))
        .unwrap_or_default()
        .to_string()
}

/// 大数收成人话（182000 → 18.2 万）。卡片上「182000 下载」读起来费劲。
fn compact_count(n: u64) -> String {
    if n < 10_000 {
        return n.to_string();
    }
    let wan = n as f64 / 10_000.0;
    if wan >= 100.0 {
        format!("{} 万", wan.round() as u64)
    } else {
        format!("{wan:.1} 万")
    }
}

/// slug 会拼进 URL：只放行安全字符。
fn guard_slug(s: &str) -> Result<(), String> {
    let ok = !s.is_empty()
        && s.len() <= 200
        && !s.starts_with('.')
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    if ok {
        Ok(())
    } else {
        Err(format!("非法技能标识：{s}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::market::fetch::testing::MapFetcher;

    const BASE: &str = "https://api.test";

    const LIST: &str = r#"{"code":0,"data":{"total":77305,"skills":[
        {"slug":"superpowers-tdd","name":"superpowers-tdd","version":"1.0.0",
         "description":"english","description_zh":"测试驱动开发","downloads":182000},
        {"slug":"web-tools-guide","name":"web-tools-guide","version":"1.0.2",
         "description":"only english","description_zh":"","downloads":50}
    ]}}"#;

    fn market(entries: Vec<(&str, &str)>) -> SkillMarket {
        let dbp = std::env::temp_dir().join(format!(
            "siw-skillmkt-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Arc::new(AppDatabase::open(&dbp).unwrap());
        crate::skill::store::ensure_schema(&db).unwrap();
        let map = entries
            .into_iter()
            .map(|(p, b)| (format!("{BASE}{p}"), b.as_bytes().to_vec()))
            .collect();
        SkillMarket::with_fetcher(db, BASE, Box::new(MapFetcher::new(map)))
    }

    #[test]
    fn browse_maps_entries_and_reports_global_total() {
        let m = market(vec![(
            "/api/skills?page=1&pageSize=20&sortBy=score&order=desc",
            LIST,
        )]);
        let page = m.browse(1, 20, None, None).expect("列货");

        assert_eq!(page.total, 77_305, "total 是市场总数，不是本页条数");
        assert_eq!(page.items.len(), 2);

        let tdd = &page.items[0];
        assert_eq!(tdd.slug, "superpowers-tdd", "slug 是取包与去重的键");
        assert_eq!(tdd.description, "测试驱动开发", "优先中文描述");
        assert_eq!(tdd.downloads, "18.2 万");
        assert!(!tdd.installed);

        // SkillHub 的空字段是**空串而非 null** —— 空串必须落到英文备选，否则描述会空白。
        assert_eq!(page.items[1].description, "only english");
        // 小数目原样显示，不该被「万」化成 0.0 万。
        assert_eq!(page.items[1].downloads, "50");
    }

    /// **分页与搜索必须下推到服务端**：7 万条不可能先拉全再内存过滤。
    /// map 里只喂了带 page=3 与中文关键词的那个 URL —— 能取到就证明它俩真进了查询串。
    #[test]
    fn pagination_and_keyword_go_to_the_server() {
        let m = market(vec![(
            "/api/skills?page=3&pageSize=20&sortBy=score&order=desc&keyword=%E6%B5%8B%E8%AF%95",
            r#"{"code":0,"data":{"total":7,"skills":[]}}"#,
        )]);
        assert_eq!(m.browse(3, 20, Some("测试"), None).expect("搜索").total, 7);
    }

    #[test]
    fn detail_prefers_zh_summary_and_latest_version() {
        let m = market(vec![(
            "/api/v1/skills/superpowers-tdd",
            r#"{"skill":{"slug":"superpowers-tdd","displayName":"Superpowers TDD",
                        "summary":"en","summary_zh":"测试驱动开发",
                        "sourceUrl":"https://clawhub.ai/axelhu/superpowers-tdd"},
                "latestVersion":{"version":"1.1.1"},
                "owner":{"displayName":"axelhu"}}"#,
        )]);
        let d = m.detail("superpowers-tdd").expect("详情");
        assert_eq!(d.display_name, "Superpowers TDD");
        assert_eq!(d.version, "1.1.1");
        assert_eq!(d.description, "测试驱动开发");
        assert_eq!(d.author.as_deref(), Some("axelhu"));
    }

    /// 分类按上游的 `sortOrder` 排；**已下架（active=false）与二级分类都不要**
    /// —— 二级分类有几十个，平铺成 chip 会把整个市场页挤没。
    #[test]
    fn categories_are_sorted_and_filtered() {
        let m = market(vec![(
            "/api/v1/categories",
            r#"{"count":4,"items":[
                {"key":"dev-programming","name":"开发编程","level":1,"sortOrder":30,"active":true},
                {"key":"office-efficiency","name":"办公效率","level":1,"sortOrder":10,"active":true},
                {"key":"dead","name":"已下架","level":1,"sortOrder":20,"active":false},
                {"key":"sub","name":"某二级","level":2,"sortOrder":15,"active":true}]}"#,
        )]);
        let cats = m.categories().expect("分类");
        let keys: Vec<&str> = cats.iter().map(|c| c.key.as_str()).collect();
        assert_eq!(keys, vec!["office-efficiency", "dev-programming"], "按 sortOrder 排，且滤掉下架与二级");
        assert_eq!(cats[0].name, "办公效率");
    }

    /// 分类筛选**下推到服务端**（和分页/搜索一样）。
    /// map 里只喂了带 `category=` 的那个 URL —— 取到就证明它进了查询串。
    #[test]
    fn category_filter_goes_to_the_server() {
        let m = market(vec![(
            "/api/skills?page=1&pageSize=20&sortBy=score&order=desc&category=dev-programming",
            r#"{"code":0,"data":{"total":9801,"skills":[]}}"#,
        )]);
        let page = m
            .browse(1, 20, None, Some("dev-programming"))
            .expect("按分类筛");
        assert_eq!(page.total, 9801, "total 应是该分类下的总数，而非全站总数");
    }

    /// 装之前先看正文：拉 `/files` 挑出 SKILL.md，再取它的原文。
    /// **frontmatter 必须剥掉** —— name/description 详情页已单独显示，
    /// 再当 markdown 渲一遍就是一坨 `name: x description: … metadata: {…}` 糊在最前面。
    #[test]
    fn preview_fetches_body_without_frontmatter() {
        let m = market(vec![
            (
                "/api/v1/skills/superpowers-tdd/files",
                r#"{"count":2,"files":[{"path":"SKILL.md","size":5284},
                                       {"path":"_meta.json","size":134}]}"#,
            ),
            (
                "/api/v1/skills/superpowers-tdd/file?path=SKILL.md",
                "---\nname: superpowers-tdd\ndescription: 一句话\n---\n# 测试驱动开发\n先写测试。",
            ),
        ]);
        let body = m.preview("superpowers-tdd").expect("预览");
        assert!(body.starts_with("# 测试驱动开发"), "应从正文开头起，实得：{body:?}");
        assert!(!body.contains("description:"), "frontmatter 不该出现在正文里");
        assert!(body.contains("先写测试"));
    }

    #[test]
    fn strip_frontmatter_leaves_body_alone_when_unsafe() {
        // 无 frontmatter：原样。
        let plain = "# 标题\n正文";
        assert_eq!(strip_frontmatter(plain), plain);

        // 正文里的 `---` 是**分隔线**，不是 frontmatter 结束标记 —— 不能因此吞掉前面的内容。
        let with_rule = "# 标题\n正文\n\n---\n\n后半段";
        assert_eq!(strip_frontmatter(with_rule), with_rule);

        // 只有开头的 `---` 而没有闭合 —— 不是合法 frontmatter，宁可多显示也不吞正文。
        let unclosed = "---\nname: x\n# 其实是正文";
        assert_eq!(strip_frontmatter(unclosed), unclosed);
    }

    /// 正文文件的挑法：SKILL.md > README.md > 任意 .md；**超大文件不选**
    /// （`file?path=` 能取包里任意文件，别让一个巨大附件拖垮 UI）。
    #[test]
    fn readme_picker_prefers_skill_md_and_skips_huge_files() {
        let f = |path: &str, size: u64| serde_json::json!({"path": path, "size": size});

        assert_eq!(
            pick_readme(&[f("README.md", 10), f("SKILL.md", 10)]).as_deref(),
            Some("SKILL.md"),
            "SKILL.md 优先于 README.md"
        );
        assert_eq!(
            pick_readme(&[f("_meta.json", 10), f("README.md", 10)]).as_deref(),
            Some("README.md")
        );
        assert_eq!(
            pick_readme(&[f("docs/guide.md", 10)]).as_deref(),
            Some("docs/guide.md"),
            "没有标准名时退到第一个 .md"
        );
        // 没有 markdown 可预览 → None（而不是拿 _meta.json 硬凑）。
        assert!(pick_readme(&[f("_meta.json", 10)]).is_none());
        // 超过 1 MB 的正文不选。
        assert!(pick_readme(&[f("SKILL.md", 9_999_999)]).is_none());
    }

    /// slug 会拼进 URL —— 穿越必须在**发请求前**就挡住。
    #[test]
    fn rejects_unsafe_slug() {
        let m = market(vec![]);
        assert!(m.detail("../../etc/passwd").is_err());
        assert!(m.materialize("../x", std::path::Path::new("/tmp")).is_err());
    }

    /// zip 是不可信输入。`../` 必须挡住，否则解压即写到技能目录之外（zip-slip）。
    #[test]
    fn unzip_rejects_traversal_and_strips_meta() {
        let good = build_zip(&[
            ("SKILL.md", "---\nname: a\n---\n正文".as_bytes()),
            ("_meta.json", b"{}"),
        ]);
        let files = unzip(&good).expect("解压");
        let names: Vec<&str> = files.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(
            names,
            vec!["SKILL.md"],
            "_meta.json 是发布元数据，不该落进技能目录"
        );

        assert!(
            unzip(&build_zip(&[("../../evil.md", b"x")])).is_err(),
            "zip-slip 必须挡住"
        );
        assert!(
            unzip(&build_zip(&[])).is_err(),
            "空包应报错而不是静默装空技能"
        );
    }

    #[test]
    fn compact_count_is_human_readable() {
        assert_eq!(compact_count(50), "50");
        assert_eq!(compact_count(9_999), "9999");
        assert_eq!(compact_count(182_000), "18.2 万");
        assert_eq!(compact_count(1_500_000), "150 万");
    }

    /// **真打 SkillHub 的契约检查**（默认 `#[ignore]`，不进 CI）。
    ///
    /// 上面那些测试喂的都是我手抄的响应样本 —— 它们只能证明「解析器对我以为的形状是对的」，
    /// 证明不了「SkillHub 真的是这个形状」。这条跑真实网络，专门盯**契约漂移**：
    /// 字段改名、`total` 挪位置、下载不再是 zip……
    ///
    /// 手动跑：`cargo test --lib skillhub_contract -- --ignored --nocapture`
    #[test]
    #[ignore = "打真实网络"]
    fn skillhub_contract_still_holds() {
        let dbp = std::env::temp_dir().join(format!("siw-skillhub-live-{}.db", std::process::id()));
        let db = Arc::new(AppDatabase::open(&dbp).unwrap());
        crate::skill::store::ensure_schema(&db).unwrap();
        let m = SkillMarket::new(db);

        // ① 列表：分页 + 总数。
        let page = m.browse(1, 5, None, None).expect("列技能");
        assert!(
            page.total > 1000,
            "SkillHub 应有大量技能，实得 {}",
            page.total
        );
        assert_eq!(page.items.len(), 5, "pageSize 应被服务端遵守");
        let first = page.items[0].clone();
        assert!(!first.slug.is_empty());
        println!("[live] 共 {} 个技能，首条：{first:?}", page.total);

        // ② 搜索：关键词确实生效（结果集应比全量小）。
        let hits = m.browse(1, 5, Some("test"), None).expect("搜索");
        assert!(hits.total < page.total, "关键词应缩小结果集");

        // ③ 分类：上游的分类键仍然有效（分类是 SkillHub 的，不是我们硬编码的）。
        let cats = m.categories().expect("分类");
        assert!(!cats.is_empty(), "SkillHub 应有一级分类");
        println!(
            "[live] {} 个分类：{}",
            cats.len(),
            cats.iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>()
                .join("、")
        );
        let by_cat = m
            .browse(1, 5, None, Some(&cats[0].key))
            .expect("按分类筛");
        assert!(
            by_cat.total > 0 && by_cat.total < page.total,
            "分类应缩小结果集，实得 {} / 全量 {}",
            by_cat.total,
            page.total
        );

        // ③ 详情。
        let d = m.detail(&first.slug).expect("详情");
        assert_eq!(d.slug, first.slug);

        // ④ 安装前预览：正文必须真能拿到（否则详情页只剩一句描述）。
        let body = m.preview(&first.slug).expect("预览正文");
        assert!(!body.contains("\ndescription:"), "frontmatter 应已剥掉");
        assert!(body.len() > 200, "正文不该是空壳，实得 {} 字节", body.len());
        println!("[live] 正文 {} 字节", body.len());

        // ⑤ 下载 + 解压：必须真的落出一个 SKILL.md（安装链路的全部前提）。
        let dir = std::env::temp_dir().join(format!("siw-skillhub-live-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        m.materialize(&first.slug, &dir).expect("下载并解压");

        let skill_md = dir.join("SKILL.md");
        assert!(skill_md.exists(), "包里必须有 SKILL.md，否则装不成技能");
        let body = std::fs::read_to_string(&skill_md).unwrap();
        assert!(body.starts_with("---"), "SKILL.md 必须带 frontmatter");
        assert!(
            !dir.join("_meta.json").exists(),
            "_meta.json 是发布元数据，不该落进技能目录"
        );

        // ⑥ **frontmatter 的 name 必须等于 slug** —— 「已安装」是按名字比对的，
        //    一旦上游不再保证这点，市场里所有技能都会永远显示「未安装」。
        let fm = crate::skill::frontmatter::parse_frontmatter(&body).expect("解析 frontmatter");
        assert_eq!(fm.name, first.slug, "SKILL.md 的 name 必须与 slug 一致");
        println!("[live] 解压成功：{} → {}", first.slug, fm.description);

        let _ = std::fs::remove_dir_all(&dir);
    }

    fn build_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        use std::io::Write;
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut w = zip::ZipWriter::new(&mut buf);
            let opts: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            for (name, body) in entries {
                w.start_file(*name, opts).unwrap();
                w.write_all(body).unwrap();
            }
            w.finish().unwrap();
        }
        buf.into_inner()
    }
}

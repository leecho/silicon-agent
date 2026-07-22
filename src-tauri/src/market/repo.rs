//! **静态市场仓**读取器（传输层）：`market.json` + 货架分片 + `<dir>/<name>/…` 逐文件。
//!
//! silicon-market 与第三方插件源都是这个形状：一个静态 HTTP 仓（一个 git 仓的 raw 托管）。
//! 静态托管**没有目录列举能力**，所以一个包里有哪些文件必须由 `plugin.manifest.json`
//! 显式列出，客户端照单逐个拉。
//!
//! 这里同样**不含领域概念**：它只知道「货架名」「目录名」「包名」，
//! 不知道什么是专家、什么是团队。谁读哪个货架、读出来映射成什么类型，
//! 由各自的市场服务（`expert_market` / `team_market` / `plugin_market`）自己决定。

use std::path::PathBuf;

use crate::market::fetch::{CachedHttp, Fetcher};
use crate::market::wire;

/// 静态仓里的一条货物（分片条目的原样透传）。各市场自己把它映射成领域类型。
pub type ShardEntry = wire::PluginShardEntry;

pub struct StaticRepo {
    base_url: String,
    http: CachedHttp,
}

impl StaticRepo {
    pub fn new(base_url: impl Into<String>, cache_dir: PathBuf) -> Self {
        Self {
            base_url: base_url.into(),
            http: CachedHttp::cached(cache_dir),
        }
    }

    /// 注入构造（测试：内存 Fetcher 替换真实网络）。
    pub fn with_fetcher(
        base_url: impl Into<String>,
        cache_dir: Option<PathBuf>,
        fetcher: Box<dyn Fetcher>,
    ) -> Self {
        Self {
            base_url: base_url.into(),
            http: CachedHttp::with_fetcher(cache_dir, fetcher),
        }
    }

    fn bytes(&self, path: &str) -> Result<Vec<u8>, String> {
        let url = format!("{}/{}", self.base_url.trim_end_matches('/'), path);
        self.http.get(&url)
    }

    fn text(&self, path: &str) -> Result<String, String> {
        Ok(String::from_utf8_lossy(&self.bytes(path)?).into_owned())
    }

    /// 读某个货架的全部条目。
    ///
    /// **货架不存在 → 空列表，不是错误**：一个市场只上架一部分货架是常态
    /// （官方源没有 plugin 货架、第三方插件源没有 expert 货架）。
    pub fn shelf(&self, shelf: &str) -> Result<Vec<ShardEntry>, String> {
        let root = wire::parse_root(&self.text("market.json")?)?;
        let Some(shard) = root.shelf_file(shelf) else {
            return Ok(Vec::new());
        };
        wire::parse_plugin_shard(&self.text(shard)?)
    }

    /// 读包内一个文件的原文（如 `experts/<name>/expert.json`）——详情预览用，**不拉整包**。
    pub fn manifest(&self, dir: &str, name: &str, file: &str) -> Result<String, String> {
        guard_name(name)?;
        self.text(&format!("{dir}/{name}/{file}"))
    }

    /// 拉整包（照 `plugin.manifest.json` 列出的文件逐个取）。
    pub fn files(&self, dir: &str, name: &str) -> Result<Vec<(String, Vec<u8>)>, String> {
        guard_name(name)?;
        let raw = self.text(&format!("{dir}/{name}/plugin.manifest.json"))?;
        let manifest = wire::parse_plugin_file_manifest(&raw)?;

        let mut out = Vec::with_capacity(manifest.files.len());
        for rel in &manifest.files {
            // 纵深防御：`parse_plugin_file_manifest` 已校验过，落盘/拼 URL 前再挡一次。
            if !wire::is_safe_relative_path(rel) {
                return Err(format!("包内文件路径非法（疑路径穿越）：{rel}"));
            }
            out.push((rel.clone(), self.bytes(&format!("{dir}/{name}/{rel}"))?));
        }
        Ok(out)
    }
}

/// 包名会拼进 URL 路径与落盘路径 —— 不可信来源，先挡穿越。
fn guard_name(name: &str) -> Result<(), String> {
    if wire::is_safe_component(name) {
        Ok(())
    } else {
        Err(format!("非法包名（疑路径穿越）：{name}"))
    }
}

/// 把分片的 `provides` 渲染成展示标签（如 ["3 技能", "1 MCP"]）。
/// 形态宽松：既接受 `{"skills":3,"mcp":1}` 计数对象，也接受字符串数组直接透传。
pub fn provides_labels(v: Option<&serde_json::Value>) -> Vec<String> {
    let Some(v) = v else { return Vec::new() };
    if let Some(arr) = v.as_array() {
        return arr
            .iter()
            .filter_map(|e| e.as_str())
            .map(str::to_string)
            .collect();
    }
    let Some(obj) = v.as_object() else {
        return Vec::new();
    };
    const LABELS: [(&str, &str); 6] = [
        ("skills", "技能"),
        ("agents", "专家"),
        ("mcpServers", "MCP"),
        ("mcp", "MCP"),
        ("commands", "命令"),
        ("hooks", "钩子"),
    ];
    let mut out = Vec::new();
    for (key, label) in LABELS {
        let n = obj.get(key).and_then(|x| x.as_u64()).unwrap_or(0);
        if n > 0 {
            out.push(format!("{n} {label}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::market::fetch::testing::MapFetcher;

    use std::sync::Arc;

    const BASE: &str = "https://market.test";

    fn repo(entries: Vec<(&str, &str)>) -> StaticRepo {
        repo_spy(entries).0
    }

    /// 同时拿回 Fetcher，以便断言**请求了哪些 URL**（行为，而不只是返回值）。
    fn repo_spy(entries: Vec<(&str, &str)>) -> (StaticRepo, Arc<MapFetcher>) {
        let map = entries
            .into_iter()
            .map(|(p, b)| (format!("{BASE}/{p}"), b.as_bytes().to_vec()))
            .collect();
        let spy = Arc::new(MapFetcher::new(map));
        let repo = StaticRepo::with_fetcher(BASE, None, Box::new(ArcFetcher(spy.clone())));
        (repo, spy)
    }

    /// 让测试与 `StaticRepo` 共享同一个 Fetcher（`Box<dyn>` 会拿走所有权）。
    struct ArcFetcher(Arc<MapFetcher>);
    impl crate::market::fetch::Fetcher for ArcFetcher {
        fn get(
            &self,
            url: &str,
            etag: Option<&str>,
        ) -> Result<crate::market::fetch::Fetched, String> {
            self.0.get(url, etag)
        }
    }

    /// **浏览货架只该拉 `market.json` + 分片**，绝不逐包拉文件 ——
    /// 否则打开市场页就等于把整个市场下载一遍。
    #[test]
    fn browsing_does_not_pull_package_files() {
        let (r, spy) = repo_spy(vec![
            (
                "market.json",
                r#"{"name":"官方","version":4,"shelves":{"expert":"market_expert.json"}}"#,
            ),
            (
                "market_expert.json",
                r#"[{"name":"ai-engineer","displayName":"AI 工程师","version":"1.0.0"}]"#,
            ),
        ]);
        assert_eq!(r.shelf("expert").expect("列货").len(), 1);

        let urls = spy.requested();
        assert!(
            !urls.iter().any(|u| u.contains("plugin.manifest.json")),
            "浏览不该拉包内文件，实际请求：{urls:?}"
        );
        assert_eq!(urls.len(), 2, "只该拉 market.json + 分片，实得：{urls:?}");
    }

    #[test]
    fn reads_shelf_entries() {
        let r = repo(vec![
            (
                "market.json",
                r#"{"name":"官方","version":4,"shelves":{"expert":"market_expert.json"}}"#,
            ),
            (
                "market_expert.json",
                r#"[{"name":"ai-engineer","displayName":"AI 工程师","version":"1.0.0",
                     "description":"写代码","provides":{"skills":3}}]"#,
            ),
        ]);
        let items = r.shelf("expert").expect("读货架");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "ai-engineer");
        assert_eq!(provides_labels(items[0].provides.as_ref()), vec!["3 技能"]);
    }

    /// **货架缺失是常态,不是故障**：官方源没有 plugin 货架、插件源没有 expert 货架。
    /// 若这里报错，市场页会因为「某个 tab 没货」而整体加载失败。
    #[test]
    fn missing_shelf_is_empty_not_error() {
        let r = repo(vec![(
            "market.json",
            r#"{"name":"空","version":4,"shelves":{}}"#,
        )]);
        assert!(r.shelf("team").expect("缺货架不应报错").is_empty());
    }

    #[test]
    fn files_pulls_manifest_then_each_file() {
        let r = repo(vec![
            (
                "experts/a/plugin.manifest.json",
                r#"{"files":["expert.json","skills/s/SKILL.md"]}"#,
            ),
            ("experts/a/expert.json", r#"{"name":"a"}"#),
            ("experts/a/skills/s/SKILL.md", "---\nname: s\n---\n正文"),
        ]);
        let files = r.files("experts", "a").expect("取包");
        let names: Vec<&str> = files.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, vec!["expert.json", "skills/s/SKILL.md"]);
    }

    /// 包名与清单里的路径都会拼进 URL 与落盘路径 —— 两处穿越都必须挡住。
    #[test]
    fn rejects_traversal_in_name_and_in_manifest() {
        let r = repo(vec![(
            "experts/evil/plugin.manifest.json",
            r#"{"files":["../../../etc/passwd"]}"#,
        )]);
        assert!(r.files("experts", "../../etc").is_err(), "非法包名必须挡住");
        assert!(
            r.files("experts", "evil").is_err(),
            "清单里的穿越路径必须挡住"
        );
    }
}

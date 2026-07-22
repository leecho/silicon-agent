//! 市场仓 JSON 契约解析（纯函数层）。
//!
//! 把市场仓（一个静态 git 仓）下发的 JSON 字节解析为轻量内部 DTO：瘦根 `market.json`、
//! 四个货架分片（skill/expert/team/plugin）、以及 `teams/<name>/team.json` 团队清单。
//!
//! 本层与 HTTP 解耦：只做 `&str -> Result<DTO, String>` 的纯解析，不触网、不落盘，
//! 也不映射到 `types.rs` 的 `Market*`（那些字段由后续 HTTP 源填充）。分片条目的 `name`
//! 必须是文件系统安全 slug（`[a-z0-9-]`、非空），用于防路径穿越——非法直接拒绝。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// 校验分片条目 / 目录段名：仅允许非空的 `[a-z0-9-]`。
///
/// 这是防路径穿越的第一道闸：`name` 会被拼进 `teams/<name>/…` 之类的仓内路径。
pub fn is_slug(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// 路径分量安全性：拒绝空、含 `/`、`\`、`..`。
///
/// 用于把**不可信的 frontmatter `name`**（远端 SKILL.md / 成员 md 的 `name` 字段，会成为磁盘
/// 目录/文件名）拼进路径**前**的最后一道闸——`Path::join` 不会规整化 `..`，故必须显式拦截。
/// 较 `is_slug` 宽松（允许中文等合法目录名），只堵路径穿越，不限制字符集。
pub fn is_safe_component(name: &str) -> bool {
    !name.is_empty() && !name.contains('/') && !name.contains('\\') && !name.contains("..")
}

/// 瘦根 `market.json`：市场元信息 + 货架 → 分片文件名映射。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketRoot {
    pub name: String,
    pub version: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    /// 货架键（"skill"/"expert"/"team"/"plugin"）→ 分片文件名；缺键即该货架不存在。
    #[serde(default)]
    pub shelves: HashMap<String, String>,
}

impl MarketRoot {
    /// 返回某货架的分片文件名；货架缺失返回 `None`（客户端隐藏该 tab）。
    pub fn shelf_file(&self, shelf: &str) -> Option<&str> {
        self.shelves.get(shelf).map(|s| s.as_str())
    }
}

/// plugin 货架分片条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginShardEntry {
    pub name: String,
    pub display_name: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// plugin 提供的能力清单；保持原始 JSON，交由后续任务解释。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provides: Option<serde_json::Value>,
}

/// `plugins/<name>/plugin.manifest.json` 文件清单。
///
/// 静态 HTTP 托管没有目录列举能力，所以一个 plugin 包含哪些文件必须**显式列出**，
/// 客户端据此逐个 `plugins/<name>/<file>` 拉取后物化成本地目录，再走统一装载入口。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginFileManifest {
    /// 包内相对路径列表（如 `plugin.json`、`skills/foo/SKILL.md`、`.mcp.json`）。
    pub files: Vec<String>,
}

/// 解析 plugin 文件清单；拒绝任何不安全的相对路径（不可信来源，路径会拼进 URL 与落盘路径）。
pub fn parse_plugin_file_manifest(s: &str) -> Result<PluginFileManifest, String> {
    let m: PluginFileManifest =
        serde_json::from_str(s).map_err(|e| format!("解析 plugin 文件清单失败: {e}"))?;
    if m.files.is_empty() {
        return Err("plugin 文件清单为空".into());
    }
    for f in &m.files {
        if !is_safe_relative_path(f) {
            return Err(format!("plugin 文件清单含非法路径（疑路径穿越）：{f}"));
        }
    }
    Ok(m)
}

/// 包内相对路径是否安全：非空、非绝对、无 `..`、无反斜杠、每段都是安全组件。
pub fn is_safe_relative_path(p: &str) -> bool {
    let p = p.trim();
    if p.is_empty() || p.starts_with('/') || p.contains('\\') || p.contains(':') {
        return false;
    }
    let mut segments = 0;
    for seg in p.split('/') {
        if seg.is_empty() || seg == "." || seg == ".." {
            return false;
        }
        // 允许点号开头（如 `.mcp.json`），但不允许纯 `..` / 分隔符 / 控制字符。
        if seg.chars().any(|c| c.is_control() || c == '/' || c == '\\') {
            return false;
        }
        segments += 1;
    }
    segments > 0
}

/// 解析瘦根 `market.json`。
pub fn parse_root(s: &str) -> Result<MarketRoot, String> {
    serde_json::from_str(s).map_err(|e| format!("解析 market.json 失败: {e}"))
}

/// 校验一批分片条目的 `name` 均为合法 slug，否则返回 `Err`。
fn check_slugs<'a, I>(names: I, shelf: &str) -> Result<(), String>
where
    I: IntoIterator<Item = &'a str>,
{
    for name in names {
        if !is_slug(name) {
            return Err(format!("{shelf} 分片条目 name 非法 slug: {name:?}"));
        }
    }
    Ok(())
}

/// 解析 plugin 货架分片；拒绝非 slug 的 `name`。
pub fn parse_plugin_shard(s: &str) -> Result<Vec<PluginShardEntry>, String> {
    let v: Vec<PluginShardEntry> =
        serde_json::from_str(s).map_err(|e| format!("解析 plugin 分片失败: {e}"))?;
    check_slugs(v.iter().map(|e| e.name.as_str()), "plugin")?;
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    const ROOT: &str = r#"{ "name":"官方市场","version":1,"updatedAt":"2026-07-09",
        "shelves":{"skill":"market_skill.json","expert":"market_expert.json","team":"market_team.json"} }"#;

    #[test]
    fn parses_thin_root_and_exposes_present_shelves() {
        let root = parse_root(ROOT).unwrap();
        assert_eq!(root.name, "官方市场");
        assert_eq!(root.version, 1);
        assert_eq!(root.shelf_file("expert"), Some("market_expert.json"));
        assert_eq!(root.shelf_file("plugin"), None); // 缺 plugin → 隐藏该 tab
    }

    #[test]
    fn root_missing_all_shelves() {
        let json = r#"{"name":"空市场","version":2}"#;
        let root = parse_root(json).unwrap();
        assert_eq!(root.updated_at, None);
        assert_eq!(root.shelf_file("skill"), None);
        assert_eq!(root.shelf_file("expert"), None);
    }

    #[test]
    fn rejects_path_traversal_in_plugin_file_manifest() {
        // 不可信来源：清单里的路径会拼进 URL 与落盘路径，必须挡住穿越。
        for bad in [
            r#"{"files":["../../etc/passwd"]}"#,
            r#"{"files":["/etc/passwd"]}"#,
            r#"{"files":["a/../../b"]}"#,
            r#"{"files":["skills/../../x"]}"#,
            r#"{"files":[""]}"#,
            r#"{"files":[]}"#,
        ] {
            assert!(parse_plugin_file_manifest(bad).is_err(), "应拒绝：{bad}");
        }
    }

    #[test]
    fn accepts_normal_plugin_file_manifest() {
        let ok = r#"{"files":["plugin.json",".mcp.json","skills/foo/SKILL.md","agents/a.md"]}"#;
        let m = parse_plugin_file_manifest(ok).expect("应接受正常清单");
        assert_eq!(m.files.len(), 4);
        assert!(m.files.contains(&".mcp.json".to_string()), "允许点号开头");
    }

    #[test]
    fn parses_plugin_shard() {
        let json = r#"[{"name":"my-plugin","displayName":"我的插件","version":"1.2.0",
            "provides":{"tools":["x"]}}]"#;
        let v = parse_plugin_shard(json).unwrap();
        assert_eq!(v[0].name, "my-plugin");
        assert_eq!(v[0].version, "1.2.0");
        assert!(v[0].provides.is_some());
    }

    #[test]
    fn rejects_name_not_slug() {
        // name 会拼进 URL 路径（plugins/<name>/…）与落盘目录名：非 slug 必须拒。
        let json = r#"[{"name":"Bad Name!","displayName":"x","version":"1.0.0"}]"#;
        assert!(parse_plugin_shard(json).is_err());
    }

    #[test]
    fn is_slug_accepts_and_rejects() {
        assert!(is_slug("trading-expert"));
        assert!(is_slug("s1"));
        assert!(!is_slug("Bad Name!"));
        assert!(!is_slug("UpperCase"));
        assert!(!is_slug(""));
        assert!(!is_slug("has space"));
        assert!(!is_slug("../etc"));
    }

    #[test]
    fn is_safe_component_blocks_traversal_allows_unicode() {
        // 合法目录名（含中文、空格、大写）——只堵路径穿越，不限字符集。
        assert!(is_safe_component("trading-expert"));
        assert!(is_safe_component("投研助手"));
        assert!(is_safe_component("My Skill"));
        // 路径穿越 / 分隔符 / 空 —— 拒绝。
        assert!(!is_safe_component(""));
        assert!(!is_safe_component(".."));
        assert!(!is_safe_component("../../evil"));
        assert!(!is_safe_component("a/b"));
        assert!(!is_safe_component("a\\b"));
        assert!(!is_safe_component("../../../../tmp/pwned"));
    }
}

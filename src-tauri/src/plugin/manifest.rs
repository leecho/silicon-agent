//! plugin.json 解析。本项目约定 plugin.json 直接放在**插件根目录**；其余为各家方言的回退位置
//! （Claude / Codex / QoderWork），见 `PLUGIN_MANIFEST_CANDIDATES`。维持「根优先」。
//!
//! 宽松解析：未知字段忽略；`keywords` 优先、`tags` 兜底；`commands` 解析但不加载。

use std::path::Path;

/// plugin.json 的解析结果（仅取本期需要的字段）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginManifest {
    pub name: String,
    pub display_name: String,
    pub version: String,
    pub description: String,
    pub description_zh: Option<String>,
    pub category: Option<String>,
    pub customized_from: Option<String>,
    /// 插件内 skill 目录的相对路径（如 `skills/draft-contract`）。
    pub skills: Vec<String>,
    /// legacy 命令文件相对路径；本期解析但不加载（仅用于提示）。
    pub commands: Vec<String>,
    /// 插件内专家定义文件的相对路径（如 `agents/lead.md`）。用于 plugin→agent 索引。
    pub agents: Vec<String>,
    /// 作者（`author`：对象 `{name,...}` 取 name，或字符串直接用）。
    pub author: Option<String>,
    /// 主页 URL（`homepage`）。
    pub homepage: Option<String>,
    /// 仓库 URL（`repository`：字符串或对象 `{url}` 取 url）。
    pub repository: Option<String>,
    /// 许可证（`license`）。
    pub license: Option<String>,
    /// 关键词（`keywords` 优先，`tags` 兜底；无则空）。
    pub keywords: Vec<String>,
    /// 插件声明的 MCP server（plugin.json 顶层 `mcpServers` + 插件根 `.mcp.json` 合并）。
    /// 用中性本地结构，使 plugin 模块对 mcp 模块零依赖。
    pub mcp_servers: Vec<ParsedMcpServer>,
    /// 插件声明的 hooks（plugin.json 顶层 `hooks` + 插件根 `hooks/hooks.json` 合并）。
    /// v1 仅取 `type=="command"`、event∈{PreToolUse,PostToolUse,SessionStart,Stop}。
    pub hooks: Vec<ParsedHook>,
}

/// 插件声明的一条 hook（中性表示）。
/// `event`：生命周期事件名；`matcher`：工具名匹配（空=匹配全部，仅 Pre/PostToolUse 有意义）；
/// `command`：`type=="command"` 的 shell 命令（执行时变量替换）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedHook {
    pub event: String,
    pub matcher: Option<String>,
    pub command: String,
}

/// v1 支持的 hook 事件集合。
pub const SUPPORTED_HOOK_EVENTS: [&str; 4] = ["PreToolUse", "PostToolUse", "SessionStart", "Stop"];

/// 插件声明的一条 MCP server（中性表示，不依赖 mcp 模块）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedMcpServer {
    /// server 名（plugin.json/.mcp.json 里的键名）。
    pub name: String,
    pub kind: ParsedMcpKind,
}

/// MCP 传输形态（中性）：有 command→Stdio；有 url 或 type∈{http,sse}→Http。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedMcpKind {
    Stdio {
        command: String,
        args: Vec<String>,
        env: std::collections::BTreeMap<String, String>,
        cwd: Option<String>,
    },
    Http {
        url: String,
        headers: std::collections::BTreeMap<String, String>,
        /// OAuth 手填 client_id（清单扩展字段 `clientId`）；None=动态注册（DCR）。
        oauth_client_id: Option<String>,
        /// RFC 8707 `resource` 覆盖（清单扩展字段 `oauth_resource`，Codex 插件用它）。
        oauth_resource: Option<String>,
    },
}

/// plugin.json 的候选位置，**按优先级**。根优先（本项目原生约定），其余为各家方言。
///
/// 加新方言只需在这里加一行 —— 不要再回到 if-else 链。
pub const PLUGIN_MANIFEST_CANDIDATES: [&str; 4] = [
    "plugin.json",                // 原生 / 通用
    ".claude-plugin/plugin.json", // Claude
    ".codex-plugin/plugin.json",  // Codex
    ".qoder-plugin/plugin.json",  // QoderWork
];

/// 在插件目录下定位 plugin.json（按 `PLUGIN_MANIFEST_CANDIDATES` 的优先级）。
pub fn locate_plugin_manifest(plugin_dir: &Path) -> Option<std::path::PathBuf> {
    PLUGIN_MANIFEST_CANDIDATES
        .iter()
        .map(|rel| plugin_dir.join(rel))
        .find(|p| p.is_file())
}

/// 在插件目录下定位并解析 plugin.json。维持「根优先」，其余按方言顺序回退。
pub fn parse_plugin_dir(plugin_dir: &Path) -> Result<PluginManifest, String> {
    let path = locate_plugin_manifest(plugin_dir).ok_or(
        "缺少 plugin.json（或 .claude-plugin/ .codex-plugin/ .qoder-plugin/ 下的 plugin.json）",
    )?;
    let raw = std::fs::read_to_string(&path).map_err(|e| format!("读 plugin.json 失败：{e}"))?;
    let mut m = parse_manifest(&raw)?;

    // 合并插件根 `.mcp.json`（`{ "mcpServers": { name: {...} } }`）。同名以 plugin.json 内联为准。
    let dot_mcp = plugin_dir.join(".mcp.json");
    if dot_mcp.is_file() {
        match std::fs::read_to_string(&dot_mcp) {
            Ok(text) => match serde_json::from_str::<serde_json::Value>(&text) {
                Ok(v) => {
                    let existing: std::collections::HashSet<String> =
                        m.mcp_servers.iter().map(|s| s.name.clone()).collect();
                    for s in parse_mcp_servers(v.get("mcpServers")) {
                        if !existing.contains(&s.name) {
                            m.mcp_servers.push(s);
                        }
                    }
                }
                Err(e) => eprintln!("[plugin] {}: .mcp.json 不是合法 JSON：{e}", m.name),
            },
            Err(e) => eprintln!("[plugin] {}: 读 .mcp.json 失败：{e}", m.name),
        }
    }

    // 合并 `hooks/hooks.json`（本项目原约定）与**插件根 `hooks.json`**（Codex 插件如 figma 的布局）。
    // 内联 plugin.json 的 hooks 已在 m.hooks；这里追加解析所得，按内容去重。
    for hooks_json in [
        plugin_dir.join("hooks").join("hooks.json"),
        plugin_dir.join("hooks.json"),
    ] {
        if !hooks_json.is_file() {
            continue;
        }
        let label = hooks_json
            .strip_prefix(plugin_dir)
            .unwrap_or(&hooks_json)
            .display()
            .to_string();
        match std::fs::read_to_string(&hooks_json) {
            Ok(text) => match serde_json::from_str::<serde_json::Value>(&text) {
                Ok(v) => {
                    for h in parse_hooks(v.get("hooks")) {
                        if !m.hooks.contains(&h) {
                            m.hooks.push(h);
                        }
                    }
                }
                Err(e) => eprintln!("[plugin] {}: {label} 不是合法 JSON：{e}", m.name),
            },
            Err(e) => eprintln!("[plugin] {}: 读 {label} 失败：{e}", m.name),
        }
    }

    // agents 目录约定发现：manifest 未声明 `agents` 时，扫 `agents/*.md`。
    // Codex/Claude 插件常不声明、靠目录约定（figma 的 4 个 agent 即如此）。
    // 显式声明时以声明为准，不叠加扫描（避免重复/意外收编）。
    if m.agents.is_empty() {
        let agents_dir = plugin_dir.join("agents");
        if agents_dir.is_dir() {
            let mut found: Vec<String> = Vec::new();
            match std::fs::read_dir(&agents_dir) {
                Ok(entries) => {
                    for e in entries.flatten() {
                        let p = e.path();
                        if !p.is_file() || p.extension().and_then(|x| x.to_str()) != Some("md") {
                            continue;
                        }
                        if let Some(name) = p.file_name().and_then(|x| x.to_str()) {
                            found.push(format!("agents/{name}"));
                        }
                    }
                }
                Err(e) => eprintln!("[plugin] {}: 读 agents/ 失败：{e}", m.name),
            }
            // 排序：使发现顺序与文件系统枚举顺序无关、可复现。
            found.sort();
            m.agents = found;
        }
    }

    Ok(m)
}

/// 解析 plugin.json 文本。
pub fn parse_manifest(raw: &str) -> Result<PluginManifest, String> {
    let v: serde_json::Value =
        serde_json::from_str(raw).map_err(|e| format!("plugin.json 不是合法 JSON：{e}"))?;

    let name = str_field(&v, "name").ok_or("plugin.json 缺少 name")?;
    let display_name = str_field(&v, "displayName").unwrap_or_else(|| name.clone());
    let version = str_field(&v, "version").unwrap_or_default();
    let description = str_field(&v, "description").unwrap_or_default();
    let description_zh = str_field(&v, "descriptionZh");
    let category = str_field(&v, "category");
    let customized_from = str_field(&v, "customizedFrom");
    // skills：兼容「字符串(单个目录/技能根)」与「数组(多个路径)」两种形态（Claude/Codex 用字符串
    // 指向 skills 根目录，本项目历史用数组列各 skill 目录）。运行时按目录扫描语义解析（见 service）。
    let skills = str_or_array(&v, "skills");
    let commands = str_array(&v, "commands");
    let agents = str_array(&v, "agents");
    let author = parse_author(v.get("author"));
    let homepage = str_field(&v, "homepage");
    let repository = parse_repository(v.get("repository"));
    let license = str_field(&v, "license");
    // keywords 优先，tags 兜底。
    let keywords = {
        let kw = str_array(&v, "keywords");
        if kw.is_empty() {
            str_array(&v, "tags")
        } else {
            kw
        }
    };
    let mcp_servers = parse_mcp_servers(v.get("mcpServers"));
    let hooks = parse_hooks(v.get("hooks"));

    Ok(PluginManifest {
        name,
        display_name,
        version,
        description,
        description_zh,
        category,
        customized_from,
        skills,
        commands,
        agents,
        author,
        homepage,
        repository,
        license,
        keywords,
        mcp_servers,
        hooks,
    })
}

/// 解析 hooks 对象（结构：`{ "Event": [ { "matcher"?, "hooks": [ { "type", "command" } ] } ] }`）。
/// v1 仅收 `type=="command"`、event∈[`SUPPORTED_HOOK_EVENTS`]。其余跳过并 log（非致命）。
fn parse_hooks(v: Option<&serde_json::Value>) -> Vec<ParsedHook> {
    let Some(obj) = v.and_then(|x| x.as_object()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    // 对事件名排序，使解析顺序与底层 JSON 表示无关、可复现。
    let mut events: Vec<&String> = obj.keys().collect();
    events.sort();
    for event in events {
        if !SUPPORTED_HOOK_EVENTS.contains(&event.as_str()) {
            eprintln!("[plugin] 跳过不支持的 hook 事件「{event}」");
            continue;
        }
        let Some(groups) = obj[event].as_array() else {
            continue;
        };
        for group in groups {
            let matcher = group
                .get("matcher")
                .and_then(|m| m.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string());
            let Some(entries) = group.get("hooks").and_then(|h| h.as_array()) else {
                continue;
            };
            for entry in entries {
                let kind = entry
                    .get("type")
                    .and_then(|t| t.as_str())
                    .map(str::trim)
                    .unwrap_or("");
                if kind != "command" {
                    eprintln!("[plugin] 跳过非 command 类型 hook（event={event}, type={kind:?}）");
                    continue;
                }
                let command = entry
                    .get("command")
                    .and_then(|c| c.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty());
                let Some(command) = command else {
                    eprintln!("[plugin] 跳过缺 command 的 hook（event={event}）");
                    continue;
                };
                out.push(ParsedHook {
                    event: event.clone(),
                    matcher: matcher.clone(),
                    command: command.to_string(),
                });
            }
        }
    }
    out
}

/// 解析 `author`：对象 `{name,email,url}` 取 name；字符串直接用；其余 None。
fn parse_author(v: Option<&serde_json::Value>) -> Option<String> {
    let v = v?;
    if let Some(s) = v.as_str() {
        let s = s.trim();
        return (!s.is_empty()).then(|| s.to_string());
    }
    v.get("name")
        .and_then(|x| x.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

/// 解析 `repository`：字符串直接用；对象 `{url}` 取 url；其余 None。
fn parse_repository(v: Option<&serde_json::Value>) -> Option<String> {
    let v = v?;
    if let Some(s) = v.as_str() {
        let s = s.trim();
        return (!s.is_empty()).then(|| s.to_string());
    }
    v.get("url")
        .and_then(|x| x.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

/// 解析 `mcpServers` 对象（`{ name: { command|url, ... } }`）为中性结构。
/// 每条目：有 `command` → Stdio；有 `url` 或 `type∈{http,sse}` → Http；其余跳过并 log。
/// 键名按字典序稳定输出（BTreeMap 遍历），保证 id 生成幂等。
fn parse_mcp_servers(v: Option<&serde_json::Value>) -> Vec<ParsedMcpServer> {
    let Some(obj) = v.and_then(|x| x.as_object()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    // 对键名排序，使解析顺序与底层 JSON 表示无关、可复现。
    let mut keys: Vec<&String> = obj.keys().collect();
    keys.sort();
    for name in keys {
        let entry = &obj[name];
        let name = name.trim();
        if name.is_empty() {
            continue;
        }
        let command = entry
            .get("command")
            .and_then(|c| c.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let url = entry
            .get("url")
            .and_then(|c| c.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let kind_tag = entry
            .get("type")
            .and_then(|c| c.as_str())
            .map(|s| s.trim().to_ascii_lowercase());

        let kind = if let Some(command) = command {
            ParsedMcpKind::Stdio {
                command: command.to_string(),
                args: json_str_array(entry.get("args")),
                env: json_str_map(entry.get("env")),
                cwd: entry
                    .get("cwd")
                    .and_then(|c| c.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string()),
            }
        } else if let Some(url) =
            url.filter(|_| matches!(kind_tag.as_deref(), None | Some("http") | Some("sse")))
        {
            ParsedMcpKind::Http {
                url: url.to_string(),
                headers: json_str_map(entry.get("headers")),
                oauth_client_id: entry
                    .get("clientId")
                    .and_then(|c| c.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string()),
                oauth_resource: entry
                    .get("oauth_resource")
                    .or_else(|| entry.get("oauthResource"))
                    .and_then(|c| c.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string()),
            }
        } else {
            eprintln!("[plugin] 跳过未知形态的 MCP server「{name}」（缺 command/url）");
            continue;
        };
        out.push(ParsedMcpServer {
            name: name.to_string(),
            kind,
        });
    }
    out
}

/// 取 JSON 字符串数组（过滤空白）。
fn json_str_array(v: Option<&serde_json::Value>) -> Vec<String> {
    v.and_then(|x| x.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|e| e.as_str())
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// 取 JSON 字符串映射（值非字符串项跳过）。BTreeMap 保证稳定序。
fn json_str_map(v: Option<&serde_json::Value>) -> std::collections::BTreeMap<String, String> {
    v.and_then(|x| x.as_object())
        .map(|obj| {
            obj.iter()
                .filter_map(|(k, val)| val.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default()
}

/// 取顶层字符串字段（非空白），否则 None。
fn str_field(v: &serde_json::Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(|x| x.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

/// 取字段为「字符串或字符串数组」→ 统一成 Vec<String>（过滤空白）。字符串当单元素。
fn str_or_array(v: &serde_json::Value, key: &str) -> Vec<String> {
    match v.get(key) {
        Some(serde_json::Value::String(s)) => {
            let t = s.trim();
            if t.is_empty() {
                Vec::new()
            } else {
                vec![t.to_string()]
            }
        }
        _ => str_array(v, key),
    }
}

/// 取顶层字符串数组（过滤空白项）。
fn str_array(v: &serde_json::Value, key: &str) -> Vec<String> {
    v.get(key)
        .and_then(|x| x.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|e| e.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_manifest() {
        let raw = r#"{
            "name": "legal-assistant",
            "displayName": "法务助手",
            "version": "1.0.0",
            "description": "Legal toolkit",
            "descriptionZh": "法务工具箱",
            "category": "legal",
            "customizedFrom": "投研分析",
            "skills": ["skills/draft-contract", "skills/review-contract"],
            "commands": ["commands/old.md"]
        }"#;
        let m = parse_manifest(raw).expect("parse");
        assert_eq!(m.name, "legal-assistant");
        assert_eq!(m.display_name, "法务助手");
        assert_eq!(m.description_zh.as_deref(), Some("法务工具箱"));
        assert_eq!(m.category.as_deref(), Some("legal"));
        assert_eq!(m.customized_from.as_deref(), Some("投研分析"));
        assert_eq!(m.skills.len(), 2);
        assert_eq!(m.commands.len(), 1);
    }

    #[test]
    fn display_name_falls_back_to_name() {
        let m = parse_manifest(r#"{"name":"x"}"#).expect("parse");
        assert_eq!(m.display_name, "x");
        assert!(m.skills.is_empty());
    }

    #[test]
    fn parses_agents_without_type() {
        let m = parse_manifest(r#"{"name":"x","agents":["agents/lead.md"]}"#).expect("parse");
        assert_eq!(m.agents, vec!["agents/lead.md"]);
    }

    #[test]
    fn parses_object_author_and_repository() {
        let raw = r#"{
            "name": "x",
            "author": {"name": "Jane Doe", "email": "j@e.com"},
            "homepage": "https://example.com",
            "repository": {"url": "https://github.com/x/y"},
            "license": "MIT",
            "keywords": ["a", "b"]
        }"#;
        let m = parse_manifest(raw).expect("parse");
        assert_eq!(m.author.as_deref(), Some("Jane Doe"));
        assert_eq!(m.homepage.as_deref(), Some("https://example.com"));
        assert_eq!(m.repository.as_deref(), Some("https://github.com/x/y"));
        assert_eq!(m.license.as_deref(), Some("MIT"));
        assert_eq!(m.keywords, vec!["a", "b"]);
    }

    #[test]
    fn parses_string_author_and_repository() {
        let raw = r#"{
            "name": "x",
            "author": "Jane Doe",
            "repository": "https://github.com/x/y"
        }"#;
        let m = parse_manifest(raw).expect("parse");
        assert_eq!(m.author.as_deref(), Some("Jane Doe"));
        assert_eq!(m.repository.as_deref(), Some("https://github.com/x/y"));
    }

    #[test]
    fn keywords_falls_back_to_tags() {
        let m = parse_manifest(r#"{"name":"x","tags":["t1","t2"]}"#).expect("parse");
        assert_eq!(m.keywords, vec!["t1", "t2"]);
        // keywords 存在时优先 keywords。
        let m2 = parse_manifest(r#"{"name":"x","keywords":["k1"],"tags":["t1"]}"#).expect("parse");
        assert_eq!(m2.keywords, vec!["k1"]);
    }

    #[test]
    fn metadata_missing_is_empty() {
        let m = parse_manifest(r#"{"name":"x"}"#).expect("parse");
        assert!(m.author.is_none());
        assert!(m.homepage.is_none());
        assert!(m.repository.is_none());
        assert!(m.license.is_none());
        assert!(m.keywords.is_empty());
    }

    #[test]
    fn parse_plugin_dir_falls_back_to_codex_plugin() {
        let dir = std::env::temp_dir().join(format!(
            "siw-manifest-codex-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let codex_dir = dir.join(".codex-plugin");
        std::fs::create_dir_all(&codex_dir).unwrap();
        std::fs::write(codex_dir.join("plugin.json"), r#"{"name":"codex-p"}"#).unwrap();
        let m = parse_plugin_dir(&dir).expect("parse dir");
        assert_eq!(m.name, "codex-p");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// QoderWork 插件方言：清单在 `.qoder-plugin/`，技能目录**中文名**，MCP 走根 `.mcp.json`。
    ///
    /// 照真实样本（`corporate-legal`）合成。两处曾会让它装不进来：
    /// ① 清单定位器不认 `.qoder-plugin/`；
    /// ② 包根定位器（`plugin::service::locate_plugin_root`）此前**连 `.codex-plugin/` 都不认**，
    ///    只认根与 `.claude-plugin/` —— zip 安装会在「定位包根」这一步直接失败。
    #[test]
    fn parse_plugin_dir_supports_qoder_dialect_with_cjk_skill_dirs() {
        let dir = std::env::temp_dir().join(format!(
            "siw-manifest-qoder-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let q = dir.join(".qoder-plugin");
        std::fs::create_dir_all(&q).unwrap();
        std::fs::write(
            q.join("plugin.json"),
            r#"{"name":"corporate-legal","displayName":"企业法务","version":"1.1.2",
                "skills":["./skills/合规审查","./skills/法律文书"],
                "qoderMarket":{"source":"apphub","extensionType":"plugin"}}"#,
        )
        .unwrap();
        // MCP 不在 plugin.json 里，而在插件根 .mcp.json（qoder 的布局）。
        std::fs::write(
            dir.join(".mcp.json"),
            r#"{"mcpServers":{"pkulaw":{"url":"https://example.com/mcp"}}}"#,
        )
        .unwrap();

        let m = parse_plugin_dir(&dir).expect("qoder 方言应能解析");
        assert_eq!(m.name, "corporate-legal");
        assert_eq!(m.display_name, "企业法务");
        assert!(
            m.skills.iter().any(|s| s.contains("合规审查")),
            "中文技能目录不得被丢弃：{:?}",
            m.skills
        );
        assert_eq!(m.mcp_servers.len(), 1, "根 .mcp.json 应被合并");
        assert_eq!(m.mcp_servers[0].name, "pkulaw");
        // `qoderMarket` 是未知字段 —— 宽松解析必须忽略它，而不是报错。
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_plugin_dir_root_wins_over_codex_plugin() {
        let dir = std::env::temp_dir().join(format!(
            "siw-manifest-rootwin-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let codex_dir = dir.join(".codex-plugin");
        std::fs::create_dir_all(&codex_dir).unwrap();
        std::fs::write(dir.join("plugin.json"), r#"{"name":"root-p"}"#).unwrap();
        std::fs::write(codex_dir.join("plugin.json"), r#"{"name":"codex-p"}"#).unwrap();
        let m = parse_plugin_dir(&dir).expect("parse dir");
        assert_eq!(m.name, "root-p", "根 plugin.json 优先");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_name_errors() {
        assert!(parse_manifest(r#"{"displayName":"X"}"#).is_err());
    }

    #[test]
    fn invalid_json_errors() {
        assert!(parse_manifest("not json").is_err());
    }

    #[test]
    fn parses_inline_mcp_servers_stdio_and_http() {
        let raw = r#"{
            "name": "tools",
            "mcpServers": {
                "files": {
                    "command": "node",
                    "args": ["server.js", "--port", "0"],
                    "env": {"TOKEN": "x"},
                    "cwd": "${CLAUDE_PLUGIN_ROOT}/srv"
                },
                "remote": { "url": "https://mcp.example.com/mcp", "type": "http" }
            }
        }"#;
        let m = parse_manifest(raw).expect("parse");
        assert_eq!(m.mcp_servers.len(), 2);
        // 字典序：files 在 remote 前。
        assert_eq!(m.mcp_servers[0].name, "files");
        match &m.mcp_servers[0].kind {
            ParsedMcpKind::Stdio {
                command,
                args,
                env,
                cwd,
            } => {
                assert_eq!(command, "node");
                assert_eq!(args, &vec!["server.js", "--port", "0"]);
                assert_eq!(env.get("TOKEN").map(String::as_str), Some("x"));
                assert_eq!(cwd.as_deref(), Some("${CLAUDE_PLUGIN_ROOT}/srv"));
            }
            _ => panic!("expected stdio"),
        }
        assert_eq!(m.mcp_servers[1].name, "remote");
        match &m.mcp_servers[1].kind {
            ParsedMcpKind::Http { url, .. } => assert_eq!(url, "https://mcp.example.com/mcp"),
            _ => panic!("expected http"),
        }
    }

    #[test]
    fn sse_type_with_url_is_http() {
        let raw = r#"{"name":"x","mcpServers":{"s":{"url":"https://e/sse","type":"sse"}}}"#;
        let m = parse_manifest(raw).expect("parse");
        assert!(matches!(m.mcp_servers[0].kind, ParsedMcpKind::Http { .. }));
    }

    #[test]
    fn unknown_shape_is_skipped() {
        let raw = r#"{"name":"x","mcpServers":{"bad":{"foo":"bar"}}}"#;
        let m = parse_manifest(raw).expect("parse");
        assert!(m.mcp_servers.is_empty(), "缺 command/url 应跳过");
    }

    #[test]
    fn no_mcp_servers_is_empty() {
        let m = parse_manifest(r#"{"name":"x"}"#).expect("parse");
        assert!(m.mcp_servers.is_empty());
    }

    #[test]
    fn parses_http_mcp_oauth_client_id() {
        let raw = r#"{
            "name": "figma",
            "mcpServers": {
                "figma": {
                    "type": "http",
                    "url": "https://mcp.figma.com/mcp",
                    "clientId": "cid-123"
                }
            }
        }"#;
        let m = parse_manifest(raw).expect("parse");
        match &m.mcp_servers[0].kind {
            ParsedMcpKind::Http {
                url,
                oauth_client_id,
                ..
            } => {
                assert_eq!(url, "https://mcp.figma.com/mcp");
                assert_eq!(
                    oauth_client_id.as_deref(),
                    Some("cid-123"),
                    "插件声明的 clientId 必须保留（T104 头号阻断项）"
                );
            }
            _ => panic!("expected http"),
        }
    }

    #[test]
    fn http_mcp_without_client_id_is_none() {
        let raw = r#"{"name":"x","mcpServers":{"s":{"url":"https://e/mcp"}}}"#;
        let m = parse_manifest(raw).expect("parse");
        match &m.mcp_servers[0].kind {
            ParsedMcpKind::Http {
                oauth_client_id, ..
            } => assert!(
                oauth_client_id.is_none(),
                "未声明 clientId → None（走 DCR）"
            ),
            _ => panic!("expected http"),
        }
    }

    #[test]
    fn parses_inline_hooks_and_filters_non_command() {
        let raw = r#"{
            "name": "h",
            "hooks": {
                "PreToolUse": [
                    { "matcher": "command_execute", "hooks": [{ "type": "command", "command": "echo pre" }] }
                ],
                "PostToolUse": [
                    { "hooks": [
                        { "type": "command", "command": "echo post" },
                        { "type": "http", "command": "ignored" }
                    ] }
                ],
                "SessionStart": [
                    { "hooks": [{ "type": "command", "command": "echo start" }] }
                ],
                "UserPromptSubmit": [
                    { "hooks": [{ "type": "command", "command": "echo unsupported" }] }
                ]
            }
        }"#;
        let m = parse_manifest(raw).expect("parse");
        // 排序后事件序：PostToolUse, PreToolUse, SessionStart（UserPromptSubmit 不支持被跳过）。
        assert_eq!(m.hooks.len(), 3, "非 command 与不支持事件被过滤");
        assert!(m
            .hooks
            .iter()
            .any(|h| h.event == "PreToolUse" && h.matcher.as_deref() == Some("command_execute")));
        let post = m.hooks.iter().find(|h| h.event == "PostToolUse").unwrap();
        assert_eq!(post.matcher, None, "缺 matcher 为 None");
        assert_eq!(post.command, "echo post");
        assert!(m.hooks.iter().any(|h| h.event == "SessionStart"));
        assert!(!m.hooks.iter().any(|h| h.event == "UserPromptSubmit"));
    }

    #[test]
    fn no_hooks_is_empty() {
        let m = parse_manifest(r#"{"name":"x"}"#).expect("parse");
        assert!(m.hooks.is_empty());
    }

    #[test]
    fn parse_plugin_dir_merges_hooks_json() {
        let dir = std::env::temp_dir().join(format!(
            "siw-manifest-hooks-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(dir.join("hooks")).unwrap();
        std::fs::write(
            dir.join("plugin.json"),
            r#"{"name":"p","hooks":{"Stop":[{"hooks":[{"type":"command","command":"echo inline-stop"}]}]}}"#,
        )
        .unwrap();
        std::fs::write(
            dir.join("hooks").join("hooks.json"),
            r#"{"hooks":{"PreToolUse":[{"matcher":"write_file","hooks":[{"type":"command","command":"echo guard"}]}]}}"#,
        )
        .unwrap();
        let m = parse_plugin_dir(&dir).expect("parse dir");
        assert_eq!(m.hooks.len(), 2, "内联 + hooks.json 合并");
        assert!(m.hooks.iter().any(|h| h.event == "Stop"));
        assert!(m
            .hooks
            .iter()
            .any(|h| h.event == "PreToolUse" && h.matcher.as_deref() == Some("write_file")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_plugin_dir_merges_dot_mcp_json() {
        let dir = std::env::temp_dir().join(format!(
            "siw-manifest-mcp-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("plugin.json"),
            r#"{"name":"p","mcpServers":{"inline":{"command":"node"}}}"#,
        )
        .unwrap();
        std::fs::write(
            dir.join(".mcp.json"),
            r#"{"mcpServers":{"external":{"url":"https://e/mcp"},"inline":{"command":"ignored"}}}"#,
        )
        .unwrap();
        let m = parse_plugin_dir(&dir).expect("parse dir");
        let names: Vec<&str> = m.mcp_servers.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"inline"));
        assert!(names.contains(&"external"));
        assert_eq!(
            m.mcp_servers.len(),
            2,
            "同名 inline 以 plugin.json 为准，不重复"
        );
        // inline 仍是 plugin.json 的 command=node（非 .mcp.json 的 ignored）。
        let inline = m.mcp_servers.iter().find(|s| s.name == "inline").unwrap();
        match &inline.kind {
            ParsedMcpKind::Stdio { command, .. } => assert_eq!(command, "node"),
            _ => panic!("expected stdio"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 建一个临时插件目录，返回其路径。
    fn tmp_plugin_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "siw-plg-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn root_hooks_json_is_merged() {
        // Codex 插件（如 figma）把 hooks 放在**插件根** `hooks.json`，
        // 而非本项目原本只认的 `hooks/hooks.json`。两处都要读。
        let dir = tmp_plugin_dir("roothooks");
        std::fs::write(dir.join("plugin.json"), r#"{"name":"p"}"#).unwrap();
        std::fs::write(
            dir.join("hooks.json"),
            r#"{"hooks":{"PostToolUse":[{"matcher":"Write","hooks":[{"type":"command","command":"echo root"}]}]}}"#,
        )
        .unwrap();

        let m = parse_plugin_dir(&dir).expect("parse dir");
        assert!(
            m.hooks.iter().any(|h| h.command.contains("echo root")),
            "插件根 hooks.json 应被合并，实得 {:?}",
            m.hooks
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn agents_discovered_by_directory_convention() {
        // Codex/Claude 插件常不在 manifest 声明 agents，靠 `agents/*.md` 目录约定。
        let dir = tmp_plugin_dir("agentsdir");
        std::fs::write(dir.join("plugin.json"), r#"{"name":"p"}"#).unwrap();
        std::fs::create_dir_all(dir.join("agents")).unwrap();
        std::fs::write(dir.join("agents/reviewer.md"), "审查专家正文").unwrap();
        std::fs::write(dir.join("agents/writer.md"), "写作专家正文").unwrap();
        std::fs::write(dir.join("agents/notes.txt"), "非 md 应忽略").unwrap();

        let m = parse_plugin_dir(&dir).expect("parse dir");
        let mut got: Vec<&str> = m.agents.iter().map(String::as_str).collect();
        got.sort();
        assert_eq!(
            got,
            vec!["agents/reviewer.md", "agents/writer.md"],
            "应按目录约定发现 agents/*.md（忽略非 .md）"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn declared_agents_win_over_directory_scan() {
        // manifest 显式声明时以声明为准，不叠加目录扫描（避免重复/意外收编）。
        let dir = tmp_plugin_dir("agentsdecl");
        std::fs::write(
            dir.join("plugin.json"),
            r#"{"name":"p","agents":["agents/only-this.md"]}"#,
        )
        .unwrap();
        std::fs::create_dir_all(dir.join("agents")).unwrap();
        std::fs::write(dir.join("agents/only-this.md"), "x").unwrap();
        std::fs::write(dir.join("agents/not-declared.md"), "y").unwrap();

        let m = parse_plugin_dir(&dir).expect("parse dir");
        assert_eq!(
            m.agents,
            vec!["agents/only-this.md"],
            "声明优先，不叠加扫描"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dot_mcp_json_preserves_client_id() {
        let dir = std::env::temp_dir().join(format!(
            "siw-manifest-mcp-cid-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("plugin.json"), r#"{"name":"figma"}"#).unwrap();
        // Figma 插件的真实形态：能力全在 .mcp.json 声明的 OAuth 远程 MCP。
        std::fs::write(
            dir.join(".mcp.json"),
            r#"{"mcpServers":{"figma":{"type":"http","url":"https://mcp.figma.com/mcp","clientId":"cid-abc"}}}"#,
        )
        .unwrap();

        let m = parse_plugin_dir(&dir).expect("parse dir");
        assert_eq!(m.mcp_servers.len(), 1);
        match &m.mcp_servers[0].kind {
            ParsedMcpKind::Http {
                url,
                oauth_client_id,
                ..
            } => {
                assert_eq!(url, "https://mcp.figma.com/mcp");
                assert_eq!(oauth_client_id.as_deref(), Some("cid-abc"));
            }
            _ => panic!("expected http"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

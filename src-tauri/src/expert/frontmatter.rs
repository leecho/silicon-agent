//! agent 专家 frontmatter 解析：提取 name/description/tools/model/max_turns/role，并能 strip 正文。
//!
//! 约定：文件须以行首 `---` 开头到下一行 `---`，其后为专家 system prompt 正文。
//! `tools` 仅支持内联流式列表 `tools: [a, b, c]`（覆盖本期书写约定；块状列表非本期目标）。

/// frontmatter 提取结果。
pub struct Frontmatter {
    pub name: String,
    pub description: String,
    pub tools: Vec<String>,
    pub model_tier: String,
    pub max_turns: Option<u32>,
    pub role: String,
    /// 可选展示身份（纯 UI，不影响运行）。
    pub display_name: Option<String>,
    pub profession: Option<String>,
    pub avatar: Option<String>,
    pub color: Option<String>,
    /// 用户引导语（使用该专家的提示词列表）；以单行 JSON 数组存储于 frontmatter。
    pub quick_prompts: Vec<String>,
    /// 来自广场目录的稳定标识（「加入我的」时写入用户副本；自建/导入为 None）。
    pub catalog_id: Option<String>,
    /// 以下为广场发现元数据（仅目录条目用；落库不消费）。
    pub category: Option<String>,
    pub scenario: Option<String>,
    pub tags: Vec<String>,
    pub featured: bool,
    pub order: i64,
}

impl Frontmatter {
    /// 无 YAML frontmatter 时的兜底（Codex 插件的 agent `.md` 就没有 frontmatter，
    /// 靠文件名 + 正文；T106 P4 / T104 §6）。
    ///
    /// `name` 取文件名（stem），`description` 取正文首行非空文本（去掉 markdown 标题标记，
    /// 截断到 120 字符）。其余字段用与 `parse_frontmatter` 一致的缺省值。
    pub fn fallback(file_stem: &str, content: &str) -> Self {
        let description = content
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .map(|l| l.trim_start_matches('#').trim())
            .map(|l| {
                if l.chars().count() > 120 {
                    l.chars().take(120).collect::<String>()
                } else {
                    l.to_string()
                }
            })
            .unwrap_or_default();
        Frontmatter {
            name: file_stem.to_string(),
            description,
            tools: Vec::new(),
            // 与 parse_frontmatter 缺省一致：主模型。
            model_tier: "main".to_string(),
            max_turns: None,
            role: "member".to_string(),
            display_name: None,
            profession: None,
            avatar: None,
            color: None,
            quick_prompts: Vec::new(),
            catalog_id: None,
            category: None,
            scenario: None,
            tags: Vec::new(),
            featured: false,
            order: 0,
        }
    }
}

/// 去 value 首尾空白与一对包裹引号。
fn clean_value(raw: &str) -> String {
    let t = raw.trim();
    let b = t.as_bytes();
    if b.len() >= 2
        && ((b[0] == b'"' && b[b.len() - 1] == b'"') || (b[0] == b'\'' && b[b.len() - 1] == b'\''))
    {
        t[1..t.len() - 1].to_string()
    } else {
        t.to_string()
    }
}

/// 解析内联列表 `[a, b, c]`；非中括号包裹则按逗号分割兜底；空项过滤。
fn parse_list(raw: &str) -> Vec<String> {
    let t = raw.trim();
    let inner = t
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(t);
    inner
        .split(',')
        .map(clean_value)
        .filter(|s| !s.is_empty())
        .collect()
}

/// 解析 frontmatter；缺 `---` 包裹或缺 `name` 视为错误。
pub fn parse_frontmatter(content: &str) -> Result<Frontmatter, String> {
    // 与 skill 同治：先折叠 YAML 块标量（`description: >` + 缩进多行），否则逐行解析
    // 只会取到字面的 `>`。
    let folded = fold_frontmatter(content);
    let mut lines = folded.lines();
    match lines.next() {
        Some(l) if l.trim() == "---" => {}
        _ => return Err("专家定义缺少 YAML frontmatter（需以 --- 开头）".into()),
    }
    let mut name: Option<String> = None;
    let mut description = String::new();
    let mut tools: Vec<String> = Vec::new();
    // 缺省用主模型（仅显式写 model: aux 才走辅助模型）。
    let mut model_tier = "main".to_string();
    let mut max_turns: Option<u32> = None;
    let mut role = "member".to_string();
    let mut display_name: Option<String> = None;
    let mut profession: Option<String> = None;
    let mut avatar: Option<String> = None;
    let mut color: Option<String> = None;
    let mut quick_prompts: Vec<String> = Vec::new();
    let mut catalog_id: Option<String> = None;
    let mut category: Option<String> = None;
    let mut scenario: Option<String> = None;
    let mut tags: Vec<String> = Vec::new();
    let mut featured = false;
    let mut order: i64 = 0;
    let mut closed = false;
    // 非空才记，空串归 None。
    let opt = |v: &str| {
        let c = clean_value(v);
        if c.is_empty() {
            None
        } else {
            Some(c)
        }
    };
    for line in lines {
        if line.trim() == "---" {
            closed = true;
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            match k.trim() {
                "name" => name = Some(clean_value(v)),
                "description" => description = clean_value(v),
                "tools" => tools = parse_list(v),
                "model" => {
                    let m = clean_value(v).to_ascii_lowercase();
                    model_tier = if m == "main" {
                        "main".into()
                    } else {
                        "aux".into()
                    };
                }
                "max_turns" => max_turns = clean_value(v).parse::<u32>().ok(),
                "role" => {
                    let r = clean_value(v).to_ascii_lowercase();
                    role = if r == "lead" {
                        "lead".into()
                    } else {
                        "member".into()
                    };
                }
                "display_name" | "displayName" => display_name = opt(v),
                "profession" | "title" => profession = opt(v),
                "avatar" => avatar = opt(v),
                "color" => color = opt(v),
                "quick_prompts" | "quickPrompts" => {
                    quick_prompts = serde_json::from_str(v.trim()).unwrap_or_default();
                }
                "catalog_id" | "catalogId" => catalog_id = opt(v),
                "category" => category = opt(v),
                "scenario" => scenario = opt(v),
                "tags" => tags = parse_list(v),
                "featured" => {
                    featured = matches!(
                        clean_value(v).to_ascii_lowercase().as_str(),
                        "true" | "1" | "yes"
                    )
                }
                "order" => order = clean_value(v).parse::<i64>().unwrap_or(0),
                _ => {}
            }
        }
    }
    if !closed {
        return Err("专家定义 frontmatter 未正确闭合（缺少结束 ---）".into());
    }
    let name = name
        .filter(|n| !n.is_empty())
        .ok_or("专家定义 frontmatter 缺少 name")?;
    Ok(Frontmatter {
        name,
        description,
        tools,
        model_tier,
        max_turns,
        role,
        display_name,
        profession,
        avatar,
        color,
        quick_prompts,
        catalog_id,
        category,
        scenario,
        tags,
        featured,
        order,
    })
}

/// 去掉 frontmatter 返回正文；无合法 frontmatter 时原样返回。
pub fn strip_frontmatter(content: &str) -> String {
    if !content.trim_start().starts_with("---") {
        return content.to_string();
    }
    let mut lines = content.lines();
    if lines.next().map(|l| l.trim()) != Some("---") {
        return content.to_string();
    }
    let mut body: Vec<&str> = Vec::new();
    let mut closed = false;
    for line in lines.by_ref() {
        if !closed {
            if line.trim() == "---" {
                closed = true;
            }
            continue;
        }
        body.push(line);
    }
    if !closed {
        return content.to_string();
    }
    let mut out = body.join("\n");
    if content.ends_with('\n') && !out.is_empty() {
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "---\nname: builder\ndescription: 实现/修改文件\ntools: [read_file, write_file, edit_file]\nmodel: main\nmax_turns: 20\nrole: member\n---\n你是构建执行专家。\n";

    #[test]
    fn parses_catalog_fields() {
        let md = "---\nname: x\ndescription: d\ncategory: 金融投资\nscenario: 投资分析\ntags: [a, b]\nfeatured: true\norder: 5\ncatalog_id: cid-1\n---\n正文\n";
        let fm = parse_frontmatter(md).unwrap();
        assert_eq!(fm.category.as_deref(), Some("金融投资"));
        assert_eq!(fm.scenario.as_deref(), Some("投资分析"));
        assert_eq!(fm.tags, vec!["a", "b"]);
        assert!(fm.featured);
        assert_eq!(fm.order, 5);
        assert_eq!(fm.catalog_id.as_deref(), Some("cid-1"));
    }

    #[test]
    fn parses_all_fields() {
        let fm = parse_frontmatter(SAMPLE).expect("parse");
        assert_eq!(fm.name, "builder");
        assert_eq!(fm.description, "实现/修改文件");
        assert_eq!(fm.tools, vec!["read_file", "write_file", "edit_file"]);
        assert_eq!(fm.model_tier, "main");
        assert_eq!(fm.max_turns, Some(20));
        assert_eq!(fm.role, "member");
    }

    #[test]
    fn defaults_when_optional_missing() {
        let fm = parse_frontmatter("---\nname: r\ndescription: d\n---\nbody").expect("parse");
        assert!(fm.tools.is_empty());
        assert_eq!(fm.model_tier, "main"); // 缺省主模型
        assert_eq!(fm.max_turns, None);
        assert_eq!(fm.role, "member"); // 缺省 member
    }

    #[test]
    fn parses_display_identity() {
        let raw = "---\nname: market-analyst\ndescription: 行情分析\ndisplay_name: 何执舟\nprofession: 首席策略官\navatar: \"📈\"\ncolor: \"#0F172A\"\n---\n正文";
        let fm = parse_frontmatter(raw).expect("parse");
        assert_eq!(fm.display_name.as_deref(), Some("何执舟"));
        assert_eq!(fm.profession.as_deref(), Some("首席策略官"));
        assert_eq!(fm.avatar.as_deref(), Some("📈"));
        assert_eq!(fm.color.as_deref(), Some("#0F172A"));
    }

    #[test]
    fn absent_display_fields_none() {
        let fm = parse_frontmatter(SAMPLE).expect("parse");
        assert!(fm.display_name.is_none());
        assert!(fm.profession.is_none());
        assert!(fm.avatar.is_none());
        assert!(fm.color.is_none());
    }

    #[test]
    fn missing_name_errors() {
        assert!(parse_frontmatter("---\ndescription: d\n---\nx").is_err());
    }

    #[test]
    fn strip_returns_body() {
        assert_eq!(strip_frontmatter(SAMPLE).trim(), "你是构建执行专家。");
    }
}

/// 只折叠 frontmatter 区（首尾 `---` 之间）；正文是 Markdown，其中的 `>` 是引用块，不可折。
fn fold_frontmatter(content: &str) -> String {
    let mut it = content.splitn(3, "---");
    let (Some(head), Some(fm), Some(body)) = (it.next(), it.next(), it.next()) else {
        return content.to_string();
    };
    if !head.trim().is_empty() {
        return content.to_string();
    }
    format!(
        "---\n{}\n---{}",
        crate::yaml_block::fold_block_scalars(fm.trim_matches('\n')),
        body
    )
}

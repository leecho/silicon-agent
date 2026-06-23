//! 极简 YAML frontmatter 解析：只提取 `name`/`description` 两个扁平键，不引入 serde_yaml。
//!
//! 约定：文件须以行首 `---` 开头，到下一行 `---` 之间为 frontmatter，其后为正文。
//! 设计意图：覆盖 Claude Code 生态常见写法即可；复杂 yaml（数组/嵌套）非本期目标。

/// frontmatter 提取结果。
pub struct Frontmatter {
    pub name: String,
    pub description: String,
    /// 是否对用户可见/可调（默认 true）。`false` = 内部知识库技能，不进菜单，仅供其他 skill 运行时引用。
    pub user_invocable: bool,
    /// mention 菜单输入提示（如「上传合同文件或粘贴合同文本」）。
    pub argument_hint: Option<String>,
    /// 技能版本（仅追踪，不参与逻辑）。
    pub version: Option<String>,
}

/// 去除 value 首尾空白与一对包裹引号（单/双）。
fn clean_value(raw: &str) -> String {
    let t = raw.trim();
    let bytes = t.as_bytes();
    if bytes.len() >= 2
        && ((bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\''))
    {
        t[1..t.len() - 1].to_string()
    } else {
        t.to_string()
    }
}

/// 解析 frontmatter；缺少 `---` 包裹或缺 `name` 视为错误（安装时报错、sync 时跳过）。
pub fn parse_frontmatter(content: &str) -> Result<Frontmatter, String> {
    let mut lines = content.lines();
    // 第一行必须是 ---（严格匹配 trim 后的 ---）。
    match lines.next() {
        Some(l) if l.trim() == "---" => {}
        _ => return Err("SKILL.md 缺少 YAML frontmatter（需以 --- 开头）".into()),
    }
    let mut name: Option<String> = None;
    let mut description = String::new();
    let mut description_zh = String::new();
    let mut user_invocable = true;
    let mut argument_hint: Option<String> = None;
    let mut version: Option<String> = None;
    let mut closed = false;
    for line in lines {
        if line.trim() == "---" {
            closed = true;
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            match k.trim() {
                "name" => name = Some(clean_value(v)),
                "description" => description = clean_value(v),
                // 优先展示中文描述：生态技能常带 description_zh。
                "description_zh" => description_zh = clean_value(v),
                // 默认可见；显式 false 才隐藏（其它值按 true 容错）。
                "user-invocable" => user_invocable = !clean_value(v).eq_ignore_ascii_case("false"),
                "argument-hint" => argument_hint = non_empty(clean_value(v)),
                "version" => version = non_empty(clean_value(v)),
                _ => {}
            }
        }
    }
    if !closed {
        return Err("SKILL.md 的 frontmatter 未正确闭合（缺少结束 ---）".into());
    }
    let name = name
        .filter(|n| !n.is_empty())
        .ok_or("SKILL.md frontmatter 缺少 name")?;
    // description_zh 非空则优先，否则回退 description。
    let description = if description_zh.is_empty() {
        description
    } else {
        description_zh
    };
    Ok(Frontmatter {
        name,
        description,
        user_invocable,
        argument_hint,
        version,
    })
}

/// 空串归一为 None。
fn non_empty(s: String) -> Option<String> {
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

/// 去掉 frontmatter 块返回正文；无合法 frontmatter 时原样返回。
pub fn strip_frontmatter(content: &str) -> String {
    if !content.trim_start().starts_with("---") {
        return content.to_string();
    }
    let mut lines = content.lines();
    if lines.next().map(|l| l.trim()) != Some("---") {
        return content.to_string();
    }
    let mut body_lines: Vec<&str> = Vec::new();
    let mut closed = false;
    for line in lines.by_ref() {
        if !closed {
            if line.trim() == "---" {
                closed = true;
            }
            continue;
        }
        body_lines.push(line);
    }
    if !closed {
        return content.to_string();
    }
    // 保留行间换行；若原文以换行结尾则补回。
    let mut out = body_lines.join("\n");
    if content.ends_with('\n') && !out.is_empty() {
        out.push('\n');
    }
    out
}

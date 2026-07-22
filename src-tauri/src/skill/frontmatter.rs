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
    // 先把 YAML **块标量**（`description: >` / `|` + 缩进多行）折成单行，再走逐行解析。
    // 不折的话，逐行 split_once(':') 取到的 value 就是字面的 `>` —— 用户面上技能描述
    // 直接显示成一个 `>`（真实样本：QoderWork 法务插件，8 个技能全中招）。
    let folded = fold_frontmatter(content);
    let mut lines = folded.lines();
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

/// 只折叠 frontmatter 区（首尾 `---` 之间），正文原样保留 —— 正文是 Markdown，
/// 里面的 `>` 是引用块，绝不能当 YAML 折。
fn fold_frontmatter(content: &str) -> String {
    let mut it = content.splitn(3, "---");
    let (Some(head), Some(fm), Some(body)) = (it.next(), it.next(), it.next()) else {
        return content.to_string();
    };
    if !head.trim().is_empty() {
        return content.to_string(); // 首行不是 ---，交给调用方报错
    }
    format!(
        "---\n{}\n---{}",
        crate::yaml_block::fold_block_scalars(fm.trim_matches('\n')),
        body
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// YAML **块标量**描述必须被折出正文，而不是字面的 `>`。
    ///
    /// 照真实样本合成（QoderWork 法务插件的 `合规审查/SKILL.md`）：长描述用 `description: >`
    /// 折行写，此前用户面上 8 个技能的说明全部显示成一个 `>`。
    #[test]
    fn folded_block_scalar_description_is_parsed() {
        let md = "---\n\
                  name: 合规审查\n\
                  displayName: 合规审查\n\
                  description: >\n  \
                    按领域逐项检查个人信息保护、广告合规、资质牌照。\n  \
                    当用户要求\"合规审查\"、\"数据出境评估\"时触发此技能。\n\
                  argument-hint: \"输入需要审查的业务事项\"\n\
                  ---\n\n\
                  # 正文\n\n\
                  > 这是 Markdown 引用块，**不得**被当成 YAML 折叠。\n";
        let fm = parse_frontmatter(md).expect("应解析成功");
        assert_eq!(fm.name, "合规审查");
        assert_ne!(fm.description, ">", "描述不得是字面的 >");
        assert!(
            fm.description.contains("个人信息保护") && fm.description.contains("数据出境评估"),
            "折行的两段都要在：{}",
            fm.description
        );
        assert_eq!(
            fm.argument_hint.as_deref(),
            Some("输入需要审查的业务事项"),
            "块标量后的普通字段不能被吃掉"
        );
    }

    /// 单行描述（绝大多数技能）行为不变 —— 折叠器不得误伤。
    #[test]
    fn plain_single_line_description_unchanged() {
        let md = "---\nname: a\ndescription: 一句话说明\n---\n正文\n";
        let fm = parse_frontmatter(md).expect("解析");
        assert_eq!(fm.description, "一句话说明");
    }
}

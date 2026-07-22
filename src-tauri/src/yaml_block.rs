//! YAML **块标量**（block scalar）折叠：把 `key: >` / `key: |` 后面缩进的多行值，
//! 折成一行 `key: <值>`，供逐行式 frontmatter 解析器直接消费。
//!
//! **为什么需要**：技能/专家的 frontmatter 解析器是逐行 `split_once(':')` 取值的。遇到
//!
//! ```yaml
//! description: >
//!   第一行……
//!   第二行……
//! ```
//!
//! 它拿到的 value 就是字面的 `>` —— 用户面上技能描述直接显示成一个 `>`。
//! 而**长描述恰恰最需要块标量**（真实样本：QoderWork 的法务插件，8 个技能全中招）。
//!
//! 顺带堵住一个连带隐患：续行里若含 ASCII 冒号（如 `注意: 需先签署`），会被逐行解析器
//! 误当成新 key，**可能覆盖掉已解析的字段**。折叠后续行不再单独露出，隐患消失。
//!
//! 支持的形态：`>`、`|`，以及带 chomping/indent 指示符的 `>-` `|-` `>+` `|+` `>2` 等。
//! 折叠语义按 YAML：`>` 折行（行间用空格连接，空行=段落分隔），`|` 保留换行。

/// 把 frontmatter 正文（不含首尾 `---`）里的块标量折叠成单行 `key: value`。
///
/// 非块标量的行原样保留 —— 本函数只做这一件事，不试图成为 YAML 解析器。
pub fn fold_block_scalars(body: &str) -> String {
    let lines: Vec<&str> = body.lines().collect();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        let Some((key, marker)) = block_scalar_header(line) else {
            out.push(line.to_string());
            i += 1;
            continue;
        };
        let base_indent = indent_of(line);

        // 收续行：比 key 更深缩进的行；空行先收着（可能是段落分隔），由折叠逻辑决定去留。
        let mut chunk: Vec<&str> = Vec::new();
        let mut j = i + 1;
        while j < lines.len() {
            let l = lines[j];
            if l.trim().is_empty() {
                chunk.push("");
                j += 1;
                continue;
            }
            if indent_of(l) <= base_indent {
                break;
            }
            chunk.push(l.trim());
            j += 1;
        }
        // 尾部空行不属于值。
        while matches!(chunk.last(), Some(s) if s.is_empty()) {
            chunk.pop();
        }

        let value = if marker == '|' {
            chunk.join("\n")
        } else {
            fold_lines(&chunk)
        };
        out.push(format!("{key}: {value}"));
        i = j;
    }
    out.join("\n")
}

/// `>` 折行：连续非空行用空格连接；空行是段落分隔（保留为换行）。
fn fold_lines(chunk: &[&str]) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut cur: Vec<&str> = Vec::new();
    for l in chunk {
        if l.is_empty() {
            if !cur.is_empty() {
                parts.push(cur.join(" "));
                cur.clear();
            }
        } else {
            cur.push(l);
        }
    }
    if !cur.is_empty() {
        parts.push(cur.join(" "));
    }
    parts.join("\n")
}

/// 这行是不是块标量头（`key: >` / `key: |`，可带 `-`/`+`/数字指示符）？
/// 是则返回 `(key, '>' | '|')`。
fn block_scalar_header(line: &str) -> Option<(&str, char)> {
    let (k, v) = line.split_once(':')?;
    let key = k.trim();
    if key.is_empty() || key.contains(' ') {
        return None;
    }
    let v = v.trim();
    let mut cs = v.chars();
    let marker = match cs.next()? {
        '>' => '>',
        '|' => '|',
        _ => return None,
    };
    // 其余只允许 chomping/indent 指示符；有别的内容说明不是块标量头
    // （例如 `url: https://x` —— 那是普通值，绝不能误折）。
    if cs.any(|c| !matches!(c, '-' | '+' | '0'..='9')) {
        return None;
    }
    Some((key, marker))
}

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_folded_scalar_into_one_line() {
        let src = "name: 合规审查\ndescription: >\n  第一行。\n  第二行。\nversion: 1";
        let out = fold_block_scalars(src);
        assert_eq!(
            out,
            "name: 合规审查\ndescription: 第一行。 第二行。\nversion: 1"
        );
    }

    #[test]
    fn literal_scalar_keeps_newlines() {
        let src = "description: |\n  一\n  二";
        assert_eq!(fold_block_scalars(src), "description: 一\n二");
    }

    #[test]
    fn honors_chomping_indicators() {
        let src = "description: >-\n  甲\n  乙";
        assert_eq!(fold_block_scalars(src), "description: 甲 乙");
    }

    #[test]
    fn blank_line_is_paragraph_break() {
        let src = "d: >\n  一\n\n  二";
        assert_eq!(fold_block_scalars(src), "d: 一\n二");
    }

    /// **不得误折普通值**：`url: https://x` 的 value 以 `h` 开头、含 `:`，
    /// 但它不是块标量。误判会把整份 frontmatter 吃掉。
    #[test]
    fn plain_values_are_untouched() {
        let src = "url: https://example.com/a\nname: x\nhint: \"a > b\"";
        assert_eq!(fold_block_scalars(src), src);
    }

    /// 续行里含 ASCII 冒号：折叠后不再单独成行，不会被逐行解析器误当成新 key。
    #[test]
    fn continuation_with_colon_does_not_leak_as_key() {
        let src = "description: >\n  注意: 需先签署\n  再提交\nname: x";
        let out = fold_block_scalars(src);
        assert_eq!(out, "description: 注意: 需先签署 再提交\nname: x");
        // name 未被续行污染。
        assert!(out.lines().any(|l| l == "name: x"));
    }

    #[test]
    fn empty_block_yields_empty_value() {
        let src = "description: >\nname: x";
        assert_eq!(fold_block_scalars(src), "description: \nname: x");
    }
}

//! 结构感知分块：按 Markdown 标题/段落边界切分，带字符级重叠。
//! 纯文本（无 `#` 标题）退化为按段落 + 重叠切分，heading_path 为空。

/// 一个待入库的片段（未落库，无 id）。
#[derive(Debug, Clone, PartialEq)]
pub struct ChunkPiece {
    pub content: String,
    pub heading_path: String,
    pub ordinal: usize,
}

/// 把正文切成片段。`max_chars` 单片上限，`overlap` 相邻片重叠字符数。
/// 中文按 char 计数（非字节）。空白正文返回空 vec。
pub fn chunk_text(text: &str, max_chars: usize, overlap: usize) -> Vec<ChunkPiece> {
    let mut pieces = Vec::new();
    let mut heading_stack: Vec<(usize, String)> = Vec::new(); // (层级, 标题文本)
    let mut ordinal = 0usize;

    // 把已积累的正文 buffer 落成片段（超长按 max_chars 滑窗 + overlap）。
    let mut buffer = String::new();
    let flush = |buffer: &mut String,
                 heading_path: &str,
                 ordinal: &mut usize,
                 pieces: &mut Vec<ChunkPiece>| {
        let trimmed = buffer.trim();
        if trimmed.is_empty() {
            buffer.clear();
            return;
        }
        let chars: Vec<char> = trimmed.chars().collect();
        if chars.len() <= max_chars {
            pieces.push(ChunkPiece {
                content: trimmed.to_string(),
                heading_path: heading_path.to_string(),
                ordinal: *ordinal,
            });
            *ordinal += 1;
        } else {
            let step = max_chars.saturating_sub(overlap).max(1);
            let mut start = 0;
            while start < chars.len() {
                let end = (start + max_chars).min(chars.len());
                let slice: String = chars[start..end].iter().collect();
                pieces.push(ChunkPiece {
                    content: slice,
                    heading_path: heading_path.to_string(),
                    ordinal: *ordinal,
                });
                *ordinal += 1;
                if end == chars.len() {
                    break;
                }
                start += step;
            }
        }
        buffer.clear();
    };

    let current_path =
        |stack: &[(usize, String)]| -> String { stack.iter().map(|(_, t)| t.clone()).collect::<Vec<_>>().join(" › ") };

    for line in text.lines() {
        let t = line.trim_start();
        if let Some(rest) = t.strip_prefix('#') {
            // 计算 # 个数得层级。
            let level = 1 + rest.chars().take_while(|c| *c == '#').count();
            let title = t.trim_start_matches('#').trim().to_string();
            // 先把已积累的正文按旧路径落片。
            let path = current_path(&heading_stack);
            flush(&mut buffer, &path, &mut ordinal, &mut pieces);
            // 弹出 ≥ 当前层级的旧标题，压入新标题。
            heading_stack.retain(|(lvl, _)| *lvl < level);
            heading_stack.push((level, title));
        } else if t.is_empty() {
            let path = current_path(&heading_stack);
            flush(&mut buffer, &path, &mut ordinal, &mut pieces);
        } else {
            if !buffer.is_empty() {
                buffer.push('\n');
            }
            buffer.push_str(line);
        }
    }
    let path = current_path(&heading_stack);
    flush(&mut buffer, &path, &mut ordinal, &mut pieces);
    pieces
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_yields_no_chunks() {
        assert!(chunk_text("   \n\n  ", 100, 10).is_empty());
    }

    #[test]
    fn markdown_headings_become_heading_path() {
        let md = "# A\n\n段落一。\n\n## B\n\n段落二。";
        let chunks = chunk_text(md, 100, 10);
        assert!(chunks.iter().any(|c| c.heading_path == "A" && c.content.contains("段落一")));
        assert!(chunks
            .iter()
            .any(|c| c.heading_path == "A › B" && c.content.contains("段落二")));
    }

    #[test]
    fn long_paragraph_splits_with_overlap() {
        let body = "甲".repeat(250);
        let chunks = chunk_text(&body, 100, 20);
        assert!(chunks.len() >= 3, "应被切成多片");
        // 重叠：后一片开头与前一片结尾有交集。
        let tail: String = chunks[0]
            .content
            .chars()
            .rev()
            .take(20)
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        assert!(chunks[1].content.starts_with(&tail));
    }

    #[test]
    fn ordinals_are_sequential() {
        let md = format!("# T\n\n{}", "乙".repeat(300));
        let chunks = chunk_text(&md, 100, 10);
        for (i, c) in chunks.iter().enumerate() {
            assert_eq!(c.ordinal, i);
        }
    }
}

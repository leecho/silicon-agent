//! 远程输出格式化与回复解析的纯函数：分段、节流判定、HITL 渲染、编号解析。
//! 纯逻辑与 IO 分离，便于单测。

use crate::session::{AskQuestion, PendingPermission};

/// 按「字符数」上限把文本切成多段（不破坏多字节字符）。空串返回空 Vec。
pub fn segment_text(text: &str, max_len: usize) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    let max = max_len.max(1);
    let chars: Vec<char> = text.chars().collect();
    chars
        .chunks(max)
        .map(|c| c.iter().collect::<String>())
        .collect()
}

/// 进度节流判定（纯函数，时间以毫秒传入）。
/// last_emit_ms=None（本会话首条）放行；否则 now-last >= window 才放行。
pub fn should_emit_progress(last_emit_ms: Option<u64>, now_ms: u64, window_ms: u64) -> bool {
    match last_emit_ms {
        None => true,
        Some(last) => now_ms.saturating_sub(last) >= window_ms,
    }
}

/// 风险工具确认提示。input 为工具参数 JSON，原样展示（截断超长）。
pub fn render_permission(p: &PendingPermission) -> String {
    let arg = truncate_chars(&p.input, 500);
    format!(
        "⚠️ 需要确认操作\n工具：{}\n参数：{}\n回复 1 批准 / 2 拒绝",
        p.tool_name, arg
    )
}

/// 渲染 ask_user 的第 idx 题（共 total 题），选项带 1 起编号。multi_select 时提示可多选。
pub fn render_ask_question(q: &AskQuestion, idx: usize, total: usize) -> String {
    let mut out = String::new();
    if total > 1 {
        out.push_str(&format!("（第 {}/{} 题）", idx + 1, total));
        out.push('\n');
    }
    if !q.header.is_empty() {
        out.push_str(&format!("[{}] ", q.header));
    }
    out.push_str(&q.question);
    out.push('\n');
    for (i, opt) in q.options.iter().enumerate() {
        out.push_str(&format!("{}. {}\n", i + 1, opt));
    }
    if q.multi_select {
        out.push_str("可多选，用逗号分隔（如 1,3）。回复编号：");
    } else {
        out.push_str("回复编号：");
    }
    out
}

/// 计划批准提示。
pub fn render_plan(plan_text: &str) -> String {
    format!(
        "📋 待批准计划\n{}\n回复 1 批准执行 / 2 拒绝",
        truncate_chars(plan_text, 1500)
    )
}

/// 解析批准/拒绝回复。只认 1 / 2 / /y / /n（高置信度结构化信号），其余返回 None。
pub fn parse_yes_no(reply: &str) -> Option<bool> {
    match reply.trim() {
        "1" | "/y" => Some(true),
        "2" | "/n" => Some(false),
        _ => None,
    }
}

/// 解析选项编号回复，返回 0 起的索引列表。
/// option_count：选项总数；multi：是否允许多选。任何越界 / 非数字 / 单选给多个 → None。
pub fn parse_choice(reply: &str, option_count: usize, multi: bool) -> Option<Vec<usize>> {
    let parts: Vec<&str> = reply
        .trim()
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    if parts.is_empty() {
        return None;
    }
    if !multi && parts.len() > 1 {
        return None;
    }
    let mut out = Vec::new();
    for p in parts {
        let n: usize = p.parse().ok()?;
        if n == 0 || n > option_count {
            return None;
        }
        out.push(n - 1);
    }
    Some(out)
}

fn truncate_chars(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max {
        s.to_string()
    } else {
        let mut t: String = chars[..max].iter().collect();
        t.push('…');
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{AskQuestion, PendingPermission};

    #[test]
    fn segment_short_text_single_chunk() {
        assert_eq!(segment_text("hello", 10), vec!["hello".to_string()]);
    }

    #[test]
    fn segment_long_text_by_char_count() {
        let s = "一二三四五六七八九十"; // 10 个汉字
        let chunks = segment_text(s, 4);
        assert_eq!(chunks, vec!["一二三四", "五六七八", "九十"]);
    }

    #[test]
    fn segment_empty_yields_empty_vec() {
        assert!(segment_text("", 10).is_empty());
    }

    #[test]
    fn throttle_allows_first_then_blocks_within_window() {
        assert!(should_emit_progress(None, 1000, 3000));
        assert!(!should_emit_progress(Some(1000), 3000, 3000));
        assert!(should_emit_progress(Some(1000), 4000, 3000));
        assert!(should_emit_progress(Some(1000), 6000, 3000));
    }

    #[test]
    fn render_permission_prompt_numbered() {
        let p = PendingPermission {
            session_id: "sess1".into(),
            tool_call_id: "tc1".into(),
            tool_name: "command_tool".into(),
            input: "{\"command\":\"rm -rf ./tmp\"}".into(),
        };
        let out = render_permission(&p);
        assert!(out.contains("需要确认操作"));
        assert!(out.contains("command_tool"));
        assert!(out.contains("rm -rf ./tmp"));
        assert!(out.contains("1 批准"));
        assert!(out.contains("2 拒绝"));
    }

    #[test]
    fn render_ask_question_lists_options() {
        let q = AskQuestion {
            header: "环境".into(),
            question: "部署到哪个环境？".into(),
            multi_select: false,
            options: vec!["生产".into(), "预发".into()],
        };
        let out = render_ask_question(&q, 0, 1);
        assert!(out.contains("部署到哪个环境？"));
        assert!(out.contains("1. 生产"));
        assert!(out.contains("2. 预发"));
    }

    #[test]
    fn render_plan_prompt_numbered() {
        let out = render_plan("先读配置，再改三处，最后跑测试。");
        assert!(out.contains("先读配置"));
        assert!(out.contains("1 批准执行"));
        assert!(out.contains("2 拒绝"));
    }

    #[test]
    fn parse_yes_no_reply() {
        assert_eq!(parse_yes_no("1"), Some(true));
        assert_eq!(parse_yes_no("/y"), Some(true));
        assert_eq!(parse_yes_no(" 2 "), Some(false));
        assert_eq!(parse_yes_no("/n"), Some(false));
        assert_eq!(parse_yes_no("批准吧"), None);
        assert_eq!(parse_yes_no("3"), None);
    }

    #[test]
    fn parse_choice_single_and_multi() {
        assert_eq!(parse_choice("2", 4, false), Some(vec![1]));
        assert_eq!(parse_choice("1,3", 4, true), Some(vec![0, 2]));
        assert_eq!(parse_choice("5", 4, false), None);
        assert_eq!(parse_choice("1,2", 4, false), None);
        assert_eq!(parse_choice("是", 4, false), None);
    }
}

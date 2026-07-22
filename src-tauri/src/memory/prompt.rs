//! 记忆段渲染：把召回结果渲染成注入 system_prompt 的「## 用户画像」+「## 相关记忆」两段。
//!
//! 见 spec §3.5。由 `context/prompt.rs` 调用拼接——记忆的呈现逻辑归属 memory 模块，
//! context 只负责把返回的段落拼进整体 system prompt。

use crate::memory::types::Memory;

/// 画像段字符预算（Tier1 常驻，给足）。
const PROFILE_BUDGET_CHARS: usize = 4000;
/// 相关记忆段字符预算（Tier2 检索结果拼接上限）。
const FACTS_BUDGET_CHARS: usize = 4000;

/// 渲染记忆段。`profile`=用户画像整段（None/空则省略）；`facts`=召回的事实条目；
/// `episodes`=召回的情景（会话历史摘要）。三者皆空返回空串（调用方不拼接）。
pub fn render(profile: Option<&str>, facts: &[Memory], episodes: &[Memory]) -> String {
    let mut out = String::new();

    if let Some(p) = profile {
        let p = p.trim();
        if !p.is_empty() {
            out.push_str("## 用户画像\n");
            out.push_str(clip(p, PROFILE_BUDGET_CHARS).as_str());
            out.push('\n');
        }
    }

    let facts_body = bullet_list(facts, FACTS_BUDGET_CHARS);
    if !facts_body.is_empty() {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str("## 相关记忆\n");
        out.push_str(&facts_body);
        out.push_str(
            "（以上为与当前任务相关的长期记忆，回答时纳入考虑；如发现新的值得长期记住的事实/偏好，用 remember 工具记录。）",
        );
    }

    let ep_body = bullet_list(episodes, FACTS_BUDGET_CHARS);
    if !ep_body.is_empty() {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str("## 相关历史\n");
        out.push_str(&ep_body);
        out.push_str("（以上为相关的过往会话摘要，供参考延续。）");
    }

    out
}

/// 把记忆条目渲染成「- 内容」列表，超预算截断尾部并加提示。空集返回空串。
fn bullet_list(items: &[Memory], budget: usize) -> String {
    if items.is_empty() {
        return String::new();
    }
    let mut body = String::new();
    let mut truncated = false;
    for m in items {
        let line = format!("- {}\n", m.content);
        if body.chars().count() + line.chars().count() > budget {
            truncated = true;
            break;
        }
        body.push_str(&line);
    }
    if truncated {
        body.push_str("…（更多未列出）\n");
    }
    body
}

/// 超预算则截断尾部（按字符）。
fn clip(text: &str, budget: usize) -> String {
    if text.chars().count() <= budget {
        return text.to_string();
    }
    let head: String = text.chars().take(budget).collect();
    format!("{head}…（已截断）")
}

#[cfg(test)]
mod tests {
    use super::render;
    use crate::memory::types::Memory;

    fn mem(content: &str) -> Memory {
        Memory {
            id: content.into(),
            content: content.into(),
            created_at: "1".into(),
        }
    }

    #[test]
    fn renders_all_sections() {
        let facts = vec![mem("项目用 Rust")];
        let episodes = vec![mem("上次部署到 K8s")];
        let out = render(Some("用户偏好简洁"), &facts, &episodes);
        assert!(out.contains("## 用户画像"));
        assert!(out.contains("用户偏好简洁"));
        assert!(out.contains("## 相关记忆"));
        assert!(out.contains("项目用 Rust"));
        assert!(out.contains("## 相关历史"));
        assert!(out.contains("上次部署到 K8s"));
    }

    #[test]
    fn empty_when_nothing() {
        assert_eq!(render(None, &[], &[]), "");
        assert_eq!(render(Some("  "), &[], &[]), "");
    }

    #[test]
    fn facts_only_omits_other_sections() {
        let out = render(None, &[mem("事实")], &[]);
        assert!(!out.contains("## 用户画像"));
        assert!(!out.contains("## 相关历史"));
        assert!(out.contains("## 相关记忆"));
    }
}

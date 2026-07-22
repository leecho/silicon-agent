//! 主动整理（Curator）：模型驱动的事实去重/合并 + 用户画像抽取。
//!
//! 对应 Hermes Curator/InsightsEngine 的简化对齐（spec §3.4）。把累积的零散 fact 交给模型
//! 去重合并成精炼集合，并从中抽取/更新用户画像。模型输出经 schema 校验后落库（遵守 AGENTS.md
//! 「模型驱动 + schema 校验，不用脆弱关键词规则」）。
//!
//! 应用策略：保留置顶 fact 与画像/情景，只重建未置顶 fact——避免误删用户手工锁定的内容。

use crate::app_state::now_string;
use crate::memory::types::{Memory, MemoryScope};
use crate::memory::MemoryStore;
use crate::provider::client::{ModelCallRequest, ModelClient, ModelEvent};
use crate::provider::message::ModelMessage;
use crate::provider::model::ResolvedModel;

/// 触发整理的最小 fact 数：低于此值整理收益不大，直接跳过。
const MIN_FACTS_TO_CURATE: usize = 6;

/// 模型整理结果（schema 校验目标）：去重合并后的 fact 列表 + 可选画像。
#[derive(Debug, serde::Deserialize)]
pub struct CurationResult {
    /// 去重合并后保留的事实条目（已合并同类、去重）。
    #[serde(default)]
    pub facts: Vec<String>,
    /// 抽取/更新后的用户画像整段（无变化时模型回原画像；空表示不设画像）。
    #[serde(default)]
    pub profile: Option<String>,
}

/// 整理结果概要（返回给调用方/命令）。
#[derive(Debug, serde::Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CurationOutcome {
    pub ran: bool,
    pub facts_before: usize,
    pub facts_after: usize,
    pub profile_updated: bool,
}

/// 构造整理提示：要求模型对事实去重/合并、并据此抽取/更新画像，**只输出 JSON**。
pub fn build_prompt(facts: &[Memory], current_profile: &str) -> String {
    let list = facts
        .iter()
        .map(|m| format!("- {}", m.content))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "你是长期记忆整理员。下面是累积的零散「事实」记忆与当前「用户画像」。\n\
         请：①合并重复/同类事实，删除明显过时项，得到精炼的事实列表；\
         ②从事实中抽取稳定的用户偏好/背景，更新画像（无新信号则原样返回画像）。\n\n\
         当前画像：\n{current_profile}\n\n\
         事实列表：\n{list}\n\n\
         只输出 JSON，格式：{{\"facts\":[\"...\",\"...\"],\"profile\":\"...\"}}。\
         不要输出任何解释或代码围栏。",
    )
}

/// 解析模型输出为 `CurationResult`：容忍 ```json 围栏与前后噪声，截取首个 `{` 到末个 `}`。
pub fn parse_curation(text: &str) -> Result<CurationResult, String> {
    let start = text.find('{').ok_or("模型输出缺少 JSON 对象")?;
    let end = text.rfind('}').ok_or("模型输出缺少 JSON 对象")?;
    if end < start {
        return Err("模型输出 JSON 不完整".into());
    }
    serde_json::from_str::<CurationResult>(&text[start..=end])
        .map_err(|e| format!("整理结果 JSON 解析失败：{e}"))
}

/// 应用整理结果：用合并后的 fact 重建未置顶 fact，并更新画像（非空时）。
/// 返回 (重建后的 fact 数, 画像是否更新)。
pub fn apply_curation(
    store: &MemoryStore,
    result: &CurationResult,
    now: &str,
) -> Result<(usize, bool), String> {
    store.clear_unpinned_facts()?;
    let mut added = 0usize;
    for content in &result.facts {
        let c = content.trim();
        if c.is_empty() {
            continue;
        }
        store.add_memory(c, now, MemoryScope::Global)?;
        added += 1;
    }
    let mut profile_updated = false;
    if let Some(p) = &result.profile {
        if !p.trim().is_empty() {
            store.set_profile(p, now)?;
            profile_updated = true;
        }
    }
    Ok((added, profile_updated))
}

/// 整理编排：取 fact → 调模型 → 解析 → 应用。fact 不足阈值则跳过（ran=false）。
pub fn curate(
    store: &MemoryStore,
    client: &dyn ModelClient,
    selection: Option<&ResolvedModel>,
) -> Result<CurationOutcome, String> {
    let facts = store.list_memories()?;
    let before = facts.len();
    if before < MIN_FACTS_TO_CURATE {
        return Ok(CurationOutcome {
            ran: false,
            facts_before: before,
            facts_after: before,
            profile_updated: false,
        });
    }
    let current_profile = store.get_profile()?.unwrap_or_default();
    let prompt = build_prompt(&facts, &current_profile);

    let request = ModelCallRequest {
        messages: vec![ModelMessage::user(&prompt)],
        max_output_tokens: Some(1200),
        stream: false,
        model_selection: selection.map(|r| r.selection()),
        attribution: crate::provider::message::ModelAttribution {
            usage_type: Some("curation".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };
    let resp = client
        .complete_model(request)
        .map_err(|e| format!("记忆整理失败：{}", e.message))?;
    let text = final_assistant_text(&resp);
    let result = parse_curation(&text)?;

    let now = now_string();
    let (after, profile_updated) = apply_curation(store, &result, &now)?;
    Ok(CurationOutcome {
        ran: true,
        facts_before: before,
        facts_after: after,
        profile_updated,
    })
}

/// 取最终 assistant 文本（与 compaction 同型 helper）。
fn final_assistant_text(result: &crate::provider::client::ModelCallResult) -> String {
    for event in result.events.iter().rev() {
        if let ModelEvent::AssistantMessageCompleted { content } = event {
            return content.clone();
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_tolerates_code_fence_and_noise() {
        let text = "整理完成：\n```json\n{\"facts\":[\"用户用 Rust\"],\"profile\":\"工程师\"}\n```";
        let r = parse_curation(text).expect("parse");
        assert_eq!(r.facts, vec!["用户用 Rust".to_string()]);
        assert_eq!(r.profile.as_deref(), Some("工程师"));
    }

    #[test]
    fn parse_rejects_non_json() {
        assert!(parse_curation("没有 JSON 的纯文本").is_err());
    }
}

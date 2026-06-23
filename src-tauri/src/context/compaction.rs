//! 上下文压缩：边界计算 + 摘要内核。
//!
//! 把较早的对话历史摘要进 `compaction_summary` 并标 compacted=1，使其不再进模型上下文、
//! 但仍持久化可见。手动 `/compact` 与自动压缩共用 [`compact`]（自身不发事件，可见性由调用方决定）。

use crate::app_state::now_string;
use crate::provider::client::{ModelCallRequest, ModelClient, ModelEvent};
use crate::provider::message::ModelMessage;
use crate::provider::model::ResolvedModel;
use crate::session::types::Message;
use crate::session::{new_id, SessionStore};

/// compact 保留的最近消息条数：这之外的未压缩旧消息会被摘要吸收。
pub const KEEP_RECENT: usize = 12;

/// 计算压缩边界：返回「保留区」起始下标 boundary —— all[..boundary] 为待压候选、
/// all[boundary..] 保留。在 len-keep_recent 基础上向前收敛：若保留区开头是 tool 结果，
/// 把边界前移使其 assistant 调用一并保留，绝不留下孤儿 tool 结果（否则部分 provider 报 400）。
pub fn compact_boundary(all: &[Message], keep_recent: usize) -> usize {
    let mut boundary = all.len().saturating_sub(keep_recent);
    while boundary > 0 && all.get(boundary).map(|m| m.role == "tool").unwrap_or(false) {
        boundary -= 1;
    }
    boundary
}

/// 从归一化结果取最终 assistant 文本（流式累积为空时的回退）。
///
/// 与 engine 的同名 helper 等价；此处保留本地私有副本，避免 context → engine 反向依赖。
fn final_assistant_text(result: &crate::provider::client::ModelCallResult) -> String {
    for event in result.events.iter().rev() {
        if let ModelEvent::AssistantMessageCompleted { content } = event {
            return content.clone();
        }
    }
    String::new()
}

/// 压缩内核：摘要 `all[..boundary]` 中未压缩旧消息进 `compaction_summary` 并标记
/// compacted=1。返回是否真的压缩（候选 < 4 条则跳过返回 false）。
pub fn compact(
    session: &SessionStore,
    client: &dyn ModelClient,
    selection: Option<&ResolvedModel>,
    session_id: &str,
) -> Result<bool, String> {
    let all = session.list_messages(session_id)?;
    let boundary = compact_boundary(&all, KEEP_RECENT);
    let old: Vec<&Message> = all[..boundary].iter().filter(|m| !m.compacted).collect();
    if old.len() < 4 {
        return Ok(false);
    }

    let prev = session
        .get_compaction_summary(session_id)?
        .unwrap_or_default();
    let text = old
        .iter()
        .map(|m| format!("{}: {}", m.role, m.content))
        .collect::<Vec<_>>()
        .join("\n");
    let prompt = format!(
        "你是上下文整理员。把下面的对话历史浓缩成简洁摘要(中文, 保留关键事实/决策/进度/未完成项, 300 字内)。\n\n已有摘要:\n{prev}\n\n待压缩对话:\n{text}\n\n只输出摘要正文。"
    );

    let mut request = ModelCallRequest {
        messages: vec![ModelMessage::user(&prompt)],
        max_output_tokens: Some(600),
        stream: false,
        model_selection: selection.map(|r| r.selection()),
        ..Default::default()
    };
    request.attribution.session_id = session_id.to_string();
    request.attribution.usage_type = Some("compaction".to_string());

    let result = client
        .complete_model(request)
        .map_err(|e| format!("压缩失败：{}", e.message))?;
    let summary = final_assistant_text(&result);
    if summary.trim().is_empty() {
        return Err("压缩失败：模型未返回摘要".into());
    }

    let now = now_string();
    session.set_compaction_summary(session_id, &summary, &now)?;
    let ids: Vec<String> = old.iter().map(|m| m.id.clone()).collect();
    session.mark_compacted(session_id, &ids)?;

    // 落一条持久「已压缩」分隔提示：专属 role="compaction" 供前端渲染成分隔线；
    // 标 compacted=1 → 不进模型上下文（即便 role 落入 user 分支也已被跳过）。
    // 使提示在 reload 后仍存在（前端事件推的临时行只用于即时反馈）。
    let marker_id = new_id("msg");
    session.append_message(
        &marker_id,
        session_id,
        "compaction",
        "已压缩较早对话历史，上下文已精简。",
        None,
        &now,
    )?;
    session.mark_compacted(session_id, &[marker_id])?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造一条最小消息，仅设测试关心的 role / tool_calls。
    fn msg(role: &str, with_tool_calls: bool) -> Message {
        Message {
            id: format!("m-{role}-{}", with_tool_calls as u8),
            session_id: "s".into(),
            role: role.into(),
            content: "x".into(),
            reasoning: None,
            tool_calls_json: if with_tool_calls {
                Some("[]".into())
            } else {
                None
            },
            tool_call_id: None,
            tool_name: None,
            tool_status: None,
            compacted: false,
            created_at: "0".into(),
        }
    }

    #[test]
    fn boundary_plain_conversation_is_len_minus_keep() {
        // 20 条纯 user/assistant，keep=12 → 边界=8
        let mut all = Vec::new();
        for i in 0..20 {
            all.push(msg(if i % 2 == 0 { "user" } else { "assistant" }, false));
        }
        assert_eq!(compact_boundary(&all, 12), 8);
    }

    #[test]
    fn boundary_retreats_before_assistant_when_kept_head_is_tool() {
        // idx7=assistant(tool_calls)，idx8=tool 结果。keep=12 → 初始边界=8(tool)，应回退到 7，
        // 使该 assistant 与其 tool 结果一同保留，不产生孤儿 tool 结果。
        let mut all = Vec::new();
        for _ in 0..7 {
            all.push(msg("user", false));
        }
        all.push(msg("assistant", true)); // idx7
        all.push(msg("tool", false)); // idx8
        for _ in 0..11 {
            all.push(msg("assistant", false));
        }
        assert_eq!(all.len(), 20);
        assert_eq!(compact_boundary(&all, 12), 7);
    }

    #[test]
    fn boundary_zero_when_shorter_than_keep() {
        let all = vec![msg("user", false), msg("assistant", false)];
        assert_eq!(compact_boundary(&all, 12), 0);
    }
}

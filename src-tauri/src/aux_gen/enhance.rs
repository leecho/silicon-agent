//! 「增强消息」的辅助生成：把用户在输入框写的口语化草稿润色 + 补全缺失上下文，
//! 改写成结构清晰、指令明确的提示词（一次性非流式 LLM 调用）。
//!
//! 与 title/suggestions 不同，本生成是**同步请求/响应**——用户在等待，结果直接回填输入框，
//! 因此不走后台 spawn + emit，而是由命令层调用本函数并把结果返回前端。

use crate::provider::ProviderGateway;

use super::shared::extract_completion_text;

/// 指导模型如何增强：保留原意、补全上下文、改写成清晰提示词，只输出正文。
const ENHANCE_SYSTEM_PROMPT: &str = "你是提示词增强助手。用户会给你一段写给 AI 的草稿（往往口语化、\
上下文不全）。请在**严格保留用户原意**的前提下，把它改写成结构清晰、指令明确的提示词：\
修正表达、补全明显缺失的上下文、让需求和期望更具体。不要替用户臆造未提及的事实或约束，\
不要扩大需求范围。直接输出改写后的提示词正文，使用与草稿相同的语言，\
不要任何解释、引号、标题或前后缀。";

/// 把草稿润色 + 补全为清晰提示词（一次性非流式调用）。成功返回改写后的正文，失败返回 Err。
pub fn enhance_message(
    provider: &ProviderGateway,
    selection: Option<crate::provider::model::ResolvedModel>,
    session_id: &str,
    draft: &str,
) -> Result<String, String> {
    use crate::provider::client::{ModelCallRequest, ModelClient};
    use crate::provider::message::ModelMessage;

    let mut request = ModelCallRequest {
        messages: vec![
            ModelMessage::system(ENHANCE_SYSTEM_PROMPT),
            ModelMessage::user(draft),
        ],
        // 推理模型先耗 token 做思维链，太小会把答案截没；给足空间（与 title/suggestions 一致）。
        max_output_tokens: Some(1024),
        timeout_ms: Some(30_000),
        stream: false,
        model_selection: selection.as_ref().map(|r| r.selection()),
        ..Default::default()
    };
    request.attribution.session_id = session_id.to_string();
    request.attribution.usage_type = Some("enhance".to_string());

    let result = provider.complete_model(request).map_err(|err| {
        eprintln!("[enhance] 模型调用失败 会话={session_id}：{}", err.message);
        err.message
    })?;
    let raw = extract_completion_text(&result);
    eprintln!(
        "[enhance] 模型原始输出 会话={session_id} finish={:?}：{raw}",
        result.finish_reason
    );
    let enhanced = raw.trim().to_string();
    if enhanced.is_empty() {
        return Err("增强结果为空".to_string());
    }
    Ok(enhanced)
}

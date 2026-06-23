//! OpenAI-compatible provider request and response normalization.
//!
//! 本模块只处理 provider 协议形态，不承担 Agent 决策、工具授权或事实持久化。

use std::collections::BTreeMap;

use crate::provider::client::{ModelCallResult, ModelEvent, ModelUsage, ProviderCallError};
use crate::provider::message::{ModelMessage, ModelMessageRole, ModelToolChoice, ToolSpecForModel};

#[derive(Debug, Default)]
struct ToolCallAccumulator {
    id: Option<String>,
    name: Option<String>,
    arguments_json: String,
}

/// 将模型消息映射为 OpenAI-compatible chat message。
pub fn model_message_to_openai(message: &ModelMessage) -> serde_json::Value {
    let role = match message.role {
        ModelMessageRole::System => "system",
        ModelMessageRole::User => "user",
        ModelMessageRole::Assistant => "assistant",
        ModelMessageRole::Tool => "tool",
    };
    let mut value = serde_json::json!({
        "role": role,
        "content": message.content,
    });
    if let Some(tool_call_id) = &message.tool_call_id {
        value["tool_call_id"] = serde_json::Value::String(tool_call_id.clone());
    }
    if let Some(tool_calls) = &message.tool_calls {
        value["tool_calls"] = serde_json::Value::Array(
            tool_calls
                .iter()
                .map(|tool_call| {
                    serde_json::json!({
                        "id": tool_call.id,
                        "type": "function",
                        "function": {
                            "name": tool_call.name,
                            "arguments": tool_call.arguments_json
                        }
                    })
                })
                .collect(),
        );
    }
    value
}

/// 将模型可见工具映射为 OpenAI-compatible function tool。
pub fn tool_spec_to_openai(tool: &ToolSpecForModel) -> serde_json::Value {
    serde_json::json!({
        "type": "function",
        "function": {
            "name": tool.name,
            "description": tool.description,
            "parameters": tool.input_schema
        }
    })
}

/// 将工具选择策略映射为 OpenAI-compatible `tool_choice`。
pub fn tool_choice_to_openai(choice: &ModelToolChoice) -> serde_json::Value {
    match choice {
        ModelToolChoice::Auto => serde_json::Value::String("auto".into()),
        ModelToolChoice::None => serde_json::Value::String("none".into()),
        ModelToolChoice::Required => serde_json::Value::String("required".into()),
        ModelToolChoice::Named(name) => serde_json::json!({
            "type": "function",
            "function": { "name": name }
        }),
    }
}

/// 归一化 OpenAI-compatible 非流式响应。
pub fn normalize_chat_completion_response(
    value: serde_json::Value,
) -> Result<ModelCallResult, ProviderCallError> {
    let choice = value
        .pointer("/choices/0")
        .ok_or_else(|| ProviderCallError::new("provider response missing choices[0]"))?;
    let message = choice
        .get("message")
        .ok_or_else(|| ProviderCallError::new("provider response missing choices[0].message"))?;
    let mut events = Vec::new();
    if let Some(tool_calls) = message.get("tool_calls").and_then(|value| value.as_array()) {
        for tool_call in tool_calls {
            if let Some(event) = tool_call_value_to_event(tool_call) {
                events.push(event);
            }
        }
    }
    if let Some(content) = message.get("content").and_then(|value| value.as_str()) {
        if !content.is_empty() {
            events.push(ModelEvent::AssistantMessageCompleted {
                content: content.into(),
            });
        }
    }
    let finish_reason = choice
        .get("finish_reason")
        .and_then(|value| value.as_str())
        .map(str::to_string);
    let usage = value.get("usage").map(parse_model_usage);
    Ok(ModelCallResult {
        events,
        usage,
        finish_reason,
    })
}

/// 归一化 OpenAI-compatible SSE 行。
///
/// token delta 只生成 transient `ModelEvent::Delta`；完整 assistant 内容在流结束时归并为
/// `AssistantMessageCompleted`，供上层决定是否持久化结构化事实。
pub fn normalize_chat_completion_stream_lines<I, S>(
    lines: I,
) -> Result<ModelCallResult, ProviderCallError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut events = Vec::new();
    let mut content = String::new();
    let mut finish_reason = None;
    let mut tool_calls: BTreeMap<u64, ToolCallAccumulator> = BTreeMap::new();
    let mut usage: Option<ModelUsage> = None;

    for line in lines {
        let line = line.as_ref().trim();
        if line.is_empty() || line.starts_with(':') {
            continue;
        }
        let Some(payload) = line.strip_prefix("data:") else {
            continue;
        };
        let payload = payload.trim();
        if payload == "[DONE]" {
            break;
        }
        let value: serde_json::Value = match serde_json::from_str(payload) {
            Ok(value) => value,
            Err(_) => continue, // 坏块/非 JSON 噪声：跳过不杀流（ADR-0020 决策 5）
        };
        // 流内 error 帧：识别为 Error 事件，由上层按瞬时/终态分类。
        if let Some(error) = value.get("error") {
            let message = error
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("provider stream error");
            events.push(ModelEvent::Error {
                message: message.to_string(),
            });
            continue;
        }
        if let Some(usage_value) = value.get("usage") {
            if !usage_value.is_null() {
                usage = Some(parse_model_usage(usage_value));
            }
        }
        let Some(choice) = value.get("choices").and_then(|value| value.get(0)) else {
            continue;
        };
        if let Some(reason) = choice.get("finish_reason").and_then(|value| value.as_str()) {
            finish_reason = Some(reason.to_string());
        }
        let Some(delta) = choice.get("delta") else {
            continue;
        };
        if let Some(text) = delta
            .get("reasoning_content")
            .or_else(|| delta.get("reasoning"))
            .and_then(|value| value.as_str())
        {
            events.push(ModelEvent::ThinkingDelta {
                text: text.to_string(),
            });
        }
        if let Some(text) = delta.get("content").and_then(|value| value.as_str()) {
            content.push_str(text);
            events.push(ModelEvent::Delta {
                text: text.to_string(),
            });
        }
        if let Some(delta_tool_calls) = delta.get("tool_calls").and_then(|value| value.as_array()) {
            for tool_call in delta_tool_calls {
                accumulate_tool_call(tool_call, &mut tool_calls);
            }
        }
    }

    for accumulator in tool_calls.into_values() {
        if let (Some(id), Some(name)) = (accumulator.id, accumulator.name) {
            events.push(ModelEvent::ToolCallCreated {
                id,
                name,
                arguments_json: accumulator.arguments_json,
            });
        }
    }
    if !content.is_empty() {
        events.push(ModelEvent::AssistantMessageCompleted { content });
    }

    Ok(ModelCallResult {
        events,
        usage,
        finish_reason,
    })
}

/// 解析 OpenAI-compatible `usage` 对象为 `ModelUsage`，兼容两类缓存字段命名：
/// OpenAI `prompt_tokens_details.cached_tokens`；Anthropic-compat
/// `cache_read_input_tokens` / `cache_creation_input_tokens`。
pub(crate) fn parse_model_usage(usage: &serde_json::Value) -> ModelUsage {
    // OpenAI 字段优先；Anthropic-compat 字段作后备（两者同时存在时取前者）。
    let cache_read = usage
        .pointer("/prompt_tokens_details/cached_tokens")
        .and_then(|v| v.as_u64())
        .or_else(|| {
            usage
                .get("cache_read_input_tokens")
                .and_then(|v| v.as_u64())
        });
    let cache_create = usage
        .get("cache_creation_input_tokens")
        .and_then(|v| v.as_u64());
    ModelUsage {
        input_tokens: usage.get("prompt_tokens").and_then(|v| v.as_u64()),
        output_tokens: usage.get("completion_tokens").and_then(|v| v.as_u64()),
        cache_read_tokens: cache_read,
        cache_create_tokens: cache_create,
    }
}

fn tool_call_value_to_event(value: &serde_json::Value) -> Option<ModelEvent> {
    let id = value.get("id")?.as_str()?.to_string();
    let function = value.get("function")?;
    let name = function.get("name")?.as_str()?.to_string();
    let arguments_json = function
        .get("arguments")
        .and_then(|value| value.as_str())
        .unwrap_or("{}")
        .to_string();
    Some(ModelEvent::ToolCallCreated {
        id,
        name,
        arguments_json,
    })
}

fn accumulate_tool_call(
    value: &serde_json::Value,
    tool_calls: &mut BTreeMap<u64, ToolCallAccumulator>,
) {
    let index = value
        .get("index")
        .and_then(|value| value.as_u64())
        .unwrap_or(0);
    let accumulator = tool_calls.entry(index).or_default();
    if let Some(id) = value.get("id").and_then(|value| value.as_str()) {
        accumulator.id = Some(id.to_string());
    }
    if let Some(function) = value.get("function") {
        if let Some(name) = function.get("name").and_then(|value| value.as_str()) {
            accumulator.name = Some(name.to_string());
        }
        if let Some(arguments) = function.get("arguments").and_then(|value| value.as_str()) {
            accumulator.arguments_json.push_str(arguments);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_chunk_is_skipped_not_fatal() {
        use crate::provider::client::ModelEvent;
        let lines = vec![
            "data: {bad json".to_string(), // 坏块：跳过
            ": keep-alive".to_string(),    // 注释心跳：跳过
            r#"data: {"choices":[{"delta":{"content":"hi"}}]}"#.to_string(),
            "data: [DONE]".to_string(),
        ];
        let result = normalize_chat_completion_stream_lines(lines).expect("tolerant");
        assert!(result.events.iter().any(
            |e| matches!(e, ModelEvent::AssistantMessageCompleted { content } if content == "hi")
        ));
    }

    #[test]
    fn in_stream_error_frame_surfaces_as_error_event() {
        use crate::provider::client::ModelEvent;
        let lines =
            vec![r#"data: {"error":{"message":"overloaded","type":"server_error"}}"#.to_string()];
        let result = normalize_chat_completion_stream_lines(lines).expect("ok");
        assert!(result
            .events
            .iter()
            .any(|e| matches!(e, ModelEvent::Error { message } if message.contains("overloaded"))));
    }
}

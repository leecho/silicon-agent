//! Anthropic 原生 Messages API 协议适配：端点/鉴权头、请求体构造（system 抽分、
//! content blocks、tool_use input 为对象、max_tokens 必填）、非流响应归一化、SSE 解析。
//!
//! 纯协议转换层，与持久化无关。`AnthropicAdapter`（provider/protocol.rs）按需调用这些自由函数。
//! 仅在 provider 模块内使用，故 `pub(super)`。

use std::collections::BTreeMap;
use std::io::BufRead;

use serde_json::{json, Value};

use super::client::{ModelCallRequest, ModelCallResult, ModelEvent, ModelUsage, ProviderCallError};
use super::message::{
    ModelMessage, ModelMessageRole, ModelResponseSchema, ModelToolChoice, ToolSpecForModel,
};

const ANTHROPIC_VERSION: &str = "2023-06-01";
/// Anthropic 要求 max_tokens 必填；调用方未给时用此保守默认。
const DEFAULT_MAX_TOKENS: u32 = 4096;

/// 流式 tool_use 累积：index → (id, name, 已累积 partial_json)。
type ToolAccMap = BTreeMap<u64, (String, String, String)>;

pub(super) fn messages_endpoint(base_url: &str) -> String {
    let base = base_url.trim().trim_end_matches('/');
    if base.ends_with("/v1/messages") {
        base.into()
    } else {
        format!("{base}/v1/messages")
    }
}

/// Anthropic 鉴权头：x-api-key + anthropic-version（不发 Authorization）。
pub(super) fn auth_headers(api_key: &str) -> Vec<(String, String)> {
    vec![
        ("x-api-key".into(), api_key.trim().to_string()),
        ("anthropic-version".into(), ANTHROPIC_VERSION.into()),
    ]
}

fn tool_spec_to_anthropic(tool: &ToolSpecForModel) -> Value {
    json!({
        "name": tool.name,
        "description": tool.description,
        "input_schema": tool.input_schema,
    })
}

/// tool_choice 映射；None → 不发 tools/tool_choice（本回合不用工具）。
fn tool_choice_to_anthropic(choice: &ModelToolChoice) -> Option<Value> {
    match choice {
        ModelToolChoice::Auto => Some(json!({"type": "auto"})),
        ModelToolChoice::Required => Some(json!({"type": "any"})),
        ModelToolChoice::Named(name) => Some(json!({"type": "tool", "name": name})),
        ModelToolChoice::None => None,
    }
}

/// 一条非 system 消息 → (role, content blocks)。
fn message_blocks(message: &ModelMessage) -> (&'static str, Vec<Value>) {
    match message.role {
        // Tool-role 消息上的图片不被表达：ModelMessage.content 为 String，tool_result 仅承载文本。
        ModelMessageRole::Tool => (
            "user",
            vec![json!({
                "type": "tool_result",
                "tool_use_id": message.tool_call_id.clone().unwrap_or_default(),
                "content": message.content,
            })],
        ),
        ModelMessageRole::Assistant => {
            let mut blocks = Vec::new();
            if !message.content.is_empty() {
                blocks.push(json!({"type": "text", "text": message.content}));
            }
            if let Some(calls) = &message.tool_calls {
                for call in calls {
                    // input 必须是 JSON 对象（非字符串）；解析失败回退 {}。
                    let input: Value =
                        serde_json::from_str(&call.arguments_json).unwrap_or_else(|_| json!({}));
                    blocks.push(json!({
                        "type": "tool_use",
                        "id": call.id,
                        "name": call.name,
                        "input": input,
                    }));
                }
            }
            ("assistant", blocks)
        }
        ModelMessageRole::User => {
            let mut blocks = vec![json!({"type": "text", "text": message.content})];
            for img in &message.images {
                blocks.push(json!({
                    "type": "image",
                    "source": {
                        "type": "base64",
                        "media_type": img.media_type,
                        "data": img.base64_data,
                    }
                }));
            }
            ("user", blocks)
        }
        // system 由 build_messages_body 抽分处理，不会走到这里。
        ModelMessageRole::System => ("user", Vec::new()),
    }
}

/// 相邻同 role 合并到一条消息的 content 数组（Anthropic 要求 user/assistant 交替）。
fn push_or_merge(messages: &mut Vec<Value>, role: &str, blocks: Vec<Value>) {
    if blocks.is_empty() {
        return;
    }
    if let Some(last) = messages.last_mut() {
        if last.get("role").and_then(|r| r.as_str()) == Some(role) {
            if let Some(arr) = last.get_mut("content").and_then(|c| c.as_array_mut()) {
                arr.extend(blocks);
                return;
            }
        }
    }
    messages.push(json!({"role": role, "content": blocks}));
}

pub(super) fn build_messages_body(
    model: &str,
    request: &ModelCallRequest,
    stream: bool,
) -> Value {
    let mut system_parts: Vec<String> = Vec::new();
    let mut messages: Vec<Value> = Vec::new();

    for message in &request.messages {
        if matches!(message.role, ModelMessageRole::System) {
            if !message.content.is_empty() {
                system_parts.push(message.content.clone());
            }
            continue;
        }
        let (role, blocks) = message_blocks(message);
        push_or_merge(&mut messages, role, blocks);
    }

    // 结构化输出：Anthropic 无 json_object 开关，向 system 注入「只输出 JSON」。
    if !matches!(request.response_schema, ModelResponseSchema::FreeformAssistant) {
        system_parts.push(
            "仅输出单个合法的 JSON 对象，不要包含任何额外文本、解释或代码块标记。".into(),
        );
    }

    let mut body = json!({
        "model": model,
        "max_tokens": request.max_output_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
        "messages": messages,
        "stream": stream,
    });
    if !system_parts.is_empty() {
        body["system"] = Value::String(system_parts.join("\n\n"));
    }
    if !request.tools.is_empty() && !matches!(request.tool_choice, ModelToolChoice::None) {
        body["tools"] = Value::Array(
            request.tools.iter().map(tool_spec_to_anthropic).collect(),
        );
        if let Some(choice) = tool_choice_to_anthropic(&request.tool_choice) {
            body["tool_choice"] = choice;
        }
    }
    body
}

fn parse_usage(usage: &Value) -> ModelUsage {
    ModelUsage {
        input_tokens: usage.get("input_tokens").and_then(|v| v.as_u64()),
        output_tokens: usage.get("output_tokens").and_then(|v| v.as_u64()),
        cache_read_tokens: usage.get("cache_read_input_tokens").and_then(|v| v.as_u64()),
        cache_create_tokens: usage
            .get("cache_creation_input_tokens")
            .and_then(|v| v.as_u64()),
    }
}

pub(super) fn normalize_messages_response(
    value: Value,
) -> Result<ModelCallResult, ProviderCallError> {
    let content = value
        .get("content")
        .and_then(|c| c.as_array())
        .ok_or_else(|| ProviderCallError::new("anthropic response missing content"))?;
    let mut events = Vec::new();
    let mut text = String::new();
    for block in content {
        match block.get("type").and_then(|t| t.as_str()) {
            Some("text") => {
                if let Some(t) = block.get("text").and_then(|v| v.as_str()) {
                    text.push_str(t);
                }
            }
            Some("tool_use") => {
                let id = block
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                let name = block
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                let arguments_json = block
                    .get("input")
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "{}".into());
                events.push(ModelEvent::ToolCallCreated {
                    id,
                    name,
                    arguments_json,
                });
            }
            _ => {}
        }
    }
    // tool_use 事件先于累积的 AssistantMessageCompleted 推入，与 OpenAI 路径一致；消费方按事件类型索引，不依赖顺序。
    if !text.is_empty() {
        events.push(ModelEvent::AssistantMessageCompleted { content: text });
    }
    let finish_reason = value
        .get("stop_reason")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let usage = value.get("usage").map(parse_usage);
    Ok(ModelCallResult {
        events,
        usage,
        finish_reason,
    })
}

fn merge_usage(usage: &mut ModelUsage, u: &Value) {
    if let Some(v) = u.get("input_tokens").and_then(|x| x.as_u64()) {
        usage.input_tokens = Some(v);
    }
    if let Some(v) = u.get("output_tokens").and_then(|x| x.as_u64()) {
        usage.output_tokens = Some(v);
    }
    if let Some(v) = u.get("cache_read_input_tokens").and_then(|x| x.as_u64()) {
        usage.cache_read_tokens = Some(v);
    }
    if let Some(v) = u.get("cache_creation_input_tokens").and_then(|x| x.as_u64()) {
        usage.cache_create_tokens = Some(v);
    }
}

/// 单行 SSE 处理；按 data JSON 的 `type` 分发。返回 false 表示 on_event 要求取消。
fn parse_line(
    line: &str,
    text: &mut String,
    finish_reason: &mut Option<String>,
    usage: &mut ModelUsage,
    tools: &mut ToolAccMap,
    on_event: &mut dyn FnMut(ModelEvent) -> bool,
) -> Result<bool, ProviderCallError> {
    let Some(payload) = line.trim().strip_prefix("data:") else {
        return Ok(true);
    };
    let payload = payload.trim();
    if payload.is_empty() {
        return Ok(true);
    }
    let value: Value = match serde_json::from_str(payload) {
        Ok(v) => v,
        Err(_) => return Ok(true), // 噪声块跳过，不杀流
    };
    match value.get("type").and_then(|t| t.as_str()) {
        Some("message_start") => {
            if let Some(u) = value.pointer("/message/usage") {
                merge_usage(usage, u);
            }
            Ok(true)
        }
        Some("content_block_start") => {
            let idx = value.get("index").and_then(|i| i.as_u64()).unwrap_or(0);
            if let Some(cb) = value.get("content_block") {
                if cb.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                    let entry = tools.entry(idx).or_default();
                    entry.0 = cb
                        .get("id")
                        .and_then(|x| x.as_str())
                        .unwrap_or_default()
                        .to_string();
                    entry.1 = cb
                        .get("name")
                        .and_then(|x| x.as_str())
                        .unwrap_or_default()
                        .to_string();
                }
            }
            Ok(true)
        }
        Some("content_block_delta") => {
            let idx = value.get("index").and_then(|i| i.as_u64()).unwrap_or(0);
            let delta = value.get("delta");
            match delta.and_then(|d| d.get("type")).and_then(|t| t.as_str()) {
                Some("text_delta") => {
                    if let Some(t) = delta.and_then(|d| d.get("text")).and_then(|x| x.as_str()) {
                        text.push_str(t);
                        return Ok(on_event(ModelEvent::Delta { text: t.to_string() }));
                    }
                    Ok(true)
                }
                Some("thinking_delta") => {
                    if let Some(t) =
                        delta.and_then(|d| d.get("thinking")).and_then(|x| x.as_str())
                    {
                        return Ok(on_event(ModelEvent::ThinkingDelta {
                            text: t.to_string(),
                        }));
                    }
                    Ok(true)
                }
                Some("input_json_delta") => {
                    if let Some(frag) = delta
                        .and_then(|d| d.get("partial_json"))
                        .and_then(|x| x.as_str())
                    {
                        let entry = tools.entry(idx).or_default();
                        entry.2.push_str(frag);
                        if !entry.1.is_empty() {
                            let id = if entry.0.is_empty() {
                                entry.1.clone()
                            } else {
                                entry.0.clone()
                            };
                            return Ok(on_event(ModelEvent::ToolCallCreated {
                                id,
                                name: entry.1.clone(),
                                arguments_json: entry.2.clone(),
                            }));
                        }
                    }
                    Ok(true)
                }
                _ => Ok(true),
            }
        }
        Some("message_delta") => {
            if let Some(r) = value.pointer("/delta/stop_reason").and_then(|x| x.as_str()) {
                *finish_reason = Some(r.to_string());
            }
            if let Some(u) = value.get("usage") {
                merge_usage(usage, u);
            }
            Ok(true)
        }
        Some("error") => {
            let message = value
                .pointer("/error/message")
                .and_then(|x| x.as_str())
                .unwrap_or("anthropic stream error");
            Ok(on_event(ModelEvent::Error {
                message: message.to_string(),
            }))
        }
        // message_stop / content_block_stop / ping 等：忽略。
        _ => Ok(true),
    }
}

/// 读取并解析 Anthropic SSE 流，归一化为 ModelCallResult。
pub(super) fn stream_messages(
    reader: &mut dyn BufRead,
    cancel: &std::sync::atomic::AtomicBool,
    on_event: &mut dyn FnMut(ModelEvent) -> bool,
) -> Result<ModelCallResult, ProviderCallError> {
    let mut text = String::new();
    let mut finish_reason: Option<String> = None;
    let mut usage = ModelUsage {
        input_tokens: None,
        output_tokens: None,
        cache_read_tokens: None,
        cache_create_tokens: None,
    };
    let mut tools: ToolAccMap = BTreeMap::new();

    super::stream_read::read_sse_lines(
        reader,
        cancel,
        std::time::Duration::from_millis(super::adapter::STREAM_IDLE_BUDGET_MS),
        |line| {
            parse_line(
                line,
                &mut text,
                &mut finish_reason,
                &mut usage,
                &mut tools,
                on_event,
            )
        },
    )?;

    let mut events = Vec::new();
    for (_, (id, name, args)) in tools {
        if name.is_empty() {
            continue;
        }
        let arguments_json = if args.is_empty() { "{}".into() } else { args };
        let id = if id.is_empty() { name.clone() } else { id };
        events.push(ModelEvent::ToolCallCreated {
            id,
            name,
            arguments_json,
        });
    }
    if !text.is_empty() {
        events.push(ModelEvent::AssistantMessageCompleted { content: text });
    }
    let usage = if usage.input_tokens.is_some()
        || usage.output_tokens.is_some()
        || usage.cache_read_tokens.is_some()
        || usage.cache_create_tokens.is_some()
    {
        Some(usage)
    } else {
        None
    };
    Ok(ModelCallResult {
        events,
        usage,
        finish_reason,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::message::{ModelImage, ModelMessage};
    use std::io::Cursor;

    fn req(messages: Vec<ModelMessage>) -> ModelCallRequest {
        ModelCallRequest {
            messages,
            ..Default::default()
        }
    }

    #[test]
    fn endpoint_appends_v1_messages() {
        assert_eq!(messages_endpoint("https://api.anthropic.com"), "https://api.anthropic.com/v1/messages");
        assert_eq!(messages_endpoint("https://api.anthropic.com/"), "https://api.anthropic.com/v1/messages");
        assert_eq!(messages_endpoint("https://x/v1/messages"), "https://x/v1/messages");
    }

    #[test]
    fn system_is_lifted_and_max_tokens_required() {
        let body = build_messages_body(
            "claude-opus-4-8",
            &req(vec![ModelMessage::system("be brief"), ModelMessage::user("hi")]),
            false,
        );
        assert_eq!(body["system"], "be brief");
        assert_eq!(body["max_tokens"], 4096);
        let msgs = body["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["role"], "user");
        assert_eq!(msgs[0]["content"][0]["text"], "hi");
    }

    #[test]
    fn assistant_tool_call_input_is_object_and_tool_result_is_user() {
        let messages = vec![
            ModelMessage::assistant_tool_call("toolu_1", "write_file", r#"{"path":"a.txt"}"#),
            ModelMessage::tool("toolu_1", "ok"),
        ];
        let body = build_messages_body("m", &req(messages), false);
        let msgs = body["messages"].as_array().unwrap();
        assert_eq!(msgs[0]["role"], "assistant");
        assert_eq!(msgs[0]["content"][0]["type"], "tool_use");
        // input 是对象，不是字符串
        assert_eq!(msgs[0]["content"][0]["input"]["path"], "a.txt");
        assert_eq!(msgs[1]["role"], "user");
        assert_eq!(msgs[1]["content"][0]["type"], "tool_result");
        assert_eq!(msgs[1]["content"][0]["tool_use_id"], "toolu_1");
    }

    #[test]
    fn user_image_becomes_base64_block() {
        let mut m = ModelMessage::user("look");
        m.images.push(ModelImage { media_type: "image/png".into(), base64_data: "AAAA".into() });
        let body = build_messages_body("m", &req(vec![m]), false);
        let content = body["messages"][0]["content"].as_array().unwrap();
        assert_eq!(content[1]["type"], "image");
        assert_eq!(content[1]["source"]["media_type"], "image/png");
        assert_eq!(content[1]["source"]["data"], "AAAA");
    }

    #[test]
    fn json_schema_injects_system_instruction() {
        let mut r = req(vec![ModelMessage::user("x")]);
        r.response_schema = ModelResponseSchema::AgentFinalOutput;
        let body = build_messages_body("m", &r, false);
        assert!(body["system"].as_str().unwrap().contains("JSON"));
    }

    #[test]
    fn normalize_extracts_text_tooluse_usage_stopreason() {
        let value = serde_json::json!({
            "content": [
                {"type": "text", "text": "hello"},
                {"type": "tool_use", "id": "toolu_9", "name": "search", "input": {"q": "rust"}}
            ],
            "stop_reason": "tool_use",
            "usage": {"input_tokens": 10, "output_tokens": 3, "cache_read_input_tokens": 7}
        });
        let result = normalize_messages_response(value).unwrap();
        assert!(result.events.iter().any(|e|
            matches!(e, ModelEvent::AssistantMessageCompleted { content } if content == "hello")));
        assert!(result.events.iter().any(|e|
            matches!(e, ModelEvent::ToolCallCreated { name, arguments_json, .. }
                if name == "search" && arguments_json.contains("rust"))));
        assert_eq!(result.finish_reason.as_deref(), Some("tool_use"));
        let usage = result.usage.unwrap();
        assert_eq!(usage.input_tokens, Some(10));
        assert_eq!(usage.output_tokens, Some(3));
        assert_eq!(usage.cache_read_tokens, Some(7));
    }

    #[test]
    fn stream_text_thinking_tooluse_and_usage() {
        let sse = concat!(
            "data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":5}}}\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"hmm\"}}\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"hi\"}}\n",
            "data: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"tool_use\",\"id\":\"toolu_2\",\"name\":\"run\"}}\n",
            "data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"a\\\":1}\"}}\n",
            "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":4}}\n",
            "data: {\"type\":\"message_stop\"}\n",
        );
        let mut reader = Cursor::new(sse.as_bytes().to_vec());
        let mut seen = Vec::new();
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let result = stream_messages(&mut reader, &cancel, &mut |e| {
            seen.push(e);
            true
        })
        .unwrap();
        assert!(seen.iter().any(|e| matches!(e, ModelEvent::ThinkingDelta { text } if text == "hmm")));
        assert!(seen.iter().any(|e| matches!(e, ModelEvent::Delta { text } if text == "hi")));
        assert!(result.events.iter().any(|e|
            matches!(e, ModelEvent::AssistantMessageCompleted { content } if content == "hi")));
        assert!(result.events.iter().any(|e|
            matches!(e, ModelEvent::ToolCallCreated { name, arguments_json, .. }
                if name == "run" && arguments_json.contains("\"a\":1"))));
        assert_eq!(result.finish_reason.as_deref(), Some("end_turn"));
        let usage = result.usage.unwrap();
        assert_eq!(usage.input_tokens, Some(5));
        assert_eq!(usage.output_tokens, Some(4));
    }

    #[test]
    fn stream_error_frame_surfaces_error_event() {
        let sse = "data: {\"type\":\"error\",\"error\":{\"message\":\"overloaded\"}}\n";
        let mut reader = Cursor::new(sse.as_bytes().to_vec());
        let mut seen = Vec::new();
        let cancel = std::sync::atomic::AtomicBool::new(false);
        stream_messages(&mut reader, &cancel, &mut |e| { seen.push(e); true }).unwrap();
        assert!(seen.iter().any(|e| matches!(e, ModelEvent::Error { message } if message.contains("overloaded"))));
    }

    #[test]
    fn stream_zero_arg_tool_finalizes_empty_object_without_preview() {
        // 零参工具：只有 content_block_start，无 input_json_delta。
        let sse = concat!(
            "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"toolu_z\",\"name\":\"ping\"}}\n",
            "data: {\"type\":\"content_block_stop\",\"index\":0}\n",
            "data: {\"type\":\"message_stop\"}\n",
        );
        let mut reader = Cursor::new(sse.as_bytes().to_vec());
        let mut seen = Vec::new();
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let result = stream_messages(&mut reader, &cancel, &mut |e| {
            seen.push(e);
            true
        })
        .unwrap();
        // 流式期间没有为该工具发过 ToolCallCreated 预览（无 input_json_delta）。
        assert!(!seen
            .iter()
            .any(|e| matches!(e, ModelEvent::ToolCallCreated { .. })));
        // 收尾时归一化为参数为 "{}" 的 tool 调用。
        assert!(result.events.iter().any(|e| matches!(
            e,
            ModelEvent::ToolCallCreated { name, arguments_json, .. }
                if name == "ping" && arguments_json == "{}"
        )));
    }
}

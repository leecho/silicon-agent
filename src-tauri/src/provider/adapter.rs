//! OpenAI-compatible 协议适配：端点/鉴权头拼装、HTTP 错误分类与友好文案、请求体构造、
//! SSE 流式增量解析。
//!
//! 纯协议转换层，与持久化无关。ModelClient 实现（provider/store.rs）按需调用这些自由函数。
//! 全部仅在 provider 模块内使用，故 `pub(super)`。

use std::time::Duration;

use super::call::{model_message_to_openai, tool_choice_to_openai, tool_spec_to_openai};
use super::client::{ModelCallRequest, ModelEvent, ProviderCallError, ProviderErrorClass};
use super::message::ModelResponseSchema;

pub(super) fn chat_completions_endpoint(base_url: &str) -> String {
    let base = base_url.trim().trim_end_matches('/');
    if base.ends_with("/chat/completions") {
        base.into()
    } else {
        format!("{base}/chat/completions")
    }
}

pub(super) fn authorization_header_value(api_key: &str) -> String {
    format!("Bearer {}", api_key.trim())
}

/// 流式读超时（毫秒）= chunk 间 idle 超时。ureq 的 `timeout_read` 是**单次 socket 读**超时，
/// 对 SSE 而言等价于"两个 chunk 之间最长无数据时间"，正是我们要的 idle 超时。
/// 优先使用 `request.timeout_ms`，缺省 60s。
pub(super) fn stream_read_timeout_ms(request_timeout_ms: Option<u64>) -> u64 {
    request_timeout_ms.unwrap_or(60_000)
}

/// 构造带超时的 ureq agent。连接超时固定 10s；读超时按调用方语义传入。
pub(super) fn timed_agent(read_timeout_ms: u64) -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_millis(read_timeout_ms))
        .build()
}

/// HTTP 状态码 → 错误分类。408/429/5xx 视为瞬时可重试，其余终态。
pub(super) fn classify_status(status: u16) -> ProviderErrorClass {
    match status {
        408 | 429 => ProviderErrorClass::Transient,
        500..=599 => ProviderErrorClass::Transient,
        _ => ProviderErrorClass::Terminal,
    }
}

pub(super) fn provider_call_error(prefix: &str, err: ureq::Error) -> ProviderCallError {
    match err {
        ureq::Error::Status(status, response) => {
            let body = response.into_string().unwrap_or_default();
            // 把 provider 的原始错误体转成面向用户的友好文案（如「余额不足」）。
            let msg = friendly_error_message(&body, Some(status));
            // HTTP 错误按状态码分类（429/408/5xx 瞬时，其余终态）。
            let error = match classify_status(status) {
                ProviderErrorClass::Transient => ProviderCallError::transient(msg),
                ProviderErrorClass::Terminal => ProviderCallError::new(msg),
            };
            error.with_status(status)
        }
        // 连接失败/重置/超时等传输层错误 = 瞬时，可退避重试。
        other => ProviderCallError::transient(friendly_error_message(
            &format!("{prefix}: {other}"),
            None,
        )),
    }
}

/// 解析 provider 错误体，取出 (message, code)。兼容 `{"error":{...}}` 与顶层 `{"message","code"}`。
fn parse_provider_error(body: &str) -> (String, String) {
    let v: serde_json::Value = match serde_json::from_str(body.trim()) {
        Ok(v) => v,
        Err(_) => return (String::new(), String::new()),
    };
    let obj = v.get("error").unwrap_or(&v);
    let msg = obj
        .get("message")
        .and_then(|m| m.as_str())
        .unwrap_or("")
        .to_string();
    let code = obj
        .get("code")
        .and_then(|c| c.as_str())
        .unwrap_or("")
        .to_string();
    (msg, code)
}

/// 把 provider 的原始错误（HTTP body + 状态码）转成面向用户的简短中文文案。
/// 命中已知模式返回友好句；否则回退到提取出的 message，再否则原始片段。
pub(super) fn friendly_error_message(body: &str, status: Option<u16>) -> String {
    let (msg, code) = parse_provider_error(body);
    let hay = format!("{msg} {code} {body}").to_lowercase();
    let has = |k: &str| hay.contains(k);

    if (has("insufficient") && has("balance")) || has("余额") || has("欠费") {
        return "余额不足，请充值后重试。".into();
    }
    if matches!(status, Some(401) | Some(403))
        || has("invalid api key")
        || has("incorrect api key")
        || has("invalid_api_key")
        || has("unauthorized")
        || has("authentication")
    {
        return "API Key 无效或未授权，请检查密钥配置。".into();
    }
    if matches!(status, Some(429))
        || has("rate limit")
        || has("rate_limit")
        || has("too many requests")
    {
        return "请求过于频繁（限流），请稍后重试。".into();
    }
    if has("context length")
        || has("maximum context")
        || has("context_length_exceeded")
        || has("reduce the length")
        || has("too many tokens")
    {
        return "上下文超出模型上限，请压缩历史或新开会话。".into();
    }
    if has("model") && (has("not found") || has("does not exist") || has("not exist")) {
        return "模型不存在或不可用，请检查模型配置。".into();
    }
    if has("timeout") || has("timed out") || has("超时") {
        return "请求超时，请稍后重试。".into();
    }

    // 兜底：优先用提取到的 message；否则原始片段（截断）；再否则按状态码。
    if !msg.trim().is_empty() {
        return msg;
    }
    let trimmed = body.trim();
    if !trimmed.is_empty() {
        return trimmed.chars().take(200).collect();
    }
    match status {
        Some(s) => format!("请求失败（HTTP {s}）"),
        None => "请求失败".into(),
    }
}

/// 结构化输出 schema（非 `FreeformAssistant`）启用 provider JSON 模式。
fn response_schema_requires_json_mode(schema: &ModelResponseSchema) -> bool {
    !matches!(schema, ModelResponseSchema::FreeformAssistant)
}

pub(super) fn build_chat_completion_body(
    model: &str,
    request: &ModelCallRequest,
    stream: bool,
) -> serde_json::Value {
    // model 已由调用方据 model_selection / 默认解析，直接使用。
    let messages = request
        .messages
        .iter()
        .map(model_message_to_openai)
        .collect::<Vec<_>>();
    let mut body = serde_json::json!({
        "model": model,
        "messages": messages,
        "temperature": 0.2,
        "stream": stream
    });
    // 流式调用请求末尾 usage chunk（OpenAI-compatible），供用量分析采集 token。
    if stream {
        body["stream_options"] = serde_json::json!({ "include_usage": true });
    }
    if let Some(max_tokens) = request.max_output_tokens {
        body["max_tokens"] = serde_json::Value::Number(max_tokens.into());
    }
    // 结构化输出 schema 透传为 provider JSON 模式；FreeformAssistant/缺省不带 response_format。
    if response_schema_requires_json_mode(&request.response_schema) {
        body["response_format"] = serde_json::json!({ "type": "json_object" });
    }
    // MVP 统一：function tools 一律按支持处理（不再区分 native web_search 模型）。
    if !request.tools.is_empty() {
        body["tools"] = serde_json::Value::Array(
            request
                .tools
                .iter()
                .map(tool_spec_to_openai)
                .collect::<Vec<_>>(),
        );
        body["tool_choice"] = tool_choice_to_openai(&request.tool_choice);
    }
    body
}

/// 流式 tool_calls 的逐块累积状态：`index -> (id, name, 已累积 arguments)`。
/// OpenAI 流式把一个 tool_call 拆成多帧（首帧带 name/id，后续帧仅 arguments 分片）。
/// 累积后每帧都 emit 一个带"当前累积参数"的 `ToolCallCreated`，驱动前端实时预览生成进度，
/// 避免大参数（如把整篇报告写进 write_file 参数）生成时界面长时间无反馈。
#[derive(Default)]
pub(super) struct ToolCallStreamAcc {
    by_index: std::collections::BTreeMap<u64, (String, String, String)>,
}

pub(super) fn emit_stream_line_delta(
    line: &str,
    tool_acc: &mut ToolCallStreamAcc,
    on_event: &mut dyn FnMut(ModelEvent) -> bool,
) -> Result<bool, ProviderCallError> {
    let Some(payload) = line.trim().strip_prefix("data:") else {
        return Ok(true);
    };
    let payload = payload.trim();
    if payload.is_empty() || payload == "[DONE]" {
        return Ok(true);
    }
    let value: serde_json::Value = match serde_json::from_str(payload) {
        Ok(value) => value,
        Err(_) => return Ok(true), // 噪声块跳过，不杀流
    };
    // 流内 error 帧：识别为 Error 事件，由上层按瞬时/终态分类。
    if let Some(error) = value.get("error") {
        let message = error
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("provider stream error");
        return Ok(on_event(ModelEvent::Error {
            message: message.to_string(),
        }));
    }
    let Some(delta) = value
        .get("choices")
        .and_then(|value| value.get(0))
        .and_then(|choice| choice.get("delta"))
    else {
        return Ok(true);
    };
    if let Some(text) = delta
        .get("reasoning_content")
        .or_else(|| delta.get("reasoning"))
        .and_then(|value| value.as_str())
    {
        return Ok(on_event(ModelEvent::ThinkingDelta {
            text: text.to_string(),
        }));
    }
    if let Some(text) = delta.get("content").and_then(|value| value.as_str()) {
        return Ok(on_event(ModelEvent::Delta {
            text: text.to_string(),
        }));
    }
    // 流式 tool_calls：按 index 累积 id/name/arguments 分片，每帧 emit 一个带"当前累积参数"的
    // ToolCallCreated，让前端实时预览工具块与参数生成进度。arguments 完整体仍由 final result 归一化提供。
    if let Some(tool_calls) = delta.get("tool_calls").and_then(|value| value.as_array()) {
        for call in tool_calls {
            let index = call.get("index").and_then(|v| v.as_u64()).unwrap_or(0);
            let entry = tool_acc.by_index.entry(index).or_default();
            if let Some(id) = call
                .get("id")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
            {
                entry.0 = id.to_string();
            }
            let function = call.get("function");
            if let Some(name) = function
                .and_then(|f| f.get("name"))
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
            {
                entry.1 = name.to_string();
            }
            if let Some(fragment) = function
                .and_then(|f| f.get("arguments"))
                .and_then(|v| v.as_str())
            {
                entry.2.push_str(fragment);
            }
            // 无工具名无法显示工具块，跳过（等带 name 的帧）。
            if entry.1.is_empty() {
                continue;
            }
            let id = if entry.0.is_empty() {
                entry.1.clone()
            } else {
                entry.0.clone()
            };
            if !on_event(ModelEvent::ToolCallCreated {
                id,
                name: entry.1.clone(),
                arguments_json: entry.2.clone(),
            }) {
                return Ok(false);
            }
        }
        return Ok(true);
    }
    Ok(true)
}

//! Provider 协议维度：`Protocol` 枚举 + `ProtocolAdapter` trait（请求怎么发、响应怎么解）。
//! gateway 据 `CallTarget.protocol` 选 adapter，本身协议无关。
//!
//! `OpenAiAdapter` 包装现有 OpenAI 自由函数（adapter.rs/call.rs）；
//! `AnthropicAdapter` 包装 anthropic.rs 自由函数。

use std::io::BufRead;

use super::client::{ModelCallRequest, ModelCallResult, ModelEvent, ProviderCallError};

/// provider 协议。未知字符串保守回退 OpenAi。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    OpenAi,
    Anthropic,
}

impl Protocol {
    pub fn from_str(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "anthropic" => Protocol::Anthropic,
            "openai" | "" => Protocol::OpenAi,
            other => {
                eprintln!("unknown protocol {other:?}, falling back to openai");
                Protocol::OpenAi
            }
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Protocol::OpenAi => "openai",
            Protocol::Anthropic => "anthropic",
        }
    }
}

/// call_target 解析结果：调用一次模型所需的端点凭证 + 协议。
pub struct CallTarget {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub protocol: Protocol,
}

/// 协议适配器：端点、鉴权头、请求体、响应归一化、SSE 解析（自带循环）。
pub trait ProtocolAdapter {
    fn endpoint(&self, base_url: &str) -> String;
    fn auth_headers(&self, api_key: &str) -> Vec<(String, String)>;
    fn build_body(&self, model: &str, request: &ModelCallRequest, stream: bool)
        -> serde_json::Value;
    fn normalize_response(
        &self,
        value: serde_json::Value,
    ) -> Result<ModelCallResult, ProviderCallError>;
    fn parse_stream(
        &self,
        reader: &mut dyn BufRead,
        cancel: &std::sync::atomic::AtomicBool,
        on_event: &mut dyn FnMut(ModelEvent) -> bool,
    ) -> Result<ModelCallResult, ProviderCallError>;
}

pub fn adapter_for(protocol: Protocol) -> Box<dyn ProtocolAdapter> {
    match protocol {
        Protocol::OpenAi => Box::new(OpenAiAdapter),
        Protocol::Anthropic => Box::new(AnthropicAdapter),
    }
}

/// OpenAI-compatible 适配器：委托现有自由函数，行为零变化。
pub(super) struct OpenAiAdapter;

impl ProtocolAdapter for OpenAiAdapter {
    fn endpoint(&self, base_url: &str) -> String {
        super::adapter::chat_completions_endpoint(base_url)
    }

    fn auth_headers(&self, api_key: &str) -> Vec<(String, String)> {
        vec![(
            "Authorization".into(),
            super::adapter::authorization_header_value(api_key),
        )]
    }

    fn build_body(
        &self,
        model: &str,
        request: &ModelCallRequest,
        stream: bool,
    ) -> serde_json::Value {
        super::adapter::build_chat_completion_body(model, request, stream)
    }

    fn normalize_response(
        &self,
        value: serde_json::Value,
    ) -> Result<ModelCallResult, ProviderCallError> {
        super::call::normalize_chat_completion_response(value)
    }

    fn parse_stream(
        &self,
        reader: &mut dyn BufRead,
        cancel: &std::sync::atomic::AtomicBool,
        on_event: &mut dyn FnMut(ModelEvent) -> bool,
    ) -> Result<ModelCallResult, ProviderCallError> {
        let mut lines = Vec::new();
        let mut tool_acc = super::adapter::ToolCallStreamAcc::default();
        super::stream_read::read_sse_lines(
            reader,
            cancel,
            std::time::Duration::from_millis(super::adapter::STREAM_IDLE_BUDGET_MS),
            |line| {
                let cont = super::adapter::emit_stream_line_delta(line, &mut tool_acc, on_event)?;
                if cont {
                    lines.push(line.to_string());
                }
                Ok(cont)
            },
        )?;
        super::call::normalize_chat_completion_stream_lines(lines)
    }
}

/// Anthropic 原生 Messages API 适配器：委托 anthropic.rs 自由函数。
pub(super) struct AnthropicAdapter;

impl ProtocolAdapter for AnthropicAdapter {
    fn endpoint(&self, base_url: &str) -> String {
        super::anthropic::messages_endpoint(base_url)
    }

    fn auth_headers(&self, api_key: &str) -> Vec<(String, String)> {
        super::anthropic::auth_headers(api_key)
    }

    fn build_body(
        &self,
        model: &str,
        request: &ModelCallRequest,
        stream: bool,
    ) -> serde_json::Value {
        super::anthropic::build_messages_body(model, request, stream)
    }

    fn normalize_response(
        &self,
        value: serde_json::Value,
    ) -> Result<ModelCallResult, ProviderCallError> {
        super::anthropic::normalize_messages_response(value)
    }

    fn parse_stream(
        &self,
        reader: &mut dyn BufRead,
        cancel: &std::sync::atomic::AtomicBool,
        on_event: &mut dyn FnMut(ModelEvent) -> bool,
    ) -> Result<ModelCallResult, ProviderCallError> {
        super::anthropic::stream_messages(reader, cancel, on_event)
    }
}

#[cfg(test)]
mod tests {
    use super::{adapter_for, Protocol};

    #[test]
    fn adapter_for_delegates_to_correct_endpoint() {
        assert!(adapter_for(Protocol::OpenAi)
            .endpoint("https://api.openai.com/v1")
            .ends_with("/chat/completions"));
        assert!(adapter_for(Protocol::Anthropic)
            .endpoint("https://api.anthropic.com")
            .ends_with("/v1/messages"));
    }

    #[test]
    fn protocol_from_str_roundtrip_and_fallback() {
        assert_eq!(Protocol::from_str("anthropic"), Protocol::Anthropic);
        assert_eq!(Protocol::from_str("Anthropic"), Protocol::Anthropic);
        assert_eq!(Protocol::from_str("openai"), Protocol::OpenAi);
        assert_eq!(Protocol::from_str(""), Protocol::OpenAi);
        assert_eq!(Protocol::from_str("weird"), Protocol::OpenAi);
        assert_eq!(Protocol::Anthropic.as_str(), "anthropic");
        assert_eq!(Protocol::OpenAi.as_str(), "openai");
    }
}

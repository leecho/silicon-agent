//! Provider client 边界。
//!
//! Agent Runtime 只依赖这个 trait；测试注入 deterministic client，桌面运行时由 ProviderGateway
//! （store 之上的调用网关）组装 OpenAI-compatible 请求。

use crate::provider::message::{
    ModelAttribution, ModelMessage, ModelResponseSchema, ModelToolChoice, ToolSpecForModel,
};

/// 模型选择：定位「哪个厂商的哪个模型」。携带在 ModelCallRequest 上，
/// 由 ProviderStore 据 provider_id 解析对应 base_url + api_key，用 model 作为请求模型名。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelSelection {
    pub provider_id: String,
    pub model: String,
}

/// 旧模型调用请求。
///
/// 仅保留给迁移期测试；主路径必须使用 `ModelCallRequest`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelRequest {
    pub session_id: String,
    pub user_input: String,
    pub context_summary: Option<String>,
}

/// 旧模型调用响应。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelResponse {
    pub text: String,
}

/// 模型调用请求。
#[derive(Debug, Clone, PartialEq)]
pub struct ModelCallRequest {
    pub messages: Vec<ModelMessage>,
    pub tools: Vec<ToolSpecForModel>,
    pub tool_choice: ModelToolChoice,
    pub response_schema: ModelResponseSchema,
    pub attribution: ModelAttribution,
    pub max_output_tokens: Option<u32>,
    pub timeout_ms: Option<u64>,
    pub stream: bool,
    /// 模型选择。非 None 时本次调用用该选择对应厂商凭证 + 模型；None 时由 Gateway 取全局默认模型。
    pub model_selection: Option<ModelSelection>,
}

impl Default for ModelCallRequest {
    fn default() -> Self {
        Self {
            messages: Vec::new(),
            tools: Vec::new(),
            tool_choice: ModelToolChoice::None,
            response_schema: ModelResponseSchema::FreeformAssistant,
            attribution: ModelAttribution {
                session_id: String::new(),
                ..Default::default()
            },
            max_output_tokens: None,
            timeout_ms: None,
            stream: false,
            model_selection: None,
        }
    }
}

/// Provider 归一化后的模型事件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelEvent {
    ThinkingDelta {
        text: String,
    },
    Delta {
        text: String,
    },
    AssistantMessageCompleted {
        content: String,
    },
    ToolCallCreated {
        id: String,
        name: String,
        arguments_json: String,
    },
    Error {
        message: String,
    },
}

/// Provider usage 元数据。`input_tokens` 为原始 prompt_tokens（含缓存命中部分）；
/// 缓存命中/写入单列，落库时再拆出非缓存输入，见 UsageStore::record。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cache_read_tokens: Option<u64>,
    pub cache_create_tokens: Option<u64>,
}

/// 模型调用结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelCallResult {
    pub events: Vec<ModelEvent>,
    pub usage: Option<ModelUsage>,
    pub finish_reason: Option<String>,
}

/// Provider 错误分类：瞬时可重试 vs 终态不重试。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderErrorClass {
    /// 瞬时错误（连接/超时/限流/5xx）：驱动器可有界退避重试。
    Transient,
    /// 终态错误（认证/请求非法等）：重试无意义，直接收口。
    Terminal,
}

/// Provider 调用错误。`class` 决定驱动器是否退避重试。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderCallError {
    pub message: String,
    /// 错误分类，区分瞬时可重试与终态不可重试。
    pub class: ProviderErrorClass,
    /// 429 `Retry-After`（毫秒），存在时优先于退避公式。
    pub retry_after_ms: Option<u64>,
    /// 触发该错误的 HTTP 状态码（若来自 HTTP 响应）。
    pub http_status: Option<u16>,
}

impl ProviderCallError {
    /// 默认按**终态**构造（保守：未知错误不自动重试）。
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            class: ProviderErrorClass::Terminal,
            retry_after_ms: None,
            http_status: None,
        }
    }

    /// 瞬时错误（连接/超时/限流/5xx），驱动器可重试。
    pub fn transient(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            class: ProviderErrorClass::Transient,
            retry_after_ms: None,
            http_status: None,
        }
    }

    /// 附加触发该错误的 HTTP 状态码。
    pub fn with_status(mut self, status: u16) -> Self {
        self.http_status = Some(status);
        self
    }

    /// 附加 `Retry-After`（毫秒），供退避决策优先采用。
    pub fn with_retry_after_ms(mut self, ms: u64) -> Self {
        self.retry_after_ms = Some(ms);
        self
    }
}

/// 可注入的模型 client。
pub trait ModelClient: Send + Sync {
    /// 返回配置的备用模型选择，供主模型瞬时重试耗尽后降级；默认无（不降级）。
    fn fallback_model(&self) -> Option<ModelSelection> {
        None
    }

    /// 返回当前生效的 (provider, model)，供用量采集归因；默认 None（测试 fake 可不实现）。
    fn active_model_provider(&self) -> Option<(String, String)> {
        None
    }

    fn complete_model(
        &self,
        request: ModelCallRequest,
    ) -> Result<ModelCallResult, ProviderCallError> {
        let _ = request;
        Err(ProviderCallError::new(
            "complete_model is not implemented for this client",
        ))
    }

    fn stream_model(
        &self,
        request: ModelCallRequest,
    ) -> Result<ModelCallResult, ProviderCallError> {
        let _ = request;
        Err(ProviderCallError::new(
            "stream_model is not implemented for this client",
        ))
    }

    fn stream_model_with_events(
        &self,
        request: ModelCallRequest,
        on_event: &mut dyn FnMut(ModelEvent) -> bool,
    ) -> Result<ModelCallResult, ProviderCallError> {
        let result = self.stream_model(request)?;
        for event in result.events.iter().cloned() {
            if !on_event(event) {
                return Err(ProviderCallError::new("model stream cancelled"));
            }
        }
        Ok(result)
    }

    fn complete(&self, request: ModelRequest) -> Result<ModelResponse, ProviderCallError> {
        let result = self.complete_model(ModelCallRequest {
            messages: vec![
                ModelMessage::system("You are a helpful assistant."),
                ModelMessage::user(request.user_input),
            ],
            tools: Vec::new(),
            tool_choice: ModelToolChoice::None,
            response_schema: ModelResponseSchema::FreeformAssistant,
            attribution: ModelAttribution {
                session_id: request.session_id,
                ..Default::default()
            },
            max_output_tokens: None,
            timeout_ms: None,
            stream: false,
            model_selection: None,
        })?;
        let text = result
            .events
            .into_iter()
            .find_map(|event| match event {
                ModelEvent::AssistantMessageCompleted { content } => Some(content),
                ModelEvent::ThinkingDelta { .. }
                | ModelEvent::Delta { .. }
                | ModelEvent::ToolCallCreated { .. }
                | ModelEvent::Error { .. } => None,
            })
            .ok_or_else(|| ProviderCallError::new("provider response missing assistant content"))?;
        Ok(ModelResponse { text })
    }
}

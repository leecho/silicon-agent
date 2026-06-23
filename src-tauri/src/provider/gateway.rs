//! ProviderGateway：模型调用网关。持有 `Arc<ProviderStore>`，`impl ModelClient` 实现
//! OpenAI-compatible 同步/流式补全。
//!
//! 与持久化（`store`）正交：从 store 解析 call_target、经 `adapter` 构造请求体/解析流、
//! 经 `call` 归一化响应。网关本身不持有 db，只是 store 之上的「调用行为」薄包装。
//!
//! 调用观察：通过依赖倒置的 `ModelCallObserver` 钩子，把每次调用（请求/响应/耗时）交给
//! 观察者（如 call_log 的 CallLogObserver）。provider 层只认这个 trait，不反向依赖 call_log。

use std::io::{BufRead, BufReader};
use std::sync::Arc;

use super::adapter::{
    authorization_header_value, build_chat_completion_body, chat_completions_endpoint,
    emit_stream_line_delta, provider_call_error, stream_read_timeout_ms, timed_agent,
    ToolCallStreamAcc,
};
use super::call::{normalize_chat_completion_response, normalize_chat_completion_stream_lines};
use super::client::{
    ModelCallRequest, ModelCallResult, ModelClient, ModelEvent, ModelSelection, ProviderCallError,
};
use super::message::ModelAttribution;
use super::model::ResolvedModel;
use super::store::ProviderStore;

/// 一次模型调用的观察数据（调用前后由 gateway 组装，交给观察者）。
pub struct ModelCallObservation {
    pub provider: String,
    pub model: String,
    pub attribution: ModelAttribution,
    pub request_json: String,
    pub outcome: Result<ModelCallResult, ProviderCallError>,
    pub latency_ms: u64,
}

/// gateway 调用观察者钩子（依赖倒置：call_log 实现，provider 不反向依赖）。
pub trait ModelCallObserver: Send + Sync {
    /// 廉价开关；false → gateway 跳过 payload 快照与计时。
    fn enabled(&self) -> bool;
    fn on_call(&self, obs: ModelCallObservation);
}

/// 模型调用网关：store 之上的调用行为。
pub struct ProviderGateway {
    store: Arc<ProviderStore>,
    observer: Option<Arc<dyn ModelCallObserver>>,
}

impl ProviderGateway {
    pub fn new(store: Arc<ProviderStore>) -> Self {
        Self {
            store,
            observer: None,
        }
    }

    /// 带调用观察者的网关（app 运行时用，注入 call_log 观察者）。
    pub fn with_observer(store: Arc<ProviderStore>, observer: Arc<dyn ModelCallObserver>) -> Self {
        Self {
            store,
            observer: Some(observer),
        }
    }

    /// 解析模型选择（委托 store）。供需要「选模型后再调用」的调用方（如 aux_gen）使用。
    pub fn resolve_selection(&self, model_id: Option<&str>) -> Result<ResolvedModel, String> {
        self.store.resolve_selection(model_id)
    }

    /// 组装 request_json（messages/tools/tool_choice/model_selection）。
    /// model_selection 单独平铺成 {providerId, model}，因其类型未派生 Serialize。
    fn request_snapshot_json(request: &ModelCallRequest) -> String {
        let sel = request
            .model_selection
            .as_ref()
            .map(|s| serde_json::json!({ "providerId": s.provider_id, "model": s.model }));
        serde_json::json!({
            "messages": request.messages,
            "tools": request.tools,
            "toolChoice": request.tool_choice,
            "modelSelection": sel,
        })
        .to_string()
    }

    /// 本次调用的厂商名（best-effort，仅供日志展示）：优先按 selection 解析，回退默认。
    fn provider_name_for(&self, request: &ModelCallRequest) -> String {
        request
            .model_selection
            .as_ref()
            .and_then(|s| self.store.resolve_selection(Some(&s.model)).ok())
            .map(|r| r.provider_name)
            .or_else(|| self.active_model_provider().map(|(p, _)| p))
            .unwrap_or_default()
    }

    /// complete_model 的实际 HTTP 部分（不含观察钩子）。
    fn complete_model_inner(
        &self,
        request: &ModelCallRequest,
    ) -> Result<ModelCallResult, ProviderCallError> {
        let (base_url, api_key, model) = self.store.call_target(request)?;
        let endpoint = chat_completions_endpoint(&base_url);
        let body = build_chat_completion_body(&model, request, false);
        let agent = timed_agent(request.timeout_ms.unwrap_or(120_000));
        let response: serde_json::Value = agent
            .post(&endpoint)
            .set("Authorization", &authorization_header_value(&api_key))
            .set("Content-Type", "application/json")
            .send_json(body)
            .map_err(|err| provider_call_error("provider call failed", err))?
            .into_json()
            .map_err(|err| {
                ProviderCallError::new(format!("provider response is not valid JSON: {err}"))
            })?;
        normalize_chat_completion_response(response)
    }

    /// stream_model_with_events 的实际 SSE 部分（不含观察钩子）。
    fn stream_model_inner(
        &self,
        request: &ModelCallRequest,
        on_event: &mut dyn FnMut(ModelEvent) -> bool,
    ) -> Result<ModelCallResult, ProviderCallError> {
        let (base_url, api_key, model) = self.store.call_target(request)?;
        let endpoint = chat_completions_endpoint(&base_url);
        let body = build_chat_completion_body(&model, request, true);
        let agent = timed_agent(stream_read_timeout_ms(request.timeout_ms));
        let response = agent
            .post(&endpoint)
            .set("Authorization", &authorization_header_value(&api_key))
            .set("Content-Type", "application/json")
            .send_json(body)
            .map_err(|err| provider_call_error("provider stream failed", err))?;
        let reader = BufReader::new(response.into_reader());
        let mut lines = Vec::new();
        let mut tool_acc = ToolCallStreamAcc::default();
        for line in reader.lines() {
            let line = line.map_err(|err| {
                ProviderCallError::transient(format!("provider stream read failed: {err}"))
            })?;
            if !emit_stream_line_delta(&line, &mut tool_acc, on_event)? {
                return Err(ProviderCallError::new("model stream cancelled"));
            }
            lines.push(line);
        }
        normalize_chat_completion_stream_lines(lines)
    }

    /// 若开启观察则组装 observation 并交给观察者（best-effort，不影响调用结果）。
    fn observe(
        &self,
        request: &ModelCallRequest,
        outcome: &Result<ModelCallResult, ProviderCallError>,
        latency_ms: u64,
    ) {
        let Some(observer) = self.observer.as_ref().filter(|o| o.enabled()) else {
            return;
        };
        observer.on_call(ModelCallObservation {
            provider: self.provider_name_for(request),
            model: request
                .model_selection
                .as_ref()
                .map(|s| s.model.clone())
                .unwrap_or_default(),
            attribution: request.attribution.clone(),
            request_json: Self::request_snapshot_json(request),
            outcome: outcome.clone(),
            latency_ms,
        });
    }
}

impl ModelClient for ProviderGateway {
    fn fallback_model(&self) -> Option<ModelSelection> {
        let id = self.store.get_setting("fallback_model_id").ok().flatten()?;
        self.store
            .resolve_selection(Some(&id))
            .ok()
            .map(|r| r.selection())
    }

    fn active_model_provider(&self) -> Option<(String, String)> {
        let r = self.store.resolve_selection(None).ok()?;
        Some((r.provider_name, r.model))
    }

    fn complete_model(
        &self,
        request: ModelCallRequest,
    ) -> Result<ModelCallResult, ProviderCallError> {
        let observe = self.observer.as_ref().is_some_and(|o| o.enabled());
        let started = std::time::Instant::now();
        let outcome = self.complete_model_inner(&request);
        if observe {
            self.observe(&request, &outcome, started.elapsed().as_millis() as u64);
        }
        outcome
    }

    fn stream_model(
        &self,
        request: ModelCallRequest,
    ) -> Result<ModelCallResult, ProviderCallError> {
        self.stream_model_with_events(request, &mut |_| true)
    }

    fn stream_model_with_events(
        &self,
        request: ModelCallRequest,
        on_event: &mut dyn FnMut(ModelEvent) -> bool,
    ) -> Result<ModelCallResult, ProviderCallError> {
        let observe = self.observer.as_ref().is_some_and(|o| o.enabled());
        let started = std::time::Instant::now();
        let outcome = self.stream_model_inner(&request, on_event);
        if observe {
            self.observe(&request, &outcome, started.elapsed().as_millis() as u64);
        }
        outcome
    }
}

#[cfg(test)]
mod observer_tests {
    use super::*;
    use crate::provider::message::ModelMessage;

    #[test]
    fn snapshot_json_includes_messages_and_tool_choice() {
        let mut req = ModelCallRequest::default();
        req.messages = vec![ModelMessage::user("hello world")];
        let json = ProviderGateway::request_snapshot_json(&req);
        assert!(json.contains("hello world"));
        assert!(json.contains("messages"));
        assert!(json.contains("toolChoice"));
    }
}

//! ProviderGateway：模型调用网关。持有 `Arc<ProviderStore>`，`impl ModelClient` 实现
//! 同步/流式补全。网关本身协议无关：据 `CallTarget.protocol` 选对应 `ProtocolAdapter`
//! （OpenAI / Anthropic）来构造请求体、设鉴权头、归一化响应与解析 SSE 流。
//!
//! 与持久化（`store`）正交：从 store 解析 call_target，再委托所选 adapter 完成请求构造、
//! 响应归一化与流解析。网关本身不持有 db，只是 store 之上的「调用行为」薄包装。
//!
//! 调用观察：通过依赖倒置的 `ModelCallObserver` 钩子，把每次调用（请求/响应/耗时）交给
//! 观察者（如 call_log 的 CallLogObserver）。provider 层只认这个 trait，不反向依赖 call_log。

use std::io::BufReader;
use std::sync::Arc;

use super::protocol::adapter_for;
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
        let target = self.store.call_target(request)?;
        let adapter = adapter_for(target.protocol);
        let endpoint = adapter.endpoint(&target.base_url);
        let body = adapter.build_body(&target.model, request, false);
        let http_req = crate::http::HttpRequest::post(endpoint)
            .headers(adapter.auth_headers(&target.api_key))
            .json_body(&body)
            .map_err(super::adapter::http_error_to_provider)?
            .timeout(std::time::Duration::from_millis(
                request.timeout_ms.unwrap_or(120_000),
            ));
        let resp = crate::http::HttpClient::new()
            .send(http_req)
            .map_err(super::adapter::http_error_to_provider)?;
        if !resp.is_success() {
            return Err(super::adapter::http_error_to_provider(
                crate::http::HttpError::Status {
                    code: resp.status,
                    body: resp.text(),
                },
            ));
        }
        let response: serde_json::Value = resp.json().map_err(|err| {
            ProviderCallError::new(format!("provider response is not valid JSON: {err:?}"))
        })?;
        adapter.normalize_response(response)
    }

    /// stream_model_with_events 的实际 SSE 部分（不含观察钩子）。
    fn stream_model_inner(
        &self,
        request: &ModelCallRequest,
        cancel: &std::sync::atomic::AtomicBool,
        on_event: &mut dyn FnMut(ModelEvent) -> bool,
    ) -> Result<ModelCallResult, ProviderCallError> {
        let target = self.store.call_target(request)?;
        let adapter = adapter_for(target.protocol);
        let endpoint = adapter.endpoint(&target.base_url);
        let body = adapter.build_body(&target.model, request, true);
        // 统一 HTTP：reqwest async 读响应头（宽超时保慢首字节，不再误判瞬时→重试风暴）+
        // 正文经 CancellableReader 供给；取消靠 abort（连接 drop）即时生效、与读超时解耦。
        // parse_stream / read_sse_lines 复用不改（CancellableReader 返回 WouldBlock 供其查 cancel）。
        let http_req = crate::http::HttpRequest::post(endpoint)
            .headers(adapter.auth_headers(&target.api_key))
            .json_body(&body)
            .map_err(super::adapter::http_error_to_provider)?;
        let reader = crate::http::HttpClient::new()
            .stream_body(http_req)
            .map_err(super::adapter::http_error_to_provider)?;
        let mut reader = BufReader::new(reader);
        adapter.parse_stream(&mut reader, cancel, on_event)
    }

    /// 调 embedding 模型把多段文本转向量（OpenAI-compatible /embeddings）。
    /// 复用与对话相同的目标解析（call_target → base_url/api_key/model）。
    pub fn embed_texts(&self, model_id: &str, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        // 先用 resolve_selection 拿到 ModelSelection，再构造最小 ModelCallRequest 调 call_target
        let resolved = self
            .store
            .resolve_selection(Some(model_id))
            .map_err(|e| format!("解析 embedding 模型失败：{e}"))?;
        let request = super::client::ModelCallRequest {
            model_selection: Some(resolved.selection()),
            ..Default::default()
        };
        let target = self
            .store
            .call_target(&request)
            .map_err(|e| format!("获取 embedding 目标失败：{}", e.message))?;

        // Anthropic 等无 OpenAI-compatible /embeddings 端点的协议直接拒绝，给可读提示。
        if matches!(target.protocol, super::protocol::Protocol::Anthropic) {
            return Err(
                "所选 embedding 模型的厂商不支持 /embeddings 接口，请改用 OpenAI 兼容的 embedding 模型"
                    .into(),
            );
        }

        let base = target.base_url.trim_end_matches('/');
        let endpoint = format!("{base}/embeddings");
        let body = serde_json::json!({ "model": target.model, "input": texts });
        let mut req = crate::http::HttpRequest::post(endpoint)
            .json_body(&body)
            .map_err(|e| format!("embedding 请求构造失败：{e:?}"))?
            .timeout(std::time::Duration::from_secs(60));
        // 复用全仓鉴权 helper（trim api_key 防止前后空白导致鉴权失败）。
        if !target.api_key.trim().is_empty() {
            req = req.header(
                "Authorization",
                super::adapter::authorization_header_value(&target.api_key),
            );
        }
        let http_resp = crate::http::HttpClient::new()
            .send(req)
            .map_err(|e| format!("embedding 调用失败：{e:?}"))?;
        if !http_resp.is_success() {
            return Err(format!("embedding 调用失败：HTTP {}", http_resp.status));
        }
        let resp: serde_json::Value = http_resp
            .json()
            .map_err(|e| format!("embedding 响应非法 JSON：{e:?}"))?;
        let data = resp
            .get("data")
            .and_then(|d| d.as_array())
            .ok_or_else(|| "embedding 响应缺 data 字段".to_string())?;
        let mut out = Vec::with_capacity(data.len());
        for item in data {
            let arr = item
                .get("embedding")
                .and_then(|e| e.as_array())
                .ok_or_else(|| "embedding 响应项缺 embedding 字段".to_string())?;
            out.push(
                arr.iter()
                    .filter_map(|v| v.as_f64().map(|f| f as f32))
                    .collect::<Vec<f32>>(),
            );
        }
        Ok(out)
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
        // 无 cancel 的探测路径（如 fallback 试探）：传一个永不置位的标记。
        let never = std::sync::atomic::AtomicBool::new(false);
        self.stream_model_with_events(request, &never, &mut |_| true)
    }

    fn stream_model_with_events(
        &self,
        request: ModelCallRequest,
        cancel: &std::sync::atomic::AtomicBool,
        on_event: &mut dyn FnMut(ModelEvent) -> bool,
    ) -> Result<ModelCallResult, ProviderCallError> {
        let observe = self.observer.as_ref().is_some_and(|o| o.enabled());
        let started = std::time::Instant::now();
        let outcome = self.stream_model_inner(&request, cancel, on_event);
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

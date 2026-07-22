use std::sync::mpsc::sync_channel;
use std::time::Duration;

use bytes::Bytes;
use futures_util::StreamExt;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde::de::DeserializeOwned;

use crate::http::error::HttpError;
use crate::http::runtime;
use crate::http::stream::{CancellableReader, Chunk};

/// 通道供给读的查 cancel 节拍（recv 超时）：与 socket 读超时无关，仅决定空闲态取消延迟。
const STREAM_POLL_MS: u64 = 250;
/// 连接超时。
const CONNECT_TIMEOUT_S: u64 = 10;
/// 流式读超时（首字节/正文）：宽值容忍慢首字节；取消不靠它、靠 abort。
const STREAM_READ_TIMEOUT_S: u64 = 120;
/// 非流式请求默认整体超时。
const DEFAULT_TIMEOUT_S: u64 = 60;
/// 通道缓冲：有界，背压。
const CHANNEL_CAP: usize = 32;

#[derive(Clone, Copy)]
pub(crate) enum Method {
    Get,
    Post,
}

/// 统一 HTTP 请求描述（构造器风格）。
pub(crate) struct HttpRequest {
    pub method: Method,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub content_type: Option<String>,
    pub timeout: Option<Duration>,
}

impl HttpRequest {
    pub(crate) fn get(url: impl Into<String>) -> Self {
        Self::new(Method::Get, url)
    }
    pub(crate) fn post(url: impl Into<String>) -> Self {
        Self::new(Method::Post, url)
    }
    fn new(method: Method, url: impl Into<String>) -> Self {
        Self {
            method,
            url: url.into(),
            headers: Vec::new(),
            body: Vec::new(),
            content_type: None,
            timeout: None,
        }
    }
    pub(crate) fn header(mut self, k: impl Into<String>, v: impl Into<String>) -> Self {
        self.headers.push((k.into(), v.into()));
        self
    }
    pub(crate) fn headers(mut self, hs: Vec<(String, String)>) -> Self {
        self.headers.extend(hs);
        self
    }
    /// JSON 正文（同时设 Content-Type: application/json）。
    pub(crate) fn json_body(mut self, v: &serde_json::Value) -> Result<Self, HttpError> {
        self.body = serde_json::to_vec(v).map_err(|e| HttpError::Decode(e.to_string()))?;
        self.content_type = Some("application/json".to_string());
        Ok(self)
    }
    pub(crate) fn string_body(mut self, s: impl Into<String>) -> Self {
        self.body = s.into().into_bytes();
        self
    }
    /// 表单正文（application/x-www-form-urlencoded）。
    pub(crate) fn form_body(mut self, form: &[(&str, &str)]) -> Self {
        let mut ser = url::form_urlencoded::Serializer::new(String::new());
        for (k, v) in form {
            ser.append_pair(k, v);
        }
        self.body = ser.finish().into_bytes();
        self.content_type = Some("application/x-www-form-urlencoded".to_string());
        self
    }
    pub(crate) fn content_type(mut self, ct: impl Into<String>) -> Self {
        self.content_type = Some(ct.into());
        self
    }
    pub(crate) fn timeout(mut self, d: Duration) -> Self {
        self.timeout = Some(d);
        self
    }
}

/// 非流式响应：状态 + 头 + 完整正文。**非 2xx 不报错**（作为数据返回，调用方按状态处理，
/// 如 304/401）；传输/超时错误才是 `HttpError`。
pub(crate) struct HttpResponse {
    pub status: u16,
    /// 最终 URL（跟随重定向后）。
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl HttpResponse {
    pub(crate) fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }
    pub(crate) fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).to_string()
    }
    pub(crate) fn json<T: DeserializeOwned>(&self) -> Result<T, HttpError> {
        serde_json::from_slice(&self.body).map_err(|e| HttpError::Decode(e.to_string()))
    }
    /// 大小写不敏感取响应头（首个匹配）。
    pub(crate) fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

pub(crate) struct HttpClient;

impl HttpClient {
    pub(crate) fn new() -> Self {
        HttpClient
    }

    /// 非流式请求：block_on 完成整个请求-响应并读全正文。非 2xx 作为 `HttpResponse` 返回
    /// （不报错），传输/超时错误 → `HttpError`。
    pub(crate) fn send(&self, req: HttpRequest) -> Result<HttpResponse, HttpError> {
        let hmap = build_headers(&req)?;
        let timeout = req.timeout.unwrap_or(Duration::from_secs(DEFAULT_TIMEOUT_S));
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(CONNECT_TIMEOUT_S))
            .timeout(timeout)
            .build()
            .map_err(|e| HttpError::Transport(e.to_string()))?;

        let url = req.url.clone();
        let body = req.body.clone();
        let method = req.method;
        runtime().block_on(async move {
            let builder = match method {
                Method::Get => client.get(&url),
                Method::Post => client.post(&url).body(body),
            };
            let resp = builder
                .headers(hmap)
                .send()
                .await
                .map_err(map_reqwest_err)?;
            let status = resp.status().as_u16();
            let final_url = resp.url().to_string();
            let headers = resp
                .headers()
                .iter()
                .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
                .collect();
            let bytes = resp.bytes().await.map_err(map_reqwest_err)?;
            Ok(HttpResponse {
                status,
                url: final_url,
                headers,
                body: bytes.to_vec(),
            })
        })
    }

    /// 可取消流式（cancel-poll 模式）：读者每 250ms 返回 WouldBlock 供上层查 cancel。
    /// 用于 provider 流式（配合 read_sse_lines）。
    pub(crate) fn stream_body(&self, req: HttpRequest) -> Result<CancellableReader, HttpError> {
        self.open_stream(req, Some(Duration::from_millis(STREAM_POLL_MS)))
    }

    /// 阻塞流式（无 WouldBlock）：用于 std `reader.lines()` 这类不容忍 WouldBlock 的读者（MCP SSE）。
    pub(crate) fn open_sse(&self, req: HttpRequest) -> Result<CancellableReader, HttpError> {
        self.open_stream(req, None)
    }

    /// 打开流式响应：async 读响应头并校验状态（宽超时→慢首字节不误超时），
    /// 2xx 则 spawn 任务把字节流经 channel 送回，返回 CancellableReader。
    /// 非 2xx / 传输错误 → 立即返回 HttpError（保状态码，供域映射分类）。
    /// `poll`：读者查 cancel 节拍；`None` 为阻塞模式。
    fn open_stream(
        &self,
        req: HttpRequest,
        poll: Option<Duration>,
    ) -> Result<CancellableReader, HttpError> {
        let rt = runtime();
        let hmap = build_headers(&req)?;
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(CONNECT_TIMEOUT_S))
            .read_timeout(Duration::from_secs(STREAM_READ_TIMEOUT_S))
            .build()
            .map_err(|e| HttpError::Transport(e.to_string()))?;

        let url = req.url.clone();
        let body = req.body.clone();
        let method = req.method;
        let resp = rt.block_on(async move {
            let builder = match method {
                Method::Get => client.get(&url),
                Method::Post => client.post(&url).body(body),
            };
            let resp = builder
                .headers(hmap)
                .send()
                .await
                .map_err(map_reqwest_err)?;
            let status = resp.status();
            if !status.is_success() {
                let text = resp.text().await.unwrap_or_default();
                return Err(HttpError::Status {
                    code: status.as_u16(),
                    body: text,
                });
            }
            Ok(resp)
        })?;

        let (tx, rx) = sync_channel::<Chunk>(CHANNEL_CAP);
        let handle = rt.spawn(async move {
            let mut stream = resp.bytes_stream();
            while let Some(item) = stream.next().await {
                match item {
                    Ok(bytes) => {
                        let b: Bytes = bytes;
                        if tx.send(Chunk::Data(b.to_vec())).is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Chunk::Err(map_reqwest_err(e)));
                        break;
                    }
                }
            }
        });

        Ok(CancellableReader::new(rx, poll, handle))
    }
}

fn build_headers(req: &HttpRequest) -> Result<HeaderMap, HttpError> {
    let mut hmap = HeaderMap::new();
    if let Some(ct) = &req.content_type {
        hmap.insert(
            reqwest::header::CONTENT_TYPE,
            HeaderValue::from_str(ct).map_err(|e| HttpError::Transport(e.to_string()))?,
        );
    }
    for (k, v) in &req.headers {
        let name =
            HeaderName::from_bytes(k.as_bytes()).map_err(|e| HttpError::Transport(e.to_string()))?;
        let val = HeaderValue::from_str(v).map_err(|e| HttpError::Transport(e.to_string()))?;
        hmap.insert(name, val);
    }
    Ok(hmap)
}

/// reqwest 错误 → HttpError（超时单列，其余传输层）。
fn map_reqwest_err(e: reqwest::Error) -> HttpError {
    if e.is_timeout() {
        HttpError::Timeout
    } else {
        HttpError::Transport(e.to_string())
    }
}

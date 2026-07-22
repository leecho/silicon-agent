use std::fmt;

/// 中立传输层错误。各域自行映射到本域错误语义（provider→ProviderCallError）。
#[derive(Debug, Clone)]
pub(crate) enum HttpError {
    /// 连接/读超时。
    Timeout,
    /// HTTP 非 2xx 响应（含原始 body，供友好文案解析）。
    Status { code: u16, body: String },
    /// 连接/重置/DNS/TLS 等传输层错误。
    Transport(String),
    /// 响应体解码（JSON 等）失败。
    Decode(String),
}

impl fmt::Display for HttpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HttpError::Timeout => write!(f, "请求超时"),
            HttpError::Status { code, .. } => write!(f, "HTTP {code}"),
            HttpError::Transport(m) => write!(f, "{m}"),
            HttpError::Decode(m) => write!(f, "解析失败：{m}"),
        }
    }
}

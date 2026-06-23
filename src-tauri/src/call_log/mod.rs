//! 模型调用日志模块：model_call_log 表的采集与查询。
//!
//! 与 token_usage（用量聚合）正交：保存每次调用的完整请求 payload + 原始响应，
//! 供调试/审计。默认关闭（app_settings.model_call_log_enabled），写入 best-effort。
//! 捕获点是 ProviderGateway 的 ModelCallObserver 钩子（见 observer 子模块）。

mod observer;
mod store;

pub use observer::CallLogObserver;
pub use store::CallLogStore;

/// 一次调用日志的采集输入（observer 据 ModelCallObservation 组装）。
#[derive(Debug, Clone)]
pub struct CallLogRecord {
    pub created_at: String,
    pub session_id: Option<String>,
    pub message_id: Option<String>,
    pub parent_session_id: Option<String>,
    pub parent_tool_call_id: Option<String>,
    pub expert_name: Option<String>,
    pub usage_type: String,
    pub provider: String,
    pub model: String,
    pub request_json: String,
    pub response_text: Option<String>,
    pub response_tool_calls_json: Option<String>,
    pub reasoning_text: Option<String>,
    pub finish_reason: Option<String>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_create_tokens: u64,
    pub latency_ms: u64,
    pub status: String, // "ok" | "error"
    pub error_message: Option<String>,
    pub error_class: Option<String>,
    pub http_status: Option<u16>,
}

/// 列表筛选条件。
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallLogFilter {
    pub session_id: Option<String>,
    pub model: Option<String>,
    pub provider: Option<String>,
    pub usage_type: Option<String>,
    pub status: Option<String>,
    pub since: Option<i64>,
    pub until: Option<i64>,
    pub search: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

/// 列表行（摘要，不含大 payload）。
#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CallLogRow {
    pub id: String,
    pub created_at: String,
    pub session_id: Option<String>,
    pub usage_type: String,
    pub provider: String,
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_create_tokens: u64,
    pub latency_ms: u64,
    pub status: String,
    pub truncated: bool,
}

/// 单条明细（含完整 payload）。
#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CallLogDetail {
    pub id: String,
    pub created_at: String,
    pub session_id: Option<String>,
    pub message_id: Option<String>,
    pub parent_session_id: Option<String>,
    pub parent_tool_call_id: Option<String>,
    pub expert_name: Option<String>,
    pub usage_type: String,
    pub provider: String,
    pub model: String,
    pub request_json: String,
    pub response_text: Option<String>,
    pub response_tool_calls_json: Option<String>,
    pub reasoning_text: Option<String>,
    pub finish_reason: Option<String>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_create_tokens: u64,
    pub latency_ms: u64,
    pub status: String,
    pub error_message: Option<String>,
    pub error_class: Option<String>,
    pub http_status: Option<u16>,
    pub request_bytes: u64,
    pub truncated: bool,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CallLogStats {
    pub count: u64,
    pub bytes: u64,
}

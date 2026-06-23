//! Provider Gateway DTO 和去敏投影（多模型）。

/// Provider 连通性检查结果。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderCheckResult {
    pub status: String,
    pub detail: String,
    pub checked_at: String,
}

/// 厂商去敏投影（不含明文 api_key）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderView {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub has_secret: bool,
    pub secret_hint: Option<String>,
    pub enabled: bool,
    pub last_check: Option<ProviderCheckResult>,
    pub sort_order: i64,
}

/// 厂商写入输入。`api_key`：None=保持现有，空串=清除，非空=设置。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInput {
    pub id: Option<String>,
    pub name: String,
    pub base_url: String,
    pub api_key: Option<String>,
    pub enabled: bool,
}

/// 模型去敏投影。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ModelView {
    pub id: String,
    pub provider_id: String,
    pub model: String,
    pub display_name: Option<String>,
    pub enabled: bool,
    pub is_default: bool,
    pub sort_order: i64,
    /// 该模型的上下文窗口上限（token）覆盖；None 表示用内置查表 `model_context_limit`。
    pub context_limit: Option<i64>,
}

/// 模型写入输入（id=None 新建）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInput {
    pub id: Option<String>,
    pub provider_id: String,
    pub model: String,
    pub display_name: Option<String>,
    pub enabled: bool,
    /// 上下文窗口上限覆盖（token）；None/缺省表示沿用内置查表。
    #[serde(default)]
    pub context_limit: Option<i64>,
}

/// 供 Composer 的「启用厂商 → 启用模型」分组视图。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EnabledProviderModels {
    pub provider_id: String,
    pub provider_name: String,
    pub models: Vec<ModelView>,
}

/// 未命中查表时的保守上下文窗口上限（token）。
pub const DEFAULT_CONTEXT_LIMIT: i64 = 128_000;

/// 模型上下文窗口上限（token），按模型名子串匹配，未命中给保守默认。
///
/// 仅用于 composer 的 context meter 分母展示，不参与请求构造。表项靠前优先匹配，
/// 故更具体的名字（如 gemini-1.5）须排在更宽泛的名字（gemini）之前。
pub fn model_context_limit(model: &str) -> i64 {
    let m = model.to_ascii_lowercase();
    const TABLE: &[(&str, i64)] = &[
        ("claude", 200_000),
        ("gpt-4.1", 1_000_000),
        ("gpt-4o", 128_000),
        ("o1", 200_000),
        ("o3", 200_000),
        ("deepseek", 128_000),
        ("qwen", 128_000),
        ("glm", 128_000),
        ("kimi", 128_000),
        ("moonshot", 128_000),
        ("doubao", 128_000),
        ("gemini-1.5", 1_000_000),
        ("gemini-2", 1_000_000),
        ("gemini", 1_000_000),
    ];
    for (key, limit) in TABLE {
        if m.contains(key) {
            return *limit;
        }
    }
    DEFAULT_CONTEXT_LIMIT
}

#[cfg(test)]
mod context_limit_tests {
    use super::{model_context_limit, DEFAULT_CONTEXT_LIMIT};

    #[test]
    fn matches_known_families_case_insensitive() {
        assert_eq!(model_context_limit("claude-opus-4-8"), 200_000);
        assert_eq!(model_context_limit("Claude-3-5-Sonnet"), 200_000);
        assert_eq!(model_context_limit("deepseek-chat"), 128_000);
        assert_eq!(model_context_limit("qwen2.5-72b"), 128_000);
        assert_eq!(model_context_limit("glm-4-plus"), 128_000);
    }

    #[test]
    fn specific_wins_over_generic() {
        // gemini-1.5 须先于 gemini 命中
        assert_eq!(model_context_limit("gemini-1.5-pro"), 1_000_000);
        assert_eq!(model_context_limit("gpt-4.1-mini"), 1_000_000);
        assert_eq!(model_context_limit("gpt-4o-mini"), 128_000);
    }

    #[test]
    fn unknown_falls_back_to_default() {
        assert_eq!(
            model_context_limit("some-unknown-model"),
            DEFAULT_CONTEXT_LIMIT
        );
        assert_eq!(model_context_limit(""), DEFAULT_CONTEXT_LIMIT);
    }
}

/// 解析结果：把 model_id（或全局默认）解析为可调用的厂商 + 模型。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedModel {
    pub model_id: String,
    pub provider_id: String,
    pub provider_name: String,
    pub model: String,
}

impl ResolvedModel {
    pub fn selection(&self) -> crate::provider::client::ModelSelection {
        crate::provider::client::ModelSelection {
            provider_id: self.provider_id.clone(),
            model: self.model.clone(),
        }
    }
}

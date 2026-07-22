//! Provider 模型消息、工具和流式事件契约。
//!
//! 本模块只表达 provider-agnostic DTO，不决定 Agent 语义、不执行工具、不持久化业务事实。

/// Provider 消息角色。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModelMessageRole {
    System,
    User,
    Assistant,
    Tool,
}

/// 发送给多模态模型的图片（base64）。`media_type` 形如 `image/png`。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ModelImage {
    pub media_type: String,
    pub base64_data: String,
}

/// 发送给模型的一条归一化消息。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ModelMessage {
    pub role: ModelMessageRole,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ModelToolCall>>,
    /// 随本条消息发送的图片（仅多模态模型；空则 content 按字符串发送）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<ModelImage>,
}

/// Assistant 消息中的工具调用引用。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ModelToolCall {
    pub id: String,
    pub name: String,
    pub arguments_json: String,
}

impl ModelMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: ModelMessageRole::System,
            content: content.into(),
            tool_call_id: None,
            tool_calls: None,
            images: Vec::new(),
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: ModelMessageRole::User,
            content: content.into(),
            tool_call_id: None,
            tool_calls: None,
            images: Vec::new(),
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: ModelMessageRole::Assistant,
            content: content.into(),
            tool_call_id: None,
            tool_calls: None,
            images: Vec::new(),
        }
    }

    pub fn assistant_tool_call(
        id: impl Into<String>,
        name: impl Into<String>,
        arguments_json: impl Into<String>,
    ) -> Self {
        Self {
            role: ModelMessageRole::Assistant,
            content: String::new(),
            tool_call_id: None,
            tool_calls: Some(vec![ModelToolCall {
                id: id.into(),
                name: name.into(),
                arguments_json: arguments_json.into(),
            }]),
            images: Vec::new(),
        }
    }

    pub fn tool(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: ModelMessageRole::Tool,
            content: content.into(),
            tool_call_id: Some(tool_call_id.into()),
            tool_calls: None,
            images: Vec::new(),
        }
    }

    /// 一条 assistant 消息携带多个 tool_calls（单回合多工具）。
    pub fn assistant_tool_calls(calls: Vec<ModelToolCall>) -> Self {
        Self {
            role: ModelMessageRole::Assistant,
            content: String::new(),
            tool_call_id: None,
            tool_calls: Some(calls),
            images: Vec::new(),
        }
    }
}

/// 模型工具选择策略。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModelToolChoice {
    Auto,
    None,
    Required,
    Named(String),
}

/// 模型输出 schema 目标。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModelResponseSchema {
    AgentFinalOutput,
    PlanDraft,
    StepIntent,
    FreeformAssistant,
    RecoveryProposal,
}

/// 模型调用归属，用于审计、usage 和恢复。
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ModelAttribution {
    pub session_id: String,
    pub task_id: Option<String>,
    pub execution_id: Option<String>,
    pub agent_run_id: Option<String>,
    /// T76 调用日志归因：本次调用对应的 assistant 消息 id。
    #[serde(default)]
    pub message_id: Option<String>,
    /// 调用类型：main_agent/sub_agent/title/suggestion/compaction/curation；空时日志记 other。
    #[serde(default)]
    pub usage_type: Option<String>,
    #[serde(default)]
    pub parent_session_id: Option<String>,
    #[serde(default)]
    pub parent_tool_call_id: Option<String>,
    #[serde(default)]
    pub expert_name: Option<String>,
}

/// 暴露给模型的工具定义。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolSpecForModel {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    pub permission: String,
    pub risk: String,
}

impl ToolSpecForModel {
    pub fn json_schema(
        name: impl Into<String>,
        description: impl Into<String>,
        input_schema: serde_json::Value,
        permission: impl Into<String>,
        risk: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            input_schema,
            permission: permission.into(),
            risk: risk.into(),
        }
    }
}

#[cfg(test)]
mod attribution_tests {
    use super::ModelAttribution;

    #[test]
    fn defaults_and_old_json_deserialize() {
        // 旧 JSON（仅 4 字段）应能反序列化，新字段为 None。
        let old = r#"{"sessionId":"s1","taskId":null,"executionId":null,"agentRunId":null}"#;
        let a: ModelAttribution = serde_json::from_str(old).unwrap();
        assert_eq!(a.session_id, "s1");
        assert!(a.usage_type.is_none());
        assert!(a.message_id.is_none());
        // Default 可用。
        let d = ModelAttribution::default();
        assert_eq!(d.session_id, "");
    }
}

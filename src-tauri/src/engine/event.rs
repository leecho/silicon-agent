#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentStreamEvent {
    pub kind: String, // "message_delta" | "thinking_delta" | "message_completed" | "message_failed" | "tool_call" | "tool_result"
    pub session_id: String,
    pub message_id: String,
    pub sequence: u64,
    pub text: Option<String>,
    pub status: Option<String>,
    /// 工具名（tool_call / tool_result 事件携带）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    /// 工具的面向用户标签（来自 `Tool::label()`；tool_call / tool_result 携带）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_label: Option<String>,
    /// 工具调用 id（tool_call / tool_result 事件携带）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// 当前会话的整组待办清单（仅 `todos_updated` 事件携带）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub todos: Option<Vec<crate::session::TodoItem>>,
    /// 当前会话的整组产物（仅 `artifacts_updated` 事件携带）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifacts: Option<Vec<crate::session::Artifact>>,
    /// 子运行来源标记（仅 child 子运行事件携带；顶层会话为 None）。前端据此把 child 事件路由到专家面板。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expert_name: Option<String>,
    pub created_at: String,
}

pub type StreamEmitter = std::sync::Arc<dyn Fn(AgentStreamEvent) + Send + Sync>;

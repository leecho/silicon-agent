#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub pinned: bool,
    pub group_id: Option<String>,
    /// 会话工作模式："normal"（普通，写工具需确认）| "plan"（计划模式，仅只读工具调研）。默认 normal。
    pub mode: String,
    /// 用户为该会话显式选择的工作目录（沙箱根）；None 表示用默认 {home}/.siliconagent/{session_id}。
    #[serde(default)]
    pub working_dir: Option<String>,
    /// 该会话的权限模式覆盖（manual/auto/full）；None 表示继承全局默认。
    #[serde(default)]
    pub permission_mode: Option<String>,
    /// 会话选中的模型 id；None 表示用全局默认模型。
    #[serde(default)]
    pub selected_model_id: Option<String>,
    /// 会话来源：`user`（用户在界面新建，默认）| `scheduled`（定时任务触发）| 预留 `im` 等。
    /// 用于侧边栏白名单过滤：SessionManager 只展示 `user` 来源，其余各有专属入口。
    #[serde(default)]
    pub origin: String,
    /// 是否草稿：true 表示未提交的草稿会话（隐藏在「草稿」区，不进任务列表）。
    #[serde(default)]
    pub is_draft: bool,
    /// 草稿内容：Composer 整条待发内容的序列化串（含 ⟦@附件⟧ / ⟦技能：名⟧ / 正文）；非草稿为空串。
    #[serde(default)]
    pub draft_content: String,
    /// 上一轮结束生成的快捷建议（持久化，供 reload/切会话回显）；发新消息即清空。
    #[serde(default)]
    pub last_suggestions: Vec<String>,
    /// 该会话当前是否有 run 在后台运行。运行态来自 RunRegistry，不持久化。
    #[serde(default)]
    pub is_running: bool,
    /// 当前 run 的开始时间（Unix epoch 秒字符串）。运行态来自 RunRegistry，不持久化。
    #[serde(default)]
    pub run_started_at: Option<String>,
    /// 父会话 id；`None` = 顶层会话。子运行（origin="subagent"）指向其上级会话。
    #[serde(default)]
    pub parent_session_id: Option<String>,
    /// 父会话中派生本子运行的 dispatch tool_call id（UI 锚点：挂到哪次 dispatch 卡片）。
    #[serde(default)]
    pub parent_tool_call_id: Option<String>,
    /// 本子运行所用的 agent 角色名（= agents 表 name）；顶层会话为 None。
    #[serde(default)]
    pub expert_name: Option<String>,
    /// 本子运行的任务契约（序列化）；顶层会话为 None。
    #[serde(default)]
    pub agent_task: Option<String>,
    /// 父 run 停泊态：非 None = 正等待该 child_session_id 的子运行完成；child 完成后清空。
    #[serde(default)]
    pub awaiting_subagent: Option<String>,
    /// ad-hoc（动态生成）专家的 system prompt；声明式专家（散装/plugin）为 None（运行时查 spec）。
    #[serde(default)]
    pub expert_system_prompt: Option<String>,
    /// ad-hoc 专家的工具白名单（逗号连接）；声明式为 None。
    #[serde(default)]
    pub expert_tools: Option<String>,
    /// 所属持久智能体 id；None 表示普通会话或项目会话。
    /// 持久智能体是会话归属实体，拥有自己的工作目录、记忆和详情聚合。
    #[serde(default)]
    pub agent_id: Option<String>,
    /// 运行角色类型：expert/team；None 表示自由模式。
    /// 项目和持久智能体不是角色定义，不进入该字段。
    #[serde(default)]
    pub role_kind: Option<String>,
    /// 运行角色 id：kind="expert" 时为专家 id；kind="team" 时为团队 id。
    #[serde(default)]
    pub role_id: Option<String>,
    /// 历史遗留列（裁剪后单会话模型不再产生后台派发）；保留为惰性列以兼容旧数据，恒为 false。
    #[serde(default)]
    pub is_background: bool,
    /// T57：子运行终态 "done"|"failed"|"cancelled"（供 collect 读取；运行中为 None）。
    #[serde(default)]
    pub run_outcome: Option<String>,
    /// T57：父 collect 停泊态（JSON `{collectCallId, remaining:[childId...]}`）；None=未在 collect 等待。
    #[serde(default)]
    pub pending_collect: Option<String>,
    /// T59：所属项目 id（项目线程及其 child 会话非空）；普通会话/智能体会话为 None。
    #[serde(default)]
    pub project_id: Option<String>,
    /// T70：会话任务队列（FIFO 邮箱）。JSON 数组，元素见 session::task_queue::SessionTaskItem。
    /// 队头 status="running"=在飞任务，其余 queued；None/空=空闲。session-store 通用字段。
    #[serde(default)]
    pub pending_tasks: Option<String>,
}

/// 专家（child 子运行）在父会话右侧面板的摘要：名字 + 任务 + 计算出的状态。
#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChildAgentSummary {
    pub session_id: String,
    pub expert_name: String,
    pub task: String,
    /// "running"（运行中）| "paused"（暂停：等确认/纠偏）| "done"（已回禀）| "failed"（失败）。
    pub status: String,
    pub created_at: String,
    /// 轮次键 = 产出该专家 dispatch 调用的 assistant 消息 id。同一轮 fan-out 的专家共享此值，
    /// 供面板按轮次分组（最新一轮=本轮，其余折叠为历史）。无法定位时回退为 session_id。
    pub round_id: String,
    /// 可选展示身份（来自专家定义；ad-hoc/缺省为空，前端回退 expert_name）。
    pub display_name: Option<String>,
    pub profession: Option<String>,
    pub avatar: Option<String>,
}

/// 会话分组：命名分组 + 颜色标记。会话通过 `Session::group_id` 归入某分组。
#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionGroup {
    pub id: String,
    pub label: String,
    pub color_key: String,
    pub created_at: String,
    pub built_in: bool,
    pub sort_order: i64,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub id: String,
    pub session_id: String,
    pub role: String, // "user" | "assistant" | "tool"
    pub content: String,
    pub reasoning: Option<String>,
    /// assistant 消息携带的工具调用（JSON 序列化的 Vec<ModelToolCall>）。
    pub tool_calls_json: Option<String>,
    /// tool 消息对应的工具调用 id。
    pub tool_call_id: Option<String>,
    /// tool 消息执行的工具名。
    pub tool_name: Option<String>,
    /// tool 消息执行状态（"done" | "failed"）；非 tool 消息为 None。
    pub tool_status: Option<String>,
    /// 该消息是否已被 compact 压缩（被摘要吸收）。压缩后引擎组装上下文时跳过它，
    /// 但消息仍持久化、feed 显示不变。默认 false。
    #[serde(default)]
    pub compacted: bool,
    pub created_at: String,
}

/// 引擎因风险工具需用户确认而暂停时返回的待确认请求。
///
/// 定义在 session/types 避免 session → engine 循环依赖；
/// engine 层直接 use 此类型（engine 依赖 session，session 不依赖 engine）。
#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PendingPermission {
    /// 该暂停归属的会话 id（顶层会话或某子运行的子会话）；UI/remote 据此寻址。
    pub session_id: String,
    pub tool_call_id: String,
    pub tool_name: String,
    /// 工具调用的输入参数 JSON（`arguments_json`）。
    pub input: String,
}

/// ask_user 的单个问题：主题(可空) + 问题文本 + 单/多选 + 选项(可空)。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AskQuestion {
    pub header: String,
    pub question: String,
    pub multi_select: bool,
    pub options: Vec<String>,
}

/// 待回答的模型提问（引擎调 ask_user 暂停时非空）。一次可含多题，前端分页作答。
///
/// 与 `PendingPermission` 并列定义在 session/types，避免 session → engine 循环依赖。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PendingAsk {
    /// 该暂停归属的会话 id（顶层会话或某子运行的子会话）；UI/remote 据此寻址。
    pub session_id: String,
    pub tool_call_id: String,
    pub questions: Vec<AskQuestion>,
}

/// 引擎因模型调用 `propose_plan`（计划模式提交计划）而暂停时返回的待批准计划。
///
/// 与 `PendingPermission`/`PendingAsk` 并列定义在 session/types，避免 session → engine 循环依赖。
#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PendingPlan {
    /// 该暂停归属的会话 id（顶层会话或某子运行的子会话）；UI/remote 据此寻址。
    pub session_id: String,
    pub tool_call_id: String,
    pub title: String,
    pub summary: String,
    pub plan_markdown: String,
    pub risk_level: String,
}

/// 当前会话的待办清单一项。`update_todos` 工具整组覆写、持久化到 session，
/// 前端固定 TodoPanel 据此渲染。`status` 取 pending | in_progress | completed。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TodoItem {
    pub id: u32,
    pub content: String,
    pub status: String, // pending | in_progress | completed
}

/// 一个「产物」：Agent 显式登记的最终交付文件。path 为工作目录相对路径；
/// message_id/tool_call_id 绑定产生它的 assistant 消息与该 add_artifact 调用。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Artifact {
    pub path: String,
    pub title: String,
    /// 分类：final（最终交付文件）| working（脚本/中间文件）。
    pub kind: String,
    pub message_id: Option<String>,
    pub tool_call_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub session: SessionInfo,
    pub messages: Vec<Message>,
    /// 引擎暂停等待用户授权时非空；正常收口时为 None。
    pub pending_permission: Option<PendingPermission>,
    /// 引擎暂停等待用户回答 ask_user 时非空；正常收口时为 None。
    pub pending_ask: Option<PendingAsk>,
    /// 引擎暂停等待用户批准 propose_plan 计划时非空；正常收口时为 None。
    pub pending_plan: Option<PendingPlan>,
    /// 当前会话的待办清单（无则空 Vec）。
    pub todos: Vec<TodoItem>,
    /// 解析后的实际工作目录绝对路径（始终非空，由 AppState::session_with_pending 回填，供前端展示）。
    #[serde(default)]
    pub resolved_working_dir: String,
    /// 当前会话已登记的产物列表（无则空 Vec）。
    #[serde(default)]
    pub artifacts: Vec<Artifact>,
    /// 该会话当前是否有引擎 run 在后台运行（运行时态，不持久化；由 AppState 据运行锁回填）。
    #[serde(default)]
    pub is_running: bool,
}

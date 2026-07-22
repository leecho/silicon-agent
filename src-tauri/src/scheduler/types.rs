use serde::{Deserialize, Serialize};

// 注：每次运行都新建 session，不再有「执行目标」概念（无 TargetMode）。
// 无人值守不再用自有枚举：改为复用会话的 permission_mode（manual/auto/full）+ model_id，
// 触发时写入新建会话，引擎按会话级系统生效（headless 仅影响 ask_user）。

/// 创建/更新任务的入参（schedule_spec 已由命令层归一化为 6 字段 cron）。
#[derive(Debug, Clone)]
pub struct TaskInput {
    pub name: String,
    pub prompt: String,
    pub schedule_spec: String,
    pub schedule_display: Option<String>,
    pub working_dir: Option<String>,
    /// 运行归属项目；与 agent_id 互斥。
    pub project_id: Option<String>,
    /// 运行归属持久智能体；与 project_id 互斥。
    pub agent_id: Option<String>,
    /// 运行角色定义（expert/team）；None=自由模式。
    pub role_kind: Option<String>,
    /// 运行角色 id。
    pub role_id: Option<String>,
    /// 会话权限模式（manual/auto/full）；None=继承全局默认。新建默认 full（命令层兜底）。
    pub permission_mode: Option<String>,
    /// 运行使用的模型 id；None=用全局默认模型。
    pub model_id: Option<String>,
}

/// 定时任务（落库行 + 派生展示字段）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduledTask {
    pub id: String,
    pub name: String,
    pub prompt: String,
    pub schedule_spec: String,
    pub schedule_display: Option<String>,
    pub working_dir: Option<String>,
    pub project_id: Option<String>,
    pub agent_id: Option<String>,
    pub role_kind: Option<String>,
    pub role_id: Option<String>,
    pub permission_mode: Option<String>,
    pub model_id: Option<String>,
    pub enabled: bool,
    pub next_run_at: Option<i64>,
    pub last_run_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
    /// 派生：该任务历史执行条数（列表角标用；get/list 时回填）。
    #[serde(default)]
    pub execution_count: i64,
    /// 派生：最近一次执行状态（列表徽标用）。
    #[serde(default)]
    pub last_status: Option<String>,
}

/// 一次执行记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskExecution {
    pub id: String,
    pub task_id: String,
    pub task_name: String,
    pub session_id: String,
    pub status: String,  // running|completed|needs_attention|failed|skipped
    pub trigger: String, // schedule|catchup|manual
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub error: Option<String>,
    /// 该次运行所属 session 的标题（LEFT JOIN sessions.title）；session 被删则为 None。
    /// 侧边栏 TaskTree 据此显示行文案，并过滤掉已删除会话的执行项。
    #[serde(default)]
    pub session_title: Option<String>,
}

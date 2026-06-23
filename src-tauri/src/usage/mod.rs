//! 用量分析模块：token_usage 表的采集与聚合。
//!
//! 采集：引擎每次模型调用后写一行。聚合：按日期/模型/会话/小时 SQL GROUP BY，
//! 返回 UsageAnalyticsView 供设置页用量分析渲染。纯 Token 维度，不折算金额。

mod store;

pub use store::UsageStore;

use crate::provider::client::ModelUsage;

/// 一次模型调用的用量采集输入（引擎在调用后构造）。
#[derive(Debug, Clone)]
pub struct UsageRecord {
    pub session_id: String,
    pub message_id: Option<String>,
    pub provider: String,
    pub model: String,
    pub usage_type: String,
    pub created_at: String,
    pub usage: ModelUsage,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UsageTotals {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_create: u64,
    pub total: u64,
    pub calls: u64,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UsageDateBucket {
    pub date: String,
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_create: u64,
    pub total: u64,
    pub calls: u64,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UsageModelRow {
    pub provider: String,
    pub model: String,
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_create: u64,
    pub total: u64,
    pub calls: u64,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UsageSessionRow {
    pub session_id: String,
    pub title: String,
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_create: u64,
    pub total: u64,
    pub calls: u64,
}

/// 项目维度聚合行（按 sessions.project_id 归属）。
#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UsageProjectRow {
    pub project_id: String,
    pub name: String,
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_create: u64,
    pub total: u64,
    pub calls: u64,
}

/// 智能体维度聚合行（按「最近活动角色智能体」递归归属）。
#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UsageAgentRow {
    pub agent_id: String,
    pub name: String,
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_create: u64,
    pub total: u64,
    pub calls: u64,
}

/// 作用域（项目 / 智能体）用量详情：总计 + 按会话明细。
#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ScopedUsageView {
    pub totals: UsageTotals,
    pub by_session: Vec<UsageSessionRow>,
}

/// 单条消息（一次 assistant 回合）的用量；供会话→消息二层展开。
#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UsageMessageRow {
    pub message_id: String,
    /// 消息内容摘要（已截断）；用于行标签，无内容时为空。
    pub snippet: String,
    pub role: String,
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_create: u64,
    pub total: u64,
    /// 该消息 created_at（epoch 秒字符串）；无对应消息记录时回退为最早用量时间。
    pub ts: String,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UsageHourBucket {
    pub hour: u8,
    pub total: u64,
    pub calls: u64,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UsageDateModel {
    pub date: String,
    pub model: String,
    pub total: u64,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UsageCallRow {
    pub ts: String,
    pub provider: String,
    pub model: String,
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_create: u64,
    pub total: u64,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UsageAnalyticsView {
    pub totals: UsageTotals,
    pub by_date: Vec<UsageDateBucket>,
    pub by_model: Vec<UsageModelRow>,
    pub by_session: Vec<UsageSessionRow>,
    pub by_project: Vec<UsageProjectRow>,
    pub by_agent: Vec<UsageAgentRow>,
    pub by_hour: Vec<UsageHourBucket>,
    pub by_date_model: Vec<UsageDateModel>,
    pub recent_calls: Vec<UsageCallRow>,
    pub recent_cache_calls: Vec<UsageCallRow>,
    pub sessions: u64,
    pub messages: u64,
    pub generated_at: String,
}

/// 单会话上下文窗口占用（供 composer 的 context meter 展示）。
/// `used_tokens` 取该会话最近一次主体调用的总用量，`max_tokens` 取该模型上下文上限。
#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ContextUsageView {
    pub used_tokens: u64,
    pub max_tokens: u64,
    pub percent: u8,
    pub model: String,
}

/// 解析时间范围为 cutoff（epoch 秒下界，闭区间）。滚动窗口：
/// "7d" = now-7d，"30d" = now-30d，其它（含 "all"）= 0（不过滤）。
pub fn resolve_cutoff(range: &str, now_epoch: i64) -> i64 {
    match range {
        "7d" => (now_epoch - 7 * 86_400).max(0),
        "30d" => (now_epoch - 30 * 86_400).max(0),
        _ => 0,
    }
}

#[cfg(test)]
mod range_tests {
    use super::resolve_cutoff;

    #[test]
    fn cutoff_windows_and_all() {
        let now = 1_000_000_000;
        assert_eq!(resolve_cutoff("7d", now), now - 7 * 86_400);
        assert_eq!(resolve_cutoff("30d", now), now - 30 * 86_400);
        assert_eq!(resolve_cutoff("all", now), 0);
        assert_eq!(resolve_cutoff("unknown", now), 0);
    }
}

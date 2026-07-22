//! plugin 模块对 command 暴露的 DTO（camelCase 序列化，与前端 TS 对齐）。

use serde::Serialize;

use crate::plugin::model::{PluginRecord, PluginSource};
use crate::skill::SkillSummary;

/// 插件列表项 / 启停返回。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginSummary {
    pub id: String,
    pub source: PluginSource,
    pub name: String,
    pub display_name: String,
    pub version: String,
    pub description: String,
    pub description_zh: Option<String>,
    pub category: Option<String>,
    pub customized_from: Option<String>,
    pub enabled: bool,
    pub installed_at: String,
    /// 其下 skill 数量（含隐藏的内部知识库 skill）。
    pub skill_count: usize,
}

impl PluginSummary {
    pub fn from_record(r: PluginRecord, skill_count: usize) -> Self {
        PluginSummary {
            id: r.id,
            source: r.source,
            name: r.name,
            display_name: r.display_name,
            version: r.version,
            description: r.description,
            description_zh: r.description_zh,
            category: r.category,
            customized_from: r.customized_from,
            enabled: r.enabled,
            installed_at: r.installed_at,
            skill_count,
        }
    }
}

/// 插件详情：能力包元数据 + 其下 skill 列表 + 提供的 agents（plugin 是能力包，无 type/team 框架）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginDetail {
    pub plugin: PluginSummary,
    pub skills: Vec<SkillSummary>,
    /// 该插件提供的专家（由命令层从 agents 表填充）。
    pub agents: Vec<crate::expert::ExpertSummary>,
    /// 该插件提供的 MCP server（由命令层从 mcp store/状态填充）。
    pub mcp_servers: Vec<PluginMcpSummary>,
    /// 该插件声明的 hooks（由命令层从 manifest 填充）。
    pub hooks: Vec<PluginHookSummary>,
    /// 作者（从 plugin.json 解析；缺失为空）。
    pub author: Option<String>,
    /// 主页 URL。
    pub homepage: Option<String>,
    /// 仓库 URL。
    pub repository: Option<String>,
    /// 许可证。
    pub license: Option<String>,
    /// 关键词。
    pub keywords: Vec<String>,
}

/// 插件提供的 MCP server 展示摘要（详情页用）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginMcpSummary {
    /// 命名空间化的 server 名（`<plugin>:<server>`）。
    pub name: String,
    /// 传输类型：stdio | http。
    pub transport: String,
    /// stdio 的 command 或 http 的 url（展示用）。
    pub target: String,
    /// 连接状态：disconnected | connecting | connected | failed | unauthorized。
    pub state: String,
}

/// 插件声明的 hook 展示摘要（详情页用）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginHookSummary {
    /// 生命周期事件：PreToolUse | PostToolUse | SessionStart | Stop。
    pub event: String,
    /// 工具名匹配（空=匹配全部；仅 Pre/PostToolUse 有意义）。
    pub matcher: Option<String>,
    /// 命令（展示用，前端截断）。
    pub command: String,
}

/// 统一装载入口的结果：一个包要么装成能力包（plugin），要么装成团队（team）。
/// 内部标签联合 → 前端按 `kind` 判别。
/// 「运送≠合并」（T106）：团队即便由 plugin 包运送进来，装完仍活在团队页、走角色槽，
/// 不会变成一条全局能力。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum InstalledExtension {
    Plugin(PluginSummary),
    Team(crate::team::types::TeamSummary),
    /// silicon 专家包（`expert.json`）：落成散装专家 + 其 `expert_id` 私有技能。
    /// T108 前入口不认这一类，expert 包会被当 plugin 装、私有技能变公开。
    Expert(crate::expert::types::ExpertSummary),
}

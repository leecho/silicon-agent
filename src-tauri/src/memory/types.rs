//! 记忆类型：一条长期记忆与其分层/种类。
//!
//! `Memory` 自 session 迁出——记忆是全局、跨会话的长期知识，与会话/消息正交。
//! 新增字段为 Hermes 分层做准备：kind 区分 画像/事实/情景；tier 区分 常驻注入/检索召回。

/// 记忆种类。`profile`=用户画像（USER），`fact`=普通事实（默认），`episode`=会话历史摘要。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryKind {
    Profile,
    Fact,
    Episode,
}

impl MemoryKind {
    pub fn as_str(self) -> &'static str {
        match self {
            MemoryKind::Profile => "profile",
            MemoryKind::Fact => "fact",
            MemoryKind::Episode => "episode",
        }
    }

    pub fn from_str(s: &str) -> MemoryKind {
        match s {
            "profile" => MemoryKind::Profile,
            "episode" => MemoryKind::Episode,
            _ => MemoryKind::Fact,
        }
    }
}

/// 记忆作用域：决定写入哪层、召回时并入哪层。三层互斥——`project_id`/`agent_id` 至多一列非空。
/// 见 `docs/04-specs/2026-06-18-memory-project-scope-design.md`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryScope<'a> {
    /// 用户全局：跨一切。
    Global,
    /// 项目层：同项目所有线程/Expert 共享。
    Project(&'a str),
    /// 智能体私有（伴随体实例，T69）。
    Agent(&'a str),
}

impl<'a> MemoryScope<'a> {
    /// 从会话归属实体解析作用域：project_id 优先 → agent_id → 全局。
    pub fn from_session(project_id: &'a str, agent_id: &'a str) -> Self {
        if !project_id.is_empty() {
            MemoryScope::Project(project_id)
        } else if !agent_id.is_empty() {
            MemoryScope::Agent(agent_id)
        } else {
            MemoryScope::Global
        }
    }

    /// 写入用的 project_id 列值（非项目层为空串）。
    pub fn project_id(&self) -> &str {
        if let MemoryScope::Project(p) = self {
            p
        } else {
            ""
        }
    }

    /// 写入用的 agent_id 列值（非私有层为空串）。
    pub fn agent_id(&self) -> &str {
        if let MemoryScope::Agent(a) = self {
            a
        } else {
            ""
        }
    }

    /// 召回用的作用域谓词：返回 (SQL 片段, 可选绑定值 `:scope`)。
    /// 召回语义恒为「全局 ∪ 当前作用域」：Global 仅全局行；Project/Agent 为「全局行 OR 本层行」。
    /// 片段中的 `project_id`/`agent_id` 列在 memories 上无歧义（FTS 表仅含 id/content）。
    pub fn predicate(&self) -> (String, Option<String>) {
        match self {
            MemoryScope::Global => ("project_id = '' and agent_id = ''".to_string(), None),
            MemoryScope::Project(p) => (
                "((project_id = '' and agent_id = '') or project_id = :scope)".to_string(),
                Some((*p).to_string()),
            ),
            MemoryScope::Agent(a) => (
                "((project_id = '' and agent_id = '') or agent_id = :scope)".to_string(),
                Some((*a).to_string()),
            ),
        }
    }
}

/// 一条长期记忆（全局，跨会话）。`tier`：1=常驻注入，2=检索召回（默认）。
/// `content` 对外保持 camelCase 序列化，前端 `Memory{id,content,createdAt}` 不变。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Memory {
    pub id: String,
    pub content: String,
    pub created_at: String,
}

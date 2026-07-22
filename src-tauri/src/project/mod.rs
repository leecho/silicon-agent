//! 项目（Project）多专家协作空间：数据层。
//!
//! T59 起，项目运行时改为复用 session/engine 体系——群聊=项目归属线程会话、
//! 任务=dispatch 派生的 child 会话、ask/plan/permission 原生上浮。本文件只保留项目与成员的存储
//! （projects / project_members）；旧的 chat_messages/tasks/task_logs/task_artifacts 表与运行时已删除。

use serde::Serialize;
use std::sync::Arc;

use crate::expert::{ExpertSource, ExpertSpec, ExpertSummary};
use crate::session::new_id;
use crate::storage::AppDatabase;

pub mod runtime_skills;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub description: String,
    pub workspace_dir: Option<String>,
    /// 成员任务 run 的权限模式：manual|auto|full，默认 manual（让 ask/plan/permission 真正上浮）。
    pub permission_mode: String,
    /// 项目章程/PM 指令：定义项目经理(lead)的职责与风格；运行时合成为 lead 人格。空=用通用 PM。
    pub instructions: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectMember {
    pub id: String,
    pub project_id: String,
    /// 引用 agent 货币（散装 agent name）。
    pub expert_name: String,
    /// 展示职责标签（如「前端工程师」）。
    pub role_label: Option<String>,
    /// 路由依据：该成员适合接什么（注入 PM 名册供其编排选派）。
    pub responsibilities: Option<String>,
    /// 是否为协调者(PM)：作 lead 编排；无则合成 PM。
    pub is_coordinator: bool,
    pub sort: i64,
    /// 展示名/头像：快照成员（从团队导入）落库；散装成员留空，运行时按 expert_name 解析回填。
    pub display_name: Option<String>,
    pub avatar: Option<String>,
    // ---- 快照（方案C「项目私有副本」）：从团队导入时把成员定义复制进项目，与源团队解耦。
    //      system_prompt 非空 = 快照成员，展示与运行皆用本行，不再按名查 ExpertService。
    /// 专家描述（用于 ExpertSpec.description）。
    pub description: Option<String>,
    /// 正文（system prompt）。非空即视为快照成员。
    pub system_prompt: Option<String>,
    /// 工具白名单（受限 registry 用）。
    pub tools: Vec<String>,
    /// 模型档位：main|aux。
    pub model_tier: Option<String>,
    /// 来源团队 id（方案C 快照成员从该团队导入）：运行时据此软引用源 team 的私有 skill，
    /// 让团队成员被复制进项目后仍能用到它原本依赖的团队技能。散装成员留空。
    pub origin_team_id: Option<String>,
}

/// 项目级任务看板投影：本项目各线程下成员 dispatch 的一次 child 运行（不建表，运行时聚合）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectChildRun {
    pub session_id: String,
    pub thread_id: String,
    pub thread_title: String,
    pub expert_name: String,
    pub display_name: Option<String>,
    pub task: String,
    /// running | blocked | done | failed | cancelled
    pub status: String,
    pub artifact_count: usize,
}

/// 项目级产物投影：某成员任务登记的一个交付物（运行时聚合各 child 会话 artifacts）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectArtifact {
    pub path: String,
    pub title: String,
    pub session_id: String,
    pub expert_name: String,
    pub display_name: Option<String>,
    pub task: String,
}

/// T61 任务台账一项：编排线程的计划项；委派项关联 child run、状态由 run 派生。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectTask {
    pub id: String,
    /// 所属编排线程（群聊会话）。
    pub thread_session_id: String,
    /// 项目聚合用；团队线程为空。
    pub project_id: Option<String>,
    /// 父任务 id：空 = 主任务（本轮基调）；非空 = 该主任务下的子任务。
    pub parent_task_id: Option<String>,
    /// 主任务锚定的用户消息 id（本轮请求）；子任务为空，按 parent 归属。
    pub round_message_id: Option<String>,
    pub title: String,
    /// 委派成员 name；空 = PM 自办。
    pub assignee: Option<String>,
    /// pending | in_progress | done | failed（主任务状态由其子任务派生）
    pub status: String,
    /// 关联的 child run 会话（最近一次；重试更新）。
    pub run_session_id: Option<String>,
    pub sort: i64,
    pub created_at: String,
    pub updated_at: String,
}

/// `upsert_tasks` 的输入项：带 id=更新，无 id=新建。
#[derive(Debug, Clone)]
pub struct TaskInput {
    pub id: Option<String>,
    pub title: String,
    pub assignee: Option<String>,
    pub status: Option<String>,
}

pub fn ensure_schema(db: &AppDatabase) -> Result<(), String> {
    db.with_connection(|c| {
        c.execute_batch(
            "create table if not exists projects (
                id text primary key, name text not null, description text not null default '',
                workspace_dir text, permission_mode text not null default 'manual',
                instructions text not null default '',
                created_at text not null, updated_at text not null
            );
            create table if not exists project_members (
                id text primary key, project_id text not null, expert_name text not null,
                role_label text, responsibilities text, is_coordinator integer not null default 0,
                sort integer not null default 0,
                display_name text, avatar text, description text,
                system_prompt text, tools text, model_tier text, origin_team_id text
            );
            create index if not exists idx_pm_project on project_members(project_id, sort);
            create table if not exists project_tasks (
                id text primary key, thread_session_id text not null, project_id text,
                parent_task_id text, round_message_id text,
                title text not null, assignee text,
                status text not null default 'pending', run_session_id text,
                sort integer not null default 0,
                created_at text not null, updated_at text not null
            );
            create index if not exists idx_ptask_thread on project_tasks(thread_session_id, sort);
            create index if not exists idx_ptask_project on project_tasks(project_id);",
        )?;
        // T67 遗留修复：老库 project_members 列 `agent_name` → `expert_name`（T67 改了 SQL 但漏了本表迁移，
        // 导致 select expert_name 报「no such column」）。保数据改名；新库基表已是 expert_name，跳过。
        {
            let has_old: i64 = c.query_row(
                "select count(*) from pragma_table_info('project_members') where name = 'agent_name'",
                [],
                |r| r.get(0),
            )?;
            let has_new: i64 = c.query_row(
                "select count(*) from pragma_table_info('project_members') where name = 'expert_name'",
                [],
                |r| r.get(0),
            )?;
            if has_old > 0 && has_new == 0 {
                c.execute("alter table project_members rename column agent_name to expert_name", [])?;
            }
        }
        // 幂等迁移：老库 project_tasks 补 parent_task_id / round_message_id（主/子任务两级）。
        // 必须先补列、再建引用该列的索引——否则老库会在建索引时报「no such column」中断整批迁移。
        for col in ["parent_task_id", "round_message_id"] {
            let has: i64 = c.query_row(
                &format!("select count(*) from pragma_table_info('project_tasks') where name = '{col}'"),
                [],
                |r| r.get(0),
            )?;
            if has == 0 {
                c.execute(&format!("alter table project_tasks add column {col} text"), [])?;
            }
        }
        c.execute("create index if not exists idx_ptask_parent on project_tasks(parent_task_id)", [])?;
        // 幂等迁移：老库 project_members 补快照列（方案C 项目私有副本）。
        for col in [
            "display_name",
            "avatar",
            "description",
            "system_prompt",
            "tools",
            "model_tier",
            "origin_team_id",
        ] {
            let has: i64 = c.query_row(
                &format!("select count(*) from pragma_table_info('project_members') where name = '{col}'"),
                [],
                |r| r.get(0),
            )?;
            if has == 0 {
                c.execute(&format!("alter table project_members add column {col} text"), [])?;
            }
        }
        // 幂等迁移：老库 projects 补 permission_mode。
        let has: i64 = c.query_row(
            "select count(*) from pragma_table_info('projects') where name = 'permission_mode'",
            [],
            |r| r.get(0),
        )?;
        if has == 0 {
            c.execute(
                "alter table projects add column permission_mode text not null default 'manual'",
                [],
            )?;
        }
        let has_instr: i64 = c.query_row(
            "select count(*) from pragma_table_info('projects') where name = 'instructions'",
            [],
            |r| r.get(0),
        )?;
        if has_instr == 0 {
            c.execute(
                "alter table projects add column instructions text not null default ''",
                [],
            )?;
        }
        // T59：丢弃旧运行时的四张表（无生产数据；运行时已迁移到 session/engine）。
        for t in ["chat_messages", "tasks", "task_logs", "task_artifacts"] {
            c.execute(&format!("drop table if exists {t}"), [])?;
        }
        Ok(())
    })
    .map_err(|e| e.to_string())
}

fn now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
        .to_string()
}

const MEMBER_COLS: &str = "id, project_id, expert_name, role_label, responsibilities, is_coordinator, sort, display_name, avatar, description, system_prompt, tools, model_tier, origin_team_id";

fn row_to_member(r: &rusqlite::Row<'_>) -> rusqlite::Result<ProjectMember> {
    let coord: i64 = r.get(5)?;
    let tools: Option<String> = r.get(11)?;
    Ok(ProjectMember {
        id: r.get(0)?,
        project_id: r.get(1)?,
        expert_name: r.get(2)?,
        role_label: r.get(3)?,
        responsibilities: r.get(4)?,
        is_coordinator: coord != 0,
        sort: r.get(6)?,
        display_name: r.get(7)?,
        avatar: r.get(8)?,
        description: r.get(9)?,
        system_prompt: r.get(10)?,
        tools: tools
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        model_tier: r.get(12)?,
        origin_team_id: r.get(13)?,
    })
}

const TASK_COLS: &str = "id, thread_session_id, project_id, parent_task_id, round_message_id, title, assignee, status, run_session_id, sort, created_at, updated_at";

fn row_to_task(r: &rusqlite::Row<'_>) -> rusqlite::Result<ProjectTask> {
    Ok(ProjectTask {
        id: r.get(0)?,
        thread_session_id: r.get(1)?,
        project_id: r.get(2)?,
        parent_task_id: r.get(3)?,
        round_message_id: r.get(4)?,
        title: r.get(5)?,
        assignee: r.get(6)?,
        status: r.get(7)?,
        run_session_id: r.get(8)?,
        sort: r.get(9)?,
        created_at: r.get(10)?,
        updated_at: r.get(11)?,
    })
}

/// 项目服务：项目与成员的数据 CRUD。成员展示信息经 `ExpertService` 解析（注入）。
pub struct ProjectService {
    db: Arc<AppDatabase>,
    agents: Arc<crate::expert::ExpertService>,
}

impl ProjectService {
    pub fn new(db: Arc<AppDatabase>, agents: Arc<crate::expert::ExpertService>) -> Self {
        let _ = ensure_schema(&db);
        Self { db, agents }
    }

    // ---- projects ----
    pub fn create(
        &self,
        name: &str,
        description: &str,
        instructions: &str,
        workspace_dir: Option<&str>,
    ) -> Result<Project, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("项目名不能为空".into());
        }
        let now = now();
        let p = Project {
            id: new_id("project"),
            name: name.to_string(),
            description: description.trim().to_string(),
            workspace_dir: workspace_dir
                .map(|s| s.to_string())
                .filter(|s| !s.trim().is_empty()),
            permission_mode: "manual".to_string(),
            instructions: instructions.trim().to_string(),
            created_at: now.clone(),
            updated_at: now,
        };
        self.db
            .with_connection(|c| {
                c.execute(
                    "insert into projects (id, name, description, workspace_dir, permission_mode, instructions, created_at, updated_at) values (?1,?2,?3,?4,?5,?6,?7,?8)",
                    rusqlite::params![p.id, p.name, p.description, p.workspace_dir, p.permission_mode, p.instructions, p.created_at, p.updated_at],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())?;
        Ok(p)
    }

    pub fn list(&self) -> Result<Vec<Project>, String> {
        self.db
            .with_connection(|c| {
                let mut stmt = c.prepare("select id, name, description, workspace_dir, permission_mode, instructions, created_at, updated_at from projects order by updated_at desc, id desc")?;
                let rows = stmt.query_map([], |r| {
                    Ok(Project {
                        id: r.get(0)?, name: r.get(1)?, description: r.get(2)?,
                        workspace_dir: r.get(3)?, permission_mode: r.get(4)?, instructions: r.get(5)?, created_at: r.get(6)?, updated_at: r.get(7)?,
                    })
                })?;
                let mut out = Vec::new();
                for x in rows { out.push(x?); }
                Ok(out)
            })
            .map_err(|e| e.to_string())
    }

    pub fn get(&self, id: &str) -> Result<Option<Project>, String> {
        self.db
            .with_connection(|c| {
                let mut stmt = c.prepare("select id, name, description, workspace_dir, permission_mode, instructions, created_at, updated_at from projects where id = ?1")?;
                let mut rows = stmt.query_map([id], |r| {
                    Ok(Project {
                        id: r.get(0)?, name: r.get(1)?, description: r.get(2)?,
                        workspace_dir: r.get(3)?, permission_mode: r.get(4)?, instructions: r.get(5)?, created_at: r.get(6)?, updated_at: r.get(7)?,
                    })
                })?;
                Ok(match rows.next() { Some(r) => Some(r?), None => None })
            })
            .map_err(|e| e.to_string())
    }

    /// 更新项目名称与描述。
    pub fn update(&self, id: &str, name: &str, description: &str) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("项目名不能为空".into());
        }
        self.db
            .with_connection(|c| {
                c.execute(
                    "update projects set name = ?1, description = ?2, updated_at = ?3 where id = ?4",
                    rusqlite::params![name, description.trim(), now(), id],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    pub fn set_workspace(&self, id: &str, dir: &str) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                c.execute(
                    "update projects set workspace_dir = ?1, updated_at = ?2 where id = ?3",
                    rusqlite::params![dir, now(), id],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// 设项目指令（章程/PM 指令）——运行时合成为 lead 人格。
    pub fn set_instructions(&self, id: &str, instructions: &str) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                c.execute(
                    "update projects set instructions = ?1, updated_at = ?2 where id = ?3",
                    rusqlite::params![instructions, now(), id],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// 设项目权限模式（manual|auto|full）——线程及成员 child 默认继承。
    pub fn set_permission_mode(&self, id: &str, mode: &str) -> Result<(), String> {
        let mode = match mode {
            "manual" | "auto" | "full" => mode,
            _ => return Err(format!("非法权限模式：{mode}")),
        };
        self.db
            .with_connection(|c| {
                c.execute(
                    "update projects set permission_mode = ?1, updated_at = ?2 where id = ?3",
                    rusqlite::params![mode, now(), id],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    pub fn delete(&self, id: &str) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                c.execute(
                    "delete from project_members where project_id = ?1",
                    rusqlite::params![id],
                )?;
                c.execute("delete from projects where id = ?1", rusqlite::params![id])?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    // ---- members ----
    fn enrich_member(&self, m: &mut ProjectMember) {
        // 快照成员（项目私有副本）展示信息已落库，直接用，不再按名查 ExpertService。
        if Self::is_snapshot(m) {
            return;
        }
        // 散装成员：按名解析（散装→全局兜底），回填名称/头像/职业。
        if let Some(s) = self
            .agents
            .summary_by_owner("", "", &m.expert_name)
            .or_else(|| self.agents.summary_by_name(&m.expert_name))
        {
            // 职业(role_label) 未显式指定时回退到专家自身的 profession。
            if m.role_label
                .as_deref()
                .map(str::trim)
                .unwrap_or("")
                .is_empty()
            {
                m.role_label = s.profession;
            }
            m.display_name = s.display_name.or(Some(s.name));
            m.avatar = s.avatar;
        }
    }

    /// 是否为快照成员（system_prompt 非空 = 从团队导入的项目私有副本）。
    fn is_snapshot(m: &ProjectMember) -> bool {
        !m.system_prompt
            .as_deref()
            .map(str::trim)
            .unwrap_or("")
            .is_empty()
    }

    /// 按快照成员构造展示摘要（不查 ExpertService）。
    fn summary_from_snapshot(m: &ProjectMember) -> ExpertSummary {
        ExpertSummary {
            id: m.id.clone(),
            source: ExpertSource::User,
            name: m.expert_name.clone(),
            description: m.description.clone().unwrap_or_default(),
            tools: m.tools.clone(),
            model_tier: m.model_tier.clone().unwrap_or_else(|| "main".to_string()),
            max_turns: None,
            role: "member".to_string(),
            plugin_id: String::new(),
            team_id: String::new(),
            display_name: m.display_name.clone(),
            profession: m.role_label.clone(),
            avatar: m.avatar.clone(),
            color: None,
            enabled: true,
            installed_at: String::new(),
            catalog_id: None,
            group_id: None,
        }
    }

    /// 按快照成员构造角色定义（含正文，供子运行）。
    fn spec_from_snapshot(m: &ProjectMember) -> ExpertSpec {
        ExpertSpec {
            name: m.expert_name.clone(),
            description: m.description.clone().unwrap_or_default(),
            system_prompt: m.system_prompt.clone().unwrap_or_default(),
            tools: m.tools.clone(),
            model_tier: m.model_tier.clone().unwrap_or_else(|| "main".to_string()),
            max_turns: None,
            role: "member".to_string(),
            plugin_id: String::new(),
            team_id: String::new(),
        }
    }

    /// 解析一个项目成员的展示摘要：快照成员用本行，散装成员按名解析（散装→全局兜底）。
    pub fn member_summary(&self, m: &ProjectMember) -> Option<ExpertSummary> {
        if Self::is_snapshot(m) {
            return Some(Self::summary_from_snapshot(m));
        }
        self.agents
            .summary_by_owner("", "", &m.expert_name)
            .or_else(|| self.agents.summary_by_name(&m.expert_name))
    }

    /// 解析一个项目成员的角色定义：快照成员用本行，散装成员按名解析（散装→全局兜底）。
    pub fn member_spec(&self, m: &ProjectMember) -> Option<ExpertSpec> {
        if Self::is_snapshot(m) {
            return Some(Self::spec_from_snapshot(m));
        }
        self.agents
            .load_spec_by_owner("", "", &m.expert_name)
            .ok()
            .flatten()
            .or_else(|| self.agents.load_spec_by_name(&m.expert_name).ok().flatten())
    }

    pub fn add_member(
        &self,
        project_id: &str,
        expert_name: &str,
        role_label: Option<&str>,
        responsibilities: Option<&str>,
        is_coordinator: bool,
    ) -> Result<ProjectMember, String> {
        self.insert_member(ProjectMember {
            id: new_id("pmember"),
            project_id: project_id.to_string(),
            expert_name: expert_name.to_string(),
            role_label: role_label.map(|s| s.to_string()),
            responsibilities: responsibilities.map(|s| s.to_string()),
            is_coordinator,
            sort: self.list_members(project_id)?.len() as i64,
            display_name: None,
            avatar: None,
            description: None,
            system_prompt: None,
            tools: Vec::new(),
            model_tier: None,
            origin_team_id: None,
        })
    }

    /// 加入一个「项目私有副本」成员（方案C）：把完整定义快照落库，与源团队解耦。
    /// `origin_team_id` 记录来源团队：人设是硬拷贝，team 私有 skill 仍软引用源团队（运行时按它注入）。
    #[allow(clippy::too_many_arguments)]
    pub fn add_member_snapshot(
        &self,
        project_id: &str,
        expert_name: &str,
        display_name: Option<&str>,
        role_label: Option<&str>,
        avatar: Option<&str>,
        description: Option<&str>,
        system_prompt: &str,
        tools: Vec<String>,
        model_tier: &str,
        origin_team_id: Option<&str>,
    ) -> Result<ProjectMember, String> {
        self.insert_member(ProjectMember {
            id: new_id("pmember"),
            project_id: project_id.to_string(),
            expert_name: expert_name.to_string(),
            role_label: role_label.map(|s| s.to_string()),
            responsibilities: None,
            is_coordinator: false,
            sort: self.list_members(project_id)?.len() as i64,
            display_name: display_name.map(|s| s.to_string()),
            avatar: avatar.map(|s| s.to_string()),
            description: description.map(|s| s.to_string()),
            system_prompt: Some(system_prompt.to_string()),
            tools,
            model_tier: Some(model_tier.to_string()),
            origin_team_id: origin_team_id.map(|s| s.to_string()),
        })
    }

    /// 单成员的来源团队 id（快照成员；散装/无来源返回 None）。供子代理运行继承源 team 私有 skill。
    pub fn member_origin_team_id(
        &self,
        project_id: &str,
        expert_name: &str,
    ) -> Result<Option<String>, String> {
        Ok(self
            .list_members(project_id)?
            .into_iter()
            .find(|m| m.expert_name == expert_name)
            .and_then(|m| m.origin_team_id)
            .filter(|s| !s.trim().is_empty()))
    }

    /// 本项目全部成员涉及的来源团队 id（去重、非空）。供 PM/lead 运行注入各源 team 的私有 skill。
    pub fn origin_team_ids(&self, project_id: &str) -> Result<Vec<String>, String> {
        let mut ids: Vec<String> = self
            .list_members(project_id)?
            .into_iter()
            .filter_map(|m| m.origin_team_id)
            .filter(|s| !s.trim().is_empty())
            .collect();
        ids.sort();
        ids.dedup();
        Ok(ids)
    }

    /// 本项目全部成员的 agent name（去重、非空）。供运行时注入各成员的 agent 私有 skill
    /// （手动加进项目的散装专家若自带私有技能，PM/lead 编排层也能用到）。
    pub fn member_expert_names(&self, project_id: &str) -> Result<Vec<String>, String> {
        let mut names: Vec<String> = self
            .list_members(project_id)?
            .into_iter()
            .map(|m| m.expert_name)
            .filter(|s| !s.trim().is_empty())
            .collect();
        names.sort();
        names.dedup();
        Ok(names)
    }

    fn insert_member(&self, mut m: ProjectMember) -> Result<ProjectMember, String> {
        let tools = m.tools.join(",");
        self.db
            .with_connection(|c| {
                c.execute(
                    "insert into project_members (id, project_id, expert_name, role_label, responsibilities, is_coordinator, sort, display_name, avatar, description, system_prompt, tools, model_tier, origin_team_id) values (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
                    rusqlite::params![m.id, m.project_id, m.expert_name, m.role_label, m.responsibilities, m.is_coordinator as i64, m.sort, m.display_name, m.avatar, m.description, m.system_prompt, tools, m.model_tier, m.origin_team_id],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())?;
        self.enrich_member(&mut m);
        Ok(m)
    }

    pub fn list_members(&self, project_id: &str) -> Result<Vec<ProjectMember>, String> {
        let mut out = self
            .db
            .with_connection(|c| {
                let mut stmt = c.prepare(&format!("select {MEMBER_COLS} from project_members where project_id = ?1 order by sort, id"))?;
                let rows = stmt.query_map([project_id], row_to_member)?;
                let mut v = Vec::new();
                for x in rows { v.push(x?); }
                Ok(v)
            })
            .map_err(|e| e.to_string())?;
        for m in &mut out {
            self.enrich_member(m);
        }
        Ok(out)
    }

    pub fn get_member(&self, member_id: &str) -> Result<Option<ProjectMember>, String> {
        Ok(self
            .db
            .with_connection(|c| {
                let mut stmt = c.prepare(&format!(
                    "select {MEMBER_COLS} from project_members where id = ?1"
                ))?;
                let mut rows = stmt.query_map([member_id], row_to_member)?;
                match rows.next() {
                    Some(r) => Ok(Some(r?)),
                    None => Ok(None),
                }
            })
            .map_err(|e| e.to_string())?
            .map(|mut m| {
                self.enrich_member(&mut m);
                m
            }))
    }

    /// 按 (project_id, expert_name) 取成员（供引擎按名解析项目成员）。
    pub fn get_member_by_name(
        &self,
        project_id: &str,
        name: &str,
    ) -> Result<Option<ProjectMember>, String> {
        Ok(self
            .db
            .with_connection(|c| {
                let mut stmt = c.prepare(&format!("select {MEMBER_COLS} from project_members where project_id = ?1 and expert_name = ?2"))?;
                let mut rows = stmt.query_map(rusqlite::params![project_id, name], row_to_member)?;
                match rows.next() {
                    Some(r) => Ok(Some(r?)),
                    None => Ok(None),
                }
            })
            .map_err(|e| e.to_string())?
            .map(|mut m| { self.enrich_member(&mut m); m }))
    }

    pub fn remove_member(&self, member_id: &str) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                c.execute(
                    "delete from project_members where id = ?1",
                    rusqlite::params![member_id],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    // ---- T61 任务台账（主任务/子任务两级）----

    /// 主任务状态由其子任务派生：任一进行中→进行中；否则任一失败→失败；否则全部完成→完成；否则待办。
    /// 无子任务的主任务保留自身状态。
    fn derive_main_status(tasks: &mut [ProjectTask]) {
        use std::collections::HashMap;
        let mut by_parent: HashMap<String, Vec<String>> = HashMap::new();
        for t in tasks.iter() {
            if let Some(p) = &t.parent_task_id {
                by_parent
                    .entry(p.clone())
                    .or_default()
                    .push(t.status.clone());
            }
        }
        for t in tasks.iter_mut() {
            if t.parent_task_id.is_none() {
                if let Some(cs) = by_parent.get(&t.id).filter(|cs| !cs.is_empty()) {
                    t.status = if cs.iter().any(|s| s == "in_progress") {
                        "in_progress"
                    } else if cs.iter().any(|s| s == "failed") {
                        "failed"
                    } else if cs.iter().all(|s| s == "done") {
                        "done"
                    } else if cs.iter().any(|s| s == "cancelled") {
                        // 有取消、且无进行中/失败、又非全完成 → 本轮被取消（含部分已完成）。
                        "cancelled"
                    } else {
                        "pending"
                    }
                    .to_string();
                }
            }
        }
    }

    fn query_tasks(&self, where_col: &str, val: &str) -> Result<Vec<ProjectTask>, String> {
        self.db
            .with_connection(|c| {
                let mut stmt = c.prepare(&format!(
                    "select {TASK_COLS} from project_tasks where {where_col} = ?1 order by sort, id"
                ))?;
                let rows = stmt.query_map([val], row_to_task)?;
                let mut v = Vec::new();
                for x in rows {
                    v.push(x?);
                }
                Ok(v)
            })
            .map_err(|e| e.to_string())
    }

    /// 列某编排线程的全部任务（主+子），主任务状态已按子任务派生。
    pub fn list_tasks(&self, thread_session_id: &str) -> Result<Vec<ProjectTask>, String> {
        let mut v = self.query_tasks("thread_session_id", thread_session_id)?;
        Self::derive_main_status(&mut v);
        Ok(v)
    }

    /// 项目级聚合：跨该项目所有线程的任务（主任务状态已派生）。
    pub fn tasks_by_project(&self, project_id: &str) -> Result<Vec<ProjectTask>, String> {
        let mut v = self.query_tasks("project_id", project_id)?;
        Self::derive_main_status(&mut v);
        Ok(v)
    }

    /// 智能体级聚合：跨该智能体直接激活的所有顶层会话读取任务台账。
    pub fn tasks_by_agent(&self, agent_id: &str) -> Result<Vec<ProjectTask>, String> {
        self.db
            .with_connection(|c| {
                let mut stmt = c.prepare(&format!(
                    "select {TASK_COLS}
                       from project_tasks
                      where thread_session_id in (
                            select id
                              from sessions
                             where agent_id = ?1
                               and parent_session_id is null
                       )
                      order by thread_session_id, sort, id"
                ))?;
                let rows = stmt.query_map([agent_id], row_to_task)?;
                let mut out = Vec::new();
                for row in rows {
                    out.push(row?);
                }
                Ok(out)
            })
            .map(|mut v| {
                Self::derive_main_status(&mut v);
                v
            })
            .map_err(|e| e.to_string())
    }

    pub fn get_task(&self, id: &str) -> Result<Option<ProjectTask>, String> {
        self.db
            .with_connection(|c| {
                let mut stmt = c.prepare(&format!(
                    "select {TASK_COLS} from project_tasks where id = ?1"
                ))?;
                let mut rows = stmt.query_map([id], row_to_task)?;
                Ok(match rows.next() {
                    Some(r) => Some(r?),
                    None => None,
                })
            })
            .map_err(|e| e.to_string())
    }

    /// 维护本轮（round_message_id 锚定的用户请求）的任务台账（`update_tasks` 工具用）：
    /// - 先确保该轮的**主任务**存在（goal 作标题；缺省"本轮任务"）；后续同轮调用更新其标题。
    /// - `items` 作该主任务下的**子任务**全量覆写：带 id→更新；无 id 且 title 命中已有子任务→更新（去重兜底）；
    ///   否则新建。已有 run 的子任务不改 status/run、且不被移除（保护执行记录）。
    /// - 同轮、未被保留/命中、且无 run 的子任务 → 删除。其它轮（历史）不受影响。
    /// 返回该线程全部任务（含派生主状态、新建 id），供 PM 引用。
    pub fn upsert_tasks(
        &self,
        thread_session_id: &str,
        project_id: Option<&str>,
        round_message_id: &str,
        goal: &str,
        items: &[TaskInput],
    ) -> Result<Vec<ProjectTask>, String> {
        let now = now();
        let goal = goal.trim();
        let existing = self.query_tasks("thread_session_id", thread_session_id)?;

        // 1. 确保本轮主任务存在。
        let main = existing.iter().find(|t| {
            t.parent_task_id.is_none() && t.round_message_id.as_deref() == Some(round_message_id)
        });
        let main_id = match main {
            Some(m) => {
                if !goal.is_empty() && goal != m.title {
                    let id = m.id.clone();
                    self.db
                        .with_connection(|c| {
                            c.execute(
                                "update project_tasks set title = ?1, updated_at = ?2 where id = ?3",
                                rusqlite::params![goal, now, id],
                            )?;
                            Ok(())
                        })
                        .map_err(|e| e.to_string())?;
                }
                m.id.clone()
            }
            None => {
                let id = new_id("ptask");
                let title = if goal.is_empty() {
                    "本轮任务".to_string()
                } else {
                    goal.to_string()
                };
                let sort = existing
                    .iter()
                    .filter(|t| t.parent_task_id.is_none())
                    .count() as i64;
                let (id_c, t_id, t_pid, t_round, t_title) = (
                    id.clone(),
                    thread_session_id.to_string(),
                    project_id.map(|s| s.to_string()),
                    round_message_id.to_string(),
                    title,
                );
                self.db
                    .with_connection(|c| {
                        c.execute(
                            "insert into project_tasks (id, thread_session_id, project_id, parent_task_id, round_message_id, title, assignee, status, run_session_id, sort, created_at, updated_at) values (?1,?2,?3,NULL,?4,?5,NULL,'pending',NULL,?6,?7,?7)",
                            rusqlite::params![id_c, t_id, t_pid, t_round, t_title, sort, now],
                        )?;
                        Ok(())
                    })
                    .map_err(|e| e.to_string())?;
                id
            }
        };

        // 2. 覆写本主任务下的子任务。
        let subs: Vec<&ProjectTask> = existing
            .iter()
            .filter(|t| t.parent_task_id.as_deref() == Some(main_id.as_str()))
            .collect();
        let keep_ids: std::collections::HashSet<&str> =
            items.iter().filter_map(|i| i.id.as_deref()).collect();
        let keep_titles: std::collections::HashSet<&str> = items
            .iter()
            .filter(|i| i.id.is_none())
            .map(|i| i.title.trim())
            .collect();
        self.db
            .with_connection(|c| {
                // 删除：本主任务下、未被 id/标题保留、且无 run 的子任务。
                for s in &subs {
                    let kept = keep_ids.contains(s.id.as_str())
                        || keep_titles.contains(s.title.as_str())
                        || s.run_session_id.is_some();
                    if !kept {
                        c.execute("delete from project_tasks where id = ?1", rusqlite::params![s.id])?;
                    }
                }
                for (idx, it) in items.iter().enumerate() {
                    let sort = idx as i64;
                    let title = it.title.trim();
                    // 目标:显式 id;否则按标题命中本主任务下已有子任务(去重兜底)。
                    let target_id = it
                        .id
                        .clone()
                        .or_else(|| subs.iter().find(|s| s.title == title).map(|s| s.id.clone()));
                    match target_id {
                        Some(id) => {
                            let has_run = subs.iter().find(|s| s.id == id).map(|s| s.run_session_id.is_some()).unwrap_or(false);
                            if has_run {
                                c.execute(
                                    "update project_tasks set title = ?1, assignee = ?2, sort = ?3, updated_at = ?4 where id = ?5",
                                    rusqlite::params![title, it.assignee, sort, now, id],
                                )?;
                            } else {
                                let status = it.status.clone().unwrap_or_else(|| "pending".to_string());
                                c.execute(
                                    "update project_tasks set title = ?1, assignee = ?2, status = ?3, sort = ?4, updated_at = ?5 where id = ?6",
                                    rusqlite::params![title, it.assignee, status, sort, now, id],
                                )?;
                            }
                        }
                        None => {
                            let status = it.status.clone().unwrap_or_else(|| "pending".to_string());
                            c.execute(
                                "insert into project_tasks (id, thread_session_id, project_id, parent_task_id, round_message_id, title, assignee, status, run_session_id, sort, created_at, updated_at) values (?1,?2,?3,?4,NULL,?5,?6,?7,NULL,?8,?9,?9)",
                                rusqlite::params![new_id("ptask"), thread_session_id, project_id, main_id, title, it.assignee, status, sort, now],
                            )?;
                        }
                    }
                }
                Ok(())
            })
            .map_err(|e| e.to_string())?;
        self.list_tasks(thread_session_id)
    }

    /// 派发时关联 run：置 run_session_id + status=in_progress；assignee 空则回填被派成员。
    pub fn set_task_run(
        &self,
        task_id: &str,
        run_session_id: &str,
        assignee_fallback: &str,
    ) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                c.execute(
                    "update project_tasks set run_session_id = ?1, status = 'in_progress', assignee = coalesce(nullif(assignee,''), ?2), updated_at = ?3 where id = ?4",
                    rusqlite::params![run_session_id, assignee_fallback, now(), task_id],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// run 终态联动：按 run 反查任务并置状态（done|failed）。
    pub fn set_task_status_by_run(&self, run_session_id: &str, status: &str) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                c.execute(
                    "update project_tasks set status = ?1, updated_at = ?2 where run_session_id = ?3",
                    rusqlite::params![status, now(), run_session_id],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// 停止会话时：把该线程下未结束（pending/in_progress）的任务标为已取消（含 PM 自办、未派发子任务、
    /// 主任务）。已完成/失败的保留不动。
    pub fn cancel_pending_tasks(&self, thread_session_id: &str) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                c.execute(
                    "update project_tasks set status = 'cancelled', updated_at = ?1 where thread_session_id = ?2 and status in ('pending','in_progress')",
                    rusqlite::params![now(), thread_session_id],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// 重试：把指向旧 run 的任务改指新 run + status=in_progress。
    pub fn reassign_task_run(&self, old_run: &str, new_run: &str) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                c.execute(
                    "update project_tasks set run_session_id = ?1, status = 'in_progress', updated_at = ?2 where run_session_id = ?3",
                    rusqlite::params![new_run, now(), old_run],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn svc() -> ProjectService {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let dbp =
            std::env::temp_dir().join(format!("siw-proj-{}-{}.db", std::process::id(), nanos));
        let _ = std::fs::remove_file(&dbp);
        let root =
            std::env::temp_dir().join(format!("siw-proj-root-{}-{}", std::process::id(), nanos));
        let db = Arc::new(AppDatabase::open(&dbp).unwrap());
        let agents = Arc::new(crate::expert::ExpertService::new(db.clone(), root));
        ProjectService::new(db, agents)
    }

    #[test]
    fn project_member_crud_and_permission_mode() {
        let s = svc();
        let p = s.create("研发项目", "做个 App", "", None).expect("create");
        assert_eq!(p.permission_mode, "manual");
        assert_eq!(s.list().unwrap().len(), 1);
        let m = s
            .add_member(&p.id, "investor", Some("投研"), Some("做投资分析"), true)
            .expect("add member");
        assert!(m.is_coordinator);
        assert_eq!(s.list_members(&p.id).unwrap().len(), 1);
        assert_eq!(
            s.get_member(&m.id).unwrap().unwrap().expert_name,
            "investor"
        );
        s.set_permission_mode(&p.id, "full").expect("set mode");
        assert_eq!(s.get(&p.id).unwrap().unwrap().permission_mode, "full");
        assert!(s.set_permission_mode(&p.id, "bogus").is_err());
        s.remove_member(&m.id).expect("remove member");
        assert!(s.list_members(&p.id).unwrap().is_empty());
        s.delete(&p.id).expect("del");
        assert!(s.list().unwrap().is_empty());
    }

    #[test]
    fn migrates_legacy_project_members_agent_name_to_expert_name() {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dbp =
            std::env::temp_dir().join(format!("siw-proj-mig-{}-{}.db", std::process::id(), nanos));
        let _ = std::fs::remove_file(&dbp);
        let root = std::env::temp_dir().join(format!(
            "siw-proj-mig-root-{}-{}",
            std::process::id(),
            nanos
        ));
        let db = Arc::new(AppDatabase::open(&dbp).unwrap());
        // 造 T67 之前的老库：project_members 用列名 agent_name + 一行数据。
        db.with_connection(|c| {
            c.execute_batch(
                "create table project_members (
                    id text primary key, project_id text not null, agent_name text not null,
                    role_label text, responsibilities text, is_coordinator integer not null default 0,
                    sort integer not null default 0
                 );
                 insert into project_members (id, project_id, agent_name, is_coordinator, sort)
                   values ('m1','p1','investor',1,0);",
            )?;
            Ok(())
        })
        .unwrap();
        // 构造 ProjectService → ensure_schema 迁移 agent_name→expert_name（且补快照列），不再「no such column」。
        let experts = Arc::new(crate::expert::ExpertService::new(db.clone(), root));
        let svc = ProjectService::new(db, experts);
        let members = svc
            .list_members("p1")
            .expect("list_members 不应报 no such column");
        assert_eq!(members.len(), 1);
        assert_eq!(
            members[0].expert_name, "investor",
            "老 agent_name 数据应保留为 expert_name"
        );
    }

    #[test]
    fn project_task_ledger_crud() {
        let s = svc();
        let p = s.create("内容项目", "", "", None).expect("create");
        let thread = "thread-1";
        // 第一轮：主任务 + 两条子任务（无 id）。
        let t = s
            .upsert_tasks(
                thread,
                Some(&p.id),
                "msg-1",
                "本轮：写稿",
                &[
                    TaskInput {
                        id: None,
                        title: "写稿".into(),
                        assignee: Some("writer".into()),
                        status: None,
                    },
                    TaskInput {
                        id: None,
                        title: "汇总".into(),
                        assignee: None,
                        status: None,
                    },
                ],
            )
            .expect("upsert");
        // 1 主 + 2 子。
        assert_eq!(t.len(), 3);
        let main = t.iter().find(|x| x.parent_task_id.is_none()).expect("main");
        assert_eq!(main.title, "本轮：写稿");
        assert_eq!(main.round_message_id.as_deref(), Some("msg-1"));
        let subs: Vec<&ProjectTask> = t
            .iter()
            .filter(|x| x.parent_task_id.as_deref() == Some(main.id.as_str()))
            .collect();
        assert_eq!(subs.len(), 2);
        let write_id = subs.iter().find(|x| x.title == "写稿").unwrap().id.clone();

        // 派发关联 run → 子任务进行中；主任务派生为进行中。
        s.set_task_run(&write_id, "child-1", "writer")
            .expect("set run");
        assert_eq!(
            s.get_task(&write_id).unwrap().unwrap().status,
            "in_progress"
        );
        let listed = s.list_tasks(thread).unwrap();
        let main_now = listed.iter().find(|x| x.parent_task_id.is_none()).unwrap();
        assert_eq!(main_now.status, "in_progress");

        // 同轮去重兜底：无 id 但标题命中"写稿"→更新而非重复；有 run 不被删。
        let t3 = s
            .upsert_tasks(
                thread,
                Some(&p.id),
                "msg-1",
                "本轮：写稿",
                &[TaskInput {
                    id: None,
                    title: "写稿".into(),
                    assignee: Some("writer".into()),
                    status: None,
                }],
            )
            .expect("upsert2");
        // 仍是 1 主 + 1 子（"汇总"无 run 被删；"写稿"有 run 保留、不重复）。
        let subs3: Vec<&ProjectTask> = t3.iter().filter(|x| x.parent_task_id.is_some()).collect();
        assert_eq!(subs3.len(), 1);
        assert_eq!(subs3[0].id, write_id);
        assert_eq!(subs3[0].run_session_id.as_deref(), Some("child-1"));

        // run 终态联动 + 主任务派生完成。
        s.set_task_status_by_run("child-1", "done")
            .expect("status by run");
        let listed2 = s.list_tasks(thread).unwrap();
        assert_eq!(
            listed2
                .iter()
                .find(|x| x.parent_task_id.is_none())
                .unwrap()
                .status,
            "done"
        );

        // 第二轮（历史保留）：新主任务，旧轮不受影响。
        let t4 = s
            .upsert_tasks(
                thread,
                Some(&p.id),
                "msg-2",
                "本轮：配图",
                &[TaskInput {
                    id: None,
                    title: "配图".into(),
                    assignee: Some("designer".into()),
                    status: None,
                }],
            )
            .expect("upsert round2");
        let mains: Vec<&ProjectTask> = t4.iter().filter(|x| x.parent_task_id.is_none()).collect();
        assert_eq!(mains.len(), 2); // 两轮主任务都在（历史）
    }
}

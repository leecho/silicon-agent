//! TeamService：team 聚合面向 command 与 runtime 的受控入口。
//!
//! 持有 db（teams 索引）与 `Arc<ExpertService>`（解析成员引用 → ExpertSummary/ExpertSpec）。
//! team 引用 agent/skill 货币；删除 team 时级联其私有组件（skills/agents 表的 `team_id`）。

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use crate::expert::{ExpertService, ExpertSpec, ExpertSummary};
use crate::session::new_id;
use crate::skill::model::{SkillRecord, SkillSource};
use crate::skill::{frontmatter as skill_fm, store as skill_store};
use crate::storage::AppDatabase;
use crate::team::import;
use crate::team::model::{TeamMember, TeamRecord, TeamSource};
use crate::team::store;
use crate::team::types::{TeamDetail, TeamSummary};

/// 内联专家定义（AI 创建团队时，主理人/成员的现场设定）；落成该 team 的私有 agent。
/// 也作广场团队详情的成员展示载体：序列化只暴露轻量字段（system_prompt/tools/model_tier 不下发）。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InlineExpert {
    pub name: String,
    pub description: String,
    #[serde(skip)]
    pub system_prompt: String,
    #[serde(skip)]
    pub tools: Vec<String>,
    #[serde(skip)]
    pub model_tier: String,
    pub display_name: Option<String>,
    pub profession: Option<String>,
}

pub struct TeamService {
    db: Arc<AppDatabase>,
    agents: Arc<ExpertService>,
    /// 导入的团队包受管根目录（{workspace_base}/teams）；其私有 agent/skill 文件存绝对路径。
    root: PathBuf,
}

impl TeamService {
    /// 构造服务并确保 teams 索引表存在。
    pub fn new(db: Arc<AppDatabase>, agents: Arc<ExpertService>, root: PathBuf) -> Self {
        let _ = store::ensure_schema(&db);
        Self { db, agents, root }
    }

    fn now() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or_default()
            .to_string()
    }

    /// 把一个成员引用解析成 ExpertSummary（展示覆盖优先，回退被引用 agent 记录）；解析不到返回 None。
    fn resolve_summary(&self, m: &TeamMember) -> Option<ExpertSummary> {
        let mut s = self
            .agents
            .summary_by_owner(&m.plugin_id, &m.team_id, &m.name)?;
        if m.display_name.is_some() {
            s.display_name = m.display_name.clone();
        }
        if m.profession.is_some() {
            s.profession = m.profession.clone();
        }
        if m.avatar.is_some() {
            s.avatar = m.avatar.clone();
        }
        Some(s)
    }

    /// 列出全部团队。
    pub fn list(&self) -> Result<Vec<TeamSummary>, String> {
        Ok(store::list(&self.db)?
            .iter()
            .map(TeamSummary::from_record)
            .collect())
    }

    /// 列出启用团队（供 composer 角色槽）。
    pub fn list_enabled(&self) -> Result<Vec<TeamSummary>, String> {
        Ok(store::list(&self.db)?
            .iter()
            .filter(|r| r.enabled)
            .map(TeamSummary::from_record)
            .collect())
    }

    /// 团队详情：元数据 + 解析后的 lead/成员 + 开场引导语。
    pub fn detail(&self, id: &str) -> Result<TeamDetail, String> {
        let r = store::get_by_id(&self.db, id)?.ok_or_else(|| format!("团队不存在：{id}"))?;
        let lead = r.lead.as_ref().and_then(|m| self.resolve_summary(m));
        let members = r
            .members
            .iter()
            .filter_map(|m| self.resolve_summary(m))
            .collect();
        Ok(TeamDetail {
            team: TeamSummary::from_record(&r),
            lead,
            members,
            quick_prompts: r.quick_prompts.clone(),
            skills: Vec::new(), // 由命令层按 owner=team id 填充
        })
    }

    /// 新建用户团队（source=user）。lead 可空；members 为对 agent 的引用。
    pub fn create(
        &self,
        name: &str,
        display_name: &str,
        description: &str,
        lead: Option<TeamMember>,
        members: Vec<TeamMember>,
    ) -> Result<TeamSummary, String> {
        if name.trim().is_empty() {
            return Err("团队 name 不能为空".into());
        }
        if store::get_by_name(&self.db, name)?.is_some() {
            return Err("团队名已存在".into());
        }
        let now = Self::now();
        let rec = TeamRecord {
            id: new_id("team"),
            source: TeamSource::User,
            name: name.to_string(),
            display_name: display_name.to_string(),
            description: description.to_string(),
            lead,
            members,
            avatar: None,
            category: None,
            quick_prompts: Vec::new(),
            enabled: true,
            installed_at: now.clone(),
            updated_at: now,
            catalog_id: None,
            group_id: None,
        };
        store::upsert(&self.db, &rec)?;
        let r = store::get_by_name(&self.db, name)?.ok_or("创建后读取团队失败")?;
        Ok(TeamSummary::from_record(&r))
    }

    /// 设置团队的「我的」分组（None=移出）。
    pub fn set_group(&self, id: &str, group_id: Option<&str>) -> Result<(), String> {
        store::set_group(&self.db, id, group_id)
    }

    /// 把某分组下团队全部归零（删除分组时调用）。
    pub fn clear_group(&self, group_id: &str) -> Result<(), String> {
        store::clear_group(&self.db, group_id)
    }

    /// 切换团队启用状态。
    pub fn toggle(&self, id: &str, enabled: bool) -> Result<TeamSummary, String> {
        store::set_enabled(&self.db, id, enabled, &Self::now())?;
        let r = store::get_by_id(&self.db, id)?.ok_or_else(|| format!("团队不存在：{id}"))?;
        Ok(TeamSummary::from_record(&r))
    }

    /// 删除团队：先级联删其私有组件（skills/agents 表的 team_id），再删 team 行。内置拒绝。
    pub fn delete(&self, id: &str) -> Result<(), String> {
        let r = store::get_by_id(&self.db, id)?.ok_or_else(|| format!("团队不存在：{id}"))?;
        if r.source == TeamSource::Builtin {
            return Err("内置团队不可删除".into());
        }
        crate::expert::store::delete_by_team(&self.db, id)?;
        crate::skill::store::delete_by_team(&self.db, id)?;
        // 导入的团队：删受管目录（按 name）。自建团队无目录，忽略。
        let dir = self.root.join(&r.name);
        if dir.exists() {
            let _ = std::fs::remove_dir_all(&dir);
        }
        store::delete(&self.db, id)
    }

    /// AI 创建团队：用现场设定的 lead/members 现造该 team 的**私有** agent（写进受管 teams/{name}/agents/
    /// 并索引），再落成 team。同名团队已存在则报错。
    pub fn create_with_members(
        &self,
        name: &str,
        display_name: &str,
        description: &str,
        lead: Option<InlineExpert>,
        members: Vec<InlineExpert>,
        quick_prompts: Vec<String>,
        catalog_id: Option<String>,
    ) -> Result<TeamSummary, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("团队 name 不能为空".into());
        }
        if name.contains('/') || name.contains('\\') || name.contains("..") {
            return Err("团队 name 含非法字符".into());
        }
        if lead.is_none() && members.is_empty() {
            return Err("团队至少要有一名成员或主理人".into());
        }
        if store::get_by_name(&self.db, name)?.is_some() {
            return Err("团队名已存在".into());
        }
        let dest = self.root.join(name);
        if dest.exists() {
            return Err("团队目录已存在".into());
        }
        let experts_dir = dest.join("agents");
        std::fs::create_dir_all(&experts_dir).map_err(|e| format!("创建团队目录失败：{e}"))?;

        let team_id = new_id("team");
        let now = Self::now();
        let write_index = |spec: &InlineExpert, role: &str| -> Result<TeamMember, String> {
            // 成员 `name` 会成为磁盘文件名 `<name>.md`：拒绝路径穿越（坏成员名不能安全落盘 → 整团失败）。
            if !crate::market::wire::is_safe_component(&spec.name) {
                return Err(format!("团队成员 name 含非法字符（疑路径穿越）：{}", spec.name));
            }
            let model_tier = if spec.model_tier == "main" {
                "main"
            } else {
                "aux"
            };
            let md = crate::expert::service::serialize_expert_md(
                &spec.name,
                &spec.description,
                &spec.system_prompt,
                &spec.tools,
                model_tier,
                spec.display_name.as_deref(),
                spec.profession.as_deref(),
                None,
                &[],
                None,
            );
            let file = experts_dir.join(format!("{}.md", spec.name));
            std::fs::write(&file, md).map_err(|e| format!("写 agent 文件失败：{e}"))?;
            self.agents.index_team_expert(&team_id, &file, &now)?;
            Ok(TeamMember {
                plugin_id: String::new(),
                team_id: team_id.clone(),
                name: spec.name.clone(),
                role: role.to_string(),
                display_name: spec.display_name.clone(),
                profession: spec.profession.clone(),
                avatar: None,
            })
        };
        let lead_member = match &lead {
            Some(l) => Some(write_index(l, "lead")?),
            None => None,
        };
        let mut member_refs = Vec::new();
        for m in &members {
            member_refs.push(write_index(m, "member")?);
        }

        let rec = TeamRecord {
            id: team_id,
            source: TeamSource::User,
            name: name.to_string(),
            display_name: display_name.trim().to_string(),
            description: description.trim().to_string(),
            lead: lead_member,
            members: member_refs,
            avatar: None,
            category: None,
            quick_prompts,
            enabled: true,
            installed_at: now.clone(),
            updated_at: now,
            catalog_id,
            group_id: None,
        };
        store::upsert(&self.db, &rec)?;
        let r = store::get_by_name(&self.db, name)?.ok_or("创建后读取团队失败")?;
        Ok(TeamSummary::from_record(&r))
    }

    /// 把广场携带的 skill 物化为该团队的私有 skill：
    /// 文件写到 `{root}/<team_name>/skills/<skill.name>/<relpath>`，再索引为 owner=team_id。
    /// 单个 skill 失败仅 log 跳过，不阻断「加入我的」。
    pub fn attach_private_skills(
        &self,
        team_id: &str,
        team_name: &str,
        skills: Vec<crate::market::MaterializedSkill>,
    ) -> Result<(), String> {
        if skills.is_empty() {
            return Ok(());
        }
        let now = Self::now();
        for sk in skills {
            // 不可信的远端 skill `name` 会成为磁盘目录名：跳过路径穿越者（与写失败同样软跳过）。
            if !crate::market::wire::is_safe_component(&sk.name) {
                eprintln!(
                    "[catalog-team] {team_name}: 跳过 skill {}（非法名，疑路径穿越）",
                    sk.name
                );
                continue;
            }
            let skill_dir = self.root.join(team_name).join("skills").join(&sk.name);
            if let Err(e) = crate::expert::service::write_skill_files(&skill_dir, &sk.files) {
                eprintln!(
                    "[catalog-team] {team_name}: 跳过 skill {}（写文件 {e}）",
                    sk.name
                );
                continue;
            }
            let rec = SkillRecord {
                id: new_id("skill"),
                source: SkillSource::User,
                name: sk.name.clone(),
                description: sk.description,
                dir_name: skill_dir.to_string_lossy().into_owned(),
                enabled: true,
                installed_at: now.clone(),
                updated_at: now.clone(),
                plugin_id: None,
                team_id: Some(team_id.to_string()),
                expert_id: None,
                user_invocable: sk.user_invocable,
                argument_hint: sk.argument_hint,
                group_id: None,
            };
            if let Err(e) = skill_store::upsert(&self.db, &rec) {
                eprintln!(
                    "[catalog-team] {team_name}: 索引 skill {} 失败 {e}",
                    sk.name
                );
            }
        }
        Ok(())
    }

    /// 从本地目录导入一个**团队结构**的包（原生 / codebuddy / Claude 方言）→ 落成 team，
    /// 其 `agents/` 与 `skills/` 复制进受管 teams 根并索引为该 team 的**私有**组件。同名团队已存在则报错。
    /// `source` 决定来源标记：团队页直接导入 → `Imported`；经统一装载入口由 plugin 包运送 → `Plugin`。
    pub fn import_from_path(&self, path: &str, source: TeamSource) -> Result<TeamSummary, String> {
        // zip → 解压定位根；目录 → 直接定位。守卫(_guard)存活到 copy 完成。
        let (pkg_root, _guard) = import::stage_source(path)?;
        let m = import::parse_team_package(&pkg_root)?;
        if m.name.contains('/') || m.name.contains('\\') || m.name.contains("..") {
            return Err("团队 name 含非法字符".into());
        }
        if store::get_by_name(&self.db, &m.name)?.is_some() {
            return Err("团队名已存在".into());
        }
        std::fs::create_dir_all(&self.root).map_err(|e| format!("创建 teams 目录失败：{e}"))?;
        let dest = self.root.join(&m.name);
        if dest.exists() {
            return Err("团队目录已存在".into());
        }
        import::copy_dir_all(&pkg_root, &dest)?;

        let team_id = new_id("team");
        let now = Self::now();
        // 索引私有 agent（team_id 命名空间，file_name 存绝对路径）。
        let mut indexed: HashSet<String> = HashSet::new();
        for rel in &m.agents {
            let abs = dest.join(rel);
            match self.agents.index_team_expert(&team_id, &abs, &now) {
                Ok(name) => {
                    indexed.insert(name);
                }
                Err(e) => eprintln!("[team-import] {}: 跳过 agent {rel}（{e}）", m.name),
            }
        }
        // 索引私有 skill（team_id 命名空间，dir_name 存绝对路径）。
        for rel in &m.skills {
            let skill_dir = dest.join(rel);
            let md = skill_dir.join("SKILL.md");
            let content = match std::fs::read_to_string(&md) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!(
                        "[team-import] {}: 跳过 skill {rel}（读 SKILL.md {e}）",
                        m.name
                    );
                    continue;
                }
            };
            let fm = match skill_fm::parse_frontmatter(&content) {
                Ok(fm) => fm,
                Err(e) => {
                    eprintln!(
                        "[team-import] {}: 跳过 skill {rel}（frontmatter {e}）",
                        m.name
                    );
                    continue;
                }
            };
            let rec = SkillRecord {
                id: new_id("skill"),
                source: SkillSource::User,
                name: fm.name,
                description: fm.description,
                dir_name: skill_dir.to_string_lossy().into_owned(),
                enabled: true,
                installed_at: now.clone(),
                updated_at: now.clone(),
                plugin_id: None,
                team_id: Some(team_id.clone()),
                expert_id: None,
                user_invocable: fm.user_invocable,
                argument_hint: fm.argument_hint,
                group_id: None,
            };
            let _ = skill_store::upsert(&self.db, &rec);
        }

        // 组装 lead/members 引用（仅对实际索引成功的 agent）；展示覆盖取 manifest members[]。
        let make = |name: &str, role: &str| -> TeamMember {
            let d = m.member_display.get(name);
            TeamMember {
                plugin_id: String::new(),
                team_id: team_id.clone(),
                name: name.to_string(),
                role: role.to_string(),
                display_name: d.and_then(|x| x.display_name.clone()),
                profession: d.and_then(|x| x.profession.clone()),
                avatar: d.and_then(|x| x.avatar.clone()),
            }
        };
        let lead = m
            .lead
            .as_ref()
            .filter(|n| indexed.contains(*n))
            .map(|n| make(n, "lead"));
        let members: Vec<TeamMember> = m
            .member_names
            .iter()
            .filter(|n| indexed.contains(*n) && m.lead.as_deref() != Some(n.as_str()))
            .map(|n| make(n, "member"))
            .collect();

        let rec = TeamRecord {
            id: team_id,
            source,
            name: m.name.clone(),
            display_name: m.display_name,
            description: m.description,
            lead,
            members,
            avatar: None,
            category: None,
            quick_prompts: m.quick_prompts,
            enabled: true,
            installed_at: now.clone(),
            updated_at: now,
            catalog_id: None,
            group_id: None,
        };
        store::upsert(&self.db, &rec)?;
        let r = store::get_by_name(&self.db, &m.name)?.ok_or("导入后读取团队失败")?;
        Ok(TeamSummary::from_record(&r))
    }

    /// 运行时解析：取 team 的 (lead_spec, roster_specs)。lead 解析不到则 None；成员解析不到的跳过。
    pub fn resolve_for_run(
        &self,
        id: &str,
    ) -> Result<(Option<ExpertSpec>, Vec<ExpertSpec>), String> {
        let r = store::get_by_id(&self.db, id)?.ok_or_else(|| format!("团队不存在：{id}"))?;
        let lead = match &r.lead {
            Some(m) => self
                .agents
                .load_spec_by_owner(&m.plugin_id, &m.team_id, &m.name)?,
            None => None,
        };
        let mut roster = Vec::new();
        for m in &r.members {
            if let Some(spec) = self
                .agents
                .load_spec_by_owner(&m.plugin_id, &m.team_id, &m.name)?
            {
                roster.push(spec);
            }
        }
        Ok((lead, roster))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn setup() -> (Arc<AppDatabase>, Arc<ExpertService>, PathBuf) {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let dbp =
            std::env::temp_dir().join(format!("siw-teamsvc-{}-{}.db", std::process::id(), nanos));
        let _ = std::fs::remove_file(&dbp);
        let root =
            std::env::temp_dir().join(format!("siw-teamsvc-root-{}-{}", std::process::id(), nanos));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let db = Arc::new(AppDatabase::open(&dbp).unwrap());
        // 生产由 SkillService 建 skills 表；测试里显式确保，供级联删验证。
        crate::skill::store::ensure_schema(&db).expect("skill schema");
        let agents = Arc::new(ExpertService::new(db.clone(), root.clone()));
        (db, agents, root)
    }

    fn teams_root() -> PathBuf {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let p =
            std::env::temp_dir().join(format!("siw-teams-root-{}-{}", std::process::id(), nanos));
        let _ = std::fs::remove_dir_all(&p);
        p
    }

    fn member(plugin_id: &str, team_id: &str, name: &str, role: &str) -> TeamMember {
        TeamMember {
            plugin_id: plugin_id.into(),
            team_id: team_id.into(),
            name: name.into(),
            role: role.into(),
            display_name: None,
            profession: None,
            avatar: None,
        }
    }

    #[test]
    fn create_detail_resolve_and_cascade_delete() {
        let (db, agents, root) = setup();
        let svc = TeamService::new(db.clone(), agents.clone(), teams_root());

        // 散装 agent "m-std"（root 下写文件 + sync）作为被引用的全局成员。
        std::fs::write(
            root.join("m-std.md"),
            "---\nname: m-std\ndescription: 散装成员\n---\n散装成员正文。\n",
        )
        .unwrap();
        agents.sync().expect("sync");

        // 先建 team 拿到 id（lead 引用先占位），再把私有 lead 索引到该 team 命名空间、回填引用。
        let summary = svc
            .create(
                "trade",
                "交易台",
                "投研",
                None,
                vec![member("", "", "m-std", "member")],
            )
            .expect("create");
        let team_id = summary.id.clone();

        let lead_f = std::env::temp_dir().join(format!("siw-leada-{}.md", std::process::id()));
        std::fs::write(
            &lead_f,
            "---\nname: lead-a\ndescription: 主理\ndisplay_name: 何执舟\n---\n你是主理人。\n",
        )
        .unwrap();
        agents
            .index_team_expert(&team_id, &lead_f, "1")
            .expect("idx lead");
        let mut rec = store::get_by_id(&db, &team_id).unwrap().unwrap();
        rec.lead = Some(member("", &team_id, "lead-a", "lead"));
        store::upsert(&db, &rec).expect("fix lead ref");

        // detail：lead 命中私有(展示覆盖回退 agent 记录=何执舟)，member 命中散装。
        let d = svc.detail(&team_id).expect("detail");
        assert_eq!(d.lead.as_ref().unwrap().name, "lead-a");
        assert_eq!(
            d.lead.as_ref().unwrap().display_name.as_deref(),
            Some("何执舟")
        );
        assert_eq!(d.members.len(), 1);
        assert_eq!(d.members[0].name, "m-std");

        // resolve_for_run：lead spec + 1 roster spec。
        let (lead_spec, roster) = svc.resolve_for_run(&team_id).expect("resolve");
        assert!(lead_spec.unwrap().system_prompt.contains("主理人"));
        assert_eq!(roster.len(), 1);

        // 级联删：私有 lead-a 没了，散装 m-std 还在。
        svc.delete(&team_id).expect("del");
        assert!(
            crate::expert::store::get_by_owner_and_name(&db, "", &team_id, "lead-a")
                .unwrap()
                .is_none()
        );
        assert!(
            crate::expert::store::get_by_owner_and_name(&db, "", "", "m-std")
                .unwrap()
                .is_some()
        );
        assert!(store::get_by_id(&db, &team_id).unwrap().is_none());
    }

    #[test]
    fn import_codebuddy_team_package() {
        let (db, agents, _root) = setup();
        let troot = teams_root();
        let svc = TeamService::new(db.clone(), agents.clone(), troot.clone());

        // 造一个 codebuddy 风格团队包目录：.codebuddy-plugin/plugin.json + agents/ + skills/。
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let pkg = std::env::temp_dir().join(format!("siw-cb-pkg-{}-{}", std::process::id(), nanos));
        std::fs::create_dir_all(pkg.join(".codebuddy-plugin")).unwrap();
        std::fs::create_dir_all(pkg.join("agents")).unwrap();
        std::fs::create_dir_all(pkg.join("skills/kb")).unwrap();
        std::fs::write(
            pkg.join(".codebuddy-plugin/plugin.json"),
            r#"{ "name":"content-team", "displayName":{"zh":"内容团队"}, "expertType":"team",
                 "agents":["./agents/lead.md","./agents/copy.md"], "skills":["./skills/kb"],
                 "teamInfo":{"leadAgent":"lead","memberAgents":["copy"]},
                 "members":[{"id":"lead","name":{"zh":"司远"},"role":"lead"},{"id":"copy","name":{"zh":"笔澜"},"role":"member"}],
                 "quickPrompts":[{"zh":"做个情绪板"}] }"#,
        )
        .unwrap();
        std::fs::write(
            pkg.join("agents/lead.md"),
            "---\nname: lead\ndescription: 主理\n---\n你是主理人。\n",
        )
        .unwrap();
        std::fs::write(
            pkg.join("agents/copy.md"),
            "---\nname: copy\ndescription: 文案\n---\n你是文案。\n",
        )
        .unwrap();
        std::fs::write(
            pkg.join("skills/kb/SKILL.md"),
            "---\nname: kb\ndescription: 知识库\n---\n知识库正文。\n",
        )
        .unwrap();

        let summary = svc
            .import_from_path(pkg.to_str().unwrap(), TeamSource::Imported)
            .expect("import");
        assert_eq!(summary.display_name, "内容团队");
        assert_eq!(summary.member_count, 1);

        // 详情：lead=司远(展示覆盖)、member=笔澜；私有 skill 不进全局池但按 team 可见。
        let d = svc.detail(&summary.id).expect("detail");
        assert_eq!(
            d.lead.as_ref().unwrap().display_name.as_deref(),
            Some("司远")
        );
        assert_eq!(d.members.len(), 1);
        assert_eq!(d.quick_prompts, vec!["做个情绪板"]);
        assert!(crate::skill::store::list_enabled(&db)
            .expect("le")
            .is_empty());
        assert_eq!(
            crate::skill::store::list_enabled_by_team(&db, &summary.id)
                .expect("lt")
                .len(),
            1
        );

        // 私有 agent/skill 落在 team 命名空间。
        assert!(agents.summary_by_owner("", &summary.id, "lead").is_some());

        // 删除：级联私有组件 + 受管目录。
        svc.delete(&summary.id).expect("del");
        assert!(crate::skill::store::list_enabled_by_team(&db, &summary.id)
            .expect("lt2")
            .is_empty());
        assert!(!troot.join("content-team").exists());
    }

    #[test]
    fn create_with_members_authors_private_agents() {
        let (db, agents, _root) = setup();
        let troot = teams_root();
        let svc = TeamService::new(db.clone(), agents.clone(), troot.clone());

        let spec = |name: &str| InlineExpert {
            name: name.into(),
            description: format!("{name} 的职责"),
            system_prompt: format!("你是 {name}，按结论/证据/风险输出。"),
            tools: vec!["read_file".into()],
            model_tier: "aux".into(),
            display_name: Some(format!("展示-{name}")),
            profession: Some("分析师".into()),
        };
        let summary = svc
            .create_with_members(
                "trade-desk",
                "交易台",
                "投研团队",
                Some(spec("coordinator")),
                vec![spec("researcher"), spec("writer")],
                vec!["分析下这家公司".into(), "出一份周报".into()],
                None,
            )
            .expect("create");
        assert_eq!(summary.member_count, 2);

        // 私有 agent 落在 team 命名空间、可解析正文。
        let d = svc.detail(&summary.id).expect("detail");
        assert_eq!(d.lead.as_ref().unwrap().name, "coordinator");
        assert_eq!(d.members.len(), 2);
        assert_eq!(d.quick_prompts.len(), 2);
        let (lead_spec, roster) = svc.resolve_for_run(&summary.id).expect("resolve");
        assert!(lead_spec.unwrap().system_prompt.contains("coordinator"));
        assert_eq!(roster.len(), 2);
        // 私有成员不污染散装列表。
        assert!(agents.list_standalone().expect("ls").is_empty());

        // 同名团队再建报错；目录已存在。
        assert!(svc
            .create_with_members(
                "trade-desk",
                "x",
                "",
                None,
                vec![spec("a")],
                Vec::new(),
                None
            )
            .is_err());

        // 删除级联私有 agent + 目录。
        svc.delete(&summary.id).expect("del");
        assert!(agents
            .summary_by_owner("", &summary.id, "coordinator")
            .is_none());
        assert!(!troot.join("trade-desk").exists());
    }
}

//! AgentService：伴随体面向 command 与 runtime 的受控操作入口。
//!
//! ⚠️ 构造顺序：`AgentService::new` 会 `store::ensure_schema` 建 `agents` 表。因表名复用 T67 腾出的
//! `agents`，**必须在 `ExpertService::new`（其 ensure_schema 完成旧 agents→experts 改名）之后构造**，
//! 否则本新表会被 T67 旧迁移误改名。builder/启动序列须固定此顺序。

use std::path::PathBuf;
use std::sync::Arc;

use crate::agent::model::{AgentRecord, SoulVersion};
use crate::agent::soul_store;
use crate::agent::store;
use crate::expert::ExpertService;
use crate::session::new_id;
use crate::storage::AppDatabase;

pub struct AgentService {
    db: Arc<AppDatabase>,
    workspace_base: Option<PathBuf>,
}

impl AgentService {
    pub fn new(db: Arc<AppDatabase>) -> Self {
        Self::new_inner(db, None)
    }

    pub fn new_with_workspace_base(db: Arc<AppDatabase>, workspace_base: PathBuf) -> Self {
        Self::new_inner(db, Some(workspace_base))
    }

    fn new_inner(db: Arc<AppDatabase>, workspace_base: Option<PathBuf>) -> Self {
        let _ = store::ensure_schema(&db);
        let _ = soul_store::ensure_schema(&db); // T73：SOUL 版本表
        let svc = Self { db, workspace_base };
        if let Err(e) = svc.backfill_working_dirs() {
            eprintln!("[agent] 补齐默认工作目录失败：{e}");
        }
        if let Err(e) = svc.seed_existing_souls() {
            eprintln!("[agent] 补种 SOUL 初版失败：{e}");
        }
        svc
    }

    /// T73：为尚无 SOUL 版本的存量伴随体补种一条 `active/seed`（= 其当前 instructions）。
    fn seed_existing_souls(&self) -> Result<(), String> {
        for rec in store::list(&self.db)? {
            let _ = soul_store::seed_if_empty(&self.db, &rec.id, &rec.instructions, &Self::now())?;
        }
        Ok(())
    }

    fn now() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or_default()
            .to_string()
    }

    /// 由源 expert **播种**一个伴随体：软复制其 system_prompt→instructions、tools、model_tier；
    /// 记 `source_expert_id`=源 expert 名（运行时技能引用键 + 溯源）。
    /// 唯一标识 `name` 由后端从 `display_name`（空则源 expert 名）派生并保证唯一（冲突追加 -2/-3…）；
    /// 调用方（新建表单）只暴露显示名。
    pub fn create_from_expert(
        &self,
        experts: &ExpertService,
        source_expert_name: &str,
        display_name: &str,
    ) -> Result<AgentRecord, String> {
        let spec = experts
            .load_spec_by_name(source_expert_name)?
            .ok_or_else(|| format!("源 expert 不存在：{source_expert_name}"))?;
        let dn = display_name.trim();
        let base = if dn.is_empty() {
            source_expert_name
        } else {
            dn
        };
        let name = self.unique_name(base)?;
        let now = Self::now();
        let id = new_id("agent");
        let working_dir = self.ensure_default_working_dir(&id)?;
        // T74：IDENTITY 锚由结构化身份（此处仅 display_name 可用，profession 创建时为空）种；
        // 留空亦可（注入退化为仅 SOUL）。用户后续可在「智能体」页编辑。
        let identity = if dn.is_empty() {
            String::new()
        } else {
            format!("你是{dn}。")
        };
        let rec = AgentRecord {
            id,
            name,
            instructions: spec.system_prompt.trim().to_string(), // SOUL：软复制导入人格（去首尾空白），此后伴随体自有可编辑
            identity,
            evolution_enabled: false,
            last_reflection_at: None,
            tools: spec.tools,
            model_tier: spec.model_tier,
            source_expert_id: Some(source_expert_name.to_string()),
            working_dir,
            display_name: if dn.is_empty() {
                None
            } else {
                Some(dn.to_string())
            },
            profession: None,
            avatar: None,
            color: None,
            enabled: true,
            group_id: None,
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        store::upsert(&self.db, &rec)?;
        // T73：导入的 SOUL 立为活跃种子版本（演化在其上叠加）。
        let _ = soul_store::seed_if_empty(&self.db, &rec.id, &rec.instructions, &now)?;
        Ok(rec)
    }

    fn default_working_dir(&self, id: &str) -> Option<PathBuf> {
        self.workspace_base
            .as_ref()
            .map(|base| base.join("agents").join(id))
    }

    fn ensure_default_working_dir(&self, id: &str) -> Result<Option<String>, String> {
        let Some(dir) = self.default_working_dir(id) else {
            return Ok(None);
        };
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("创建智能体工作目录失败 {}：{e}", dir.display()))?;
        Ok(Some(dir.to_string_lossy().to_string()))
    }

    fn backfill_working_dirs(&self) -> Result<(), String> {
        if self.workspace_base.is_none() {
            return Ok(());
        }
        for mut rec in store::list(&self.db)? {
            if rec
                .working_dir
                .as_deref()
                .map(|s| s.trim().is_empty())
                .unwrap_or(true)
            {
                rec.working_dir = self.ensure_default_working_dir(&rec.id)?;
                store::upsert(&self.db, &rec)?;
            }
        }
        Ok(())
    }

    /// 生成唯一 `name`：base 去空白（空则兜底「智能体」）；与现有 name 冲突时追加 -2/-3…。
    fn unique_name(&self, base: &str) -> Result<String, String> {
        let trimmed = base.trim();
        let base = if trimmed.is_empty() {
            "智能体"
        } else {
            trimmed
        };
        if store::get_by_name(&self.db, base)?.is_none() {
            return Ok(base.to_string());
        }
        let mut n = 2;
        loop {
            let candidate = format!("{base}-{n}");
            if store::get_by_name(&self.db, &candidate)?.is_none() {
                return Ok(candidate);
            }
            n += 1;
        }
    }

    pub fn list(&self) -> Result<Vec<AgentRecord>, String> {
        store::list(&self.db)
    }

    pub fn get_by_id(&self, id: &str) -> Result<Option<AgentRecord>, String> {
        store::get_by_id(&self.db, id)
    }

    pub fn ensure_workspace(&self, id: &str) -> Result<PathBuf, String> {
        let mut rec = store::get_by_id(&self.db, id)?.ok_or_else(|| format!("伴随体不存在：{id}"))?;
        if let Some(dir) = rec
            .working_dir
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            let path = PathBuf::from(dir);
            std::fs::create_dir_all(&path)
                .map_err(|e| format!("创建智能体工作目录失败 {}：{e}", path.display()))?;
            return Ok(path);
        }
        let dir = self
            .default_working_dir(id)
            .ok_or_else(|| "智能体默认工作目录未配置".to_string())?;
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("创建智能体工作目录失败 {}：{e}", dir.display()))?;
        rec.working_dir = Some(dir.to_string_lossy().to_string());
        store::upsert(&self.db, &rec)?;
        Ok(dir)
    }

    /// 保存（编辑）：刷新 updated_at 后 upsert。name 唯一冲突由表索引兜底（映射为友好错误）。
    pub fn save(&self, mut rec: AgentRecord) -> Result<AgentRecord, String> {
        // 改名时校验是否撞别的伴随体。
        if let Some(other) = store::get_by_name(&self.db, &rec.name)? {
            if other.id != rec.id {
                return Err(format!("伴随体名已存在：{}", rec.name));
            }
        }
        rec.updated_at = Self::now();
        store::upsert(&self.db, &rec)?;
        Ok(rec)
    }

    pub fn toggle(&self, id: &str, enabled: bool) -> Result<(), String> {
        store::set_enabled(&self.db, id, enabled, &Self::now())
    }

    pub fn set_group(&self, id: &str, group_id: Option<&str>) -> Result<(), String> {
        store::set_group(&self.db, id, group_id)
    }

    pub fn delete(&self, id: &str) -> Result<(), String> {
        let _ = soul_store::delete_by_agent(&self.db, id); // T73：随伴随体清理 SOUL 版本
        store::delete(&self.db, id)
    }

    // ---- T73 自我演化 ----

    /// 设置「允许自我演化」开关。
    pub fn set_evolution_enabled(&self, id: &str, enabled: bool) -> Result<(), String> {
        store::set_evolution(&self.db, id, enabled, &Self::now())
    }

    /// 列出某伴随体的 SOUL 版本史（新在前）。
    pub fn list_soul_versions(&self, id: &str) -> Result<Vec<SoulVersion>, String> {
        soul_store::list_by_agent(&self.db, id)
    }

    /// 批准一个 SOUL 版本（pending 或回滚目标）：设为活跃并同步 `agents.instructions`（= SOUL）。
    pub fn approve_soul(&self, agent_id: &str, version_id: &str) -> Result<(), String> {
        let v = soul_store::get(&self.db, version_id)?
            .ok_or_else(|| format!("SOUL 版本不存在：{version_id}"))?;
        if v.agent_id != agent_id {
            return Err("SOUL 版本不属于该伴随体".to_string());
        }
        soul_store::set_active(&self.db, agent_id, version_id)?;
        store::set_instructions(&self.db, agent_id, &v.soul, &Self::now())
    }

    /// 反思运行提交一份 SOUL 改写提案：写一条 `pending`/`reflection` 版本，不动活跃人格。返回版本 id。
    pub fn propose_soul(&self, agent_id: &str, new_soul: &str, summary: &str) -> Result<String, String> {
        let id = new_id("soul");
        soul_store::insert(
            &self.db,
            &SoulVersion {
                id: id.clone(),
                agent_id: agent_id.to_string(),
                soul: new_soul.to_string(),
                status: "pending".to_string(),
                summary: summary.to_string(),
                source: "reflection".to_string(),
                created_at: Self::now(),
            },
        )?;
        Ok(id)
    }

    /// 拒绝一个待批准提案（pending → archived），不动活跃版本。
    pub fn reject_soul(&self, version_id: &str) -> Result<(), String> {
        soul_store::reject(&self.db, version_id)
    }

    /// 回滚到某历史版本：等价于把它重新设为活跃并同步 SOUL。
    pub fn rollback_soul(&self, agent_id: &str, version_id: &str) -> Result<(), String> {
        self.approve_soul(agent_id, version_id)
    }

    /// 记录上次触发反思的时刻（epoch 秒）。供演化扫描线程（Phase 2）防抖 + 补跑判定。
    pub fn mark_reflected(&self, id: &str, at: i64) -> Result<(), String> {
        store::set_last_reflection_at(&self.db, id, at, &Self::now())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn seeds_companion_from_expert_softcopy_instructions() {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!("siw-agent-svc-{}-{}", std::process::id(), n));
        std::fs::create_dir_all(&base).unwrap();
        let db = Arc::new(AppDatabase::open(base.join("t.db")).unwrap());
        // 顺序：先 expert（其 ensure_schema 完成 T67 agents→experts 改名），再 agent（建伴随体 agents 表）。
        let experts = ExpertService::new(db.clone(), base.join("experts"));
        experts
            .create_standalone(
                "研究员",
                "严谨研究",
                "你是严谨的研究员",
                vec!["web_search".into()],
                "main",
                None,
                None,
                None,
                vec![],
                None,
            )
            .unwrap();
        let agents = AgentService::new(db.clone());

        let a = agents
            .create_from_expert(&experts, "研究员", "小研")
            .unwrap();
        assert_eq!(a.name, "小研"); // 显示名作 name 基底
        assert_eq!(a.display_name.as_deref(), Some("小研"));
        assert_eq!(a.identity, "你是小研。"); // T74：由 display_name 种锚
        assert_eq!(a.instructions, "你是严谨的研究员"); // SOUL = 导入人格不变
        assert_eq!(a.tools, vec!["web_search".to_string()]);
        assert_eq!(a.source_expert_id.as_deref(), Some("研究员")); // 技能引用键
        assert_eq!(a.model_tier, "main");

        // 显示名相同 → name 自动去重（不再报错）。
        let b = agents
            .create_from_expert(&experts, "研究员", "小研")
            .unwrap();
        assert_eq!(b.name, "小研-2");
        assert_eq!(b.display_name.as_deref(), Some("小研"));
        // 源 expert 不存在报错。
        assert!(agents
            .create_from_expert(&experts, "查无此人", "X")
            .is_err());

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn evolution_seed_and_approve_sync_instructions() {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!("siw-agent-evo-{}-{}", std::process::id(), n));
        std::fs::create_dir_all(&base).unwrap();
        let db = Arc::new(AppDatabase::open(base.join("t.db")).unwrap());
        let experts = ExpertService::new(db.clone(), base.join("experts"));
        experts
            .create_standalone(
                "研究员",
                "严谨研究",
                "你是严谨的研究员",
                vec![],
                "main",
                None,
                None,
                None,
                vec![],
                None,
            )
            .unwrap();
        let agents = AgentService::new(db.clone());
        let a = agents.create_from_expert(&experts, "研究员", "小研").unwrap();

        // 播种：1 条 active/seed，soul == 导入 instructions。
        let versions = agents.list_soul_versions(&a.id).unwrap();
        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0].status, "active");
        assert_eq!(versions[0].source, "seed");
        assert_eq!(versions[0].soul, "你是严谨的研究员");

        // 模拟反思提案（Phase 2 由工具产；此处直接插）→ 批准 → instructions 同步、旧 active 归档。
        crate::agent::soul_store::insert(
            &db,
            &SoulVersion {
                id: "p1".into(),
                agent_id: a.id.clone(),
                soul: "你是严谨且简洁的研究员".into(),
                status: "pending".into(),
                summary: "习得：偏好简洁".into(),
                source: "reflection".into(),
                created_at: "9".into(),
            },
        )
        .unwrap();
        agents.approve_soul(&a.id, "p1").unwrap();

        let got = agents.get_by_id(&a.id).unwrap().unwrap();
        assert_eq!(got.instructions, "你是严谨且简洁的研究员"); // SOUL 已同步
        let active = crate::agent::soul_store::active_for(&db, &a.id).unwrap().unwrap();
        assert_eq!(active.id, "p1");

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn creates_default_working_dir_under_agents_id() {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!("siw-agent-dir-{}-{}", std::process::id(), n));
        std::fs::create_dir_all(&base).unwrap();
        let db = Arc::new(AppDatabase::open(base.join("t.db")).unwrap());
        let experts = ExpertService::new(db.clone(), base.join("experts"));
        experts
            .create_standalone(
                "研究员",
                "严谨研究",
                "你是严谨的研究员",
                vec![],
                "main",
                None,
                None,
                None,
                vec![],
                None,
            )
            .unwrap();
        let agents = AgentService::new_with_workspace_base(db.clone(), base.clone());

        let a = agents
            .create_from_expert(&experts, "研究员", "小研")
            .unwrap();
        let expected = base.join("agents").join(&a.id);
        let expected_str = expected.to_string_lossy().to_string();
        assert_eq!(a.working_dir.as_deref(), Some(expected_str.as_str()));
        assert!(expected.is_dir());

        let _ = std::fs::remove_dir_all(&base);
    }
}

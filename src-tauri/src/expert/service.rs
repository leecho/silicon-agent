//! ExpertService：agent 模块面向 command 与 runtime 的受控操作入口（镜像 SkillService）。
//!
//! 持有 db（索引）与专家根目录（磁盘事实源）。sync 物化内置 + 扫描磁盘 upsert + 清孤儿。

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use crate::expert::model::{ExpertRecord, ExpertSource};
use crate::expert::types::ExpertSummary;
use crate::expert::{builtin, frontmatter, store};
use crate::session::new_id;
use crate::storage::AppDatabase;

/// 一个解析好的专家角色定义（供运行时构造专家会话）。
pub struct ExpertSpec {
    pub name: String,
    pub description: String,
    pub system_prompt: String,
    pub tools: Vec<String>,
    pub model_tier: String,
    pub max_turns: Option<u32>,
    pub role: String,
    /// owner 归属 plugin id（plugin 提供则非空，否则空）。
    pub plugin_id: String,
    /// owner 归属 team id（team 私有则非空，否则空）。owner = plugin_id XOR team_id。
    pub team_id: String,
}

/// 专家服务。`root` = `{workspace_base}/agent`，内置物化 + 用户安装同处此目录。
pub struct ExpertService {
    db: Arc<AppDatabase>,
    root: PathBuf,
}

impl ExpertService {
    /// 构造服务并确保索引表存在（不自动 sync，sync 由启动流程显式调用）。
    pub fn new(db: Arc<AppDatabase>, root: PathBuf) -> Self {
        let _ = store::ensure_schema(&db);
        Self { db, root }
    }

    fn now() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or_default()
            .to_string()
    }

    /// 启动同步：① 物化内置 → ② 扫描磁盘 `*.md` 解析 frontmatter upsert → ③ 清孤儿。幂等。
    pub fn sync(&self) -> Result<(), String> {
        std::fs::create_dir_all(&self.root).map_err(|e| format!("创建团队根目录失败：{e}"))?;
        builtin::materialize(&self.root)?;
        let builtin_set: HashSet<String> = builtin::builtin_names().into_iter().collect();
        let now = Self::now();

        let mut seen: HashSet<String> = HashSet::new();
        for entry in std::fs::read_dir(&self.root).map_err(|e| format!("读团队根目录失败：{e}"))?
        {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            // 仅处理 .md 文件。
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let file_name = entry.file_name().to_string_lossy().into_owned();
            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("[agent] 跳过 {file_name}：读文件失败 {e}");
                    continue;
                }
            };
            let fm = match frontmatter::parse_frontmatter(&content) {
                Ok(fm) => fm,
                Err(e) => {
                    eprintln!("[agent] 跳过 {file_name}：frontmatter 解析失败 {e}");
                    continue;
                }
            };
            let source = if builtin_set.contains(&fm.name) {
                ExpertSource::Builtin
            } else {
                ExpertSource::User
            };
            let rec = ExpertRecord {
                id: new_id("agent"),
                source,
                name: fm.name.clone(),
                description: fm.description,
                tools: fm.tools,
                model_tier: fm.model_tier,
                max_turns: fm.max_turns,
                role: fm.role,
                plugin_id: String::new(), // 散装/内置：空命名空间。
                team_id: String::new(),
                display_name: fm.display_name,
                profession: fm.profession,
                avatar: fm.avatar,
                color: fm.color,
                file_name,
                enabled: true, // 新行默认启用；已存在行 upsert 保留原 enabled。
                installed_at: now.clone(),
                updated_at: now.clone(),
                catalog_id: fm.catalog_id, // 广场副本重扫描时保留来源标记。
                group_id: None,            // 分组由 store upsert 在冲突时保留，不从 .md 读。
            };
            store::upsert(&self.db, &rec)?;
            seen.insert(fm.name);
        }

        // 清孤儿：仅清**散装**（plugin_id 与 team_id 均空）中磁盘已无对应 name 的行。
        // plugin 提供的 agent（启动后由 app_state 重新索引）与 team 私有 agent（导入后常驻、不参与本扫描）
        // 不在此清理范围，否则 team 私有 agent 会在重启时被误删且无人重建。
        for rec in store::list(&self.db)? {
            if rec.plugin_id.is_empty() && rec.team_id.is_empty() && !seen.contains(&rec.name) {
                store::delete(&self.db, &rec.id)?;
            }
        }
        Ok(())
    }

    /// 列出全部专家（内置 + 用户）。
    pub fn list(&self) -> Result<Vec<ExpertSummary>, String> {
        Ok(store::list(&self.db)?.into_iter().map(Into::into).collect())
    }

    /// 列出启用专家（供引擎注入「团队」清单）。
    pub fn list_enabled(&self) -> Result<Vec<ExpertSummary>, String> {
        Ok(store::list_enabled(&self.db)?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    /// 列出某 plugin（激活团队）下的启用专家（供激活团队后注入 roster）。
    pub fn list_enabled_by_plugin(&self, plugin_id: &str) -> Result<Vec<ExpertSummary>, String> {
        Ok(store::list_enabled_by_plugin(&self.db, plugin_id)?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    /// 列出某 plugin 下的全部专家（含未启用）；供套件详情页展示其成员。
    pub fn list_by_plugin(&self, plugin_id: &str) -> Result<Vec<ExpertSummary>, String> {
        Ok(store::list_by_plugin(&self.db, plugin_id)?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    /// 取某专家的元数据摘要（含展示身份），按命名空间解析（激活 plugin → 散装兜底）。
    /// 仅查索引、不读盘；供面板展示。不存在返回 None（ad-hoc 专家无索引行）。
    pub fn find_summary(
        &self,
        active_plugin_id: Option<&str>,
        name: &str,
    ) -> Option<ExpertSummary> {
        let rec = match active_plugin_id.filter(|p| !p.is_empty()) {
            Some(pid) => store::get_by_plugin_and_name(&self.db, pid, name)
                .ok()
                .flatten()
                .or_else(|| {
                    store::get_by_plugin_and_name(&self.db, "", name)
                        .ok()
                        .flatten()
                }),
            None => store::get_by_plugin_and_name(&self.db, "", name)
                .ok()
                .flatten(),
        };
        rec.map(Into::into)
    }

    /// 索引一个 plugin 内的专家定义文件（绝对路径），upsert 到 agents 表带 `plugin_id`。返回其 name。
    /// 正文不入库；运行时按 `file_name`（此处存绝对路径）读盘。
    pub fn index_plugin_expert(
        &self,
        plugin_id: &str,
        abs_path: &std::path::Path,
        now: &str,
    ) -> Result<String, String> {
        let content = std::fs::read_to_string(abs_path)
            .map_err(|e| format!("读 plugin 专家文件失败 {}：{e}", abs_path.display()))?;
        // 无 frontmatter 时按文件名 + 正文兜底（Codex 插件的 agent 就没有 frontmatter；T106 P4）。
        let fm = match frontmatter::parse_frontmatter(&content) {
            Ok(fm) => fm,
            Err(_) => {
                let stem = abs_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default();
                // name 会成为唯一键与技能引用键：拒绝不安全名（路径穿越等）。
                if !crate::market::wire::is_safe_component(stem) {
                    return Err(format!(
                        "plugin 专家文件名不安全，且无 frontmatter 可用：{}",
                        abs_path.display()
                    ));
                }
                frontmatter::Frontmatter::fallback(stem, &content)
            }
        };
        let rec = ExpertRecord {
            id: new_id("agent"),
            source: ExpertSource::Plugin,
            name: fm.name.clone(),
            description: fm.description,
            tools: fm.tools,
            model_tier: fm.model_tier,
            max_turns: fm.max_turns,
            role: fm.role,
            plugin_id: plugin_id.to_string(),
            team_id: String::new(),
            display_name: fm.display_name,
            profession: fm.profession,
            avatar: fm.avatar,
            color: fm.color,
            file_name: abs_path.to_string_lossy().into_owned(), // plugin agent 存绝对路径
            enabled: true,
            installed_at: now.to_string(),
            updated_at: now.to_string(),
            catalog_id: None,
            group_id: None,
        };
        store::upsert(&self.db, &rec)?;
        Ok(fm.name)
    }

    /// 清掉某 plugin 下不再声明的孤儿专家（plugin 降级/卸载）。
    pub fn clear_plugin_experts_except(
        &self,
        plugin_id: &str,
        keep: &[String],
    ) -> Result<(), String> {
        store::delete_plugin_orphans(&self.db, plugin_id, keep)
    }

    /// 切换专家启用状态。
    pub fn toggle(&self, id: &str, enabled: bool) -> Result<ExpertSummary, String> {
        store::set_enabled(&self.db, id, enabled, &Self::now())?;
        store::get_by_id(&self.db, id)?
            .map(Into::into)
            .ok_or_else(|| format!("专家不存在：{id}"))
    }

    /// 列出**散装** agent（plugin_id 与 team_id 均空；含未启用）。供「专家」管理页。
    pub fn list_standalone(&self) -> Result<Vec<ExpertSummary>, String> {
        Ok(store::list(&self.db)?
            .into_iter()
            .filter(|r| r.plugin_id.is_empty() && r.team_id.is_empty())
            .map(Into::into)
            .collect())
    }

    /// 列出「扩展 → 专家」页可见的全部 agent：**散装 + plugin 提供**（含未启用），
    /// 排除 team 私有（那是团队的内部组件，不进扩展页）。
    /// 与 `list_standalone`（只散装）和 `list`（另有语义）均不同。T106 §5.2。
    pub fn list_manageable(&self) -> Result<Vec<ExpertSummary>, String> {
        Ok(store::list(&self.db)?
            .into_iter()
            .filter(|r| r.team_id.is_empty())
            .map(Into::into)
            .collect())
    }

    /// 新建一个散装 agent：写 `{name}.md` 到 agent 根并 upsert。同名散装已存在则报错。
    #[allow(clippy::too_many_arguments)]
    pub fn create_standalone(
        &self,
        name: &str,
        description: &str,
        system_prompt: &str,
        tools: Vec<String>,
        model_tier: &str,
        display_name: Option<String>,
        profession: Option<String>,
        avatar: Option<String>,
        quick_prompts: Vec<String>,
        catalog_id: Option<String>,
    ) -> Result<ExpertSummary, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("专家 name 不能为空".into());
        }
        if name.contains('/') || name.contains('\\') || name.contains("..") {
            return Err("专家 name 含非法字符".into());
        }
        if store::get_by_owner_and_name(&self.db, "", "", name)?.is_some() {
            return Err("同名散装专家已存在".into());
        }
        std::fs::create_dir_all(&self.root).map_err(|e| format!("创建 agent 根失败：{e}"))?;
        let model_tier = if model_tier == "main" { "main" } else { "aux" };
        let file_name = format!("{name}.md");
        let md = serialize_expert_md(
            name,
            description,
            system_prompt,
            &tools,
            model_tier,
            display_name.as_deref(),
            profession.as_deref(),
            avatar.as_deref(),
            &quick_prompts,
            catalog_id.as_deref(),
        );
        std::fs::write(self.root.join(&file_name), md)
            .map_err(|e| format!("写 agent 文件失败：{e}"))?;
        let now = Self::now();
        let rec = ExpertRecord {
            id: new_id("agent"),
            source: ExpertSource::User,
            name: name.to_string(),
            description: description.to_string(),
            tools,
            model_tier: model_tier.to_string(),
            max_turns: None,
            role: "member".to_string(),
            plugin_id: String::new(),
            team_id: String::new(),
            display_name,
            profession,
            avatar,
            color: None,
            file_name,
            enabled: true,
            installed_at: now.clone(),
            updated_at: now,
            catalog_id,
            group_id: None,
        };
        store::upsert(&self.db, &rec)?;
        store::get_by_owner_and_name(&self.db, "", "", name)?
            .map(Into::into)
            .ok_or_else(|| "创建后读取专家失败".into())
    }

    /// 把广场携带的 skill 物化为该散装 agent 的私有 skill：
    /// 文件写到 `{root}/<expert_name>/skills/<skill.name>/<relpath>`，再索引为 owner=agent name。
    /// 单个 skill 失败仅 log 跳过，不阻断「加入我的」。
    pub fn attach_private_skills(
        &self,
        expert_name: &str,
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
                    "[catalog-agent] {expert_name}: 跳过 skill {}（非法名，疑路径穿越）",
                    sk.name
                );
                continue;
            }
            let skill_dir = self.root.join(expert_name).join("skills").join(&sk.name);
            if let Err(e) = write_skill_files(&skill_dir, &sk.files) {
                eprintln!(
                    "[catalog-agent] {expert_name}: 跳过 skill {}（写文件 {e}）",
                    sk.name
                );
                continue;
            }
            let rec = crate::skill::model::SkillRecord {
                id: new_id("skill"),
                source: crate::skill::model::SkillSource::User,
                name: sk.name.clone(),
                description: sk.description,
                dir_name: skill_dir.to_string_lossy().into_owned(),
                enabled: true,
                installed_at: now.clone(),
                updated_at: now.clone(),
                plugin_id: None,
                team_id: None,
                expert_id: Some(expert_name.to_string()),
                user_invocable: sk.user_invocable,
                argument_hint: sk.argument_hint,
                group_id: None,
            };
            if let Err(e) = crate::skill::store::upsert(&self.db, &rec) {
                eprintln!(
                    "[catalog-agent] {expert_name}: 索引 skill {} 失败 {e}",
                    sk.name
                );
            }
        }
        Ok(())
    }

    /// 设置散装专家的「我的」分组（None=移出）。
    pub fn set_group(&self, id: &str, group_id: Option<&str>) -> Result<(), String> {
        store::set_group(&self.db, id, group_id)
    }

    /// 把某分组下的专家全部归零（删除分组时调用）。
    pub fn clear_group(&self, group_id: &str) -> Result<(), String> {
        store::clear_group(&self.db, group_id)
    }

    /// 删除散装 **user** agent：删 `.md` 文件 + 删索引行。内置仅可禁用；plugin/team 拥有的不在此删。
    pub fn delete_standalone(&self, id: &str) -> Result<(), String> {
        let r = store::get_by_id(&self.db, id)?.ok_or_else(|| format!("专家不存在：{id}"))?;
        if !r.plugin_id.is_empty() || !r.team_id.is_empty() {
            return Err("该专家归属套件/团队，不能在此删除".into());
        }
        if r.source != ExpertSource::User {
            return Err("内置专家不可删除，仅可禁用".into());
        }
        let f = self.root.join(&r.file_name);
        if f.exists() {
            std::fs::remove_file(&f).map_err(|e| format!("删除 agent 文件失败：{e}"))?;
        }
        // 级联删除该 agent 的私有 skill（owner=agent name）。
        let _ = crate::skill::store::delete_by_agent(&self.db, &r.name);
        store::delete(&self.db, id)
    }

    /// 解析专家角色（元数据 + 正文 system prompt）；不存在返回 None。供运行时构造专家会话。
    /// 命名空间解析：先查激活 plugin（`active_plugin_id`）下的同名，未命中再回退散装（`plugin_id=''`）。
    pub fn load_spec(
        &self,
        name: &str,
        active_plugin_id: Option<&str>,
    ) -> Result<Option<ExpertSpec>, String> {
        // 先激活 plugin 命名空间，再散装('')兜底。自由模式（None）只解析散装/内置，plugin 专家不在 scope。
        let rec = match active_plugin_id.filter(|p| !p.is_empty()) {
            Some(pid) => match store::get_by_plugin_and_name(&self.db, pid, name)? {
                Some(r) => Some(r),
                None => store::get_by_plugin_and_name(&self.db, "", name)?,
            },
            None => store::get_by_plugin_and_name(&self.db, "", name)?,
        };
        let Some(rec) = rec else {
            return Ok(None);
        };
        Ok(self.spec_from_record(rec))
    }

    /// 按 owner 命名空间（plugin_id/team_id 至多一非空；都空=散装）取摘要。供 team 成员展示解析。
    pub fn summary_by_owner(
        &self,
        plugin_id: &str,
        team_id: &str,
        name: &str,
    ) -> Option<ExpertSummary> {
        store::get_by_owner_and_name(&self.db, plugin_id, team_id, name)
            .ok()
            .flatten()
            .map(Into::into)
    }

    /// 按稳定 id 取摘要。供直接专家会话使用；不按 name 兜底。
    pub fn summary_by_id(&self, id: &str) -> Option<ExpertSummary> {
        store::get_by_id(&self.db, id)
            .ok()
            .flatten()
            .map(Into::into)
    }

    /// 按稳定 id 解析角色定义（含正文）。供直接专家会话使用；不存在返回 None。
    pub fn load_spec_by_id(&self, id: &str) -> Result<Option<ExpertSpec>, String> {
        let Some(rec) = store::get_by_id(&self.db, id)? else {
            return Ok(None);
        };
        Ok(self.spec_from_record(rec))
    }

    /// 按 owner 命名空间解析角色定义（含正文）。供 team roster/SOP 与子运行构造。不存在/读盘失败返回 None。
    pub fn load_spec_by_owner(
        &self,
        plugin_id: &str,
        team_id: &str,
        name: &str,
    ) -> Result<Option<ExpertSpec>, String> {
        let Some(rec) = store::get_by_owner_and_name(&self.db, plugin_id, team_id, name)? else {
            return Ok(None);
        };
        Ok(self.spec_from_record(rec))
    }

    /// 按 name 全局取摘要（任意 owner，命中最先匹配）。供项目成员解析——
    /// 成员可能来自团队私有命名空间（team_id 非空），散装查询查不到。
    pub fn summary_by_name(&self, name: &str) -> Option<ExpertSummary> {
        store::get_by_name(&self.db, name)
            .ok()
            .flatten()
            .map(Into::into)
    }

    /// 按 name 全局解析角色定义（**任意 owner，含 team 私有**）。
    ///
    /// ⚠️ **不要**用它做自由模式的派发解析 —— 它能捞到 team 私有专家，会捅穿团队隔离。
    /// 那条路请用 [`Self::resolve_public_spec_by_name`]。
    ///
    /// 正当调用方只有两处，它们本就需要跨私有命名空间取人：
    /// - `agent::service` 伴随体溯源其来源 expert；
    /// - `project` 解析项目成员（成员可能是从团队复制进来的私有专家）。
    pub fn load_spec_by_name(&self, name: &str) -> Result<Option<ExpertSpec>, String> {
        let Some(rec) = store::get_by_name(&self.db, name)? else {
            return Ok(None);
        };
        Ok(self.spec_from_record(rec))
    }

    /// 按裸名解析一个**公开**专家（散装 + plugin 提供；`team_id = ''`）。供自由模式派发。
    ///
    /// 规则：
    /// 1. **散装优先**（用户自己的东西压过插件带来的）；
    /// 2. 否则在公开的 plugin 专家里找 —— 恰好一个则命中；
    /// 3. **多个同名则报错并列出候选，绝不静默挑一个** —— 挑错了模型会拿着别人的人设
    ///    干活，且无从察觉。命名空间前缀（T108 §6 / P3）是根治。
    pub fn resolve_public_spec_by_name(&self, name: &str) -> Result<Option<ExpertSpec>, String> {
        // 限定名（`plugin:agent`，T108 §6）直接定位 —— 这也是同名歧义的正解。
        if let (Some(plugin_name), bare) = crate::plugin::namespace::split_qualified(name) {
            if let Some(p) = crate::plugin::store::get_by_name(&self.db, plugin_name)? {
                let rec = store::get_by_owner_and_name(&self.db, &p.id, "", bare)?;
                return Ok(rec.and_then(|r| self.spec_from_record(r)));
            }
            // 前缀没对上任何 plugin：退回按整串裸名找（专家名理论上可含冒号）。
        }
        let mut rows = store::list_public_by_name(&self.db, name)?;
        if rows.is_empty() {
            return Ok(None);
        }
        // 散装（plugin_id 为空）优先。
        if let Some(pos) = rows.iter().position(|r| r.plugin_id.is_empty()) {
            return Ok(self.spec_from_record(rows.swap_remove(pos)));
        }
        if rows.len() > 1 {
            // 候选列**限定名**而不是 plugin_id —— 用户/模型可以直接照着用（T108 §6）。
            // 取不到 plugin name 时退回用 plugin_id 作前缀：宁可难看，也不能退成裸名
            // （那样列出来是「reviewer、reviewer」，完全没消歧，等于没说）。
            let candidates = rows
                .iter()
                .map(|r| {
                    let prefix = match crate::plugin::store::get_by_id(&self.db, &r.plugin_id) {
                        Ok(Some(p)) => p.name,
                        _ => r.plugin_id.clone(),
                    };
                    crate::plugin::namespace::qualify(&prefix, &r.name)
                })
                .collect::<Vec<_>>()
                .join("、");
            return Err(format!(
                "有多个名为「{name}」的专家。请用限定名指明来源：{candidates}"
            ));
        }
        Ok(self.spec_from_record(rows.remove(0)))
    }

    /// 删除某 plugin 提供的全部专家（卸载级联）。
    pub fn delete_by_plugin(&self, plugin_id: &str) -> Result<(), String> {
        store::delete_by_plugin(&self.db, plugin_id)
    }

    /// 索引一个 team 私有专家定义文件（绝对路径），upsert 到 agents 表带 `team_id`。返回其 name。
    /// 供 P2 团队包导入；与 `index_plugin_expert` 同构，只是 owner 落 team。
    pub fn index_team_expert(
        &self,
        team_id: &str,
        abs_path: &std::path::Path,
        now: &str,
    ) -> Result<String, String> {
        let content = std::fs::read_to_string(abs_path)
            .map_err(|e| format!("读 team 专家文件失败 {}：{e}", abs_path.display()))?;
        let fm = frontmatter::parse_frontmatter(&content)?;
        let rec = ExpertRecord {
            id: new_id("agent"),
            source: ExpertSource::Plugin,
            name: fm.name.clone(),
            description: fm.description,
            tools: fm.tools,
            model_tier: fm.model_tier,
            max_turns: fm.max_turns,
            role: fm.role,
            plugin_id: String::new(),
            team_id: team_id.to_string(),
            display_name: fm.display_name,
            profession: fm.profession,
            avatar: fm.avatar,
            color: fm.color,
            file_name: abs_path.to_string_lossy().into_owned(),
            enabled: true,
            installed_at: now.to_string(),
            updated_at: now.to_string(),
            catalog_id: None,
            group_id: None,
        };
        store::upsert(&self.db, &rec)?;
        Ok(fm.name)
    }

    /// 专家详情：摘要 + 角色设定正文 + 用户引导语（读盘解析 frontmatter；读不到则空）。供详情页展示。
    pub fn detail(&self, id: &str) -> Result<crate::expert::ExpertDetail, String> {
        let rec = store::get_by_id(&self.db, id)?.ok_or_else(|| format!("专家不存在：{id}"))?;
        let summary: ExpertSummary = rec.clone().into();
        let md = if rec.plugin_id.is_empty() && rec.team_id.is_empty() {
            self.root.join(&rec.file_name)
        } else {
            std::path::PathBuf::from(&rec.file_name)
        };
        let (system_prompt, quick_prompts) = match std::fs::read_to_string(&md) {
            Ok(content) => {
                let qp = frontmatter::parse_frontmatter(&content)
                    .map(|f| f.quick_prompts)
                    .unwrap_or_default();
                (frontmatter::strip_frontmatter(&content), qp)
            }
            Err(_) => (String::new(), Vec::new()),
        };
        Ok(crate::expert::ExpertDetail {
            agent: summary,
            system_prompt,
            quick_prompts,
            skills: Vec::new(), // 由命令层按 owner=agent name 填充
        })
    }

    /// 把一条索引行解析成 ExpertSpec（读正文）。owner 都空 → 相对 root；否则 file_name 存绝对路径。
    fn spec_from_record(&self, rec: ExpertRecord) -> Option<ExpertSpec> {
        let md = if rec.plugin_id.is_empty() && rec.team_id.is_empty() {
            self.root.join(&rec.file_name)
        } else {
            std::path::PathBuf::from(&rec.file_name)
        };
        let content = std::fs::read_to_string(&md).ok()?;
        Some(ExpertSpec {
            name: rec.name,
            description: rec.description,
            system_prompt: frontmatter::strip_frontmatter(&content),
            tools: rec.tools,
            model_tier: rec.model_tier,
            max_turns: rec.max_turns,
            role: rec.role,
            plugin_id: rec.plugin_id,
            team_id: rec.team_id,
        })
    }
}

/// 序列化一个 agent 的 `.md`（frontmatter + body），格式与 `frontmatter::parse_frontmatter` 对齐。
/// 约定单行字段值（description/display 等不含换行）；body 为 system prompt 原文。供散装/团队私有创建共用。
#[allow(clippy::too_many_arguments)]
pub fn serialize_expert_md(
    name: &str,
    description: &str,
    system_prompt: &str,
    tools: &[String],
    model_tier: &str,
    display_name: Option<&str>,
    profession: Option<&str>,
    avatar: Option<&str>,
    quick_prompts: &[String],
    catalog_id: Option<&str>,
) -> String {
    let mut fm = String::from("---\n");
    fm.push_str(&format!("name: {name}\n"));
    fm.push_str(&format!(
        "description: {}\n",
        description.replace('\n', " ")
    ));
    if !tools.is_empty() {
        fm.push_str(&format!("tools: [{}]\n", tools.join(", ")));
    }
    fm.push_str(&format!("model: {model_tier}\n"));
    if let Some(d) = display_name.filter(|s| !s.is_empty()) {
        fm.push_str(&format!("display_name: {d}\n"));
    }
    if let Some(p) = profession.filter(|s| !s.is_empty()) {
        fm.push_str(&format!("profession: {p}\n"));
    }
    if let Some(a) = avatar.filter(|s| !s.is_empty()) {
        fm.push_str(&format!("avatar: {a}\n"));
    }
    if !quick_prompts.is_empty() {
        // 单行 JSON 数组，安全容纳含逗号/冒号/引号的提示词。
        if let Ok(json) = serde_json::to_string(quick_prompts) {
            fm.push_str(&format!("quick_prompts: {json}\n"));
        }
    }
    if let Some(cid) = catalog_id.filter(|s| !s.is_empty()) {
        fm.push_str(&format!("catalog_id: {cid}\n"));
    }
    fm.push_str("---\n\n");
    fm.push_str(system_prompt);
    if !system_prompt.ends_with('\n') {
        fm.push('\n');
    }
    fm
}

/// 把物化 skill 的文件集（相对路径 → 字节）写到 `skill_dir`，按需创建父目录。
/// 供「加入我的」物化广场携带 skill（agent 私有 / team 私有共用）。
pub fn write_skill_files(
    skill_dir: &std::path::Path,
    files: &[(String, Vec<u8>)],
) -> Result<(), String> {
    std::fs::create_dir_all(skill_dir).map_err(|e| format!("创建 skill 目录失败：{e}"))?;
    for (rel, bytes) in files {
        let dest = skill_dir.join(rel);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("创建 skill 子目录失败：{e}"))?;
        }
        std::fs::write(&dest, bytes).map_err(|e| format!("写 skill 文件失败：{e}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// 直接写一行 agents（绕过只建散装的 `create_standalone`），用于造 owner 三态。
    fn upsert_owned(svc: &ExpertService, name: &str, plugin_id: &str, team_id: &str) {
        let rec = ExpertRecord {
            id: format!("exp-{name}"),
            source: if plugin_id.is_empty() {
                ExpertSource::User
            } else {
                ExpertSource::Plugin
            },
            name: name.to_string(),
            description: String::new(),
            tools: vec![],
            model_tier: "aux".into(),
            max_turns: None,
            role: "member".into(),
            plugin_id: plugin_id.to_string(),
            team_id: team_id.to_string(),
            display_name: None,
            profession: None,
            avatar: None,
            color: None,
            file_name: format!("{name}.md"),
            enabled: true,
            installed_at: "0".into(),
            updated_at: "0".into(),
            catalog_id: None,
            group_id: None,
        };
        store::upsert(&svc.db, &rec).expect("upsert");
    }

    #[test]
    fn indexes_frontmatter_less_plugin_agent_by_filename() {
        // Codex 插件（figma）的 agent .md **没有 YAML frontmatter**：应按文件名 + 正文首行兜底索引，
        // 而不是整条丢弃（T106 P4 / T104 §3.3）。
        let svc = service();
        let dir = std::env::temp_dir().join(format!(
            "siw-fmless-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("design-reviewer.md");
        std::fs::write(&f, "# 设计审查专家\n\n负责审查 Figma 设计稿。\n").unwrap();

        let name = svc
            .index_plugin_expert("plg-figma", &f, "0")
            .expect("无 frontmatter 也应索引成功");
        assert_eq!(name, "design-reviewer", "name 取文件名");

        let listed = svc.list_by_plugin("plg-figma").expect("list");
        let a = listed
            .iter()
            .find(|a| a.name == "design-reviewer")
            .expect("已落库");
        assert_eq!(
            a.description, "设计审查专家",
            "description 取正文首行（去 # 标记）"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_manageable_includes_plugin_owned_excludes_team_private() {
        let svc = service();
        upsert_owned(&svc, "solo", "", ""); // 散装
        upsert_owned(&svc, "from-plugin", "plg-1", ""); // plugin 提供
        upsert_owned(&svc, "team-only", "", "team-1"); // team 私有

        let names: Vec<String> = svc
            .list_manageable()
            .expect("list_manageable")
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert!(names.contains(&"solo".to_string()), "散装应在");
        assert!(
            names.contains(&"from-plugin".to_string()),
            "plugin 提供的应在（扩展页要显示全部）"
        );
        assert!(
            !names.contains(&"team-only".to_string()),
            "team 私有的不应进扩展页"
        );

        // 对照：list_standalone 仍只给散装。
        let solo_only: Vec<String> = svc
            .list_standalone()
            .expect("list_standalone")
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert!(!solo_only.contains(&"from-plugin".to_string()));
    }

    /// 缺陷① 回归：全局池必须在 **SQL 层**排除 team 私有专家。
    ///
    /// 此前 `list_enabled` 没有 owner 过滤，而 UI 的 `list_manageable` 却在内存里过滤了——
    /// 两个口径不一致，team 私有专家「UI 看不见、却会漏进全局池」。
    #[test]
    fn list_enabled_excludes_team_private() {
        let svc = service();
        upsert_owned(&svc, "solo", "", "");
        upsert_owned(&svc, "from-plugin", "plg-1", "");
        upsert_owned(&svc, "team-only", "", "team-1");

        let names: Vec<String> = svc
            .list_enabled()
            .expect("list_enabled")
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert!(names.contains(&"solo".to_string()), "散装是公开的");
        assert!(
            names.contains(&"from-plugin".to_string()),
            "plugin 提供的是公开的（标准：与用户自己的 agent 同级）"
        );
        assert!(
            !names.contains(&"team-only".to_string()),
            "team 私有专家绝不能进全局池——这是隔离的地基"
        );
    }

    /// 造一行专家**并写出其定义文件** —— `spec_from_record` 会真去读盘，只写库拿不到 spec。
    /// id 带 owner 后缀：同名不同 owner 是合法的（唯一键是 `(plugin_id, team_id, name)`），
    /// 但主键 id 必须唯一。
    fn upsert_with_file(svc: &ExpertService, name: &str, plugin_id: &str, team_id: &str) {
        let standalone = plugin_id.is_empty() && team_id.is_empty();
        let owner = if standalone {
            "solo".to_string()
        } else {
            format!("{plugin_id}{team_id}")
        };
        std::fs::create_dir_all(&svc.root).ok();
        // 散装：file_name 是相对 root 的名字；其余：绝对路径（见 spec_from_record）。
        let (file_name, abs) = if standalone {
            let f = format!("{name}.md");
            let abs = svc.root.join(&f);
            (f, abs)
        } else {
            let abs = svc.root.join(format!("{owner}-{name}.md"));
            (abs.to_string_lossy().into_owned(), abs)
        };
        std::fs::write(&abs, format!("{name} 的人设正文")).expect("write md");

        let rec = ExpertRecord {
            id: format!("exp-{owner}-{name}"),
            source: if plugin_id.is_empty() {
                ExpertSource::User
            } else {
                ExpertSource::Plugin
            },
            name: name.to_string(),
            description: String::new(),
            tools: vec![],
            model_tier: "aux".into(),
            max_turns: None,
            role: "member".into(),
            plugin_id: plugin_id.to_string(),
            team_id: team_id.to_string(),
            display_name: None,
            profession: None,
            avatar: None,
            color: None,
            file_name,
            enabled: true,
            installed_at: "0".into(),
            updated_at: "0".into(),
            catalog_id: None,
            group_id: None,
        };
        store::upsert(&svc.db, &rec).expect("upsert");
    }

    /// 缺陷⑤ 回归：自由模式按名解析只认**公开**专家。
    ///
    /// 三条一起锁：plugin 专家可派发（标准要求）、team 私有派不动（隔离）、
    /// 同名歧义报错而不是静默挑一个（挑错了模型会拿着别人的人设干活且无从察觉）。
    #[test]
    fn resolve_public_by_name_covers_plugin_and_never_team_private() {
        let svc = service();
        upsert_with_file(&svc, "solo", "", "");
        upsert_with_file(&svc, "from-plugin", "plg-1", "");
        upsert_with_file(&svc, "team-only", "", "team-1");

        assert!(
            svc.resolve_public_spec_by_name("solo")
                .expect("solo")
                .is_some(),
            "散装可派发"
        );
        assert!(
            svc.resolve_public_spec_by_name("from-plugin")
                .expect("plugin")
                .is_some(),
            "plugin 公开专家可派发（此前闸门只认散装，一律报「没有现成专家」）"
        );
        assert!(
            svc.resolve_public_spec_by_name("team-only")
                .expect("team")
                .is_none(),
            "team 私有专家派不动——加载器过去走裸 get_by_name，是敞开的"
        );
        assert!(
            svc.resolve_public_spec_by_name("nobody")
                .expect("none")
                .is_none(),
            "查无此人 → None（交由上层走临场定义临时专家）"
        );
    }

    #[test]
    fn resolve_public_by_name_errors_on_ambiguity_and_prefers_standalone() {
        let svc = service();
        upsert_with_file(&svc, "reviewer", "plg-a", "");
        upsert_with_file(&svc, "reviewer", "plg-b", "");

        // 不用 expect_err：ExpertSpec 无 Debug（人设正文不该被格式化进日志）。
        let err = match svc.resolve_public_spec_by_name("reviewer") {
            Err(e) => e,
            Ok(_) => panic!("两个插件同名必须报歧义，不得静默挑一个"),
        };
        assert!(err.contains("plg-a") && err.contains("plg-b"), "要列出候选");

        // 散装优先：用户自己的东西压过插件带来的，歧义随之消失。
        upsert_with_file(&svc, "reviewer", "", "");
        let hit = svc
            .resolve_public_spec_by_name("reviewer")
            .expect("散装优先应消解歧义")
            .expect("命中");
        assert_eq!(hit.name, "reviewer");
    }

    /// T108 §6：**限定名**（`plugin:agent`）能精确定位到某个 plugin 的专家 —— 这才是
    /// 同名歧义的正解（歧义报错列出的候选就是这个形态，用户/模型可直接照着用）。
    #[test]
    fn resolve_public_by_qualified_name_pins_the_right_plugin() {
        let svc = service();
        // ExpertService::new 只建 experts 表；限定名解析要 join plugins，得先建它。
        crate::plugin::store::ensure_schema(&svc.db).expect("plugins schema");
        // 两个 plugin 各带一个同名专家；plugins 表里登记它们的 name。
        for (pid, pname) in [("plg-a", "alpha"), ("plg-b", "beta")] {
            crate::plugin::store::upsert(
                &svc.db,
                &crate::plugin::model::PluginRecord {
                    id: pid.into(),
                    source: crate::plugin::model::PluginSource::User,
                    name: pname.into(),
                    display_name: pname.into(),
                    version: "1.0.0".into(),
                    description: String::new(),
                    description_zh: None,
                    category: None,
                    customized_from: None,
                    dir_name: pname.into(),
                    enabled: true,
                    installed_at: "0".into(),
                    updated_at: "0".into(),
                },
            )
            .expect("plugin row");
            upsert_with_file(&svc, "reviewer", pid, "");
        }

        // 裸名 → 歧义，且候选必须是**可直接照着用的限定名**。
        let err = match svc.resolve_public_spec_by_name("reviewer") {
            Err(e) => e,
            Ok(_) => panic!("裸名应报歧义"),
        };
        assert!(
            err.contains("alpha:reviewer") && err.contains("beta:reviewer"),
            "候选要列限定名（而不是内部 plugin_id）：{err}"
        );

        // 限定名 → 精确命中，歧义消解。
        assert!(
            svc.resolve_public_spec_by_name("alpha:reviewer")
                .expect("限定名不该报错")
                .is_some(),
            "alpha:reviewer 应命中"
        );
        assert!(
            svc.resolve_public_spec_by_name("beta:reviewer")
                .expect("限定名不该报错")
                .is_some(),
            "beta:reviewer 应命中"
        );
        // 不存在的前缀 → 退回裸名查找 → 仍是歧义（而不是悄悄命中某一个）。
        assert!(
            svc.resolve_public_spec_by_name("nosuch:reviewer").is_err()
                || svc
                    .resolve_public_spec_by_name("nosuch:reviewer")
                    .unwrap()
                    .is_none(),
            "未知前缀不得静默命中某个候选"
        );
    }

    /// 缺陷② 回归：卸载插件要清掉它带来的专家（此前 `delete_by_plugin` 压根不存在）。
    #[test]
    fn delete_by_plugin_clears_only_that_plugin() {
        let svc = service();
        upsert_owned(&svc, "solo", "", "");
        upsert_owned(&svc, "a1", "plg-a", "");
        upsert_owned(&svc, "b1", "plg-b", "");

        svc.delete_by_plugin("plg-a").expect("delete_by_plugin");

        let left: Vec<String> = svc
            .list_manageable()
            .expect("list")
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert!(!left.contains(&"a1".to_string()), "plg-a 的专家应被清掉");
        assert!(left.contains(&"b1".to_string()), "别的插件不受影响");
        assert!(left.contains(&"solo".to_string()), "散装不受影响");
    }

    fn service() -> ExpertService {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let dbp =
            std::env::temp_dir().join(format!("siw-agent-svc-{}-{}.db", std::process::id(), nanos));
        let _ = std::fs::remove_file(&dbp);
        let root = std::env::temp_dir().join(format!(
            "siw-agent-svc-root-{}-{}",
            std::process::id(),
            nanos
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        // 方案B 无内置预置——测试用一个散装 probe 专家验证扫盘/读取/启停。
        std::fs::write(
            root.join("probe.md"),
            "---\nname: probe\ndescription: 探针\ntools: [read_file, grep]\nmodel: aux\nmax_turns: 5\nrole: member\n---\n勘探探针专家。\n",
        )
        .unwrap();
        let db = Arc::new(crate::storage::AppDatabase::open(&dbp).unwrap());
        ExpertService::new(db, root)
    }

    #[test]
    fn sync_scans_standalone_and_lists() {
        let svc = service();
        svc.sync().expect("sync");
        let names: Vec<_> = svc
            .list()
            .expect("list")
            .into_iter()
            .map(|s| s.name)
            .collect();
        assert!(names.contains(&"probe".to_string()));
        assert!(svc.list_enabled().expect("le").len() >= 1);
    }

    #[test]
    fn load_spec_returns_body_and_meta() {
        let svc = service();
        svc.sync().expect("sync");
        let spec = svc.load_spec("probe", None).expect("load").expect("some");
        assert_eq!(spec.name, "probe");
        assert!(spec.system_prompt.contains("勘探"));
        assert!(spec.tools.contains(&"grep".to_string()));
        assert_eq!(spec.model_tier, "aux");
        assert_eq!(spec.plugin_id, "");
    }

    #[test]
    fn plugin_agent_indexed_and_loaded() {
        let svc = service();
        svc.sync().expect("sync");
        // 把一个 plugin 专家定义写到 root 外的临时文件，按绝对路径索引。
        let pdir = std::env::temp_dir().join(format!(
            "siw-plg-{}-{}",
            std::process::id(),
            svc.root.to_string_lossy().len()
        ));
        let _ = std::fs::create_dir_all(&pdir);
        let f = pdir.join("lead.md");
        std::fs::write(
            &f,
            "---\nname: lead\ndescription: 主理\ntools: [web_search]\nmodel: main\ndisplay_name: 何执舟\nprofession: 首席策略官\n---\n你是团队主理人。\n",
        )
        .unwrap();
        let name = svc
            .index_plugin_expert("plg-trade", &f, "1")
            .expect("index");
        assert_eq!(name, "lead");

        // 命名空间解析：active=plg-trade 命中 plugin 版；正文从绝对路径读到。
        let spec = svc
            .load_spec("lead", Some("plg-trade"))
            .expect("load")
            .expect("some");
        assert_eq!(spec.plugin_id, "plg-trade");
        assert!(spec.system_prompt.contains("主理人"));

        // Summary 带展示身份。
        let listed = svc.list_enabled_by_plugin("plg-trade").expect("lbp");
        let lead = listed.iter().find(|s| s.name == "lead").expect("lead");
        assert_eq!(lead.display_name.as_deref(), Some("何执舟"));
        assert_eq!(lead.profession.as_deref(), Some("首席策略官"));

        // 散装命名空间（None / 空）查不到 plugin 专家。
        assert!(svc.load_spec("lead", None).expect("l2").is_none());
    }

    #[test]
    fn load_spec_prefers_active_plugin() {
        let svc = service();
        svc.sync().expect("sync");
        // 散装也有个 "dup"；plugin 也有个 "dup"——active 命中 plugin 版。
        std::fs::write(
            svc.root.join("dup.md"),
            "---\nname: dup\ndescription: 散装版\n---\n散装正文。\n",
        )
        .unwrap();
        svc.sync().expect("sync2");
        let pf = std::env::temp_dir().join(format!("siw-dup-{}.md", std::process::id()));
        std::fs::write(
            &pf,
            "---\nname: dup\ndescription: 插件版\n---\n插件正文。\n",
        )
        .unwrap();
        svc.index_plugin_expert("plg-x", &pf, "1").expect("idx");

        let active = svc
            .load_spec("dup", Some("plg-x"))
            .expect("la")
            .expect("some");
        assert!(active.system_prompt.contains("插件正文"));
        let free = svc.load_spec("dup", None).expect("lf").expect("some");
        assert!(free.system_prompt.contains("散装正文"));
    }

    #[test]
    fn toggle_disables() {
        let svc = service();
        svc.sync().expect("sync");
        let all = svc.list().expect("list");
        let probe = all.iter().find(|s| s.name == "probe").unwrap();
        svc.toggle(&probe.id, false).expect("toggle");
        let names: Vec<_> = svc
            .list_enabled()
            .expect("le")
            .into_iter()
            .map(|s| s.name)
            .collect();
        assert!(!names.contains(&"probe".to_string()));
    }

    #[test]
    fn create_list_and_delete_standalone() {
        let svc = service();
        svc.sync().expect("sync");
        let created = svc
            .create_standalone(
                "投研助手",
                "看财报",
                "你是投研助手，输出结论/证据/风险。",
                vec!["read_file".into(), "web_search".into()],
                "main",
                Some("老周".into()),
                Some("首席分析师".into()),
                None,
                vec![
                    "帮我分析这家公司的财报".into(),
                    "帮我看这只股票的风险".into(),
                ],
                None,
            )
            .expect("create");
        assert_eq!(created.name, "投研助手");
        assert_eq!(created.team_id, "");
        assert_eq!(created.plugin_id, "");

        // 出现在散装列表，且可被 load_spec 读到正文（写盘格式正确）。
        assert!(svc
            .list_standalone()
            .expect("ls")
            .iter()
            .any(|a| a.name == "投研助手"));
        let spec = svc.load_spec("投研助手", None).expect("ls").expect("some");
        assert!(spec.system_prompt.contains("投研助手"));
        assert_eq!(spec.model_tier, "main");
        assert!(spec.tools.contains(&"web_search".to_string()));

        // 引导语写盘并能从详情读回（含逗号也安全）。
        let detail = svc.detail(&created.id).expect("detail");
        assert_eq!(detail.quick_prompts.len(), 2);
        assert_eq!(detail.quick_prompts[0], "帮我分析这家公司的财报");

        // 同名再建报错。
        assert!(svc
            .create_standalone(
                "投研助手",
                "x",
                "y",
                vec![],
                "aux",
                None,
                None,
                None,
                vec![],
                None
            )
            .is_err());

        // 删除：文件 + 行都没了。
        svc.delete_standalone(&created.id).expect("del");
        assert!(!svc
            .list_standalone()
            .expect("ls2")
            .iter()
            .any(|a| a.name == "投研助手"));
    }

    #[test]
    fn attach_private_skills_skips_traversal_name() {
        let svc = service();
        svc.sync().expect("sync");
        crate::skill::store::ensure_schema(&svc.db).expect("skill schema");
        let good = crate::market::MaterializedSkill {
            name: "good-skill".into(),
            description: "正常".into(),
            user_invocable: true,
            argument_hint: None,
            files: vec![(
                "SKILL.md".into(),
                b"---\nname: good-skill\ndescription: d\n---\nbody\n".to_vec(),
            )],
        };
        let evil = crate::market::MaterializedSkill {
            name: "../../evil".into(),
            description: "恶意".into(),
            user_invocable: true,
            argument_hint: None,
            files: vec![("SKILL.md".into(), b"---\nname: e\n---\nbody\n".to_vec())],
        };
        // 穿越目标：root/<expert>/skills/../../evil → root/evil。
        let traversal_target = svc.root.join("evil");
        svc.attach_private_skills("投研", vec![good, evil])
            .expect("attach 不因坏 skill 整体失败");
        // 坏 skill 被跳过：root 外无 evil 目录。
        assert!(
            !traversal_target.exists(),
            "穿越目标不得被创建：{}",
            traversal_target.display()
        );
        // 好 skill 仍落地并被索引。
        let rows = crate::skill::store::list_by_expert(&svc.db, "投研").expect("by expert");
        assert_eq!(rows.len(), 1, "仅好 skill 入索引");
        assert_eq!(rows[0].name, "good-skill");
        let _ = std::fs::remove_dir_all(&svc.root);
    }
}

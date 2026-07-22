//! SkillService：skill 模块面向 command 与 runtime 的受控操作入口。
//!
//! 持有 db（索引）与技能根目录（磁盘事实源）。sync 物化内置 + 扫描磁盘 upsert + 清孤儿。

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use crate::session::new_id;
use crate::skill::model::{SkillRecord, SkillSource};
use crate::skill::types::{SkillDetail, SkillFile, SkillFilePreview, SkillSummary};
use crate::skill::{builtin, frontmatter, store};
use crate::storage::AppDatabase;

/// 技能服务。`root` = `{workspace_base}/skills`，所有技能（内置物化 + 用户安装）同处此目录。
pub struct SkillService {
    db: Arc<AppDatabase>,
    root: PathBuf,
}

impl SkillService {
    /// 构造服务并确保索引表存在（不自动 sync，sync 由启动流程显式调用）。
    pub fn new(db: Arc<AppDatabase>, root: PathBuf) -> Self {
        let _ = store::ensure_schema(&db);
        Self { db, root }
    }

    /// 当前 now（秒字符串），与全局审计列格式一致。
    fn now() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or_default()
            .to_string()
    }

    /// 启动同步：① 物化内置 → ② 扫描磁盘解析 frontmatter upsert → ③ 清理孤儿行。幂等。
    pub fn sync(&self) -> Result<(), String> {
        std::fs::create_dir_all(&self.root).map_err(|e| format!("创建技能根目录失败：{e}"))?;
        builtin::materialize(&self.root)?;
        let builtin_set: HashSet<String> = builtin::builtin_names().into_iter().collect();
        let now = Self::now();

        // 扫描磁盘技能目录。
        let mut seen: HashSet<String> = HashSet::new();
        for entry in std::fs::read_dir(&self.root).map_err(|e| format!("读技能根目录失败：{e}"))?
        {
            let entry = entry.map_err(|e| e.to_string())?;
            if !entry.path().is_dir() {
                continue;
            }
            let dir_name = entry.file_name().to_string_lossy().into_owned();
            let md_path = entry.path().join("SKILL.md");
            if !md_path.is_file() {
                continue;
            }
            let content = match std::fs::read_to_string(&md_path) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("[skill] 跳过 {dir_name}：读 SKILL.md 失败 {e}");
                    continue;
                }
            };
            let fm = match frontmatter::parse_frontmatter(&content) {
                Ok(fm) => fm,
                Err(e) => {
                    eprintln!("[skill] 跳过 {dir_name}：frontmatter 解析失败 {e}");
                    continue;
                }
            };
            let source = if builtin_set.contains(&fm.name) {
                SkillSource::Builtin
            } else {
                SkillSource::User
            };
            let rec = SkillRecord {
                id: new_id("skill"),
                source,
                name: fm.name.clone(),
                description: fm.description,
                dir_name,
                enabled: true, // 新行默认启用；已存在行 upsert 会保留原 enabled。
                installed_at: now.clone(),
                updated_at: now.clone(),
                plugin_id: None,
                team_id: None,
                expert_id: None,
                user_invocable: fm.user_invocable,
                argument_hint: fm.argument_hint,
                group_id: None, // 分组由 upsert 冲突保留，不从磁盘读。
            };
            store::upsert(&self.db, &rec)?;
            seen.insert(fm.name);
        }

        // 清理孤儿：仅清**散装** skill（plugin_id 与 team_id 均 None）中磁盘已无对应 name 的行。
        // 插件内 skill 由 PluginService 管理；team 私有 skill（team_id=Some）导入后常驻、不参与本扫描。
        for rec in store::list(&self.db)? {
            if rec.plugin_id.is_none() && rec.team_id.is_none() && !seen.contains(&rec.name) {
                store::delete(&self.db, &rec.id)?;
            }
        }
        Ok(())
    }

    /// 列出技能管理页可见的技能（散装 + plugin 提供）。**team 私有 skill 不在此**——
    /// 它们属各自团队、随团队管理，不在「技能」页单独呈现。
    pub fn list(&self) -> Result<Vec<SkillSummary>, String> {
        Ok(store::list(&self.db)?
            .into_iter()
            .filter(|r| r.team_id.is_none())
            .map(Into::into)
            .collect())
    }

    /// `plugin_id → plugin name` 映射。**现算不冗余**：plugin 改名即刻反映，不会漂移。
    fn plugin_names(&self) -> std::collections::HashMap<String, String> {
        crate::plugin::store::list(&self.db)
            .unwrap_or_default()
            .into_iter()
            .map(|p| (p.id, p.name))
            .collect()
    }

    /// 给 plugin 提供的公开技能填上**限定名**（`plugin_name:name`，T108 §6）。
    ///
    /// 散装技能与私有技能不加前缀：前者本就是用户自己的东西，后者不进全局池、作用域内唯一。
    fn fill_qualified(&self, mut list: Vec<SkillSummary>) -> Vec<SkillSummary> {
        let names = self.plugin_names();
        for s in &mut list {
            if let Some(pid) = s.plugin_id.as_deref().filter(|p| !p.is_empty()) {
                if let Some(pname) = names.get(pid) {
                    s.qualified_name = Some(crate::plugin::namespace::qualify(pname, &s.name));
                }
            }
        }
        list
    }

    /// 列出**全局**启用技能（散装 + plugin 提供，即 team_id=''；供引擎注入 system prompt）。
    /// plugin 提供的带**限定名**（`plugin:name`）——否则同名技能在用户面与模型面都无从消歧。
    pub fn list_enabled(&self) -> Result<Vec<SkillSummary>, String> {
        Ok(self.fill_qualified(
            store::list_enabled(&self.db)?
                .into_iter()
                .map(Into::into)
                .collect(),
        ))
    }

    /// 列出某 team 的私有可见技能（供引擎在选中该 team 时追加入池）。
    pub fn list_enabled_by_team(&self, team_id: &str) -> Result<Vec<SkillSummary>, String> {
        Ok(store::list_enabled_by_team(&self.db, team_id)?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    /// 某 agent 的私有可见技能（agent 激活/派发时由引擎追加入池）。
    pub fn list_enabled_by_expert(&self, expert_id: &str) -> Result<Vec<SkillSummary>, String> {
        Ok(store::list_enabled_by_expert(&self.db, expert_id)?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    /// 某 team 的全部私有技能（含未启用）——供团队详情展示。
    pub fn list_by_team(&self, team_id: &str) -> Result<Vec<SkillSummary>, String> {
        Ok(store::list_by_team(&self.db, team_id)?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    /// 某 agent 的全部私有技能（含未启用）——供专家详情展示。
    pub fn list_by_expert(&self, expert_id: &str) -> Result<Vec<SkillSummary>, String> {
        Ok(store::list_by_expert(&self.db, expert_id)?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    /// 索引一个 agent 私有 skill：owner=agent name，`dir_name` 存**绝对路径**（在 skills 根之外，
    /// 与 plugin/team 私有 skill 同构，运行时按绝对路径加载）。供「导入 agent expert」用。返回 skill name。
    pub fn index_expert_skill(
        &self,
        expert_id: &str,
        skill_dir: &std::path::Path,
        now: &str,
    ) -> Result<String, String> {
        let content = std::fs::read_to_string(skill_dir.join("SKILL.md"))
            .map_err(|e| format!("读 SKILL.md 失败：{e}"))?;
        let fm = frontmatter::parse_frontmatter(&content)?;
        let rec = SkillRecord {
            id: new_id("skill"),
            source: SkillSource::User,
            name: fm.name.clone(),
            description: fm.description,
            dir_name: skill_dir.to_string_lossy().into_owned(),
            enabled: true,
            installed_at: now.into(),
            updated_at: now.into(),
            plugin_id: None,
            team_id: None,
            expert_id: Some(expert_id.to_string()),
            user_invocable: fm.user_invocable,
            argument_hint: fm.argument_hint,
            group_id: None,
        };
        store::upsert(&self.db, &rec)?;
        Ok(fm.name)
    }

    /// 切换技能启用状态。
    pub fn toggle(&self, id: &str, enabled: bool) -> Result<SkillSummary, String> {
        store::set_enabled(&self.db, id, enabled, &Self::now())?;
        store::get_by_id(&self.db, id)?
            .map(Into::into)
            .ok_or_else(|| format!("技能不存在：{id}"))
    }

    /// 设置技能的「我的」分组（None=移出）。
    pub fn set_group(&self, id: &str, group_id: Option<&str>) -> Result<(), String> {
        store::set_group(&self.db, id, group_id)
    }

    /// 把某分组下技能全部归零（删除分组时调用）。
    pub fn clear_group(&self, group_id: &str) -> Result<(), String> {
        store::clear_group(&self.db, group_id)
    }

    /// 按 name 读取技能正文（去 frontmatter）；不存在返回 None。供引擎 load_skill 使用。
    /// 解析顺序：散装 skill 优先，其次任意启用的插件内同名 skill（含隐藏的内部知识库 skill）。
    /// 按名定位技能行，**认限定名**（`plugin:name`）也认裸名。
    ///
    /// system prompt 里 plugin 技能列的是限定名，模型会照着调 `load_skill("figma:get-file")`。
    /// 解析器若只认裸名，plugin 技能就全废了 —— 呈现与解析必须同口径。
    fn resolve_record(&self, name: &str) -> Result<Option<SkillRecord>, String> {
        if let (Some(plugin_name), bare) = crate::plugin::namespace::split_qualified(name) {
            if let Some(p) = crate::plugin::store::get_by_name(&self.db, plugin_name)? {
                if let Some(r) = store::get_by_plugin_and_name(&self.db, &p.id, bare)? {
                    return Ok(Some(r));
                }
            }
            // 前缀没对上任何 plugin：可能技能名本身含冒号，退回按整串裸名找。
        }
        match store::get_by_name(&self.db, name)? {
            Some(r) => Ok(Some(r)),
            None => store::get_enabled_by_name_any(&self.db, name),
        }
    }

    pub fn load_body(&self, name: &str) -> Result<Option<String>, String> {
        let rec = self.resolve_record(name)?;
        let Some(rec) = rec else {
            return Ok(None);
        };
        let dir = self.skill_dir(&rec);
        let md = dir.join("SKILL.md");
        match std::fs::read_to_string(&md) {
            Ok(content) => Ok(Some(substitute_vars(
                &frontmatter::strip_frontmatter(&content),
                &dir,
            ))),
            Err(_) => Ok(None),
        }
    }

    /// 按 name 解析技能行（与 load_body 同序：散装优先，其次任意启用插件内同名）。
    fn resolve_by_name(&self, name: &str) -> Result<Option<SkillRecord>, String> {
        Ok(match store::get_by_name(&self.db, name)? {
            Some(r) => Some(r),
            None => store::get_enabled_by_name_any(&self.db, name)?,
        })
    }

    /// 列出某技能目录下的「附带文件」相对路径（渐进披露第三级：SKILL.md 之外的 references/scripts 等）。
    /// 仅文本类、排除 SKILL.md 与隐藏文件；递归；按路径排序；上限 50 条。供 load_skill 披露。
    pub fn list_reference_files(&self, name: &str) -> Result<Vec<String>, String> {
        let Some(rec) = self.resolve_by_name(name)? else {
            return Ok(Vec::new());
        };
        let root = self.skill_dir(&rec);
        let mut out: Vec<String> = Vec::new();
        collect_skill_files(&root, &root, &mut out);
        out.retain(|p| p != "SKILL.md");
        out.sort();
        out.truncate(50);
        Ok(out)
    }

    /// 读取某技能目录下的一个附带文件（路径限定在该技能目录内，拒绝 `..`/越界/绝对路径逃逸）。
    /// 返回文本内容（上限约 200KB，超出截断）；技能或文件不存在返回 None。
    pub fn read_reference_file(&self, name: &str, rel: &str) -> Result<Option<String>, String> {
        let Some(rec) = self.resolve_by_name(name)? else {
            return Ok(None);
        };
        let root = self.skill_dir(&rec);
        let rel_clean = rel.trim().trim_start_matches("./");
        if rel_clean.is_empty()
            || rel_clean.contains("..")
            || std::path::Path::new(rel_clean).is_absolute()
        {
            return Err("非法的技能文件路径".into());
        }
        let target = root.join(rel_clean);
        // 规整化后必须仍在技能目录内（防 symlink/拼接逃逸）。
        let canon_root = root.canonicalize().unwrap_or(root.clone());
        let canon_target = match target.canonicalize() {
            Ok(p) => p,
            Err(_) => return Ok(None),
        };
        if !canon_target.starts_with(&canon_root) {
            return Err("技能文件路径越界".into());
        }
        match std::fs::read_to_string(&canon_target) {
            Ok(mut c) => {
                const MAX: usize = 200 * 1024;
                if c.len() > MAX {
                    c.truncate(MAX);
                    c.push_str("\n…（已截断）");
                }
                Ok(Some(substitute_vars(&c, &root)))
            }
            Err(_) => Ok(None),
        }
    }

    /// 技能目录绝对路径（供详情与内部读取）。`dir_name` 为绝对路径（插件内 skill）时直接用，
    /// 否则相对 skills 根解析（散装 skill）。
    pub fn skill_dir(&self, rec: &SkillRecord) -> PathBuf {
        let p = std::path::Path::new(&rec.dir_name);
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            self.root.join(&rec.dir_name)
        }
    }

    /// 取技能行（不存在则报错）。
    fn record(&self, id: &str) -> Result<SkillRecord, String> {
        store::get_by_id(&self.db, id)?.ok_or_else(|| format!("技能不存在：{id}"))
    }

    /// 解析安装源 → (技能目录, frontmatter, zip 临时目录守卫)。
    /// path 为 .zip 文件或技能目录；校验 SKILL.md 可读、frontmatter 合法、name 合规。
    /// 守卫须在调用方存活到 copy 完成（zip 解压目录依赖它）。
    fn stage_source(
        &self,
        path: &str,
    ) -> Result<(PathBuf, frontmatter::Frontmatter, Option<TempDir>), String> {
        let src = PathBuf::from(path);
        if !src.exists() {
            return Err(format!("路径不存在：{path}"));
        }
        // zip → 解压到临时目录；目录 → 直接用。
        let mut tmp_guard: Option<TempDir> = None;
        let work_root = if src.is_file()
            && src
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.eq_ignore_ascii_case("zip"))
                == Some(true)
        {
            let tmp = TempDir::new("siw-skill-install")?;
            extract_zip(&src, tmp.path())?;
            let p = tmp.path().to_path_buf();
            tmp_guard = Some(tmp);
            p
        } else if src.is_dir() {
            src.clone()
        } else {
            return Err("仅支持 .zip 文件或技能目录".into());
        };

        let skill_root = locate_skill_root(&work_root)?;
        let content = std::fs::read_to_string(skill_root.join("SKILL.md"))
            .map_err(|e| format!("读 SKILL.md 失败：{e}"))?;
        let fm = frontmatter::parse_frontmatter(&content)?;
        validate_skill_name(&fm.name)?;
        Ok((skill_root, fm, tmp_guard))
    }

    /// 从本地路径安装：path 为 .zip 文件或技能目录。复制到 `{root}/<name>/`，写索引。
    /// 同名已存在则报错（不覆盖）。
    pub fn install_from_path(&self, path: &str) -> Result<SkillSummary, String> {
        self.install_or_update_from_path(path, false)
    }

    /// 安装或更新技能：`overwrite=false` 时同名报错；`true` 时仅覆盖 `User` 技能
    /// （保留其 id/enabled），拒绝覆盖内置。供模型侧 `install_skill` 工具使用。
    pub fn install_or_update_from_path(
        &self,
        path: &str,
        overwrite: bool,
    ) -> Result<SkillSummary, String> {
        let (skill_root, fm, _tmp_guard) = self.stage_source(path)?;
        let now = Self::now();
        let dest = self.root.join(&fm.name);

        let rec = match store::get_by_name(&self.db, &fm.name)? {
            Some(existing) => {
                if existing.source == SkillSource::Builtin {
                    return Err("内置技能不可覆盖".into());
                }
                if !overwrite {
                    return Err("技能名已存在（如需更新请允许覆盖）".into());
                }
                // 覆盖更新：删旧目录后重新复制，保留原 id/enabled/installed_at。
                let old_dir = self.root.join(&existing.dir_name);
                if old_dir.exists() {
                    std::fs::remove_dir_all(&old_dir)
                        .map_err(|e| format!("删除旧技能目录失败：{e}"))?;
                }
                copy_dir_all(&skill_root, &dest)?;
                SkillRecord {
                    id: existing.id,
                    source: SkillSource::User,
                    name: fm.name.clone(),
                    description: fm.description,
                    dir_name: fm.name.clone(),
                    enabled: existing.enabled,
                    installed_at: existing.installed_at,
                    updated_at: now,
                    plugin_id: None,
                    team_id: None,
                    expert_id: None,
                    user_invocable: fm.user_invocable,
                    argument_hint: fm.argument_hint,
                    group_id: existing.group_id, // 覆盖安装保留用户分组。
                }
            }
            None => {
                if dest.exists() {
                    return Err("技能名已存在".into());
                }
                copy_dir_all(&skill_root, &dest)?;
                SkillRecord {
                    id: new_id("skill"),
                    source: SkillSource::User,
                    name: fm.name.clone(),
                    description: fm.description,
                    dir_name: fm.name.clone(),
                    enabled: true,
                    installed_at: now.clone(),
                    updated_at: now,
                    plugin_id: None,
                    team_id: None,
                    expert_id: None,
                    user_invocable: fm.user_invocable,
                    argument_hint: fm.argument_hint,
                    group_id: None,
                }
            }
        };
        store::upsert(&self.db, &rec)?;
        store::get_by_name(&self.db, &fm.name)?
            .map(Into::into)
            .ok_or_else(|| "安装后读取索引失败".into())
    }

    /// 把一个物化技能装成**全局散装技能**（owner 三列皆空）。按 name 幂等（重装覆盖同名全局技能）。
    /// 文件写到 `{root}/<name>/`，再索引（mirror sync 的全局记录形状）。
    pub fn install_global(&self, mat: &crate::market::MaterializedSkill) -> Result<(), String> {
        // 不可信的远端 SKILL.md frontmatter `name` 会成为磁盘目录名：拒绝路径穿越。
        if !crate::market::wire::is_safe_component(&mat.name) {
            return Err(format!("非法技能名（疑路径穿越）：{}", mat.name));
        }
        let skill_dir = self.root.join(&mat.name);
        crate::expert::service::write_skill_files(&skill_dir, &mat.files)?;
        let now = Self::now();
        let rec = SkillRecord {
            id: new_id("skill"),
            source: SkillSource::User,
            name: mat.name.clone(),
            description: mat.description.clone(),
            dir_name: skill_dir.to_string_lossy().into_owned(),
            enabled: true,
            installed_at: now.clone(),
            updated_at: now,
            plugin_id: None,
            team_id: None,
            expert_id: None,
            user_invocable: mat.user_invocable,
            argument_hint: mat.argument_hint.clone(),
            group_id: None,
        };
        store::upsert(&self.db, &rec)
    }

    /// 技能详情：元数据 + SKILL.md 原文 + 目录全部文件列表（递归，按路径排序）。
    pub fn detail(&self, id: &str) -> Result<SkillDetail, String> {
        let rec = self.record(id)?;
        let dir = self.skill_dir(&rec);
        let skill_md = std::fs::read_to_string(dir.join("SKILL.md")).unwrap_or_default();
        let mut files: Vec<SkillFile> = Vec::new();
        collect_files(&dir, &dir, &mut files)?;
        files.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
        Ok(SkillDetail {
            skill: rec.into(),
            skill_md,
            files,
        })
    }

    /// 读取技能目录内单文件用于预览。做路径穿越防护（拒绝 `..`/绝对路径，且规范化后须仍在技能目录内）。
    pub fn read_file(&self, id: &str, rel_path: &str) -> Result<SkillFilePreview, String> {
        let rel = std::path::Path::new(rel_path);
        // 第一道：拒绝绝对路径与含 `..`/根 的路径分量。
        if rel.is_absolute()
            || rel.components().any(|c| {
                matches!(
                    c,
                    std::path::Component::ParentDir | std::path::Component::RootDir
                )
            })
        {
            return Err("非法文件路径".into());
        }
        let rec = self.record(id)?;
        let dir = self.skill_dir(&rec);
        let target = dir.join(rel);
        // 第二道：规范化后仍须落在技能目录内（防符号链接逃逸）。
        let canon_dir = std::fs::canonicalize(&dir).map_err(|e| e.to_string())?;
        let canon_target =
            std::fs::canonicalize(&target).map_err(|_| "文件不存在或无法访问".to_string())?;
        if !canon_target.starts_with(&canon_dir) {
            return Err("非法文件路径".into());
        }
        let name = canon_target
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        Ok(classify_preview(&canon_target, &name))
    }

    /// 卸载用户技能：删目录 + 删索引行。内置技能拒绝。
    pub fn uninstall(&self, id: &str) -> Result<(), String> {
        let rec = self.record(id)?;
        if rec.source == SkillSource::Builtin {
            return Err("内置技能不可卸载".into());
        }
        let dir = self.skill_dir(&rec);
        if dir.exists() {
            std::fs::remove_dir_all(&dir).map_err(|e| format!("删除技能目录失败：{e}"))?;
        }
        store::delete(&self.db, id)
    }
}

/// 定位 skill 根：根目录直接含 SKILL.md，或恰有唯一顶层子目录且其含 SKILL.md（兼容 zip 多一层）。
/// 替换技能正文里的模板占位符：
/// - `{{.DataDirName}}` → 数据目录名（`.siliconworker`），内置 skill 用它指代数据目录；
/// - `{{.SkillDir}}` → 该技能在本机的绝对目录，捆绑脚本类技能用它位置无关地引用自带脚本
///   （如 `node {{.SkillDir}}/scripts/index.js`）。加载时落为真实路径，模型可直接据此执行。
fn substitute_vars(body: &str, skill_dir: &std::path::Path) -> String {
    body.replace("{{.DataDirName}}", crate::skill::DATA_DIR_NAME)
        .replace("{{.SkillDir}}", &skill_dir.to_string_lossy())
}

/// 递归收集 `dir` 下所有文件相对 `root` 的路径（跳过隐藏文件/目录与 symlink）。供 list_reference_files。
fn collect_skill_files(root: &std::path::Path, dir: &std::path::Path, out: &mut Vec<String>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') {
            continue; // 跳过隐藏文件/目录
        }
        let path = entry.path();
        let ft = match entry.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };
        if ft.is_symlink() {
            continue;
        }
        if ft.is_dir() {
            collect_skill_files(root, &path, out);
        } else if ft.is_file() {
            if let Ok(rel) = path.strip_prefix(root) {
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
        if out.len() >= 200 {
            return; // 安全上限（最终再 truncate 到 50）
        }
    }
}

fn locate_skill_root(base: &std::path::Path) -> Result<PathBuf, String> {
    if base.join("SKILL.md").is_file() {
        return Ok(base.to_path_buf());
    }
    let mut subdirs: Vec<PathBuf> = Vec::new();
    for entry in std::fs::read_dir(base).map_err(|e| e.to_string())? {
        let p = entry.map_err(|e| e.to_string())?.path();
        if p.is_dir() {
            subdirs.push(p);
        }
    }
    if subdirs.len() == 1 && subdirs[0].join("SKILL.md").is_file() {
        return Ok(subdirs[0].clone());
    }
    Err("安装包未找到 SKILL.md（需在根目录或唯一顶层子目录内）".into())
}

/// 技能 name 合法性：非空、不含路径分隔符与 `..`、非 `.`，避免目录逃逸。
fn validate_skill_name(name: &str) -> Result<(), String> {
    if name.is_empty() || name == "." || name == ".." || name.contains('/') || name.contains('\\') {
        return Err(format!("非法技能名：{name}"));
    }
    Ok(())
}

/// 递归复制目录。
fn copy_dir_all(src: &std::path::Path, dest: &std::path::Path) -> Result<(), String> {
    std::fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let to = dest.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir_all(&entry.path(), &to)?;
        } else {
            std::fs::copy(entry.path(), &to).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// 解压 zip 到 dest，使用 `enclosed_name` 防 zip-slip（越界条目直接跳过）。
fn extract_zip(zip_path: &std::path::Path, dest: &std::path::Path) -> Result<(), String> {
    let file = std::fs::File::open(zip_path).map_err(|e| format!("打开 zip 失败：{e}"))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("读取 zip 失败：{e}"))?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let Some(rel) = entry.enclosed_name() else {
            continue; // 不安全路径（zip-slip）跳过
        };
        let out = dest.join(rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut outfile = std::fs::File::create(&out).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut outfile).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// 极简临时目录：Drop 时递归删除。安装解压用，避免残留。
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> Result<Self, String> {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let path = std::env::temp_dir().join(format!("{prefix}-{}-{}", std::process::id(), nanos));
        std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
        Ok(Self { path })
    }
    fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// 递归收集目录下全部条目（文件与子目录），rel_path 相对技能根目录。
fn collect_files(
    base: &std::path::Path,
    cur: &std::path::Path,
    out: &mut Vec<SkillFile>,
) -> Result<(), String> {
    for entry in std::fs::read_dir(cur).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let rel = path
            .strip_prefix(base)
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        let is_dir = path.is_dir();
        out.push(SkillFile {
            rel_path: rel,
            is_dir,
        });
        if is_dir {
            collect_files(base, &path, out)?;
        }
    }
    Ok(())
}

/// 可作文本预览的扩展名。
const TEXT_EXTS: &[&str] = &[
    "txt", "json", "yaml", "yml", "toml", "md", "markdown", "js", "ts", "tsx", "jsx", "py", "sh",
    "rs", "go", "rb", "java", "c", "h", "cpp", "css", "html", "xml", "ini", "cfg", "csv", "log",
];
/// 可作图片预览的扩展名。
const IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "svg", "bmp"];
const MAX_TEXT_BYTES: u64 = 512 * 1024;
const MAX_IMAGE_BYTES: u64 = 5 * 1024 * 1024;

/// 按扩展名与体积判定预览类型并读取内容。
fn classify_preview(path: &std::path::Path, name: &str) -> SkillFilePreview {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(u64::MAX);

    if IMAGE_EXTS.contains(&ext.as_str()) && size <= MAX_IMAGE_BYTES {
        if let Ok(bytes) = std::fs::read(path) {
            let mime = image_mime(&ext);
            let data_url = format!("data:{mime};base64,{}", base64_encode(&bytes));
            return SkillFilePreview {
                kind: "image".into(),
                text: None,
                data_url: Some(data_url),
                name: name.into(),
            };
        }
    }
    if (ext == "md" || ext == "markdown") && size <= MAX_TEXT_BYTES {
        if let Ok(text) = std::fs::read_to_string(path) {
            return SkillFilePreview {
                kind: "markdown".into(),
                text: Some(text),
                data_url: None,
                name: name.into(),
            };
        }
    }
    if TEXT_EXTS.contains(&ext.as_str()) && size <= MAX_TEXT_BYTES {
        if let Ok(text) = std::fs::read_to_string(path) {
            return SkillFilePreview {
                kind: "text".into(),
                text: Some(text),
                data_url: None,
                name: name.into(),
            };
        }
    }
    SkillFilePreview {
        kind: "binary".into(),
        text: None,
        data_url: None,
        name: name.into(),
    }
}

/// 图片 MIME 推断。
fn image_mime(ext: &str) -> &'static str {
    match ext {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        _ => "application/octet-stream",
    }
}

/// 标准 base64 编码（std 实现，避免新增 base64 crate）。
fn base64_encode(input: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((input.len() + 2) / 3 * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            T[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            T[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod reference_tests {
    use super::SkillService;
    use crate::storage::AppDatabase;
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn tmp(tag: &str) -> std::path::PathBuf {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let d =
            std::env::temp_dir().join(format!("siw-skref-{}-{}-{}", tag, std::process::id(), n));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn lists_and_reads_reference_files_confined() {
        let root = tmp("root");
        let db = Arc::new(AppDatabase::open(root.join("t.db")).unwrap());
        let svc = SkillService::new(db, root.join("skills"));
        // 造一个带 references 的技能包并安装。
        let pkg = tmp("pkg");
        std::fs::write(
            pkg.join("SKILL.md"),
            "---\nname: kref\ndescription: d\n---\n正文，详见 references/note.md\n",
        )
        .unwrap();
        std::fs::create_dir_all(pkg.join("references")).unwrap();
        std::fs::write(pkg.join("references").join("note.md"), "深层参考内容").unwrap();
        let s = svc
            .install_from_path(pkg.to_str().unwrap())
            .expect("install");

        // 列表：含 references/note.md，排除 SKILL.md。
        let files = svc.list_reference_files(&s.name).expect("list");
        assert!(
            files.iter().any(|f| f == "references/note.md"),
            "应列出 references/note.md: {files:?}"
        );
        assert!(!files.iter().any(|f| f == "SKILL.md"));

        // 读取：正常。
        let body = svc
            .read_reference_file(&s.name, "references/note.md")
            .expect("read");
        assert_eq!(body.as_deref(), Some("深层参考内容"));

        // 越界：拒绝。
        assert!(svc.read_reference_file(&s.name, "../escape.md").is_err());
        // 不存在文件：None。
        assert!(svc
            .read_reference_file(&s.name, "references/nope.md")
            .unwrap()
            .is_none());

        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&pkg);
    }

    #[test]
    fn substitutes_skill_dir_in_body_and_reference() {
        let root = tmp("skdroot");
        let db = Arc::new(AppDatabase::open(root.join("t.db")).unwrap());
        let svc = SkillService::new(db, root.join("skills"));
        // 造一个用 {{.SkillDir}} 占位引用自带脚本的技能包。
        let pkg = tmp("skdpkg");
        std::fs::write(
            pkg.join("SKILL.md"),
            "---\nname: skd\ndescription: d\n---\n运行：node {{.SkillDir}}/scripts/index.js quote sh600519\n",
        )
        .unwrap();
        std::fs::create_dir_all(pkg.join("references")).unwrap();
        std::fs::write(
            pkg.join("references").join("note.md"),
            "脚本在 {{.SkillDir}}/scripts/index.js",
        )
        .unwrap();
        let s = svc
            .install_from_path(pkg.to_str().unwrap())
            .expect("install");

        // 安装后技能的绝对目录（散装 skill 落在 skills 根下、目录名为 name）。
        let expected_dir = root.join("skills").join("skd");
        let expected = expected_dir.to_string_lossy().to_string();

        // 正文：{{.SkillDir}} 落为真实绝对目录，不再残留占位符。
        let body = svc.load_body(&s.name).expect("load").expect("some");
        assert!(
            body.contains(&format!("node {expected}/scripts/index.js quote sh600519")),
            "正文应替换为绝对路径: {body}"
        );
        assert!(!body.contains("{{.SkillDir}}"), "不应残留占位符: {body}");

        // 参考文件：同样替换。
        let ref_body = svc
            .read_reference_file(&s.name, "references/note.md")
            .expect("read")
            .expect("some");
        assert_eq!(
            ref_body,
            format!("脚本在 {expected}/scripts/index.js"),
            "参考文件也应替换 {{.SkillDir}}"
        );

        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&pkg);
    }
}

#[cfg(test)]
mod install_global_tests {
    use super::SkillService;
    use crate::market::MaterializedSkill;
    use crate::skill::store;
    use crate::storage::AppDatabase;
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn tmp(tag: &str) -> std::path::PathBuf {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let d =
            std::env::temp_dir().join(format!("siw-skglob-{}-{}-{}", tag, std::process::id(), n));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn mat(name: &str) -> MaterializedSkill {
        MaterializedSkill {
            name: name.into(),
            description: "全局技能描述".into(),
            user_invocable: true,
            argument_hint: None,
            files: vec![(
                "SKILL.md".into(),
                format!("---\nname: {name}\ndescription: 全局技能描述\n---\n正文\n").into_bytes(),
            )],
        }
    }

    fn service(root: &std::path::Path) -> (SkillService, Arc<AppDatabase>) {
        let db = Arc::new(AppDatabase::open(root.join("t.db")).unwrap());
        (SkillService::new(db.clone(), root.join("skills")), db)
    }

    #[test]
    fn install_global_lands_owner_empty() {
        let root = tmp("owner");
        let (svc, db) = service(&root);
        svc.install_global(&mat("g1")).expect("install");
        let rows = store::list(&db).expect("list");
        let row = rows.iter().find(|r| r.name == "g1").expect("row");
        assert!(row.plugin_id.is_none());
        assert!(row.team_id.is_none());
        assert!(row.expert_id.is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn install_global_writes_skill_md_and_is_findable() {
        let root = tmp("write");
        let (svc, _db) = service(&root);
        svc.install_global(&mat("g2")).expect("install");
        let md = root.join("skills").join("g2").join("SKILL.md");
        assert!(md.is_file(), "SKILL.md 应落盘");
        let body = std::fs::read_to_string(&md).unwrap();
        assert!(body.contains("name: g2"));
        // 可见：list / list_enabled 均含。
        assert!(svc.list().unwrap().iter().any(|s| s.name == "g2"));
        assert!(svc.list_enabled().unwrap().iter().any(|s| s.name == "g2"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn install_global_idempotent_by_name() {
        let root = tmp("idem");
        let (svc, db) = service(&root);
        svc.install_global(&mat("g3")).expect("install 1");
        svc.install_global(&mat("g3")).expect("install 2");
        let count = store::list(&db)
            .unwrap()
            .into_iter()
            .filter(|r| {
                r.name == "g3"
                    && r.plugin_id.is_none()
                    && r.team_id.is_none()
                    && r.expert_id.is_none()
            })
            .count();
        assert_eq!(count, 1, "重复安装同名全局技能应只有一行");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn install_global_rejects_traversal_name() {
        let root = tmp("traversal");
        let (svc, db) = service(&root);
        // 恶意远端 SKILL.md：frontmatter name 试图逃出 skills 根写到兄弟目录。
        let evil = MaterializedSkill {
            name: "../evil".into(),
            description: "x".into(),
            user_invocable: true,
            argument_hint: None,
            files: vec![("SKILL.md".into(), b"---\nname: e\n---\nbody\n".to_vec())],
        };
        // 穿越目标：{root}/skills 的兄弟目录 {root}/evil。
        let traversal_target = root.join("evil");
        assert!(svc.install_global(&evil).is_err(), "穿越 name 应被拒绝");
        assert!(
            !traversal_target.exists(),
            "不得在 skills 根之外创建目录：{}",
            traversal_target.display()
        );
        // 索引中也不应落任何行。
        assert!(store::list(&db).unwrap().is_empty(), "拒绝安装不应写索引");
        let _ = std::fs::remove_dir_all(&root);
    }
}

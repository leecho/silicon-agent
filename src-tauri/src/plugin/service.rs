//! PluginService：plugin 模块面向 command 与启动流程的受控操作入口。
//!
//! 持有 db（plugin + skill 索引）与两个磁盘根：`root`=plugins（用户/定制插件），
//! `builtin_root`=builtin-plugins（内置插件副本，本期无内容但保留扫描位）。
//! 插件内 skill 写入 skills 表，带 `plugin_id`，`dir_name` 存**绝对路径**（在 skills 根之外）。

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::plugin::manifest::{self, PluginManifest};
use crate::plugin::model::{PluginRecord, PluginSource};
use crate::plugin::store;
use crate::plugin::types::{PluginDetail, PluginSummary};
use crate::session::new_id;
use crate::skill::model::{SkillRecord, SkillSource};
use crate::skill::{frontmatter, store as skill_store};
use crate::storage::AppDatabase;

pub struct PluginService {
    db: Arc<AppDatabase>,
    root: PathBuf,
    builtin_root: PathBuf,
}

impl PluginService {
    /// 构造服务并确保 plugin/skill 索引表存在。
    pub fn new(db: Arc<AppDatabase>, root: PathBuf, builtin_root: PathBuf) -> Self {
        let _ = store::ensure_schema(&db);
        let _ = skill_store::ensure_schema(&db);
        Self {
            db,
            root,
            builtin_root,
        }
    }

    fn now() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or_default()
            .to_string()
    }

    /// 启动同步：扫 builtin_root（Builtin）+ root（User）→ 解析 manifest → upsert 插件与其 skill
    /// → 清理孤儿（磁盘已无的插件行 + 其 skill 行；manifest 已移除的 skill 行）。幂等。
    pub fn sync(&self) -> Result<(), String> {
        std::fs::create_dir_all(&self.root).map_err(|e| format!("创建 plugins 目录失败：{e}"))?;
        let mut seen_plugin_names: HashSet<String> = HashSet::new();

        for (scan_root, source) in [
            (self.builtin_root.clone(), PluginSource::Builtin),
            (self.root.clone(), PluginSource::User),
        ] {
            if !scan_root.is_dir() {
                continue;
            }
            for entry in std::fs::read_dir(&scan_root).map_err(|e| format!("读插件根失败：{e}"))?
            {
                let entry = entry.map_err(|e| e.to_string())?;
                let plugin_dir = entry.path();
                if !plugin_dir.is_dir() {
                    continue;
                }
                let dir_name = entry.file_name().to_string_lossy().into_owned();
                match self.index_plugin(&plugin_dir, &dir_name, source) {
                    Ok(name) => {
                        seen_plugin_names.insert(name);
                    }
                    Err(e) => eprintln!("[plugin] 跳过 {dir_name}：{e}"),
                }
            }
        }

        // 清理孤儿插件：索引存在但本轮未扫到（磁盘已删）。连带删其 skill 行。
        for p in store::list(&self.db)? {
            if !seen_plugin_names.contains(&p.name) {
                skill_store::delete_by_plugin(&self.db, &p.id)?;
                store::delete(&self.db, &p.id)?;
            }
        }
        Ok(())
    }

    /// 解析并索引一个插件目录（upsert 插件行 + 其声明的 skill 行 + 清理该插件下被移除的 skill）。
    /// 返回插件 name（用于孤儿判定）。
    fn index_plugin(
        &self,
        plugin_dir: &Path,
        dir_name: &str,
        source: PluginSource,
    ) -> Result<String, String> {
        let m = manifest::parse_plugin_dir(plugin_dir)?;
        let now = Self::now();

        // upsert 插件行；conflict(name) 保留既有 id，故 upsert 后回读取真实 id。
        let rec = PluginRecord {
            id: new_id("plugin"),
            source,
            name: m.name.clone(),
            display_name: m.display_name.clone(),
            version: m.version.clone(),
            description: m.description.clone(),
            description_zh: m.description_zh.clone(),
            category: m.category.clone(),
            customized_from: m.customized_from.clone(),
            dir_name: dir_name.to_string(),
            enabled: true,
            installed_at: now.clone(),
            updated_at: now.clone(),
        };
        store::upsert(&self.db, &rec)?;
        let plugin_id = store::get_by_name(&self.db, &m.name)?
            .map(|p| p.id)
            .ok_or("插件 upsert 后读取失败")?;

        if !m.commands.is_empty() {
            eprintln!(
                "[plugin] {}: 声明了 {} 个 commands，本期暂不加载（仅支持 skills）",
                m.name,
                m.commands.len()
            );
        }

        let seen_skills = self.index_plugin_skills(&plugin_id, plugin_dir, &m, source, &now)?;
        // 清理该插件下 manifest 已移除的 skill 行。
        for s in skill_store::list_by_plugin(&self.db, &plugin_id)? {
            if !seen_skills.contains(&s.name) {
                skill_store::delete(&self.db, &s.id)?;
            }
        }
        Ok(m.name)
    }

    /// 索引插件声明的 skills，返回成功索引的 skill name 集合。
    fn index_plugin_skills(
        &self,
        plugin_id: &str,
        plugin_dir: &Path,
        m: &PluginManifest,
        source: PluginSource,
        now: &str,
    ) -> Result<HashSet<String>, String> {
        let skill_source = match source {
            PluginSource::Builtin => SkillSource::Builtin,
            PluginSource::User => SkillSource::User,
        };
        let mut seen = HashSet::new();
        for skill_dir in resolve_skill_dirs(plugin_dir, &m.skills) {
            let md = skill_dir.join("SKILL.md");
            let content = match std::fs::read_to_string(&md) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!(
                        "[plugin] {}: 跳过 skill {}（读 SKILL.md 失败 {e}）",
                        m.name,
                        skill_dir.display()
                    );
                    continue;
                }
            };
            let fm = match frontmatter::parse_frontmatter(&content) {
                Ok(fm) => fm,
                Err(e) => {
                    eprintln!(
                        "[plugin] {}: 跳过 skill {}（frontmatter {e}）",
                        m.name,
                        skill_dir.display()
                    );
                    continue;
                }
            };
            // dir_name 存绝对路径（插件 skill 在 skills 根之外）；SkillService 按绝对路径加载。
            let abs = skill_dir.to_string_lossy().into_owned();
            let rec = SkillRecord {
                id: new_id("skill"),
                source: skill_source,
                name: fm.name.clone(),
                description: fm.description,
                dir_name: abs,
                enabled: true,
                installed_at: now.to_string(),
                updated_at: now.to_string(),
                plugin_id: Some(plugin_id.to_string()),
                team_id: None,
                expert_id: None,
                user_invocable: fm.user_invocable,
                argument_hint: fm.argument_hint,
                group_id: None,
            };
            skill_store::upsert(&self.db, &rec)?;
            seen.insert(fm.name);
        }
        Ok(seen)
    }

    /// 从本地路径安装插件：path 为插件目录或 .zip 压缩包；复制到 `plugins/{name}`，索引插件与其 skill。
    /// 同名已存在则报错（不覆盖）。
    pub fn install_from_path(&self, path: &str) -> Result<PluginSummary, String> {
        self.install_or_update_from_path(path, false)
    }

    /// 安装或更新插件：`overwrite=false` 时同名报错；`true` 时先卸载同名 `User` 插件
    /// （级联删其目录与 skill）再装新版，拒绝覆盖内置。供模型侧 `install_plugin` 工具使用。
    pub fn install_or_update_from_path(
        &self,
        path: &str,
        overwrite: bool,
    ) -> Result<PluginSummary, String> {
        let src = PathBuf::from(path);
        if !src.exists() {
            return Err(format!("路径不存在：{path}"));
        }
        // zip → 解压到临时目录并定位插件根；目录 → 直接用。守卫存活到 copy 完成。
        let mut _tmp_guard: Option<TempDir> = None;
        let plugin_dir = if src.is_file() && is_zip(&src) {
            let tmp = TempDir::new("siw-plugin-install")?;
            extract_zip(&src, tmp.path())?;
            let root = locate_plugin_root(tmp.path())?;
            _tmp_guard = Some(tmp);
            root
        } else if src.is_dir() {
            src.clone()
        } else {
            return Err("仅支持插件目录或 .zip 压缩包".into());
        };

        let m = manifest::parse_plugin_dir(&plugin_dir)?;
        // name 用作目录名，禁止路径分隔与穿越。
        if m.name.contains('/') || m.name.contains('\\') || m.name.contains("..") {
            return Err("插件 name 含非法字符".into());
        }
        if let Some(existing) = store::get_by_name(&self.db, &m.name)? {
            if existing.source == PluginSource::Builtin {
                return Err("内置插件不可覆盖".into());
            }
            if !overwrite {
                return Err("插件名已存在（如需更新请允许覆盖）".into());
            }
            // 覆盖：先卸载旧插件（删目录 + 级联删其 skill 与插件行）。
            self.uninstall(&existing.id)?;
        }
        std::fs::create_dir_all(&self.root).map_err(|e| e.to_string())?;
        let dest = self.root.join(&m.name);
        if dest.exists() {
            return Err("插件名已存在".into());
        }
        copy_dir_all(&plugin_dir, &dest)?;
        self.index_plugin(&dest, &m.name, PluginSource::User)?;
        self.summary_by_name(&m.name)
    }

    /// 被禁用插件的 id 集合（供引擎级联隐藏其 skill）。
    pub fn disabled_plugin_ids(&self) -> Result<HashSet<String>, String> {
        Ok(store::disabled_ids(&self.db)?.into_iter().collect())
    }

    /// 取插件展示名（供激活团队时在 prompt 显示团队名）；不存在返回 None。
    pub fn display_name_of(&self, id: &str) -> Option<String> {
        store::get_by_id(&self.db, id)
            .ok()
            .flatten()
            .map(|p| p.display_name)
    }

    /// 列出启用且声明了 `agents` 的插件，供启动流程把其 `agents/` 索引进 agents 表（全局可用）。
    /// 返回 `(plugin_id, 插件目录绝对路径, manifest)`；manifest 解析失败的插件跳过。
    pub fn list_agent_plugins(&self) -> Result<Vec<(String, PathBuf, PluginManifest)>, String> {
        let mut out = Vec::new();
        for p in store::list(&self.db)? {
            if !p.enabled {
                continue;
            }
            let base = match p.source {
                PluginSource::Builtin => &self.builtin_root,
                PluginSource::User => &self.root,
            };
            let plugin_dir = base.join(&p.dir_name);
            let m = match manifest::parse_plugin_dir(&plugin_dir) {
                Ok(m) => m,
                Err(_) => continue,
            };
            // 去 type 行为分流：凡声明了 agents 的 plugin 都把其 agents 索引为 plugin 提供的全局 agent，
            // 不再按 `type` gate（plugin = Claude 式能力包，能力不分类型）。
            if m.agents.is_empty() {
                continue;
            }
            out.push((p.id, plugin_dir, m));
        }
        Ok(out)
    }

    /// 列出**启用**的插件及其目录与 manifest，供启动流程摄取其 MCP server。
    /// 返回 `(plugin_id, 插件目录绝对路径, manifest)`；manifest 解析失败的插件跳过。
    pub fn list_all_with_dir(&self) -> Result<Vec<(String, PathBuf, PluginManifest)>, String> {
        let mut out = Vec::new();
        for p in store::list(&self.db)? {
            if !p.enabled {
                continue;
            }
            let base = match p.source {
                PluginSource::Builtin => &self.builtin_root,
                PluginSource::User => &self.root,
            };
            let plugin_dir = base.join(&p.dir_name);
            let m = match manifest::parse_plugin_dir(&plugin_dir) {
                Ok(m) => m,
                Err(_) => continue,
            };
            out.push((p.id, plugin_dir, m));
        }
        Ok(out)
    }

    /// 列出全部插件（含各自 skill 数）。
    pub fn list(&self) -> Result<Vec<PluginSummary>, String> {
        let mut out = Vec::new();
        for p in store::list(&self.db)? {
            let count = skill_store::list_by_plugin(&self.db, &p.id)?.len();
            out.push(PluginSummary::from_record(p, count));
        }
        Ok(out)
    }

    /// 插件详情：元数据 + 其下 skill 列表（`agents` 由命令层从 agents 表填充）。
    pub fn detail(&self, id: &str) -> Result<PluginDetail, String> {
        let p = store::get_by_id(&self.db, id)?.ok_or_else(|| format!("插件不存在：{id}"))?;
        let skills = skill_store::list_by_plugin(&self.db, &p.id)?
            .into_iter()
            .map(Into::into)
            .collect();
        let count = skill_store::list_by_plugin(&self.db, &p.id)?.len();
        // 解析插件目录补元数据；定位失败/解析失败则元数据留空，不报错。
        let base = match p.source {
            PluginSource::Builtin => &self.builtin_root,
            PluginSource::User => &self.root,
        };
        let meta = manifest::parse_plugin_dir(&base.join(&p.dir_name)).ok();
        let (author, homepage, repository, license, keywords, hooks) = match meta {
            Some(m) => {
                let hooks = m
                    .hooks
                    .into_iter()
                    .map(|h| crate::plugin::types::PluginHookSummary {
                        event: h.event,
                        matcher: h.matcher,
                        command: h.command,
                    })
                    .collect();
                (
                    m.author,
                    m.homepage,
                    m.repository,
                    m.license,
                    m.keywords,
                    hooks,
                )
            }
            None => (None, None, None, None, Vec::new(), Vec::new()),
        };
        Ok(PluginDetail {
            plugin: PluginSummary::from_record(p, count),
            skills,
            agents: Vec::new(),
            mcp_servers: Vec::new(), // 由命令层从 mcp store/状态填充
            hooks,
            author,
            homepage,
            repository,
            license,
            keywords,
        })
    }

    /// 切换插件启用状态（其下 skill 的可见性由引擎按 plugin.enabled 级联）。
    pub fn toggle(&self, id: &str, enabled: bool) -> Result<PluginSummary, String> {
        store::set_enabled(&self.db, id, enabled, &Self::now())?;
        let p = store::get_by_id(&self.db, id)?.ok_or_else(|| format!("插件不存在：{id}"))?;
        let count = skill_store::list_by_plugin(&self.db, &p.id)?.len();
        Ok(PluginSummary::from_record(p, count))
    }

    /// 卸载插件：删目录 + 级联删其 **skill 与 expert** 索引行 + 删插件行。内置插件拒绝。
    ///
    /// MCP 与 hooks 不在这里 —— 它们需要断连/热卸载，由 `commands::uninstall_plugin`
    /// 经 `AppFacade` 先行处理（本服务够不到那两个子系统）。
    ///
    /// **expert 此前完全没被清理**（T108 §8 缺陷②）：`delete_by_plugin` 压根不存在，
    /// 卸载后插件带来的专家全部留在库里变孤儿。现与 skill 对称处理。
    pub fn uninstall(&self, id: &str) -> Result<(), String> {
        let p = store::get_by_id(&self.db, id)?.ok_or_else(|| format!("插件不存在：{id}"))?;
        if p.source == PluginSource::Builtin {
            return Err("内置插件不可卸载".into());
        }
        let dir = self.root.join(&p.dir_name);
        if dir.exists() {
            std::fs::remove_dir_all(&dir).map_err(|e| format!("删除插件目录失败：{e}"))?;
        }
        skill_store::delete_by_plugin(&self.db, &p.id)?;
        crate::expert::store::delete_by_plugin(&self.db, &p.id)?;
        store::delete(&self.db, &p.id)
    }

    fn summary_by_name(&self, name: &str) -> Result<PluginSummary, String> {
        let p = store::get_by_name(&self.db, name)?.ok_or("安装后读取插件失败")?;
        let count = skill_store::list_by_plugin(&self.db, &p.id)?.len();
        Ok(PluginSummary::from_record(p, count))
    }
}

/// 递归复制目录。
fn copy_dir_all(src: &Path, dest: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let from = entry.path();
        let to = dest.join(entry.file_name());
        if from.is_dir() {
            copy_dir_all(&from, &to)?;
        } else {
            std::fs::copy(&from, &to).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// 路径是否为 .zip 文件（按扩展名，忽略大小写）。
fn is_zip(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("zip"))
        == Some(true)
}

/// 在解压后的目录里定位插件根：含 plugin.json 的目录（根 / `.claude-plugin/` / `.codex-plugin/`
/// / `.qoder-plugin/`，见 `manifest::PLUGIN_MANIFEST_CANDIDATES`）。
/// 支持 zip 根目录直接是插件，或唯一顶层子目录是插件（常见的「带一层包裹文件夹」）。
///
/// **必须与 `manifest::parse_plugin_dir` 用同一份候选表**：这里认得少一种方言，
/// 那种插件就在「定位包根」这一步被拒，根本走不到解析 —— 此前这里只认根与 `.claude-plugin/`，
/// 于是 Codex / Qoder 插件从 zip 安装必然失败。
fn locate_plugin_root(base: &Path) -> Result<PathBuf, String> {
    let has_manifest = |d: &Path| manifest::locate_plugin_manifest(d).is_some();
    if has_manifest(base) {
        return Ok(base.to_path_buf());
    }
    let mut subdirs: Vec<PathBuf> = Vec::new();
    for entry in std::fs::read_dir(base).map_err(|e| e.to_string())? {
        let p = entry.map_err(|e| e.to_string())?.path();
        if p.is_dir() {
            subdirs.push(p);
        }
    }
    if subdirs.len() == 1 && has_manifest(&subdirs[0]) {
        return Ok(subdirs[0].clone());
    }
    Err("安装包未找到 plugin.json（需在根目录或唯一顶层子目录内）".into())
}

/// 解压 zip 到 dest，用 `enclosed_name` 防 zip-slip（越界条目跳过）。
fn extract_zip(zip_path: &Path, dest: &Path) -> Result<(), String> {
    let file = std::fs::File::open(zip_path).map_err(|e| format!("打开 zip 失败：{e}"))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("读取 zip 失败：{e}"))?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let Some(rel) = entry.enclosed_name() else {
            continue;
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

/// 极简临时目录：Drop 时递归删除。zip 安装解压用。
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
    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// 解析插件声明的 skill 目录集合（去重，绝对/相对插件目录）。约定（对齐 Claude/Codex）：
/// - 候选根 = 默认 `skills/` + 清单 `skills` 字段各项（字符串/数组已在 manifest 归一）；
/// - 每个候选根 R：若 `R/SKILL.md` 存在 → R 本身即一个 skill 目录；否则若 R 是目录 → 扫描其
///   **直接子目录**，凡含 `SKILL.md` 者各为一个 skill。
/// 这样 `skills:"./skills/"`（Codex/Claude 目录写法）与 `skills:["skills/x"]`（历史数组写法）都正确。
fn resolve_skill_dirs(plugin_dir: &Path, hints: &[String]) -> Vec<PathBuf> {
    // 候选根：清单**给了** skills 项 → 只按给的（数组=权威列表，本项目既有语义；字符串=目录扫描）；
    // **未给** → 回退默认 `skills/` 扫描（Claude/Codex 默认约定）。
    let cleaned: Vec<&str> = hints
        .iter()
        .map(|h| h.trim().trim_start_matches("./").trim_end_matches('/'))
        .filter(|h| !h.is_empty())
        .collect();
    let mut roots: Vec<PathBuf> = Vec::new();
    if cleaned.is_empty() {
        roots.push(plugin_dir.join("skills"));
    } else {
        for h in cleaned {
            roots.push(plugin_dir.join(h));
        }
    }
    let mut out: Vec<PathBuf> = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();
    for root in roots {
        let candidates: Vec<PathBuf> = if root.join("SKILL.md").is_file() {
            vec![root]
        } else if root.is_dir() {
            let mut subs: Vec<PathBuf> = std::fs::read_dir(&root)
                .map(|rd| {
                    rd.flatten()
                        .map(|e| e.path())
                        .filter(|p| p.is_dir() && p.join("SKILL.md").is_file())
                        .collect()
                })
                .unwrap_or_default();
            subs.sort();
            subs
        } else {
            Vec::new()
        };
        for d in candidates {
            let key = d.canonicalize().unwrap_or_else(|_| d.clone());
            if seen.insert(key) {
                out.push(d);
            }
        }
    }
    out
}

#[cfg(test)]
mod skill_dir_tests {
    use super::resolve_skill_dirs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn tmp() -> std::path::PathBuf {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let d = std::env::temp_dir().join(format!("siw-plg-skills-{}-{}", std::process::id(), n));
        std::fs::create_dir_all(&d).unwrap();
        d
    }
    fn skill(dir: &std::path::Path, name: &str) {
        let s = dir.join("skills").join(name);
        std::fs::create_dir_all(&s).unwrap();
        std::fs::write(
            s.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: x\n---\n正文\n"),
        )
        .unwrap();
    }

    #[test]
    fn string_skills_dir_scans_subdirs() {
        // skills:"./skills/"（Codex 形态）→ 扫描 skills/ 下每个含 SKILL.md 的子目录。
        let p = tmp();
        skill(&p, "gmail");
        skill(&p, "gmail-inbox-triage");
        let dirs = resolve_skill_dirs(&p, &["./skills/".to_string()]);
        assert_eq!(dirs.len(), 2, "应扫到 2 个 skill 子目录");
        assert!(dirs.iter().any(|d| d.ends_with("gmail")));
        assert!(dirs.iter().any(|d| d.ends_with("gmail-inbox-triage")));
        let _ = std::fs::remove_dir_all(&p);
    }

    #[test]
    fn default_skills_dir_and_array_dedup() {
        // 无 hints 也扫默认 skills/；数组指向具体 skill 目录与默认扫描去重。
        let p = tmp();
        skill(&p, "alpha");
        assert_eq!(resolve_skill_dirs(&p, &[]).len(), 1);
        // 数组写法（历史）：指向具体 skill 目录，含 SKILL.md → 本身即一个 skill；与默认扫描去重。
        let dirs = resolve_skill_dirs(&p, &["skills/alpha".to_string()]);
        assert_eq!(dirs.len(), 1, "默认扫描 + 数组项应去重为 1");
        let _ = std::fs::remove_dir_all(&p);
    }
}

#[cfg(test)]
mod uninstall_cascade_tests {
    use super::*;
    use crate::expert::model::{ExpertRecord, ExpertSource};
    use crate::skill::model::{SkillRecord, SkillSource};
    use crate::storage::AppDatabase;

    fn tmp(tag: &str) -> PathBuf {
        use std::time::{SystemTime, UNIX_EPOCH};
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("siw-plg-{tag}-{}-{}", std::process::id(), n))
    }

    /// 缺陷② 回归：卸载插件必须把它带来的 **skill 与 expert 一并清掉**。
    ///
    /// expert 此前**完全没被清理** —— `delete_by_plugin` 压根不存在，卸载后插件带来的
    /// 专家全部留在库里变孤儿（还会继续出现在专家列表、还能被派发）。
    #[test]
    fn uninstall_clears_plugin_skills_and_experts() {
        let dbp = tmp("db");
        let db = Arc::new(AppDatabase::open(&dbp).expect("open"));
        crate::expert::store::ensure_schema(&db).expect("expert schema");

        let root = tmp("root");
        std::fs::create_dir_all(&root).unwrap();
        let svc = PluginService::new(db.clone(), root.clone(), tmp("builtin"));

        // 两个插件，各带 1 skill + 1 expert；只卸载 plg-a。
        for (pid, dir) in [("plg-a", "a"), ("plg-b", "b")] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
            store::upsert(
                &db,
                &PluginRecord {
                    id: pid.into(),
                    source: PluginSource::User,
                    name: pid.into(),
                    display_name: pid.into(),
                    version: "1.0.0".into(),
                    description: String::new(),
                    description_zh: None,
                    category: None,
                    customized_from: None,
                    dir_name: dir.into(),
                    enabled: true,
                    installed_at: "0".into(),
                    updated_at: "0".into(),
                },
            )
            .expect("plugin row");

            skill_store::upsert(
                &db,
                &SkillRecord {
                    id: format!("sk-{pid}"),
                    source: SkillSource::User,
                    name: format!("skill-{pid}"),
                    description: String::new(),
                    dir_name: format!("{dir}/skills/s"),
                    enabled: true,
                    installed_at: "0".into(),
                    updated_at: "0".into(),
                    plugin_id: Some(pid.into()),
                    team_id: None,
                    expert_id: None,
                    user_invocable: true,
                    argument_hint: None,
                    group_id: None,
                },
            )
            .expect("skill row");

            crate::expert::store::upsert(
                &db,
                &ExpertRecord {
                    id: format!("ex-{pid}"),
                    source: ExpertSource::Plugin,
                    name: format!("expert-{pid}"),
                    description: String::new(),
                    tools: vec![],
                    model_tier: "aux".into(),
                    max_turns: None,
                    role: "member".into(),
                    plugin_id: pid.into(),
                    team_id: String::new(),
                    display_name: None,
                    profession: None,
                    avatar: None,
                    color: None,
                    file_name: format!("{dir}/agents/x.md"),
                    enabled: true,
                    installed_at: "0".into(),
                    updated_at: "0".into(),
                    catalog_id: None,
                    group_id: None,
                },
            )
            .expect("expert row");
        }

        svc.uninstall("plg-a").expect("uninstall");

        assert!(
            skill_store::list_by_plugin(&db, "plg-a")
                .unwrap()
                .is_empty(),
            "plg-a 的技能应被清掉"
        );
        assert!(
            crate::expert::store::list_by_plugin(&db, "plg-a")
                .unwrap()
                .is_empty(),
            "plg-a 的专家应被清掉（此前完全没清，留孤儿）"
        );
        assert!(
            store::get_by_id(&db, "plg-a").unwrap().is_none(),
            "插件行应被删"
        );

        // 另一个插件毫发无损。
        assert_eq!(skill_store::list_by_plugin(&db, "plg-b").unwrap().len(), 1);
        assert_eq!(
            crate::expert::store::list_by_plugin(&db, "plg-b")
                .unwrap()
                .len(),
            1
        );

        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_file(&dbp);
    }
}

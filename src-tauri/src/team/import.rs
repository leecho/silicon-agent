//! 团队包导入解析：把一个「团队结构」的包（原生 / codebuddy / Claude 方言）解析成统一的
//! `ImportedTeam`，供 TeamService 落成 team + 其私有 agents/skills。
//!
//! 方言差异（清单位置见 `locate_manifest`：根 / .claude-plugin / .codebuddy-plugin /
//! .codex-plugin / .qoder-plugin）：
//! - 原生 / Claude：`plugin.json`（或 `.claude-plugin/plugin.json`）+ `team:{lead,members}` + `agents`/`skills`。
//! - codebuddy/workbuddy：`.codebuddy-plugin/plugin.json` + `expertType:"team"` + `teamInfo{leadAgent,memberAgents}`
//!   + `members[]`（含 i18n 展示身份）+ `quickPrompts`/`defaultInitPrompt`（i18n）。
//! i18n 字段（`{en,zh}` 或纯字符串）统一取 zh 优先。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// 成员展示覆盖（来自 codebuddy `members[]`，按 agent id 键）。
#[derive(Debug, Clone, Default)]
pub struct MemberDisplay {
    pub display_name: Option<String>,
    pub profession: Option<String>,
    pub avatar: Option<String>,
}

/// 解析后的团队包（统一形态）。
#[derive(Debug, Clone)]
pub struct ImportedTeam {
    pub name: String,
    pub display_name: String,
    pub description: String,
    /// agent 定义文件相对路径（相对包根）。
    pub agents: Vec<String>,
    /// skill 目录相对路径（相对包根）。
    pub skills: Vec<String>,
    /// lead 专家 name（可空）。
    pub lead: Option<String>,
    /// 成员 name（不含 lead，按声明顺序）。
    pub member_names: Vec<String>,
    /// 按 agent id/name 的展示覆盖。
    pub member_display: HashMap<String, MemberDisplay>,
    /// 开场引导语。
    pub quick_prompts: Vec<String>,
}

/// 在包目录定位 plugin.json：原生根 → `.claude-plugin/` → `.codebuddy-plugin/`
/// → `.codex-plugin/` → `.qoder-plugin/`。
///
/// 与 `plugin::manifest::PLUGIN_MANIFEST_CANDIDATES` **必须保持同步**：这里少认一种方言，
/// 那种包在「定位包根」这一步就被拒，连 `detect_package_kind` 都走不到。
/// （本函数额外认 `.codebuddy-plugin/` —— 那是团队方言，plugin 侧用不到。）
pub fn locate_manifest(pkg_dir: &Path) -> Option<PathBuf> {
    for rel in [
        "plugin.json",
        ".claude-plugin/plugin.json",
        ".codebuddy-plugin/plugin.json",
        ".codex-plugin/plugin.json",
        ".qoder-plugin/plugin.json",
    ] {
        let p = pkg_dir.join(rel);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

/// 定位 silicon **团队清单** `team.json`（T108 §4.0）。文件名即类型标记。
pub fn locate_team_manifest(pkg_dir: &Path) -> Option<PathBuf> {
    let p = pkg_dir.join("team.json");
    p.is_file().then_some(p)
}

/// 定位 silicon **专家清单** `expert.json`（T108 §4.0）。文件名即类型标记。
pub fn locate_expert_manifest(pkg_dir: &Path) -> Option<PathBuf> {
    let p = pkg_dir.join("expert.json");
    p.is_file().then_some(p)
}

/// 解析团队包目录。
///
/// 两条来源：
/// - **`team.json`（silicon 正式格式）**：文件名即类型标记，**不再要求 `teamInfo`/`team`/`expertType`**；
/// - **`plugin.json` + `teamInfo`/`team`/`expertType=team`**：第三方（codebuddy）方言，**只读不产出**。
///
/// `team.json` 若声明 `mcpServers` → **报错**。team 只做指令编排 + 成员专家定义（T108 §4.2）：
/// MCP 是连接不是上下文，要 OAuth 登录、要保持会话，「激活团队才连」意味着每次切团队都要
/// 重新授权。需要外部服务请另装 plugin。**此前是静默丢弃**——装完无报错、连接器根本没注册，
/// 团队跑起来才发现没工具，且无从查起。
pub fn parse_team_package(pkg_dir: &Path) -> Result<ImportedTeam, String> {
    if let Some(path) = locate_team_manifest(pkg_dir) {
        let raw = std::fs::read_to_string(&path).map_err(|e| format!("读 team.json 失败：{e}"))?;
        let v: serde_json::Value =
            serde_json::from_str(&raw).map_err(|e| format!("team.json 不是合法 JSON：{e}"))?;
        if v.get("mcpServers").is_some() {
            return Err(
                "team.json 不得声明 mcpServers：团队只做指令编排与成员专家定义。\
                 需要外部服务请单独安装对应的 plugin。"
                    .into(),
            );
        }
        return parse_value_as_team(&v);
    }
    // 兼容：第三方方言仍混用 plugin.json，靠 teamInfo/team/expertType 标记。
    let path =
        locate_manifest(pkg_dir).ok_or("缺少 team.json（或第三方 plugin.json + teamInfo）")?;
    let raw = std::fs::read_to_string(&path).map_err(|e| format!("读 plugin.json 失败：{e}"))?;
    let v: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| format!("plugin.json 不是合法 JSON：{e}"))?;
    parse_value(&v)
}

/// 包的类型（T108 §4.0）：**由清单文件名判定**，不靠猜包内容。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageKind {
    /// silicon 团队包：`team.json`（或第三方 `plugin.json` + `teamInfo` 方言）。
    Team,
    /// silicon 专家包：`expert.json`。
    Expert,
    /// 标准 plugin：`plugin.json` / `.claude-plugin/plugin.json`。
    Plugin,
}

/// 探包类型。判定顺序：`team.json` → `expert.json` → `plugin.json`（→ codebuddy 方言兜底）。
///
/// 这是 T108 的关键机制：此前三类包共用 `plugin.json`，在磁盘上长得一模一样 ——
/// team 靠 `teamInfo` 还能认出来，但 **expert 包没有任何标记**，装载器无法决定
/// 该公开（标准 plugin）还是该私有（silicon expert 包）。清单文件名一分，死结即解。
pub fn detect_package_kind(path: &str) -> Result<PackageKind, String> {
    let (pkg_root, _guard) = stage_source(path)?;
    if locate_team_manifest(&pkg_root).is_some() {
        return Ok(PackageKind::Team);
    }
    if locate_expert_manifest(&pkg_root).is_some() {
        return Ok(PackageKind::Expert);
    }
    // 兜底：第三方 codebuddy 团队包仍混用 plugin.json + teamInfo。
    if parse_team_package(&pkg_root).is_ok() {
        return Ok(PackageKind::Team);
    }
    Ok(PackageKind::Plugin)
}

/// 解析 `team.json`：**文件名已经是类型标记**，故不再校验 `teamInfo`/`team`/`expertType`。
/// 字段解析与方言路径完全共用（`parse_value` 的 team 标记检查通过后走的是同一段）。
fn parse_value_as_team(v: &serde_json::Value) -> Result<ImportedTeam, String> {
    // 注入一个隐式标记，让共用的 parse_value 跳过「这是不是团队结构」的判定。
    let mut vv = v.clone();
    if let Some(obj) = vv.as_object_mut() {
        if !obj.contains_key("teamInfo")
            && !obj.contains_key("team")
            && obj.get("kind").and_then(|k| k.as_str()) != Some("team")
        {
            obj.insert("kind".into(), serde_json::Value::String("team".into()));
        }
    }
    parse_value(&vv)
}

fn parse_value(v: &serde_json::Value) -> Result<ImportedTeam, String> {
    let name = str_field(v, "name").ok_or("plugin.json 缺少 name")?;
    let display_name = i18n_field(v, "displayName").unwrap_or_else(|| name.clone());
    let description = i18n_field(v, "displayDescription")
        .or_else(|| i18n_field(v, "description"))
        .unwrap_or_default();

    let agents = str_array(v, "agents")
        .iter()
        .map(|s| normalize_rel(s))
        .collect();
    let skills = str_array(v, "skills")
        .iter()
        .map(|s| normalize_rel(s))
        .collect();

    // 团队结构判定 + lead/members 提取：teamInfo（codebuddy）优先，其次 team（原生），再 agentName 兜底 lead。
    let team_info = v.get("teamInfo");
    let team = v.get("team");
    let expert_type = str_field(v, "expertType").or_else(|| str_field(v, "type"));
    let kind = str_field(v, "kind");
    let is_team = team_info.is_some()
        || team.is_some()
        || expert_type.as_deref() == Some("team")
        || kind.as_deref() == Some("team");
    if !is_team {
        return Err(
            "该包不是团队结构（缺 team/teamInfo，且 kind/type≠team）；纯能力包请从「套件」安装"
                .into(),
        );
    }

    let lead = team_info
        .and_then(|t| str_field(t, "leadAgent"))
        .or_else(|| team.and_then(|t| str_field(t, "lead")))
        .or_else(|| str_field(v, "agentName"));
    let member_names: Vec<String> = team_info
        .map(|t| str_array(t, "memberAgents"))
        .or_else(|| team.map(|t| str_array(t, "members")))
        .unwrap_or_default();

    // members[]（codebuddy）：每项 {id, name(i18n), profession(i18n), avatar, role} → 展示覆盖。
    let mut member_display: HashMap<String, MemberDisplay> = HashMap::new();
    if let Some(arr) = v.get("members").and_then(|x| x.as_array()) {
        for m in arr {
            let Some(id) = str_field(m, "id") else {
                continue;
            };
            member_display.insert(
                id,
                MemberDisplay {
                    display_name: i18n_field(m, "name"),
                    profession: i18n_field(m, "profession"),
                    avatar: str_field(m, "avatar"),
                },
            );
        }
    }

    // 开场引导语：quickPrompts[]（每项 string 或 i18n）+ defaultInitPrompt（i18n）。
    let mut quick_prompts: Vec<String> = Vec::new();
    if let Some(arr) = v.get("quickPrompts").and_then(|x| x.as_array()) {
        for q in arr {
            if let Some(s) = i18n_value(q) {
                quick_prompts.push(s);
            }
        }
    }
    if let Some(s) = i18n_field(v, "defaultInitPrompt") {
        if !quick_prompts.contains(&s) {
            quick_prompts.push(s);
        }
    }

    Ok(ImportedTeam {
        name,
        display_name,
        description,
        agents,
        skills,
        lead,
        member_names,
        member_display,
        quick_prompts,
    })
}

/// 解析后的单 agent expert 包（统一形态）。
#[derive(Debug, Clone)]
pub struct ImportedExpert {
    /// expert name（运行角色 role_id / 子运行 expert_name / 私有 skill owner）。
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub profession: Option<String>,
    pub avatar: Option<String>,
    pub quick_prompts: Vec<String>,
    /// agent 定义文件相对路径（相对包根）。
    pub expert_rel: String,
    /// skill 目录相对路径（相对包根）→ 落为该 agent 私有。
    pub skills: Vec<String>,
}

/// 解析单 agent expert 包（原生 / codebuddy / Claude 方言）。团队结构 / 非 agent 包返回 Err。
pub fn parse_expert_package(pkg_dir: &Path) -> Result<ImportedExpert, String> {
    // `expert.json`（silicon 正式格式）优先：文件名即类型标记。
    // 回退到 `plugin.json` 仅为兼容旧包 —— 那条路只能靠「不是 team + 有 agents[]」反向推断，
    // 与标准 plugin **无法区分**（这正是 T108 §4.0 要根治的死结）。
    let (path, from_expert_json) = match locate_expert_manifest(pkg_dir) {
        Some(p) => (p, true),
        None => (
            locate_manifest(pkg_dir).ok_or("缺少 expert.json（或旧式 plugin.json）")?,
            false,
        ),
    };
    let raw = std::fs::read_to_string(&path).map_err(|e| format!("读 {path:?} 失败：{e}"))?;
    let v: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| format!("{path:?} 不是合法 JSON：{e}"))?;

    if !from_expert_json {
        let expert_type = str_field(&v, "expertType").or_else(|| str_field(&v, "type"));
        let kind = str_field(&v, "kind");
        let is_team = v.get("teamInfo").is_some()
            || v.get("team").is_some()
            || expert_type.as_deref() == Some("team")
            || kind.as_deref() == Some("team");
        if is_team {
            return Err("该包是团队结构，请从「团队」导入".into());
        }
    }
    let agents: Vec<String> = str_array(&v, "agents")
        .iter()
        .map(|s| normalize_rel(s))
        .collect();
    let expert_rel = agents
        .into_iter()
        .next()
        .ok_or("该包未声明 agents，不是 agent 包")?;

    let name = str_field(&v, "agentName")
        .or_else(|| str_field(&v, "name"))
        .ok_or("plugin.json 缺少 agentName/name")?;
    let display_name = i18n_field(&v, "displayName").unwrap_or_else(|| name.clone());
    let description = i18n_field(&v, "displayDescription")
        .or_else(|| i18n_field(&v, "description"))
        .unwrap_or_default();
    let profession = i18n_field(&v, "profession");
    let avatar = str_field(&v, "avatar");
    let skills: Vec<String> = str_array(&v, "skills")
        .iter()
        .map(|s| normalize_rel(s))
        .collect();

    let mut quick_prompts: Vec<String> = Vec::new();
    if let Some(arr) = v.get("quickPrompts").and_then(|x| x.as_array()) {
        for q in arr {
            if let Some(s) = i18n_value(q) {
                quick_prompts.push(s);
            }
        }
    }
    if let Some(s) = i18n_field(&v, "defaultInitPrompt") {
        if !quick_prompts.contains(&s) {
            quick_prompts.push(s);
        }
    }

    Ok(ImportedExpert {
        name,
        display_name,
        description,
        profession,
        avatar,
        quick_prompts,
        expert_rel,
        skills,
    })
}

/// 去掉相对路径前导 `./`。
fn normalize_rel(s: &str) -> String {
    s.trim().trim_start_matches("./").to_string()
}

/// 顶层字符串字段（非空白）。
fn str_field(v: &serde_json::Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(|x| x.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

/// i18n 字段：值可为字符串或 `{en,zh,...}` 对象；取 zh > en > 任意首个非空。
fn i18n_field(v: &serde_json::Value, key: &str) -> Option<String> {
    v.get(key).and_then(i18n_value)
}

fn i18n_value(x: &serde_json::Value) -> Option<String> {
    if let Some(s) = x.as_str() {
        let t = s.trim();
        return (!t.is_empty()).then(|| t.to_string());
    }
    if let Some(obj) = x.as_object() {
        for k in ["zh", "en"] {
            if let Some(s) = obj.get(k).and_then(|y| y.as_str()) {
                let t = s.trim();
                if !t.is_empty() {
                    return Some(t.to_string());
                }
            }
        }
        for (_k, y) in obj {
            if let Some(s) = y.as_str() {
                let t = s.trim();
                if !t.is_empty() {
                    return Some(t.to_string());
                }
            }
        }
    }
    None
}

/// 顶层字符串数组（过滤空白）。
fn str_array(v: &serde_json::Value, key: &str) -> Vec<String> {
    v.get(key)
        .and_then(|x| x.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|e| e.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// 路径是否 .zip（按扩展名，忽略大小写）。
pub fn is_zip(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("zip"))
        == Some(true)
}

/// 极简临时目录：Drop 时递归删除。zip 导入解压用。
pub struct TempDir {
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
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
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

/// 这个目录是不是一个包根 —— **三种清单任一命中即可**（T108 §4.0）。
///
/// 此前只认 `plugin.json`，于是只带 `team.json` / `expert.json` 的包在 stage 阶段就被拒了，
/// 根本走不到类型判定。
fn has_any_manifest(dir: &Path) -> bool {
    locate_team_manifest(dir).is_some()
        || locate_expert_manifest(dir).is_some()
        || locate_manifest(dir).is_some()
}

/// 在解压/给定目录里定位包根：含 `team.json` / `expert.json` / `plugin.json`
/// （或 `.claude-plugin/` `.codebuddy-plugin/` 下的 plugin.json）的目录。
/// 支持包根直接是包，或唯一顶层子目录是包（zip 常见的「带一层包裹文件夹」）。
fn locate_pkg_root(base: &Path) -> Result<PathBuf, String> {
    if has_any_manifest(base) {
        return Ok(base.to_path_buf());
    }
    let mut subdirs: Vec<PathBuf> = Vec::new();
    for entry in std::fs::read_dir(base).map_err(|e| e.to_string())? {
        let p = entry.map_err(|e| e.to_string())?.path();
        if p.is_dir() {
            subdirs.push(p);
        }
    }
    if subdirs.len() == 1 && has_any_manifest(&subdirs[0]) {
        return Ok(subdirs[0].clone());
    }
    Err("未找到清单（team.json / expert.json / plugin.json，需在根目录或唯一顶层子目录内）".into())
}

/// 把导入来源（目录或 .zip）定位为团队包根：.zip → 解压临时目录并定位根（返回的 `TempDir` 守卫存活期间根有效）；
/// 目录 → 直接定位（无守卫）。调用方在复制完成前需持有返回的守卫。
pub fn stage_source(path: &str) -> Result<(PathBuf, Option<TempDir>), String> {
    let src = PathBuf::from(path);
    if !src.exists() {
        return Err(format!("路径不存在：{path}"));
    }
    if src.is_file() && is_zip(&src) {
        let tmp = TempDir::new("siw-team-import")?;
        extract_zip(&src, tmp.path())?;
        let root = locate_pkg_root(tmp.path())?;
        Ok((root, Some(tmp)))
    } else if src.is_dir() {
        Ok((locate_pkg_root(&src)?, None))
    } else {
        Err("仅支持团队包目录或 .zip 压缩包".into())
    }
}

/// 递归复制目录（导入时把包复制进受管 teams 根）。
pub fn copy_dir_all(src: &Path, dest: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let from = entry.path();
        let to = dest.join(entry.file_name());
        // 跳过 symlink：导入包里常见指向源机绝对路径的链接（如 /Users/x/.qclaw/...），本机不可解析，
        // 也不应进入受管副本。跟随它会让整包复制因单个断链而中止（os error 2）。
        let is_symlink = entry.file_type().map(|t| t.is_symlink()).unwrap_or(false);
        if is_symlink {
            eprintln!("[import] 跳过 symlink {}", from.display());
            continue;
        }
        if from.is_dir() {
            copy_dir_all(&from, &to)?;
        } else if let Err(e) = std::fs::copy(&from, &to) {
            // 单文件复制失败不致命：记日志跳过，避免一个坏文件毁掉整包。
            eprintln!("[import] 跳过文件 {}（复制失败 {e}）", from.display());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_base(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "siw-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn pkg(base: &std::path::Path, dir: &str, manifest: &str, body: &str) -> std::path::PathBuf {
        let d = base.join(dir);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join(manifest), body).unwrap();
        d
    }

    /// T108 §4.0 判定顺序：`team.json` → `expert.json` → `plugin.json`（→ codebuddy 方言兜底）。
    ///
    /// **这是三体系分立的地基。** 此前三类包共用 `plugin.json`，在磁盘上长得一模一样 ——
    /// team 靠 `teamInfo` 还能认出来，但 **expert 包没有任何标记**，装载器无法决定该公开
    /// （标准 plugin）还是该私有（silicon expert 包）。
    #[test]
    fn detect_package_kind_by_manifest_filename() {
        let base = tmp_base("detect");

        let t = pkg(
            &base,
            "team-pkg",
            "team.json",
            r#"{"name":"t","agents":["./agents/lead.md"],"team":{"lead":"lead","members":["m1"]}}"#,
        );
        assert_eq!(
            detect_package_kind(t.to_str().unwrap()).unwrap(),
            PackageKind::Team,
            "team.json → Team（文件名即标记，不再要求 teamInfo）"
        );

        let e = pkg(
            &base,
            "expert-pkg",
            "expert.json",
            r#"{"name":"e","agents":["./agents/e.md"],"skills":["./skills/s"]}"#,
        );
        assert_eq!(
            detect_package_kind(e.to_str().unwrap()).unwrap(),
            PackageKind::Expert,
            "expert.json → Expert（此前无标记，会被误判为 plugin，私有技能变公开）"
        );

        let p = pkg(
            &base,
            "plugin-pkg",
            "plugin.json",
            r#"{"name":"c","skills":["./skills/a"],"mcpServers":{"x":{"url":"https://e/mcp"}}}"#,
        );
        assert_eq!(
            detect_package_kind(p.to_str().unwrap()).unwrap(),
            PackageKind::Plugin,
            "标准 plugin → Plugin（第三方包永远不会有 expert.json/team.json）"
        );

        // 兼容：第三方 codebuddy 方言仍混用 plugin.json + teamInfo。
        let cb = pkg(
            &base,
            "cb-pkg",
            "plugin.json",
            r#"{"name":"t2","teamInfo":{"leadAgent":"lead","memberAgents":["m1"]}}"#,
        );
        assert_eq!(
            detect_package_kind(cb.to_str().unwrap()).unwrap(),
            PackageKind::Team,
            "plugin.json + teamInfo → Team（方言兜底，只读不产出）"
        );

        // 顺序生效：同时有 team.json 与 plugin.json → team.json 胜出。
        let both = pkg(&base, "both", "plugin.json", r#"{"name":"b"}"#);
        std::fs::write(
            both.join("team.json"),
            r#"{"name":"b","agents":["./agents/l.md"],"team":{"lead":"l","members":[]}}"#,
        )
        .unwrap();
        assert_eq!(
            detect_package_kind(both.to_str().unwrap()).unwrap(),
            PackageKind::Team,
            "team.json 优先级高于 plugin.json"
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    /// T108 §4.2：team 只做指令编排 + 成员专家定义，**不得携带 MCP**。
    ///
    /// 此前是**静默丢弃** —— 装完无报错、连接器根本没注册，团队跑起来才发现没工具，
    /// 且无从查起。现在必须明确报错。
    #[test]
    fn team_json_rejects_mcp_servers() {
        let base = tmp_base("teammcp");
        let d = pkg(
            &base,
            "t",
            "team.json",
            r#"{"name":"t","agents":["./agents/l.md"],"team":{"lead":"l","members":[]},
                "mcpServers":{"figma":{"url":"https://mcp.figma.com/mcp"}}}"#,
        );
        let err =
            parse_team_package(&d).expect_err("team.json 带 mcpServers 必须报错，不得静默丢弃");
        assert!(err.contains("mcpServers"), "错误要说清是哪一项：{err}");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn parses_codebuddy_team_dialect() {
        let raw = r#"{
            "name": "ai-content-creator-team",
            "displayName": { "en": "Content Team", "zh": "内容创作专家团" },
            "displayDescription": { "zh": "多模态内容生产" },
            "expertType": "team",
            "agentName": "lead-x",
            "agents": ["./agents/lead-x.md", "./agents/copy.md"],
            "skills": ["./skills/kb"],
            "teamInfo": { "leadAgent": "lead-x", "memberAgents": ["copy"] },
            "members": [
                { "id": "lead-x", "name": {"zh":"司远"}, "profession": {"zh":"创意总监"}, "avatar": "a.png", "role": "lead" },
                { "id": "copy", "name": {"zh":"笔澜"}, "profession": {"zh":"文案"}, "role": "member" }
            ],
            "quickPrompts": [ {"zh":"做个情绪板"}, {"zh":"写条文案"} ],
            "defaultInitPrompt": {"zh":"帮我策划"}
        }"#;
        let v: serde_json::Value = serde_json::from_str(raw).unwrap();
        let t = parse_value(&v).expect("parse");
        assert_eq!(t.name, "ai-content-creator-team");
        assert_eq!(t.display_name, "内容创作专家团");
        assert_eq!(t.description, "多模态内容生产");
        assert_eq!(t.agents, vec!["agents/lead-x.md", "agents/copy.md"]);
        assert_eq!(t.skills, vec!["skills/kb"]);
        assert_eq!(t.lead.as_deref(), Some("lead-x"));
        assert_eq!(t.member_names, vec!["copy"]);
        assert_eq!(
            t.member_display
                .get("copy")
                .unwrap()
                .display_name
                .as_deref(),
            Some("笔澜")
        );
        assert_eq!(t.quick_prompts, vec!["做个情绪板", "写条文案", "帮我策划"]);
    }

    #[test]
    fn parses_native_team_dialect() {
        let raw = r#"{
            "name": "research-team",
            "displayName": "调研团队",
            "kind": "team",
            "agents": ["agents/coord.md", "agents/r.md"],
            "team": { "lead": "coord", "members": ["r"] }
        }"#;
        let v: serde_json::Value = serde_json::from_str(raw).unwrap();
        let t = parse_value(&v).expect("parse");
        assert_eq!(t.lead.as_deref(), Some("coord"));
        assert_eq!(t.member_names, vec!["r"]);
        assert_eq!(t.display_name, "调研团队");
    }

    #[test]
    fn rejects_non_team_package() {
        let raw = r#"{ "name": "toolbox", "kind": "suite", "skills": ["skills/a"] }"#;
        let v: serde_json::Value = serde_json::from_str(raw).unwrap();
        assert!(parse_value(&v).is_err());
    }
}

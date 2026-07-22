//! 导入「带技能的专家」expert 包（原生 / codebuddy / Claude 方言）→ 散装 expert + 其 skill 作该
//! expert 私有。与团队导入对齐：技能目录复制进受管区，不引用易失的源 marketplace 路径。
//!
//! 复用点：`AppState::import_expert`（运行时命令）与一次性导入工具共用同一逻辑。

use std::path::Path;

use crate::expert::{ExpertService, ExpertSummary};
use crate::skill::SkillService;

/// 导入单 agent expert 包到指定服务。
///
/// 路径约定：skill 目录**复制进受管区** `{workspace_base}/experts/<name>/<rel>`（expert 根的子目录，
/// sync 只扫顶层 `.md`、忽略子目录），索引存复制后的绝对路径；expert 正文由 `create_standalone`
/// 复制进 `{workspace_base}/experts/<name>.md`。源 marketplace 移动/删除不影响。
/// 幂等：同名散装 expert 先删（级联清其私有 skill）+ 清旧资产目录，再落新——支持「清理历史后重导」。
pub fn import_expert(
    agents: &ExpertService,
    skills: &SkillService,
    workspace_base: &Path,
    path: &str,
) -> Result<ExpertSummary, String> {
    // zip → 解压定位根；目录 → 直接定位。守卫(_guard)存活到复制完成。
    let (pkg_root, _guard) = crate::team::import::stage_source(path)?;
    let ia = crate::team::import::parse_expert_package(&pkg_root)?;
    if ia.name.contains('/') || ia.name.contains('\\') || ia.name.contains("..") {
        return Err("agent name 含非法字符".into());
    }
    // 幂等：清理同名历史散装 expert（级联其私有 skill）。
    if let Ok(list) = agents.list_standalone() {
        for a in list.into_iter().filter(|a| a.name == ia.name) {
            let _ = agents.delete_standalone(&a.id);
        }
    }
    // 受管资产目录：expert 根下以 <name> 子目录存放（sync 忽略子目录）。清旧 + 复制 skill 进来。
    let dest = workspace_base.join("experts").join(&ia.name);
    if dest.exists() {
        let _ = std::fs::remove_dir_all(&dest);
    }
    std::fs::create_dir_all(&dest).map_err(|e| format!("创建 agent 资产目录失败：{e}"))?;
    for rel in &ia.skills {
        let src = pkg_root.join(rel);
        let dst = dest.join(rel);
        if let Err(e) = crate::team::import::copy_dir_all(&src, &dst) {
            eprintln!(
                "[agent-import] {}: 跳过 skill {rel}（复制失败 {e}）",
                ia.name
            );
        }
    }

    let expert_md = pkg_root.join(&ia.expert_rel);
    let content = std::fs::read_to_string(&expert_md)
        .map_err(|e| format!("读 agent 文件失败 {}：{e}", expert_md.display()))?;
    let fm = crate::expert::frontmatter::parse_frontmatter(&content)?;
    let system_prompt = crate::expert::frontmatter::strip_frontmatter(&content);
    // avatar 只接受 emoji 形态：图片路径/文件名/URL 在前端一律回退默认图标，不入库死路径。
    let avatar = ia.avatar.clone().filter(|a| is_emoji_avatar(a));
    let now = now_string();
    let summary = agents.create_standalone(
        &ia.name,
        &ia.description,
        &system_prompt,
        fm.tools,
        &fm.model_tier,
        Some(ia.display_name.clone()),
        ia.profession.clone(),
        avatar,
        ia.quick_prompts.clone(),
        None,
    )?;
    // 该 expert 的 skill 落为「expert 私有」（owner=expert name），从**受管副本**索引（绝对路径）。
    for rel in &ia.skills {
        if let Err(e) = skills.index_expert_skill(&ia.name, &dest.join(rel), &now) {
            eprintln!("[agent-import] {}: 跳过 skill {rel}（{e}）", ia.name);
        }
    }
    Ok(summary)
}

/// avatar 是否为可渲染的 emoji 形态（与前端 `avatarEmoji` 一致）：含路径分隔符/图片扩展名/URL 的
/// 一律视为不可渲染，不入库（前端会回退默认图标，存死路径无意义）。
fn is_emoji_avatar(avatar: &str) -> bool {
    let v = avatar.trim();
    if v.is_empty() || v.contains('/') || v.contains('\\') {
        return false;
    }
    if v.starts_with("http://") || v.starts_with("https://") {
        return false;
    }
    let lower = v.to_ascii_lowercase();
    !["png", "jpg", "jpeg", "gif", "webp", "svg", "ico"]
        .iter()
        .any(|ext| lower.ends_with(&format!(".{ext}")))
}

fn now_string() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
        .to_string()
}

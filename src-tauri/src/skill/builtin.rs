//! 内置技能：编译期用 include_dir! 内嵌 `src-tauri/builtin-skills/`，启动物化到技能根目录。
//!
//! 设计意图：内置技能随二进制分发、dev 与打包行为一致；物化后与用户技能在磁盘同构，
//! 使 load_skill / 详情读取逻辑统一。每次物化覆盖（内置只读、随版本同步）。

use std::path::Path;

use include_dir::{include_dir, Dir, DirEntry};

/// 内嵌的内置技能根目录（每个顶层子目录 = 一个内置技能）。
static BUILTIN_DIR: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/builtin-skills");

/// 内置技能 name 集合（= 顶层子目录名）。sync 时据此标记 source=builtin。
pub fn builtin_names() -> Vec<String> {
    BUILTIN_DIR
        .dirs()
        .filter_map(|d| {
            d.path()
                .file_name()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string())
        })
        .collect()
}

/// 曾经的内置技能、现已废弃：从旧版升级的用户磁盘上可能残留（含旧 QoderWork 品牌/失效 MCP）。
/// 每次物化时清除其残留目录；对应 DB 行由 `SkillService::sync` 的孤儿清理收口。
pub const DEPRECATED_BUILTINS: &[&str] = &["vm-error-recovery"];

/// 把内置技能物化到 `root`。**每个内置技能整目录替换**（先删 dest 再整拷），
/// 以清掉源里已移除的旧文件（如 find-skills 早期的 `scripts/install-skill.{sh,ps1}`）；
/// 并清除已废弃内置技能的残留目录（`DEPRECATED_BUILTINS`）。内置只读、随版本同步，覆盖安全。
pub fn materialize(root: &Path) -> Result<(), String> {
    // 已废弃内置：删残留目录（DB 行由 sync 孤儿清理处理）。
    for name in DEPRECATED_BUILTINS {
        let dir = root.join(name);
        if dir.exists() {
            std::fs::remove_dir_all(&dir)
                .map_err(|e| format!("清理废弃内置技能 {name} 失败：{e}"))?;
        }
    }
    // 当前内置：逐个整目录替换，prune 掉源里已移除的文件。
    for d in BUILTIN_DIR.dirs() {
        let Some(name) = d.path().file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        let dest = root.join(name);
        if dest.exists() {
            std::fs::remove_dir_all(&dest)
                .map_err(|e| format!("清理内置技能旧目录 {name} 失败：{e}"))?;
        }
        write_dir(d, root)?;
    }
    Ok(())
}

/// 递归写出一个内嵌目录到 root（路径相对 include 根）。
fn write_dir(dir: &Dir<'_>, root: &Path) -> Result<(), String> {
    for entry in dir.entries() {
        match entry {
            DirEntry::Dir(d) => write_dir(d, root)?,
            DirEntry::File(f) => {
                let out = root.join(f.path());
                if let Some(parent) = out.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| format!("创建内置技能目录失败：{e}"))?;
                }
                std::fs::write(&out, f.contents())
                    .map_err(|e| format!("写出内置技能文件失败：{e}"))?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root() -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("sw-builtin-{}-{}", std::process::id(), nanos));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn materialize_prunes_stale_files_in_builtin() {
        let root = temp_root();
        // 预置一个源里已不存在的旧文件（模拟 find-skills 早期的 scripts/install-skill.sh）。
        let stale = root.join("find-skills").join("scripts");
        std::fs::create_dir_all(&stale).unwrap();
        std::fs::write(stale.join("install-skill.sh"), "TARGET_DIR=~/.qoderwork/skills").unwrap();
        materialize(&root).unwrap();
        // 整目录替换 → 旧 scripts/ 被清；当前 SKILL.md 在。
        assert!(!stale.exists(), "陈旧 scripts/ 应被 prune 掉");
        assert!(root.join("find-skills").join("SKILL.md").is_file());
    }

    #[test]
    fn materialize_removes_deprecated_builtin() {
        let root = temp_root();
        let dep = root.join("vm-error-recovery");
        std::fs::create_dir_all(&dep).unwrap();
        std::fs::write(dep.join("SKILL.md"), "---\nname: vm-error-recovery\n---\n").unwrap();
        materialize(&root).unwrap();
        assert!(!dep.exists(), "已废弃内置技能目录应被清除");
    }

    #[test]
    fn materialize_writes_current_builtins() {
        let root = temp_root();
        materialize(&root).unwrap();
        for name in ["find-skills", "create-skill", "docx"] {
            assert!(
                root.join(name).join("SKILL.md").is_file(),
                "{name} 应被物化"
            );
        }
    }

    #[test]
    fn builtin_names_includes_create_expert_and_team() {
        let names = builtin_names();
        assert!(names.contains(&"create-expert".to_string()), "内置技能应含 create-expert：{names:?}");
        assert!(names.contains(&"create-team".to_string()), "内置技能应含 create-team：{names:?}");
    }

    /// 守卫 `source=builtin` 的关键机制：frontmatter `name` 必须等于目录名
    /// （service.rs sync 据此判定 builtin）。仅断言目录名存在不够——SKILL.md
    /// 若 frontmatter 损坏或 name 写错，仍会静默退化为用户技能/不注册。
    #[test]
    fn create_expert_and_team_frontmatter_name_matches_dir() {
        let expert_md = include_str!("../../builtin-skills/create-expert/SKILL.md");
        let team_md = include_str!("../../builtin-skills/create-team/SKILL.md");
        let fm_expert = crate::skill::frontmatter::parse_frontmatter(expert_md)
            .expect("create-expert SKILL.md frontmatter 应可解析");
        let fm_team = crate::skill::frontmatter::parse_frontmatter(team_md)
            .expect("create-team SKILL.md frontmatter 应可解析");
        assert_eq!(fm_expert.name, "create-expert");
        assert_eq!(fm_team.name, "create-team");
        assert!(!fm_expert.description.is_empty(), "create-expert 描述不应为空");
        assert!(!fm_team.description.is_empty(), "create-team 描述不应为空");
    }
}

//! 内置专家：编译期用 include_dir! 内嵌 `src-tauri/builtin-agents/`，启动物化到 agent 根目录。
//!
//! 每个内嵌 `<name>.md` 即一个内置专家。每次物化覆盖（内置只读、随版本同步）。

use std::path::Path;

use include_dir::{include_dir, Dir, DirEntry};

static BUILTIN_DIR: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/builtin-agents");

/// 内置专家 name 集合（= 顶层 `<name>.md` 的文件名去扩展名）。
pub fn builtin_names() -> Vec<String> {
    BUILTIN_DIR
        .files()
        .filter_map(|f| {
            f.path()
                .file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string())
        })
        .collect()
}

/// 将全部内嵌内置专家文件覆盖写入 `root`。
pub fn materialize(root: &Path) -> Result<(), String> {
    write_dir(&BUILTIN_DIR, root)
}

fn write_dir(dir: &Dir<'_>, root: &Path) -> Result<(), String> {
    for entry in dir.entries() {
        match entry {
            DirEntry::Dir(d) => write_dir(d, root)?,
            DirEntry::File(f) => {
                let out = root.join(f.path());
                if let Some(parent) = out.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| format!("创建内置专家目录失败：{e}"))?;
                }
                std::fs::write(&out, f.contents())
                    .map_err(|e| format!("写出内置专家文件失败：{e}"))?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn materialize_empty_builtins_does_not_crash() {
        // 方案B：无内置预置专家（builtin-agents/ 仅 .gitkeep）。materialize 不应崩，
        // 且不产出任何 .md 专家（builtin_names 不含真实专家）。
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let dir =
            std::env::temp_dir().join(format!("siw-agent-mat-{}-{}", std::process::id(), nanos));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        materialize(&dir).expect("materialize");
        // 无内置 .md 文件。
        let md_count = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("md"))
            .count();
        assert_eq!(md_count, 0, "方案B 不应有内置 .md 专家");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

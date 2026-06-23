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

/// 将全部内嵌内置技能文件覆盖写入 `root`（保持目录结构）。
pub fn materialize(root: &Path) -> Result<(), String> {
    write_dir(&BUILTIN_DIR, root)
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

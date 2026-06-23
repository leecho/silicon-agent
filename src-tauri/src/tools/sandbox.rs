use std::path::{Path, PathBuf};

/// 把工具传入的 path 解析为绝对路径，并确保落在 workspace 根内（防逃逸）。
pub fn resolve_in_workspace(workspace: &Path, path: &str) -> Result<PathBuf, String> {
    let raw = PathBuf::from(shellexpand_tilde(path));
    let joined = if raw.is_absolute() {
        raw
    } else {
        workspace.join(raw)
    };
    let canonical = normalize(&joined);
    let ws = normalize(workspace);
    if !canonical.starts_with(&ws) {
        return Err(format!("路径越出工作区: {path} -> {}", canonical.display()));
    }
    Ok(canonical)
}

fn shellexpand_tilde(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return format!("{}/{}", home.to_string_lossy(), rest);
        }
    }
    path.to_string()
}

/// 词法规范化（不要求路径存在；处理 . 和 ..）。
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

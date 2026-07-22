//! 工作目录与产物命令、产物内容分类（薄入口 + 本地纯函数）。
use crate::app_state::AppState;
use crate::session::Session;
use tauri::State;
use tauri_plugin_opener::OpenerExt;

/// 设置会话工作目录（沙箱根）。允许随时修改（不锁定）：下一次 run 时按最新值解析沙箱根。
/// path 必须是已存在的目录。成功返回最新会话详情。
#[tauri::command]
pub fn set_session_workspace(
    services: State<'_, AppState>,
    session_id: String,
    path: String,
) -> Result<Session, String> {
    if !std::path::Path::new(&path).is_dir() {
        return Err(format!("工作目录不存在或不是目录：{path}"));
    }
    let now = crate::engine::now_string();
    services.session.set_working_dir(&session_id, &path, &now)?;
    services.session.add_recent_workspace(&path, &now)?;
    services
        .facade
        .session_with_pending(&session_id)?
        .ok_or_else(|| "session not found".into())
}

/// 打开会话工作目录。
///
/// 前端只传 session_id；后端确认会话存在并解析受控工作目录，避免把任意本地路径交给前端 opener scope。
#[tauri::command]
pub fn open_session_workspace(
    app: tauri::AppHandle,
    services: State<'_, AppState>,
    session_id: String,
) -> Result<(), String> {
    services
        .session
        .get_session(&session_id)?
        .ok_or_else(|| "session not found".to_string())?;
    let workspace = services
        .engine_builder
        .ensure_session_workspace(&session_id)?;
    app.opener()
        .open_path(workspace.to_string_lossy().into_owned(), None::<String>)
        .map_err(|err| format!("打开工作目录失败：{err}"))
}

fn resolve_artifact_action_path(
    workspace: &std::path::Path,
    path: &str,
) -> Result<std::path::PathBuf, String> {
    if path.trim().is_empty() {
        return Err("产物路径不能为空".to_string());
    }
    crate::tools::sandbox::resolve_in_workspace(workspace, path)
}

fn resolve_existing_artifact_action_path(
    services: &AppState,
    session_id: &str,
    path: &str,
) -> Result<std::path::PathBuf, String> {
    services
        .session
        .get_session(session_id)?
        .ok_or_else(|| "session not found".to_string())?;
    let workspace = services
        .engine_builder
        .resolve_session_workspace(session_id)?;
    let resolved = resolve_artifact_action_path(&workspace, path)?;
    if !resolved.exists() {
        return Err(format!("产物文件不存在：{}", resolved.display()));
    }
    Ok(resolved)
}

/// 用系统默认应用打开会话工作目录内的产物文件。
#[tauri::command]
pub fn open_artifact_file(
    app: tauri::AppHandle,
    services: State<'_, AppState>,
    session_id: String,
    path: String,
) -> Result<(), String> {
    let resolved = resolve_existing_artifact_action_path(&services, &session_id, &path)?;
    app.opener()
        .open_path(resolved.to_string_lossy().into_owned(), None::<String>)
        .map_err(|err| format!("打开产物文件失败：{err}"))
}

/// 在系统文件管理器中定位会话工作目录内的产物文件。
#[tauri::command]
pub fn reveal_artifact_file(
    app: tauri::AppHandle,
    services: State<'_, AppState>,
    session_id: String,
    path: String,
) -> Result<(), String> {
    let resolved = resolve_existing_artifact_action_path(&services, &session_id, &path)?;
    app.opener()
        .reveal_item_in_dir(&resolved)
        .map_err(|err| format!("打开所在文件夹失败：{err}"))
}

/// 列出最近使用过的工作目录（全局，按最近使用倒序，最多 8 个）。
#[tauri::command]
pub fn get_recent_workspaces(services: State<'_, AppState>) -> Result<Vec<String>, String> {
    services.session.list_recent_workspaces(8)
}

/// 列出会话工作目录内的文件相对路径，供 Composer @ 自动补全使用。
#[tauri::command]
pub fn list_session_workspace_files(
    services: State<'_, AppState>,
    session_id: String,
) -> Result<Vec<String>, String> {
    let workspace = services
        .engine_builder
        .ensure_session_workspace(&session_id)?;
    list_workspace_file_paths(&workspace, 200)
}

fn list_workspace_file_paths(
    workspace: &std::path::Path,
    limit: usize,
) -> Result<Vec<String>, String> {
    let root = std::fs::canonicalize(workspace).map_err(|e| format!("解析工作目录失败：{e}"))?;
    let mut out = Vec::new();
    collect_workspace_file_paths(&root, &root, limit, &mut out)?;
    out.sort();
    Ok(out)
}

fn collect_workspace_file_paths(
    root: &std::path::Path,
    dir: &std::path::Path,
    limit: usize,
    out: &mut Vec<String>,
) -> Result<(), String> {
    if out.len() >= limit {
        return Ok(());
    }

    let mut entries: Vec<std::fs::DirEntry> = std::fs::read_dir(dir)
        .map_err(|e| format!("读取工作目录失败：{e}"))?
        .filter_map(Result::ok)
        .collect();
    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        if out.len() >= limit {
            break;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if should_skip_workspace_entry(&name) {
            continue;
        }
        let file_type = match entry.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };
        let path = entry.path();
        if file_type.is_dir() {
            collect_workspace_file_paths(root, &path, limit, out)?;
        } else if file_type.is_file() {
            if let Ok(rel) = path.strip_prefix(root) {
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    Ok(())
}

fn should_skip_workspace_entry(name: &str) -> bool {
    name.starts_with('.')
        || matches!(
            name,
            "node_modules" | "target" | "dist" | "build" | "coverage" | "__pycache__"
        )
}

/// 把外部文件作为附件纳入会话工作目录，返回可被 agent 沙箱访问的相对路径。
/// - 源文件已在工作区内：直接返回相对路径，不复制。
/// - 源文件在工作区外：复制到 <工作区>/attachments/ 下（重名追加 -1/-2…），返回相对路径。
#[tauri::command]
pub fn attach_file(
    services: State<'_, AppState>,
    session_id: String,
    src_path: String,
) -> Result<String, String> {
    let src = std::path::PathBuf::from(&src_path);
    if !src.is_file() {
        return Err(format!("文件不存在：{src_path}"));
    }
    // 用会话沙箱根（惰性创建），并 canonicalize 以做可靠的「是否在工作区内」判断。
    let ws = services
        .engine_builder
        .ensure_session_workspace(&session_id)?;
    let ws_abs = std::fs::canonicalize(&ws).map_err(|e| format!("解析工作目录失败：{e}"))?;
    let src_abs = std::fs::canonicalize(&src).map_err(|e| format!("解析文件失败：{e}"))?;

    // 已在工作区内：直接引用，不复制（相对路径不含 .. / 符号链接，agent 词法沙箱可通过）。
    if let Ok(rel) = src_abs.strip_prefix(&ws_abs) {
        return Ok(rel.to_string_lossy().replace('\\', "/"));
    }

    // 工作区外：复制到 attachments/ 下。
    let dir = ws_abs.join("attachments");
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建附件目录失败：{e}"))?;
    let file_name = src_abs
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "attachment".into());
    let dest = unique_dest(&dir, &file_name);
    std::fs::copy(&src_abs, &dest).map_err(|e| format!("复制附件失败：{e}"))?;
    let rel = dest
        .strip_prefix(&ws_abs)
        .map_err(|_| "附件路径解析失败".to_string())?;
    Ok(rel.to_string_lossy().replace('\\', "/"))
}

/// 把剪贴板/拖拽的字节作为附件写入会话工作目录的 attachments/ 下，返回相对路径。
/// 用于粘贴文件/图片（webview File 拿不到绝对路径，只能传字节）。file_name 只取末段防穿越。
#[tauri::command]
pub fn save_attachment(
    services: State<'_, AppState>,
    session_id: String,
    file_name: String,
    data: Vec<u8>,
) -> Result<String, String> {
    let ws = services
        .engine_builder
        .ensure_session_workspace(&session_id)?;
    let ws_abs = std::fs::canonicalize(&ws).map_err(|e| format!("解析工作目录失败：{e}"))?;
    let dir = ws_abs.join("attachments");
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建附件目录失败：{e}"))?;
    let safe = std::path::Path::new(&file_name)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "pasted".into());
    let dest = unique_dest(&dir, &safe);
    std::fs::write(&dest, &data).map_err(|e| format!("写入附件失败：{e}"))?;
    let rel = dest
        .strip_prefix(&ws_abs)
        .map_err(|_| "附件路径解析失败".to_string())?;
    Ok(rel.to_string_lossy().replace('\\', "/"))
}

/// 读取会话工作目录内某个附件的原始字节（供前端预览图片等）。路径经沙箱校验，禁止越出工作区。
#[tauri::command]
pub fn read_attachment(
    services: State<'_, AppState>,
    session_id: String,
    rel_path: String,
) -> Result<Vec<u8>, String> {
    let ws = services
        .engine_builder
        .resolve_session_workspace(&session_id)?;
    let resolved = crate::tools::sandbox::resolve_in_workspace(&ws, &rel_path)?;
    std::fs::read(&resolved).map_err(|e| format!("读取附件失败：{e}"))
}

// 在 dir 内为 file_name 找一个不冲突的目标路径：name.ext / name-1.ext / name-2.ext …
fn unique_dest(dir: &std::path::Path, file_name: &str) -> std::path::PathBuf {
    let candidate = dir.join(file_name);
    if !candidate.exists() {
        return candidate;
    }
    let path = std::path::Path::new(file_name);
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = path.extension().map(|s| s.to_string_lossy().into_owned());
    for i in 1.. {
        let name = match &ext {
            Some(ext) => format!("{stem}-{i}.{ext}"),
            None => format!("{stem}-{i}"),
        };
        let candidate = dir.join(&name);
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!()
}

/// 产物预览内容：kind ∈ {"markdown","text","pdf","html","office","binary"}；binary 时 content 为空（前端走系统打开）。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactContent {
    pub kind: String,
    pub content: String,
}

/// 按字节内容与扩展名分类：.pdf → pdf；Office 扩展名 → office 静态预览；
/// 能 UTF-8 解码且为 .md/.markdown → markdown；.html/.htm → html；其它可解码 → text；
/// 不可解码 → binary（content 空）。
pub fn classify_artifact(path: &str, bytes: &[u8]) -> ArtifactContent {
    let lower = path.to_lowercase();
    if lower.ends_with(".pdf") {
        return ArtifactContent {
            kind: "pdf".to_string(),
            content: format!("data:application/pdf;base64,{}", base64_encode(bytes)),
        };
    }
    if super::artifact_preview::is_office_path(path) {
        return ArtifactContent {
            kind: "office".to_string(),
            content: super::artifact_preview::render_office_preview(path, bytes),
        };
    }

    match std::str::from_utf8(bytes) {
        Ok(text) => {
            let kind = if lower.ends_with(".md") || lower.ends_with(".markdown") {
                "markdown"
            } else if lower.ends_with(".html") || lower.ends_with(".htm") {
                "html"
            } else {
                "text"
            };
            ArtifactContent {
                kind: kind.to_string(),
                content: text.to_string(),
            }
        }
        Err(_) => ArtifactContent {
            kind: "binary".to_string(),
            content: String::new(),
        },
    }
}

fn base64_encode(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((input.len() + 2) / 3 * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((n >> 18) & 63) as usize] as char);
        out.push(TABLE[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

/// 读取某产物文件内容用于预览（沙箱限定在该 session 工作目录内）。>5MB → 当作 binary（不读全量）。
#[tauri::command]
pub fn read_artifact(
    services: State<'_, AppState>,
    session_id: String,
    path: String,
) -> Result<ArtifactContent, String> {
    let workspace = services
        .engine_builder
        .resolve_session_workspace(&session_id)?;
    read_workspace_file(&workspace, &path)
}

/// 读取工作目录内某文件用于预览（沙箱限定在 workspace 内）。>5MB → 当作 binary（不读全量）。
fn read_workspace_file(
    workspace: &std::path::Path,
    path: &str,
) -> Result<ArtifactContent, String> {
    let resolved = resolve_artifact_action_path(workspace, path)?;
    let meta = std::fs::metadata(&resolved).map_err(|e| format!("读取文件失败：{e}"))?;
    if meta.len() > 5 * 1024 * 1024 {
        return Ok(ArtifactContent {
            kind: "binary".to_string(),
            content: String::new(),
        });
    }
    let bytes = std::fs::read(&resolved).map_err(|e| format!("读取文件失败：{e}"))?;
    Ok(classify_artifact(path, &bytes))
}

/// 列出项目工作目录内的文件相对路径（工作目录 Tab 的文件树用）。
#[tauri::command]
pub fn list_project_workspace_files(
    services: State<'_, AppState>,
    project_id: String,
) -> Result<Vec<String>, String> {
    let workspace = services.facade.ensure_project_workspace(&project_id)?;
    list_workspace_file_paths(std::path::Path::new(&workspace), 500)
}

/// 读取项目工作目录内某文件用于预览。
#[tauri::command]
pub fn read_project_workspace_file(
    services: State<'_, AppState>,
    project_id: String,
    path: String,
) -> Result<ArtifactContent, String> {
    let workspace = services.facade.ensure_project_workspace(&project_id)?;
    read_workspace_file(std::path::Path::new(&workspace), &path)
}

/// 列出智能体工作目录内的文件相对路径（工作目录 Tab 的文件树用）。
#[tauri::command]
pub fn list_agent_workspace_files(
    services: State<'_, AppState>,
    agent_id: String,
) -> Result<Vec<String>, String> {
    let workspace = services.agents.ensure_workspace(&agent_id)?;
    list_workspace_file_paths(&workspace, 500)
}

/// 读取智能体工作目录内某文件用于预览。
#[tauri::command]
pub fn read_agent_workspace_file(
    services: State<'_, AppState>,
    agent_id: String,
    path: String,
) -> Result<ArtifactContent, String> {
    let workspace = services.agents.ensure_workspace(&agent_id)?;
    read_workspace_file(&workspace, &path)
}

/// 用系统默认应用打开工作目录内某文件（沙箱校验，禁止越出工作区）。
fn open_workspace_file(
    app: &tauri::AppHandle,
    workspace: &std::path::Path,
    path: &str,
) -> Result<(), String> {
    let resolved = resolve_artifact_action_path(workspace, path)?;
    if !resolved.exists() {
        return Err(format!("文件不存在：{}", resolved.display()));
    }
    app.opener()
        .open_path(resolved.to_string_lossy().into_owned(), None::<String>)
        .map_err(|err| format!("打开文件失败：{err}"))
}

/// 用系统默认应用打开项目工作目录内某文件。
#[tauri::command]
pub fn open_project_workspace_file(
    app: tauri::AppHandle,
    services: State<'_, AppState>,
    project_id: String,
    path: String,
) -> Result<(), String> {
    let workspace = services.facade.ensure_project_workspace(&project_id)?;
    open_workspace_file(&app, std::path::Path::new(&workspace), &path)
}

/// 用系统默认应用打开智能体工作目录内某文件。
#[tauri::command]
pub fn open_agent_workspace_file(
    app: tauri::AppHandle,
    services: State<'_, AppState>,
    agent_id: String,
    path: String,
) -> Result<(), String> {
    let workspace = services.agents.ensure_workspace(&agent_id)?;
    open_workspace_file(&app, &workspace, &path)
}

#[cfg(test)]
mod artifact_tests {
    use super::classify_artifact;

    #[test]
    fn resolve_artifact_action_path_stays_inside_workspace() {
        let workspace = std::path::Path::new("/tmp/silicon-worker-test/session-1");

        let resolved = super::resolve_artifact_action_path(workspace, "reports/a.docx").unwrap();
        assert_eq!(resolved, workspace.join("reports/a.docx"));

        assert!(super::resolve_artifact_action_path(workspace, "../a.docx").is_err());
        assert!(super::resolve_artifact_action_path(workspace, "/tmp/a.docx").is_err());
    }

    #[test]
    fn classify_markdown_text_and_binary() {
        let md = classify_artifact("notes.md", "# Hi".as_bytes());
        assert_eq!(md.kind, "markdown");
        assert_eq!(md.content, "# Hi");

        let txt = classify_artifact("data.json", "{\"a\":1}".as_bytes());
        assert_eq!(txt.kind, "text");
        assert_eq!(txt.content, "{\"a\":1}");

        let pdf = classify_artifact("report.pdf", b"%PDF-1.7\nbody");
        assert_eq!(pdf.kind, "pdf");
        assert!(pdf.content.starts_with("data:application/pdf;base64,"));

        let html = classify_artifact("report.html", b"<!doctype html><h1>Hi</h1>");
        assert_eq!(html.kind, "html");
        assert_eq!(html.content, "<!doctype html><h1>Hi</h1>");

        let htm = classify_artifact("report.htm", b"<p>Hi</p>");
        assert_eq!(htm.kind, "html");
        assert_eq!(htm.content, "<p>Hi</p>");

        let docx = classify_artifact("report.docx", b"not-a-valid-office-zip");
        assert_eq!(docx.kind, "office");
        assert!(docx.content.contains("Office 预览"));

        let xlsx = classify_artifact("report.xlsx", b"not-a-valid-office-zip");
        assert_eq!(xlsx.kind, "office");

        let pptx = classify_artifact("report.pptx", b"not-a-valid-office-zip");
        assert_eq!(pptx.kind, "office");

        let legacy_doc = classify_artifact("legacy.doc", b"\0H\0e\0l\0l\0o");
        assert_eq!(legacy_doc.kind, "office");
        assert!(legacy_doc.content.contains("Hello"));

        let legacy_xls = classify_artifact("legacy.xls", b"Sheet 1 Revenue Profit");
        assert_eq!(legacy_xls.kind, "office");

        let legacy_ppt = classify_artifact("legacy.ppt", b"Slide 1 Summary");
        assert_eq!(legacy_ppt.kind, "office");

        // 非 UTF-8 字节 → binary，content 空。
        let bin = classify_artifact("img.png", &[0xff, 0xfe, 0x00, 0x01]);
        assert_eq!(bin.kind, "binary");
        assert_eq!(bin.content, "");
    }
}

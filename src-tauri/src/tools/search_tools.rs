use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::tools::sandbox::resolve_in_workspace;
use crate::tools::Tool;

const IGNORED_DIRS: &[&str] = &[".git", "node_modules", "target", "dist", ".venv"];
const MAX_RESULTS: usize = 200;
const GREP_MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
const GREP_TOTAL_CAP: usize = 64 * 1024;

/// 判断目录名是否在忽略列表中。
fn is_ignored_dir(name: &str) -> bool {
    IGNORED_DIRS.contains(&name)
}

/// 递归收集 workspace(或子目录)下的所有文件,跳过忽略目录。
fn collect_files(root: &Path, out: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(root) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let file_type = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };
        if file_type.is_dir() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if is_ignored_dir(&name) {
                continue;
            }
            collect_files(&path, out);
        } else if file_type.is_file() {
            out.push(path);
        }
    }
}

/// 简单 glob 匹配:支持 `*`(任意串)与 `?`(单字符)。整段匹配文件名(basename)。
fn glob_match(pattern: &str, text: &str) -> bool {
    let pat: Vec<char> = pattern.chars().collect();
    let txt: Vec<char> = text.chars().collect();
    glob_match_inner(&pat, &txt)
}

fn glob_match_inner(pat: &[char], txt: &[char]) -> bool {
    // 经典动态规划/递归 glob 匹配。
    let (mut pi, mut ti) = (0usize, 0usize);
    let (mut star_pi, mut star_ti): (Option<usize>, usize) = (None, 0);
    while ti < txt.len() {
        if pi < pat.len() && (pat[pi] == '?' || pat[pi] == txt[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < pat.len() && pat[pi] == '*' {
            star_pi = Some(pi);
            star_ti = ti;
            pi += 1;
        } else if let Some(sp) = star_pi {
            pi = sp + 1;
            star_ti += 1;
            ti = star_ti;
        } else {
            return false;
        }
    }
    while pi < pat.len() && pat[pi] == '*' {
        pi += 1;
    }
    pi == pat.len()
}

/// 文件修改时间(用于排序),取不到则视为纪元起点。
fn mtime(path: &Path) -> SystemTime {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH)
}

pub struct Glob {
    pub workspace: PathBuf,
}

impl Tool for Glob {
    fn name(&self) -> &str {
        "glob"
    }

    fn label(&self) -> &str {
        "查找文件"
    }

    fn description(&self) -> &str {
        "按 glob 模式(支持 */?)在工作区查找文件,按修改时间倒序返回相对路径。"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {"type": "string", "description": "glob 模式,匹配文件名,如 *.rs"},
                "path": {"type": "string", "description": "起始子目录(工作区内,可选)"}
            },
            "required": ["pattern"]
        })
    }

    fn concurrency_safe(&self) -> bool {
        true
    }

    fn execute(&self, args: &serde_json::Value) -> Result<String, String> {
        let pattern = args
            .get("pattern")
            .and_then(|v| v.as_str())
            .ok_or("缺少 pattern")?;
        let sub = args.get("path").and_then(|v| v.as_str());
        let root = match sub {
            Some(p) => resolve_in_workspace(&self.workspace, p)?,
            None => self.workspace.clone(),
        };

        let mut files = Vec::new();
        collect_files(&root, &mut files);

        let mut matched: Vec<PathBuf> = files
            .into_iter()
            .filter(|p| {
                p.file_name()
                    .map(|n| glob_match(pattern, &n.to_string_lossy()))
                    .unwrap_or(false)
            })
            .collect();

        // 按修改时间倒序。
        matched.sort_by(|a, b| mtime(b).cmp(&mtime(a)));
        matched.truncate(MAX_RESULTS);

        if matched.is_empty() {
            return Ok(format!("无匹配: {pattern}"));
        }

        let lines: Vec<String> = matched
            .iter()
            .map(|p| {
                p.strip_prefix(&self.workspace)
                    .unwrap_or(p)
                    .to_string_lossy()
                    .to_string()
            })
            .collect();
        Ok(lines.join("\n"))
    }
}

/// 判断内容是否疑似二进制(含 NUL 字节)。
fn looks_binary(bytes: &[u8]) -> bool {
    bytes.contains(&0)
}

pub struct Grep {
    pub workspace: PathBuf,
}

impl Tool for Grep {
    fn name(&self) -> &str {
        "grep"
    }

    fn label(&self) -> &str {
        "搜索内容"
    }

    fn description(&self) -> &str {
        "在工作区文件中按固定字符串逐行搜索(非正则),输出 path:行号:内容。"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {"type": "string", "description": "要搜索的固定字符串"},
                "path": {"type": "string", "description": "起始子目录(工作区内,可选)"},
                "case_insensitive": {"type": "boolean", "description": "是否忽略大小写"}
            },
            "required": ["pattern"]
        })
    }

    fn concurrency_safe(&self) -> bool {
        true
    }

    fn execute(&self, args: &serde_json::Value) -> Result<String, String> {
        let pattern = args
            .get("pattern")
            .and_then(|v| v.as_str())
            .ok_or("缺少 pattern")?;
        let sub = args.get("path").and_then(|v| v.as_str());
        let case_insensitive = args
            .get("case_insensitive")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let root = match sub {
            Some(p) => resolve_in_workspace(&self.workspace, p)?,
            None => self.workspace.clone(),
        };

        let needle = if case_insensitive {
            pattern.to_lowercase()
        } else {
            pattern.to_string()
        };

        let mut files = Vec::new();
        collect_files(&root, &mut files);

        let mut hits: Vec<String> = Vec::new();
        let mut total_bytes = 0usize;
        'outer: for file in files {
            if hits.len() >= MAX_RESULTS || total_bytes >= GREP_TOTAL_CAP {
                break;
            }
            let meta = match std::fs::metadata(&file) {
                Ok(m) => m,
                Err(_) => continue,
            };
            if meta.len() > GREP_MAX_FILE_BYTES {
                continue;
            }
            let bytes = match std::fs::read(&file) {
                Ok(b) => b,
                Err(_) => continue,
            };
            if looks_binary(&bytes) {
                continue;
            }
            let text = match String::from_utf8(bytes) {
                Ok(t) => t,
                Err(_) => continue,
            };
            let rel = file
                .strip_prefix(&self.workspace)
                .unwrap_or(&file)
                .to_string_lossy()
                .to_string();
            for (idx, line) in text.lines().enumerate() {
                let hay = if case_insensitive {
                    line.to_lowercase()
                } else {
                    line.to_string()
                };
                if hay.contains(&needle) {
                    let entry = format!("{}:{}:{}", rel, idx + 1, line);
                    total_bytes += entry.len() + 1;
                    hits.push(entry);
                    if hits.len() >= MAX_RESULTS || total_bytes >= GREP_TOTAL_CAP {
                        break 'outer;
                    }
                }
            }
        }

        if hits.is_empty() {
            return Ok(format!("无匹配: {pattern}"));
        }
        Ok(hits.join("\n"))
    }
}

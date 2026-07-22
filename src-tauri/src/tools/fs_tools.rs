use std::path::PathBuf;

use crate::tools::sandbox::resolve_in_workspace;
use crate::tools::{RiskLevel, Tool};

pub struct ReadFile {
    pub workspace: PathBuf,
}

impl Tool for ReadFile {
    fn name(&self) -> &str {
        "read_file"
    }

    fn label(&self) -> &str {
        "读取文件"
    }

    fn description(&self) -> &str {
        "读取工作区内的文本文件。大文件用 offset/limit(按行)分页。"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "文件路径(工作区内)"},
                "offset": {"type": "integer", "minimum": 0, "description": "起始行(0基)"},
                "limit": {"type": "integer", "minimum": 1, "description": "最多读多少行"}
            },
            "required": ["path"]
        })
    }

    fn concurrency_safe(&self) -> bool {
        true
    }

    fn execute(&self, args: &serde_json::Value) -> Result<String, String> {
        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or("缺少 path")?;
        let resolved = resolve_in_workspace(&self.workspace, path)?;
        let meta = std::fs::metadata(&resolved).map_err(|e| crate::permissions::describe_read_error(&e, &resolved.display().to_string()))?;
        if meta.len() > 5 * 1024 * 1024 {
            return Err("文件过大(>5MB)".into());
        }
        let text = std::fs::read_to_string(&resolved).map_err(|e| crate::permissions::describe_read_error(&e, &resolved.display().to_string()))?;
        let lines: Vec<&str> = text.lines().collect();
        let offset = args.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        let limit = args
            .get("limit")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize);
        let end = limit
            .map(|l| (offset + l).min(lines.len()))
            .unwrap_or(lines.len());
        let slice = lines.get(offset..end).unwrap_or(&[]);
        Ok(format!(
            "[{}-{}/{} 行]\n{}",
            offset,
            end,
            lines.len(),
            slice.join("\n")
        ))
    }
}

pub struct WriteFile {
    pub workspace: PathBuf,
}

impl Tool for WriteFile {
    fn name(&self) -> &str {
        "write_file"
    }

    fn label(&self) -> &str {
        "写入文件"
    }

    fn description(&self) -> &str {
        "把内容写入工作区文件(覆写,自动建父目录)。"
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Low
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {"type": "string"},
                "content": {"type": "string"}
            },
            "required": ["path", "content"]
        })
    }

    fn execute(&self, args: &serde_json::Value) -> Result<String, String> {
        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or("缺少 path")?;
        let content = args
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or("缺少 content")?;
        let resolved = resolve_in_workspace(&self.workspace, path)?;
        if let Some(parent) = resolved.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("建目录失败: {e}"))?;
        }
        std::fs::write(&resolved, content).map_err(|e| format!("写入失败: {e}"))?;
        Ok(format!(
            "已写入 {} 字符到 {}",
            content.chars().count(),
            path
        ))
    }
}

pub struct EditFile {
    pub workspace: PathBuf,
}

impl Tool for EditFile {
    fn name(&self) -> &str {
        "edit_file"
    }

    fn label(&self) -> &str {
        "编辑文件"
    }

    fn description(&self) -> &str {
        "替换工作区文件中的文本(精确匹配)。若 old_text 匹配多处,需提供更多上下文或设 replace_all=true。"
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Low
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "文件路径(工作区内)"},
                "old_text": {"type": "string", "description": "要被替换的原文本(精确匹配)"},
                "new_text": {"type": "string", "description": "替换成的新文本"},
                "replace_all": {"type": "boolean", "description": "是否替换全部匹配(默认 false,要求唯一匹配)"}
            },
            "required": ["path", "old_text", "new_text"]
        })
    }

    fn concurrency_safe(&self) -> bool {
        false
    }

    fn execute(&self, args: &serde_json::Value) -> Result<String, String> {
        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or("缺少 path")?;
        let old_text = args
            .get("old_text")
            .and_then(|v| v.as_str())
            .ok_or("缺少 old_text")?;
        let new_text = args
            .get("new_text")
            .and_then(|v| v.as_str())
            .ok_or("缺少 new_text")?;
        let replace_all = args
            .get("replace_all")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let resolved = resolve_in_workspace(&self.workspace, path)?;
        let content = std::fs::read_to_string(&resolved).map_err(|e| format!("读取失败: {e}"))?;

        let count = content.matches(old_text).count();
        if count == 0 {
            return Err(format!("old_text 在 {path} 中未找到"));
        }
        if count > 1 && !replace_all {
            return Err(format!(
                "old_text 不唯一(出现 {count} 次),请加上下文或用 replace_all"
            ));
        }

        let (new_content, replaced) = if replace_all {
            (content.replace(old_text, new_text), count)
        } else {
            (content.replacen(old_text, new_text, 1), 1)
        };

        std::fs::write(&resolved, new_content).map_err(|e| format!("写入失败: {e}"))?;
        Ok(format!("已编辑 {path}(替换 {replaced} 处)"))
    }
}

#[cfg(test)]
mod risk_level_tests {
    use super::*;
    use crate::tools::{RiskLevel, Tool};
    use std::path::PathBuf;

    #[test]
    fn write_and_edit_are_low_risk() {
        let w = WriteFile {
            workspace: PathBuf::from("/tmp"),
        };
        let e = EditFile {
            workspace: PathBuf::from("/tmp"),
        };
        assert_eq!(w.risk_level(), RiskLevel::Low);
        assert_eq!(e.risk_level(), RiskLevel::Low);
        assert!(w.requires_confirmation());
        assert!(e.requires_confirmation());
    }
}

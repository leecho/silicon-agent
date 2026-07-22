//! notes 工具：把模型 action 翻译成 NotesBackend（osascript）调用，返回 JSON 文本。
//! 披露 Deferred；list/get=Safe、create/update=Low、delete=High。

use std::sync::Arc;

use crate::apple::notes::{NoteDraft, NotePatch, NotesBackend};
use crate::apple::AppleError;
use crate::tools::{Disclosure, RiskLevel, Tool};

pub const NOTES_TOOL: &str = "notes";

pub struct Notes {
    backend: Arc<dyn NotesBackend>,
}

impl Notes {
    pub fn new(backend: Arc<dyn NotesBackend>) -> Self {
        Notes { backend }
    }
}

fn opt_str(args: &serde_json::Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn req_str(args: &serde_json::Value, key: &str) -> Result<String, String> {
    opt_str(args, key).ok_or_else(|| format!("缺少必填参数：{key}"))
}

fn map_err(e: AppleError) -> String {
    match e {
        AppleError::PermissionDenied => {
            "备忘录未授权：请在 系统设置 → 隐私与安全性 → 自动化 中允许本应用控制「备忘录」后重试。".into()
        }
        other => other.to_string(),
    }
}

impl Tool for Notes {
    fn name(&self) -> &str {
        NOTES_TOOL
    }
    fn label(&self) -> &str {
        "备忘录"
    }
    fn description(&self) -> &str {
        "读写 macOS 备忘录。action：list（列备忘录，可选 folder）、get（按 id）、\
         create（需 title/body，可选 folder）、update（按 id 改 title/body）、delete（按 id 删）。\
         id 来自 list/get 返回。"
    }
    fn disclosure(&self) -> Disclosure {
        Disclosure::Deferred
    }
    fn risk_for(&self, args: &serde_json::Value) -> RiskLevel {
        match args.get("action").and_then(|v| v.as_str()) {
            Some("create") | Some("update") => RiskLevel::Low,
            Some("delete") => RiskLevel::High,
            _ => RiskLevel::Safe,
        }
    }
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["list", "get", "create", "update", "delete"],
                    "description": "操作类型"
                },
                "id": { "type": "string", "description": "备忘录 id（get/update/delete 必填）" },
                "title": { "type": "string", "description": "标题（create 必填）" },
                "body": { "type": "string", "description": "正文（create 必填）" },
                "folder": { "type": "string", "description": "文件夹名（list 过滤 / create 落入，可选）" }
            },
            "required": ["action"]
        })
    }
    fn execute(&self, args: &serde_json::Value) -> Result<String, String> {
        let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("");
        let result: Result<serde_json::Value, AppleError> = match action {
            "list" => self
                .backend
                .list_notes(opt_str(args, "folder").as_deref())
                .map(|v| serde_json::json!(v)),
            "get" => {
                let id = match req_str(args, "id") {
                    Ok(s) => s,
                    Err(e) => return Err(e),
                };
                self.backend.get_note(&id).map(|v| serde_json::json!(v))
            }
            "create" => {
                let draft = NoteDraft {
                    title: match req_str(args, "title") {
                        Ok(s) => s,
                        Err(e) => return Err(e),
                    },
                    body: match req_str(args, "body") {
                        Ok(s) => s,
                        Err(e) => return Err(e),
                    },
                    folder: opt_str(args, "folder"),
                };
                self.backend.create_note(draft).map(|v| serde_json::json!(v))
            }
            "update" => {
                let id = match req_str(args, "id") {
                    Ok(s) => s,
                    Err(e) => return Err(e),
                };
                let patch = NotePatch {
                    title: opt_str(args, "title"),
                    body: opt_str(args, "body"),
                };
                self.backend
                    .update_note(&id, patch)
                    .map(|v| serde_json::json!(v))
            }
            "delete" => {
                let id = match req_str(args, "id") {
                    Ok(s) => s,
                    Err(e) => return Err(e),
                };
                self.backend
                    .delete_note(&id)
                    .map(|_| serde_json::json!({ "deleted": id }))
            }
            other => return Err(format!("未知 action：{other}")),
        };
        result.map(|v| v.to_string()).map_err(map_err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apple::notes::MockNotes;

    fn tool() -> Notes {
        Notes::new(Arc::new(MockNotes::new()))
    }

    #[test]
    fn risk_for_by_action() {
        let t = tool();
        assert_eq!(t.risk_for(&serde_json::json!({"action":"list"})), RiskLevel::Safe);
        assert_eq!(t.risk_for(&serde_json::json!({"action":"create"})), RiskLevel::Low);
        assert_eq!(t.risk_for(&serde_json::json!({"action":"delete"})), RiskLevel::High);
    }

    #[test]
    fn create_get_delete_roundtrip() {
        let t = tool();
        let created = t
            .execute(&serde_json::json!({ "action": "create", "title": "标题", "body": "正文" }))
            .expect("create ok");
        let v: serde_json::Value = serde_json::from_str(&created).unwrap();
        let id = v.get("id").and_then(|x| x.as_str()).unwrap().to_string();
        assert!(t
            .execute(&serde_json::json!({ "action": "get", "id": id }))
            .unwrap()
            .contains("标题"));
        assert!(t
            .execute(&serde_json::json!({ "action": "delete", "id": id }))
            .is_ok());
    }

    #[test]
    fn create_missing_body_errors() {
        let t = tool();
        assert!(t
            .execute(&serde_json::json!({ "action": "create", "title": "x" }))
            .is_err());
    }
}

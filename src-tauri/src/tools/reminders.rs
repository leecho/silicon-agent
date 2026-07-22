//! reminders 工具：把模型 action 翻译成 RemindersBackend（EventKit）调用，返回 JSON 文本。
//! 披露 Deferred；list/get=Safe、create/update/complete=Low、delete=High。

use std::sync::Arc;

use crate::apple::reminders::{ReminderDraft, ReminderPatch, RemindersBackend};
use crate::apple::AppleError;
use crate::tools::{Disclosure, RiskLevel, Tool};

pub const REMINDERS_TOOL: &str = "reminders";

pub struct Reminders {
    backend: Arc<dyn RemindersBackend>,
}

impl Reminders {
    pub fn new(backend: Arc<dyn RemindersBackend>) -> Self {
        Reminders { backend }
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
            "提醒事项未授权：请在 系统设置 → 隐私与安全性 → 提醒事项 中允许本应用访问后重试。"
                .into()
        }
        other => other.to_string(),
    }
}

impl Tool for Reminders {
    fn name(&self) -> &str {
        REMINDERS_TOOL
    }
    fn label(&self) -> &str {
        "操作提醒事项"
    }
    fn description(&self) -> &str {
        "读写 macOS 提醒事项。action：list（列提醒，可选 include_completed）、get（按 id）、\
         create（需 title，可选 due/notes/list）、update（按 id 改）、complete（按 id 标记完成）、\
         delete（按 id 删）。时间为 ISO 8601；id 来自 list/get 返回。"
    }
    fn disclosure(&self) -> Disclosure {
        Disclosure::Deferred
    }
    fn risk_for(&self, args: &serde_json::Value) -> RiskLevel {
        match args.get("action").and_then(|v| v.as_str()) {
            Some("create") | Some("update") | Some("complete") => RiskLevel::Low,
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
                    "enum": ["list", "get", "create", "update", "complete", "delete"],
                    "description": "操作类型"
                },
                "id": { "type": "string", "description": "提醒 id（get/update/complete/delete 必填）" },
                "title": { "type": "string", "description": "标题（create 必填）" },
                "due": { "type": "string", "description": "截止时间 ISO 8601（可选）" },
                "notes": { "type": "string", "description": "备注（可选）" },
                "list": { "type": "string", "description": "提醒列表名（create 可选，默认主列表）" },
                "include_completed": { "type": "boolean", "description": "list 是否含已完成（默认 false）" }
            },
            "required": ["action"]
        })
    }
    fn execute(&self, args: &serde_json::Value) -> Result<String, String> {
        let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("");
        let result: Result<serde_json::Value, AppleError> = match action {
            "list" => {
                let include_completed = args
                    .get("include_completed")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                self.backend
                    .list_reminders(include_completed)
                    .map(|v| serde_json::json!(v))
            }
            "get" => {
                let id = match req_str(args, "id") {
                    Ok(s) => s,
                    Err(e) => return Err(e),
                };
                self.backend.get_reminder(&id).map(|v| serde_json::json!(v))
            }
            "create" => {
                let draft = ReminderDraft {
                    title: match req_str(args, "title") {
                        Ok(s) => s,
                        Err(e) => return Err(e),
                    },
                    due: opt_str(args, "due"),
                    notes: opt_str(args, "notes"),
                    list: opt_str(args, "list"),
                };
                self.backend
                    .create_reminder(draft)
                    .map(|v| serde_json::json!(v))
            }
            "update" => {
                let id = match req_str(args, "id") {
                    Ok(s) => s,
                    Err(e) => return Err(e),
                };
                let patch = ReminderPatch {
                    title: opt_str(args, "title"),
                    due: opt_str(args, "due"),
                    notes: opt_str(args, "notes"),
                };
                self.backend
                    .update_reminder(&id, patch)
                    .map(|v| serde_json::json!(v))
            }
            "complete" => {
                let id = match req_str(args, "id") {
                    Ok(s) => s,
                    Err(e) => return Err(e),
                };
                self.backend
                    .complete_reminder(&id)
                    .map(|v| serde_json::json!(v))
            }
            "delete" => {
                let id = match req_str(args, "id") {
                    Ok(s) => s,
                    Err(e) => return Err(e),
                };
                self.backend
                    .delete_reminder(&id)
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
    use crate::apple::reminders::MockReminders;

    fn tool() -> Reminders {
        Reminders::new(Arc::new(MockReminders::new()))
    }

    #[test]
    fn risk_for_by_action() {
        let t = tool();
        assert_eq!(
            t.risk_for(&serde_json::json!({"action":"list"})),
            RiskLevel::Safe
        );
        assert_eq!(
            t.risk_for(&serde_json::json!({"action":"create"})),
            RiskLevel::Low
        );
        assert_eq!(
            t.risk_for(&serde_json::json!({"action":"complete"})),
            RiskLevel::Low
        );
        assert_eq!(
            t.risk_for(&serde_json::json!({"action":"delete"})),
            RiskLevel::High
        );
    }

    #[test]
    fn create_complete_roundtrip() {
        let t = tool();
        let created = t
            .execute(&serde_json::json!({ "action": "create", "title": "买菜" }))
            .expect("create ok");
        let v: serde_json::Value = serde_json::from_str(&created).unwrap();
        let id = v.get("id").and_then(|x| x.as_str()).unwrap().to_string();
        let done = t
            .execute(&serde_json::json!({ "action": "complete", "id": id }))
            .expect("complete ok");
        let dv: serde_json::Value = serde_json::from_str(&done).unwrap();
        assert_eq!(dv.get("completed").and_then(|x| x.as_bool()), Some(true));
    }

    #[test]
    fn missing_id_errors() {
        let t = tool();
        assert!(t
            .execute(&serde_json::json!({ "action": "complete" }))
            .is_err());
    }
}

//! calendar 工具：把模型 action 翻译成 CalendarBackend（EventKit）调用，返回 JSON 文本。
//! 披露 Deferred；list/get=Safe、create/update=Low、delete=High（risk_for 跟随权限模式）。
//! 重复事件 v1 只读：改/删命中重复项由后端返回 Unsupported。

use std::sync::Arc;

use crate::apple::calendar::{CalendarBackend, EventDraft, EventPatch};
use crate::apple::AppleError;
use crate::tools::{Disclosure, RiskLevel, Tool};

pub const CALENDAR_TOOL: &str = "calendar";

pub struct Calendar {
    backend: Arc<dyn CalendarBackend>,
}

impl Calendar {
    pub fn new(backend: Arc<dyn CalendarBackend>) -> Self {
        Calendar { backend }
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

/// AppleError → 面向模型的错误文本（权限错误给出明确授权指引）。
fn map_err(e: AppleError) -> String {
    match e {
        AppleError::PermissionDenied => {
            "日历未授权：请在 系统设置 → 隐私与安全性 → 日历 中允许本应用访问后重试。".into()
        }
        other => other.to_string(),
    }
}

impl Tool for Calendar {
    fn name(&self) -> &str {
        CALENDAR_TOOL
    }
    fn label(&self) -> &str {
        "日历"
    }
    fn description(&self) -> &str {
        "读写 macOS 日历事件。action：list（按时间范围列事件，传 start/end）、get（按 id 取单条）、\
         create（新建，需 title/start/end）、update（按 id 改）、delete（按 id 删）。\
         所有时间为 ISO 8601（如 2026-06-27T15:00:00+08:00）；id 来自 list/get 返回。\
         暂不支持修改重复事件。"
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
                "id": { "type": "string", "description": "事件 id（get/update/delete 必填，来自 list/get）" },
                "title": { "type": "string", "description": "标题（create 必填）" },
                "start": { "type": "string", "description": "开始时间 ISO 8601；list 时为范围起点" },
                "end": { "type": "string", "description": "结束时间 ISO 8601；list 时为范围终点" },
                "all_day": { "type": "boolean", "description": "是否全天事件（create，默认 false）" },
                "location": { "type": "string", "description": "地点（可选）" },
                "notes": { "type": "string", "description": "备注（可选）" },
                "calendar": { "type": "string", "description": "日历名（create 可选，默认主日历）" }
            },
            "required": ["action"]
        })
    }
    fn execute(&self, args: &serde_json::Value) -> Result<String, String> {
        let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("");
        let result: Result<serde_json::Value, AppleError> = match action {
            "list" => {
                let start = match req_str(args, "start") {
                    Ok(s) => s,
                    Err(e) => return Err(e),
                };
                let end = match req_str(args, "end") {
                    Ok(s) => s,
                    Err(e) => return Err(e),
                };
                self.backend
                    .list_events(&start, &end)
                    .map(|v| serde_json::json!(v))
            }
            "get" => {
                let id = match req_str(args, "id") {
                    Ok(s) => s,
                    Err(e) => return Err(e),
                };
                self.backend.get_event(&id).map(|v| serde_json::json!(v))
            }
            "create" => {
                let draft = EventDraft {
                    title: match req_str(args, "title") {
                        Ok(s) => s,
                        Err(e) => return Err(e),
                    },
                    start: match req_str(args, "start") {
                        Ok(s) => s,
                        Err(e) => return Err(e),
                    },
                    end: match req_str(args, "end") {
                        Ok(s) => s,
                        Err(e) => return Err(e),
                    },
                    all_day: args.get("all_day").and_then(|v| v.as_bool()).unwrap_or(false),
                    location: opt_str(args, "location"),
                    notes: opt_str(args, "notes"),
                    calendar: opt_str(args, "calendar"),
                };
                self.backend.create_event(draft).map(|v| serde_json::json!(v))
            }
            "update" => {
                let id = match req_str(args, "id") {
                    Ok(s) => s,
                    Err(e) => return Err(e),
                };
                let patch = EventPatch {
                    title: opt_str(args, "title"),
                    start: opt_str(args, "start"),
                    end: opt_str(args, "end"),
                    location: opt_str(args, "location"),
                    notes: opt_str(args, "notes"),
                };
                self.backend
                    .update_event(&id, patch)
                    .map(|v| serde_json::json!(v))
            }
            "delete" => {
                let id = match req_str(args, "id") {
                    Ok(s) => s,
                    Err(e) => return Err(e),
                };
                self.backend
                    .delete_event(&id)
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
    use crate::apple::calendar::MockCalendar;

    fn tool() -> Calendar {
        Calendar::new(Arc::new(MockCalendar::new()))
    }

    #[test]
    fn risk_for_by_action() {
        let t = tool();
        assert_eq!(t.risk_for(&serde_json::json!({"action":"list"})), RiskLevel::Safe);
        assert_eq!(t.risk_for(&serde_json::json!({"action":"get"})), RiskLevel::Safe);
        assert_eq!(t.risk_for(&serde_json::json!({"action":"create"})), RiskLevel::Low);
        assert_eq!(t.risk_for(&serde_json::json!({"action":"update"})), RiskLevel::Low);
        assert_eq!(t.risk_for(&serde_json::json!({"action":"delete"})), RiskLevel::High);
    }

    #[test]
    fn create_then_get_roundtrip() {
        let t = tool();
        let created = t
            .execute(&serde_json::json!({
                "action": "create",
                "title": "会议",
                "start": "2026-06-27T10:00:00+08:00",
                "end": "2026-06-27T11:00:00+08:00"
            }))
            .expect("create ok");
        let v: serde_json::Value = serde_json::from_str(&created).unwrap();
        let id = v.get("id").and_then(|x| x.as_str()).unwrap().to_string();
        let got = t
            .execute(&serde_json::json!({ "action": "get", "id": id }))
            .expect("get ok");
        assert!(got.contains("会议"));
    }

    #[test]
    fn missing_required_arg_errors() {
        let t = tool();
        let err = t.execute(&serde_json::json!({ "action": "create", "title": "x" }));
        assert!(err.is_err());
    }

    #[test]
    fn unknown_action_errors() {
        let t = tool();
        assert!(t.execute(&serde_json::json!({ "action": "frobnicate" })).is_err());
    }
}

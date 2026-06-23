use crate::tools::Tool;

/// 控制工具：模型用它维护当前任务的待办清单（整组覆写）。引擎按名拦截、不会真正 execute——
/// 即时校验+持久化到 session、emit todos_updated、落工具结果汇总、继续（不像 ask_user 暂停）。
pub struct UpdateTodos;

pub const UPDATE_TODOS_TOOL: &str = "update_todos";

impl Tool for UpdateTodos {
    fn name(&self) -> &str {
        UPDATE_TODOS_TOOL
    }

    fn label(&self) -> &str {
        "更新待办"
    }
    fn description(&self) -> &str {
        "维护当前任务的待办清单。处理多步骤任务时调用：传入完整 todos 列表(全量覆盖)。每项 content 一句话；status 为 pending/in_progress/completed；同一时刻至多一项 in_progress。开始某步前标 in_progress、完成标 completed。简单单步问答不必用。"
    }
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type":"object",
            "properties":{
                "todos":{
                    "type":"array",
                    "items":{
                        "type":"object",
                        "properties":{
                            "content":{"type":"string"},
                            "status":{"type":"string","enum":["pending","in_progress","completed"]}
                        },
                        "required":["content","status"]
                    }
                }
            },
            "required":["todos"]
        })
    }
    fn requires_confirmation(&self) -> bool {
        false
    }
    fn execute(&self, _args: &serde_json::Value) -> Result<String, String> {
        // 引擎按名拦截，正常不会走到这里。
        Err("update_todos 由引擎处理，不应直接执行".into())
    }
}

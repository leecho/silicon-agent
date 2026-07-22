use crate::tools::Tool;

/// 控制工具（仅项目/团队线程）：编排者(PM)维护任务台账。引擎按名拦截、不真 execute——
/// 全量覆写本线程任务、emit tasks_updated、回传各任务 id（供 dispatch_agent 引用），继续（不暂停）。
pub struct UpdateTasks;

pub const UPDATE_TASKS_TOOL: &str = "update_tasks";

impl Tool for UpdateTasks {
    fn name(&self) -> &str {
        UPDATE_TASKS_TOOL
    }

    fn label(&self) -> &str {
        "更新任务"
    }

    fn description(&self) -> &str {
        "维护本项目/团队线程「本轮请求」的任务台账（计划）。goal=本轮主任务标题(一句话定基调)；tasks=该主任务下的子任务。**一次把计划列全**：既包括要委派给成员的子任务(assignee=名册成员 name)，也包括你自己要做的步骤(assignee 留空=自办，如「汇总各成员产出、产出最终报告」)。委派子任务的状态随其运行**自动更新**，无需手动改，也不要反复重列整张表。已有任务带回其 id 即更新；无 id 新建。返回每个子任务 id：随后用 dispatch_agent(task_id=…) 把委派子任务派给对应成员。"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type":"object",
            "properties":{
                "goal":{"type":"string","description":"本轮主任务标题(一句话基调，如「分析A股行情并给出方向/板块/标的」)"},
                "tasks":{
                    "type":"array",
                    "items":{
                        "type":"object",
                        "properties":{
                            "id":{"type":"string","description":"已有子任务的 id（更新用，新建省略）"},
                            "title":{"type":"string"},
                            "assignee":{"type":"string","description":"委派成员 name；留空=你自办"},
                            "status":{"type":"string","enum":["pending","in_progress","done"]}
                        },
                        "required":["title"]
                    }
                }
            },
            "required":["tasks"]
        })
    }

    fn requires_confirmation(&self) -> bool {
        false
    }

    fn execute(&self, _args: &serde_json::Value) -> Result<String, String> {
        // 引擎按名拦截，正常不会走到这里。
        Err("update_tasks 由引擎处理，不应直接执行".into())
    }
}

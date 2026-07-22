use crate::tools::Tool;

/// 控制工具：收取后台派发（`dispatch_agent(background=true)`）的子代理结论。
/// 引擎按名拦截（不走 registry.execute）：把目标子代理的结构化摘要拼回作本次 tool 结果；
/// 仍在运行且 wait=true 时父停泊等其完成（见 engine handle_collect_agents）。
pub struct CollectAgents;

pub const COLLECT_AGENTS_TOOL: &str = "collect_agents";

impl Tool for CollectAgents {
    fn name(&self) -> &str {
        COLLECT_AGENTS_TOOL
    }

    fn label(&self) -> &str {
        "收取子代理结论"
    }

    fn description(&self) -> &str {
        "收取你此前用 dispatch_agent(background=true) 后台派发的子代理的结论。\
         省略 handles=收取全部尚未收取的后台子代理；或传 handles(派发时返回的 handle 列表)只收指定的。\
         默认 wait=true：还没跑完的会等它们完成再一起返回；wait=false 则只取已完成的、未完成的标注仍在运行。"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "handles": { "type": "array", "items": { "type": "string" }, "description": "要收取的子代理 handle 列表（dispatch_agent 后台派发时返回）；省略=收取全部未收取的后台子代理。" },
                "wait": { "type": "boolean", "description": "可空，默认 true。true=未完成的等其完成再返回；false=只取已完成的，未完成的标注仍在运行。" }
            },
            "required": []
        })
    }

    fn requires_confirmation(&self) -> bool {
        false
    }

    fn execute(&self, _args: &serde_json::Value) -> Result<String, String> {
        Err("collect_agents 由引擎处理，不应直接执行".into())
    }
}

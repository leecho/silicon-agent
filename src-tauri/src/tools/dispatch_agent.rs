use crate::tools::Tool;

/// 控制工具：主助手把一段有边界的子任务**指派给一个专家**（agent 角色）。
/// 引擎按名拦截、不会真正 execute——派生一个 child 子运行（独立上下文/受限工具/独立模型），
/// child 跑完把摘要回填为本次 dispatch 的 tool 结果（异步派生，见设计 §6.2）。
pub struct DispatchAgent;

pub const DISPATCH_AGENT_TOOL: &str = "dispatch_agent";

impl Tool for DispatchAgent {
    fn name(&self) -> &str {
        DISPATCH_AGENT_TOOL
    }

    fn label(&self) -> &str {
        "指派专家"
    }

    fn description(&self) -> &str {
        "把一段有边界的子任务指派给一个专家，专家在独立上下文中用受限工具完成后回禀结论（结论/证据/风险/下一步）。没有现成专家时，你可以**现场定义一个临时专家**：给它 system_prompt（角色+硬约束+回禀格式）和 tools（从你可用的工具里选最小必要集）。不要把大目标整个丢给专家，拆成有明确产出的子任务；简单一步自己做、别派。"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": "专家名（临时专家起个简短角色名，如「天气检索」；或现有专家的角色名）" },
                "task": { "type": "string", "description": "下派给该专家的子任务描述（有边界、有明确期望产物）" },
                "system_prompt": { "type": "string", "description": "为这个临时专家现场写的角色定义：身份 + 硬约束 + 回禀格式（结论/证据/风险/建议下一步）。指派现有专家时可省略。" },
                "tools": { "type": "array", "items": { "type": "string" }, "description": "该专家可用的工具白名单（从你自己可用的工具里选最小必要集，如 web_search/web_fetch/read_file）。定义临时专家时必填。" },
                "expected_output": { "type": "string", "description": "期望产物（可空）" },
                "scope_limit": { "type": "string", "description": "范围限制（可空）" },
                "inputs": { "type": "array", "items": { "type": "string" }, "description": "上游产物引用（可空）：与该专家共享同一工作目录，把它需要参考/接续的上游成果列在这里——通常是工作目录内的相对文件路径（如 research/report.md），它会被告知去读取这些文件作为输入。" },
                "background": { "type": "boolean", "description": "可空，默认 false。true=后台派发：立即返回不等待，你可继续做自己的事或再派更多；之后用 collect_agents 取回结论。当多个子任务彼此独立、或你想边等边干时用它。false=等它跑完再继续（默认）。" },
                "task_id": { "type": "string", "description": "可空（项目/团队线程）：本次派发所执行的任务台账 id（来自 update_tasks 返回）。填了它，该任务会关联此次运行、状态随运行自动更新。" }
            },
            "required": ["name", "task"]
        })
    }

    fn requires_confirmation(&self) -> bool {
        false
    }

    fn execute(&self, _args: &serde_json::Value) -> Result<String, String> {
        // 引擎按名拦截，正常不会走到这里。
        Err("dispatch_agent 由引擎处理，不应直接执行".into())
    }
}

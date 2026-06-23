use crate::tools::Tool;

/// 控制工具：计划模式下模型调研完毕后调用它提交完整可执行的计划并暂停等待用户批准。
/// 引擎按名拦截、不会真正 execute——参数被解析成 PendingPlan 返回给命令层弹出计划卡。
pub struct ProposePlan;

pub const PROPOSE_PLAN_TOOL: &str = "propose_plan";

impl Tool for ProposePlan {
    fn name(&self) -> &str {
        PROPOSE_PLAN_TOOL
    }

    fn label(&self) -> &str {
        "提交计划"
    }
    fn description(&self) -> &str {
        "计划模式下提交完整可执行的计划等用户批准并暂停。仅在计划模式使用。"
    }
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type":"object",
            "properties":{
                "title":{"type":"string","description":"计划标题（简短一句）"},
                "summary":{"type":"string","description":"计划摘要（可空）"},
                "plan_markdown":{"type":"string","description":"完整可执行的计划正文（Markdown，含步骤）"},
                "risk_level":{"type":"string","enum":["low","medium","high"],"description":"风险等级（可空，默认 medium）"}
            },
            "required":["title","plan_markdown"]
        })
    }
    fn requires_confirmation(&self) -> bool {
        false
    }
    fn execute(&self, _args: &serde_json::Value) -> Result<String, String> {
        // 引擎按名拦截，正常不会走到这里。
        Err("propose_plan 由引擎处理，不应直接执行".into())
    }
}

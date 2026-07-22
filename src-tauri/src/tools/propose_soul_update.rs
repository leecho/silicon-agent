use crate::tools::Tool;

/// 控制工具（T73 自我演化）：反思运行里，伴随体用它提交一份对自己 SOUL（人格层）的改写**提案**。
/// 引擎按名拦截、不会真正 execute——写一条 `pending` 的 SOUL 版本（待用户批准），不改活跃人格、不碰 IDENTITY。
pub struct ProposeSoulUpdate;

pub const PROPOSE_SOUL_UPDATE_TOOL: &str = "propose_soul_update";

impl Tool for ProposeSoulUpdate {
    fn name(&self) -> &str {
        PROPOSE_SOUL_UPDATE_TOOL
    }

    fn disclosure(&self) -> crate::tools::Disclosure {
        crate::tools::Disclosure::Deferred
    }

    fn label(&self) -> &str {
        "提议人格更新"
    }

    fn description(&self) -> &str {
        "提交一份对你自己 SOUL（人格层）的改写提案：把近期相处中习得的、值得固化的稳定风格/偏好整合进人格。\
         只提炼稳定的模式、做最小必要改动；绝不改动你的 IDENTITY（身份锚/边界）。提案需用户批准后才生效。"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type":"object",
            "properties":{
                "new_soul":{"type":"string","description":"改写后的完整 SOUL 人格正文"},
                "summary":{"type":"string","description":"本次改动的简短人类可读摘要"}
            },
            "required":["new_soul","summary"]
        })
    }

    fn requires_confirmation(&self) -> bool {
        false
    }

    fn execute(&self, _args: &serde_json::Value) -> Result<String, String> {
        Err("propose_soul_update 由引擎处理，不应直接执行".into())
    }
}

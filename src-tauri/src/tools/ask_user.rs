use crate::tools::Tool;

/// 控制工具：模型调用它向用户提问并暂停等待回答。引擎按名拦截、不会真正 execute。
pub struct AskUser;

pub const ASK_USER_TOOL: &str = "ask_user";

impl Tool for AskUser {
    fn name(&self) -> &str {
        ASK_USER_TOOL
    }

    fn label(&self) -> &str {
        "向用户提问"
    }
    fn description(&self) -> &str {
        "需要向用户澄清需求或请其做决定时调用：一次可提一组问题（建议 1-4 个），暂停等待用户作答后再继续。\
         问题文本只写问题本身，不要把选项/答案写进问题里；可选项放各问题的 options；需要多选才设 multiSelect。\
         只用于反问，不要用它回答用户。"
    }
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "questions": {
                    "type": "array",
                    "description": "一组问题（建议 1-4 个）。每题问题文本只写问题本身，不夹选项/答案。",
                    "items": {
                        "type": "object",
                        "properties": {
                            "header": {"type": "string", "description": "问题主题/短标签（如「角色定位」），可空"},
                            "question": {"type": "string", "description": "问题文本（不夹选项）"},
                            "multiSelect": {"type": "boolean", "description": "是否多选，默认 false（单选）"},
                            "options": {"type": "array", "items": {"type": "string"}, "description": "可选项，可空（空则纯自由作答）"}
                        },
                        "required": ["question"]
                    }
                }
            },
            "required": ["questions"]
        })
    }
    fn requires_confirmation(&self) -> bool {
        false
    }
    fn execute(&self, _args: &serde_json::Value) -> Result<String, String> {
        // 引擎按名拦截，正常不会走到这里。
        Err("ask_user 由引擎处理，不应直接执行".into())
    }
}

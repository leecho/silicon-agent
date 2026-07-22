use crate::tools::Tool;

/// 控制工具：模型用它把关键事实/偏好/长期目标写进长期记忆。引擎按名拦截、不会真正 execute——
/// 即时持久化到全局 memories、落工具结果、继续（不像 ask_user 暂停）。
pub struct Remember;

pub const REMEMBER_TOOL: &str = "remember";

impl Tool for Remember {
    fn name(&self) -> &str {
        REMEMBER_TOOL
    }

    fn disclosure(&self) -> crate::tools::Disclosure {
        crate::tools::Disclosure::Deferred
    }

    fn label(&self) -> &str {
        "记录记忆"
    }
    fn description(&self) -> &str {
        "把关于用户/项目的关键事实、偏好、长期目标记入长期记忆（跨会话保留、后续自动注入上下文）。只记真正值得长期记住的，不记一次性内容。"
    }
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type":"object",
            "properties":{
                "content":{"type":"string"}
            },
            "required":["content"]
        })
    }
    fn requires_confirmation(&self) -> bool {
        false
    }
    fn execute(&self, _args: &serde_json::Value) -> Result<String, String> {
        // 引擎按名拦截，正常不会走到这里。
        Err("remember 由引擎处理，不应直接执行".into())
    }
}

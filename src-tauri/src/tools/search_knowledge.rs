use crate::tools::Tool;

/// 控制工具：专家在对话中主动查阅已挂载的资料库。引擎按名拦截、不走 registry 真执行——
/// 解析当前会话挂载的知识库 → FTS 检索 → 带来源片段回灌。
pub struct SearchKnowledge;

pub const SEARCH_KNOWLEDGE_TOOL: &str = "search_knowledge";

impl Tool for SearchKnowledge {
    fn name(&self) -> &str {
        SEARCH_KNOWLEDGE_TOOL
    }
    fn label(&self) -> &str {
        "查阅资料"
    }
    fn description(&self) -> &str {
        "在用户为当前对话挂载的资料库中查阅相关内容。当用户的问题可能依赖已添加的资料时调用；可多次以不同措辞检索。返回带来源标注的资料片段。"
    }
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "检索词或问题" },
                "top_k": { "type": "integer", "description": "返回片段数，默认 5", "default": 5 }
            },
            "required": ["query"]
        })
    }
    fn concurrency_safe(&self) -> bool {
        true
    }
    fn disclosure(&self) -> crate::tools::Disclosure {
        // 与 remember/web_* 等上下文工具对齐：默认不常驻，经 find_tools 按需激活，省 token。
        crate::tools::Disclosure::Deferred
    }
    fn execute(&self, _args: &serde_json::Value) -> Result<String, String> {
        Err("search_knowledge 由引擎处理，不应直接执行".into())
    }
}

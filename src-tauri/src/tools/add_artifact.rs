use crate::tools::Tool;

/// 控制工具：模型用它把产出的最终交付文件登记为「产物」。引擎按名拦截、不真 execute——
/// 持久化到 session_artifacts（绑定产生它的消息）、emit artifacts_updated、落工具结果、继续。
pub struct AddArtifact;

pub const ADD_ARTIFACT_TOOL: &str = "add_artifact";

impl Tool for AddArtifact {
    fn name(&self) -> &str {
        ADD_ARTIFACT_TOOL
    }

    fn label(&self) -> &str {
        "登记产物"
    }
    fn description(&self) -> &str {
        "把你产出的文件登记到侧栏，便于用户查看与预览。kind=\"final\"：交付给用户的最终成果（报告/方案/汇总文档，如 .md/.docx/.pdf/.xlsx）；kind=\"working\"：为产出最终成果而写的脚本、中间数据、临时文件（如 .py/.js/.sh）。生成报告的脚本属于 working，不是 final。path 为工作目录相对路径；title 可选（缺省取文件名）；kind 可选（缺省 final）。"
    }
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type":"object",
            "properties":{
                "path":{"type":"string","description":"工作目录相对路径"},
                "title":{"type":"string","description":"展示标题，可选"},
                "kind":{"type":"string","enum":["final","working"],"description":"final=最终交付成果；working=脚本/中间文件。缺省 final"}
            },
            "required":["path"]
        })
    }
    fn requires_confirmation(&self) -> bool {
        false
    }
    fn execute(&self, _args: &serde_json::Value) -> Result<String, String> {
        // 引擎按名拦截，正常不会走到这里。
        Err("add_artifact 由引擎处理，不应直接执行".into())
    }
}

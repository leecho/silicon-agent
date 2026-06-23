use crate::tools::Tool;

/// 控制工具：读取某技能目录下的附带文件（渐进披露第三级）。引擎按名拦截、不真正 execute——
/// 从该技能目录读相对路径文件（限定目录内）、落为 tool 结果、继续。配合 load_skill 披露的文件清单使用。
pub struct ReadSkillFile;

pub const READ_SKILL_FILE_TOOL: &str = "read_skill_file";

impl Tool for ReadSkillFile {
    fn name(&self) -> &str {
        READ_SKILL_FILE_TOOL
    }

    fn label(&self) -> &str {
        "读取技能文件"
    }
    fn description(&self) -> &str {
        "读取某技能附带的参考/脚本文件（路径来自 load_skill 返回的「附带文件」清单）。当技能正文指向 references/ 等子文件、需要其内容时调用。"
    }
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type":"object",
            "properties":{
                "name":{"type":"string","description":"技能名（与 load_skill 一致）"},
                "path":{"type":"string","description":"该技能目录内的相对路径，如 references/foo.md"}
            },
            "required":["name","path"]
        })
    }
    fn requires_confirmation(&self) -> bool {
        false
    }
    fn execute(&self, _args: &serde_json::Value) -> Result<String, String> {
        // 引擎按名拦截，正常不会走到这里。
        Err("read_skill_file 由引擎处理，不应直接执行".into())
    }
}

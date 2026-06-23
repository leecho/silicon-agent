use crate::tools::Tool;

/// 控制工具：模型调用它按名称加载某个技能的详细内容。引擎按名拦截、不会真正 execute——
/// 即时从磁盘 SKILL.md 读正文、落为 tool 结果、继续（不像 ask_user 暂停）。
pub struct LoadSkill;

pub const LOAD_SKILL_TOOL: &str = "load_skill";

impl Tool for LoadSkill {
    fn name(&self) -> &str {
        LOAD_SKILL_TOOL
    }

    fn label(&self) -> &str {
        "加载技能"
    }
    fn description(&self) -> &str {
        "按名称加载某个技能的详细内容。当 system prompt 的「可用技能」列出了相关技能、且当前任务需要其指引时调用。"
    }
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type":"object",
            "properties":{
                "name":{"type":"string","description":"技能名"}
            },
            "required":["name"]
        })
    }
    fn requires_confirmation(&self) -> bool {
        false
    }
    fn execute(&self, _args: &serde_json::Value) -> Result<String, String> {
        // 引擎按名拦截，正常不会走到这里。
        Err("load_skill 由引擎处理，不应直接执行".into())
    }
}

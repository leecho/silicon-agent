//! 系统提示装配：基础人设 + 当前本地时间 + 启用技能 + 计划模式 + 工作目录。
//!
//! 纯函数，依赖（技能/模式/工作目录）由调用方显式传入。

use chrono::Datelike;

use crate::skill::types::SkillSummary;

const SYSTEM_PROMPT_BASE: &str = "你是 SiliconAgent，一个有帮助的助手。用简洁清晰的中文回答。";

/// Agent 人设快照：身份(非空时替换默认人设句) + 灵魂(非空时追加「## 人格」段)。两者均可缺省。
#[derive(Default)]
pub struct Persona {
    pub identity: Option<String>,
    pub soul: Option<String>,
}

/// 构建系统提示：基础人设 + **当前本地日期时间** + **启用技能（名+简介，渐进式披露）**。
/// 注入当前时间，避免模型从对话上文里捡到旧日期、把「今天/现在」定位错。
/// 启用技能仅以名+简介列出；模型需要某技能详情时调 `load_skill(name)` 取全文（省 token）。
pub fn system_prompt(
    persona: &Persona,
    enabled_skills: &[SkillSummary],
    mode: &str,
    workspace: &str,
) -> String {
    let now = chrono::Local::now();
    let weekday = match now.weekday() {
        chrono::Weekday::Mon => "星期一",
        chrono::Weekday::Tue => "星期二",
        chrono::Weekday::Wed => "星期三",
        chrono::Weekday::Thu => "星期四",
        chrono::Weekday::Fri => "星期五",
        chrono::Weekday::Sat => "星期六",
        chrono::Weekday::Sun => "星期日",
    };
    // 人设头：身份非空则替换默认人设句；灵魂非空则紧随其后追加「## 人格」段。
    let base = persona
        .identity
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(SYSTEM_PROMPT_BASE);
    let mut header = base.to_string();
    if let Some(soul) = persona
        .soul
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        header.push_str(&format!("\n\n## 人格\n{soul}"));
    }
    let mut prompt = format!(
        "{}\n\n处理多步骤任务时，先用 update_todos 拆出待办清单并随推进更新状态（同一时刻至多一项 in_progress）；简单问答不必用。\n\n产出文件后用 add_artifact(path, title?, kind?) 登记到侧栏：交付给用户的最终成果（报告/方案/汇总文档）用 kind=\"final\"；为产出成果而写的脚本、中间数据、临时文件用 kind=\"working\"。生成报告的脚本属于 working，不是 final。\n\n当前时间：{} {}（本地时区）。涉及「今天/现在/最近」等时，以此为准，不要依据对话历史里出现的其它日期推断当前时间。",
        header,
        now.format("%Y-%m-%d %H:%M"),
        weekday
    );
    if !enabled_skills.is_empty() {
        prompt.push_str("\n\n## 可用技能\n");
        for skill in enabled_skills {
            prompt.push_str(&format!("- **{}**：{}\n", skill.name, skill.description));
        }
        prompt.push_str(
            "需要某技能的详细指引时，调用 load_skill(name=\"技能名\") 获取其完整内容后再据此行动。",
        );
    }
    if mode == "plan" {
        prompt.push_str("\n\n## 计划模式\n当前处于计划模式：只能用只读工具(读文件/搜索/grep/web_search/web_fetch)调研，**不能写文件、改文件或执行命令**。请充分调研后，调用 propose_plan 提交完整、可执行的计划，等待用户批准；用户批准后会切换到执行模式，你再按计划实施。");
    }
    if !workspace.is_empty() {
        prompt.push_str(&format!(
            "\n\n## 工作目录\n你的工作目录是：{workspace}\n所有你生成的文件（报告/文档/产物等）必须保存到这个工作目录内：写文件用相对路径，或用以「{workspace}」为前缀的绝对路径。run_command 默认就在该工作目录下执行，脚本里写文件请用相对路径或该工作目录前缀。**不要**把产出写到工作目录以外的位置（例如源文件所在目录、用户其它文件夹或系统路径）。add_artifact 登记产物时同样用工作目录内的相对路径。"
        ));
    }
    prompt
}

#[cfg(test)]
mod tests {
    use super::{system_prompt, Persona};

    #[test]
    fn includes_workspace_directory_and_directive() {
        let p = system_prompt(&Persona::default(), &[], "normal", "/home/u/.siliconagent/session-x");
        assert!(p.contains("/home/u/.siliconagent/session-x"));
        assert!(p.contains("你的工作目录"));
    }

    #[test]
    fn empty_workspace_omits_section() {
        let p = system_prompt(&Persona::default(), &[], "normal", "");
        assert!(!p.contains("你的工作目录"));
    }

    #[test]
    fn no_memory_section() {
        // 记忆子系统已移除：system prompt 不应再含记忆段或 remember 引导。
        let p = system_prompt(&Persona::default(), &[], "normal", "");
        assert!(!p.contains("remember"));
        assert!(!p.contains("长期记忆"));
        assert!(!p.contains("相关记忆"));
        assert!(!p.contains("用户画像"));
    }

    #[test]
    fn plan_mode_injects_plan_section() {
        let plan = system_prompt(&Persona::default(), &[], "plan", "");
        assert!(plan.contains("## 计划模式"));
        let normal = system_prompt(&Persona::default(), &[], "normal", "");
        assert!(!normal.contains("## 计划模式"));
    }

    #[test]
    fn default_persona_keeps_base_unchanged() {
        // 双 None 人设 ⇒ 仍含默认人设句、不含「## 人格」段（守护零行为变更）。
        let p = system_prompt(&Persona::default(), &[], "normal", "");
        assert!(p.contains("你是 SiliconAgent，一个有帮助的助手。"));
        assert!(!p.contains("## 人格"));
    }

    #[test]
    fn identity_replaces_base_sentence() {
        let persona = Persona {
            identity: Some("你是小硅，一名严谨的研究助手。".to_string()),
            soul: None,
        };
        let p = system_prompt(&persona, &[], "normal", "");
        assert!(p.contains("你是小硅，一名严谨的研究助手。"));
        assert!(!p.contains("你是 SiliconAgent，一个有帮助的助手。"));
        assert!(!p.contains("## 人格"));
    }

    #[test]
    fn soul_appends_personality_section() {
        let persona = Persona {
            identity: None,
            soul: Some("耐心、克制、先问后做。".to_string()),
        };
        let p = system_prompt(&persona, &[], "normal", "");
        assert!(p.contains("## 人格"));
        assert!(p.contains("耐心、克制、先问后做。"));
        assert!(p.contains("你是 SiliconAgent，一个有帮助的助手。"));
    }

    #[test]
    fn identity_and_soul_both_present_in_order() {
        let persona = Persona {
            identity: Some("你是小硅。".to_string()),
            soul: Some("严谨。".to_string()),
        };
        let p = system_prompt(&persona, &[], "normal", "");
        let id_pos = p.find("你是小硅。").unwrap();
        let soul_pos = p.find("## 人格").unwrap();
        assert!(id_pos < soul_pos);
    }
}


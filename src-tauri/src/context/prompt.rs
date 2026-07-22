//! 系统提示装配：基础人设 + 当前本地时间 + 启用技能 + 记忆段 + 计划模式 + 工作目录。
//!
//! 纯函数，依赖（技能/记忆段/模式/工作目录）由调用方显式传入。
//! 记忆段由 `memory::prompt::render` 预渲染（「## 用户画像」+「## 相关记忆」），
//! 本函数只负责把它拼进整体 system prompt——记忆的呈现逻辑归属 memory 模块。

use chrono::Datelike;

use crate::expert::ExpertSummary;
use crate::skill::types::SkillSummary;

const SYSTEM_PROMPT_BASE: &str = "你是 Silicon Worker，一个有帮助的助手。用简洁清晰的中文回答。";

/// 构建系统提示：基础人设 + **当前本地日期时间** + **启用技能（名+简介，渐进式披露）**
/// + **记忆段（由 memory::prompt 预渲染的检索式画像+相关记忆，可空）**。
/// 注入当前时间，避免模型从对话上文里捡到旧日期、把「今天/现在」定位错。
/// 启用技能仅以名+简介列出；模型需要某技能详情时调 `load_skill(name)` 取全文（省 token）。
pub fn system_prompt(
    enabled_skills: &[SkillSummary],
    enabled_experts: &[ExpertSummary],
    memory_block: &str,
    mode: &str,
    workspace: &str,
    // 激活团队（plugin）的展示名；Some = Plugin 模式（roster 聚焦该团队），None = 自由模式。
    active_team: Option<&str>,
    // 人设覆盖（type=agent 的单专家作主对话身份）；Some = 注入「当前身份」段，覆盖默认助手人设。
    persona: Option<&str>,
    // 团队协作 SOP（type=team 的 lead 正文）；Some = 在「团队」段内注入编排指引。
    team_sop: Option<&str>,
    // 是否允许下派（注入「团队/派发」段）。子代理（叶子）传 false：不再下派、不列团队。
    allow_dispatch: bool,
    // 子代理执行方式：true=串行（按派发顺序逐个跑），false=并行（同轮一起跑）。仅影响派发段的措辞，
    // 让统筹者按实际并发语义规划任务先后与叙述；运行时的串/并由调度层（RunCoordinator）强制执行。
    subagent_serial: bool,
    // 渐进式披露（T83）：未默认启用的 Deferred 工具目录（name, description）。空则不注入。
    deferred_tools: &[(String, String)],
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
    let mut prompt = format!(
        "{}\n\n处理多步骤任务时，先用 update_todos 拆出待办清单并随推进更新状态（同一时刻至多一项 in_progress）；简单问答不必用。\n\n产出文件后用 add_artifact(path, title?, kind?) 登记到侧栏：交付给用户的最终成果（报告/方案/汇总文档）用 kind=\"final\"；为产出成果而写的脚本、中间数据、临时文件用 kind=\"working\"。生成报告的脚本属于 working，不是 final。\n\n当对话中出现关于用户或项目的、值得跨会话长期记住的关键事实、偏好或长期目标时，主动用 remember(content) 记入长期记忆（后续会自动注入上下文）；只记真正长期有用的，不记一次性内容。\n\n当前时间：{} {}（本地时区）。涉及「今天/现在/最近」等时，以此为准，不要依据对话历史里出现的其它日期推断当前时间。",
        SYSTEM_PROMPT_BASE,
        now.format("%Y-%m-%d %H:%M"),
        weekday
    );
    // 人设覆盖（type=agent）：高优先级身份段，紧跟基底，定义「你是谁」，贯穿整个对话。
    if let Some(p) = persona.filter(|s| !s.trim().is_empty()) {
        prompt.push_str(&format!("\n\n## 当前身份\n{p}"));
    }
    if !enabled_skills.is_empty() {
        prompt.push_str("\n\n## 可用技能\n");
        for skill in enabled_skills {
            // plugin 提供的技能用**限定名**（`plugin:name`，T108 §6）：装两个都带同名技能的
            // plugin 时，裸名无从消歧。`load_skill` 的解析器同口径认限定名。
            let shown = skill.qualified_name.as_deref().unwrap_or(&skill.name);
            prompt.push_str(&format!("- **{}**：{}\n", shown, skill.description));
        }
        prompt.push_str(
            "需要某技能的详细指引时，调用 load_skill(name=\"技能名\") 获取其完整内容后再据此行动。",
        );
    }
    if !deferred_tools.is_empty() {
        prompt.push_str("\n\n## 可用工具目录\n以下工具未默认加载。需要时先调用 find_tools(query=\"关键词\") 或 find_tools(select=[\"精确名\"]) 加载，加载后即可直接调用：\n");
        for (name, desc) in deferred_tools {
            prompt.push_str(&format!("- **{name}**：{desc}\n"));
        }
    }
    // 子代理（叶子）不注入「团队/派发」段——它们不再下派、其 registry 也已剔除 dispatch_agent。
    if allow_dispatch {
        match active_team {
            // Plugin 模式：激活了某团队。你是团队统筹者，职责是把活拆给成员派发、汇总，而非亲自全做。
            Some(team_name) => {
                prompt.push_str(&format!(
                "\n\n## 团队\n\
                当前已激活团队「{team_name}」。**你是这支团队的统筹者**：本会话的工作应当**拆成子任务、交给团队成员完成**，再由你汇总回复——而不是你一个人把调研/检索/撰写全做了。凡是某个成员能做的子任务，就 `dispatch_agent(name=\"成员名\", task=\"...\")` 派给它（**不必写 system_prompt/tools**，成员已定义好），可一轮内派多个。下面是成员：\n",
            ));
                for m in enabled_experts {
                    let who = m.display_name.as_deref().unwrap_or(&m.name);
                    let title = m
                        .profession
                        .as_deref()
                        .map(|p| format!("·{p}"))
                        .unwrap_or_default();
                    prompt.push_str(&format!(
                        "- **{}**{}（name=`{}`）：{}\n",
                        who, title, m.name, m.description
                    ));
                }
                if let Some(sop) = team_sop.filter(|s| !s.trim().is_empty()) {
                    prompt.push_str(&format!("\n团队协作指引（来自 lead）：\n{sop}\n"));
                }
                prompt.push_str(
                "\n原则：默认派活、不亲自下场做成员该做的事。**只能指派上面名册内的成员**（按其 `name`）——这是固定团队，**不要、也不能临场新建临时专家**；若没有合适成员，就如实告诉用户「当前团队没有能接这件事的成员」，并建议把所需成员加入项目/团队，而不是自己造一个。一句话的小事可自己直接处理。多步骤工作先用 `update_tasks` 列任务台账（委派项标 `assignee`），任务状态随运行自动更新。成员**不能**再 dispatch（不递归）。",
            );
                // 派发方式按执行模式分流：并行=后台批量+collect；串行=前台逐个、靠回复推进、无需 collect。
                prompt.push_str(if subagent_serial {
                "\n派发方式（当前为**串行**）：委派子任务**逐个前台派发**——`dispatch_agent(task_id=…, name=\"成员名\", task=\"…\")`（**不要带 background**），派出后父会停下等该成员，其回复会作为这次派发的结果直接返回给你；据此再派下一个，可按上一个结果调整后续。**不要用 collect_agents**。任务有依赖就按依赖先后逐个派。全部完成后你整合各成员结论、回复用户。"
            } else {
                "\n派发方式（当前为**并行**）：委派子任务用 `dispatch_agent(task_id=…, name=\"成员名\", task=\"…\", background=true)` 后台派发（立即返回、不等待），彼此独立的子任务尽量一轮内一起派出以加快推进；随后用 `collect_agents` 取回结论（省略 handles 收全部、或传指定 handle），再整合回复用户。"
            });
            }
            // 自由模式（无激活团队）：**整段不注入**——就是默认助手 + 一个可用的 dispatch_agent。
            //
            // 这里曾有约 550 token 的「如何现场定义临时专家」指引，但它逐条都在**重复
            // `dispatch_agent` 工具描述已经说过的话**（定位、ad-hoc 定义、system_prompt 三段、
            // tools 最小必要集、拆分粒度、简单事自己做）。而该工具是**默认加载**的
            // （`builder.rs:registry.register(DispatchAgent)`），描述本就随每次请求发出，
            // 再抄一遍纯属常驻浪费。「不能递归」也不靠嘴说——子代理 registry 直接
            // `without_name(DISPATCH_AGENT_TOOL)` 结构性剔除。
            //
            // 专家名册同样不注入：expert 只在被**显式选用**时进上下文（激活团队 → roster，
            // 选中专家 → persona）。想让现成专家协作，组建团队——编排本就是 team 的职责。
            None => {}
        }
    } // end if allow_dispatch
    if !memory_block.is_empty() {
        prompt.push_str("\n\n");
        prompt.push_str(memory_block);
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
    use super::system_prompt;

    #[test]
    fn includes_workspace_directory_and_directive() {
        let p = system_prompt(
            &[],
            &[],
            "",
            "normal",
            "/home/u/.siliconworker/session-x",
            None,
            None,
            None,
            true,
            false,
            &[],
        );
        assert!(p.contains("/home/u/.siliconworker/session-x"));
        assert!(p.contains("你的工作目录"));
    }

    #[test]
    fn empty_workspace_omits_section() {
        let p = system_prompt(&[], &[], "", "normal", "", None, None, None, true, false, &[]);
        assert!(!p.contains("你的工作目录"));
    }

    #[test]
    fn base_prompt_nudges_remember_even_without_memory() {
        // 冷启动：记忆段为空时仍应有常驻的 remember 引导，避免「无记忆→无引导→永不记」死循环。
        let p = system_prompt(&[], &[], "", "normal", "", None, None, None, true, false, &[]);
        assert!(p.contains("remember(content)"));
    }

    /// 自由模式（无激活团队）**不得**注入任何已安装专家名册。
    ///
    /// 曾经这里会列出 `list_enabled()` 的全部专家，两个毛病：上下文随装包数线性膨胀；
    /// 且名册里的插件专家在自由模式下**根本派不动**（dispatch 只解析 owner 为空的散装专家）。
    /// 专家只在被显式选用（激活团队 / 选中专家）时才进上下文。
    #[test]
    fn free_mode_does_not_leak_installed_experts() {
        use crate::expert::{ExpertSource, ExpertSummary};
        let members = vec![ExpertSummary {
            id: "i".into(),
            source: ExpertSource::Builtin,
            name: "explorer".into(),
            description: "只读勘探".into(),
            tools: vec![],
            model_tier: "aux".into(),
            max_turns: None,
            role: "member".into(),
            plugin_id: String::new(),
            team_id: String::new(),
            display_name: None,
            profession: None,
            avatar: None,
            color: None,
            enabled: true,
            installed_at: "1".into(),
            catalog_id: None,
            group_id: None,
        }];
        let p = system_prompt(
            &[],
            &members,
            "",
            "normal",
            "/tmp/ws",
            None,
            None,
            None,
            true,
            false,
            &[],
        );
        // 自由模式整段不注入：既无「## 团队」，也不泄漏任何已装专家。
        // 派发能力仍在（dispatch_agent 是默认加载工具，描述随请求发出），只是不再用
        // 系统提示词把工具描述重抄一遍。
        assert!(!p.contains("## 团队"), "自由模式不应有团队段");
        assert!(!p.contains("explorer"), "自由模式不应泄漏已安装专家名");
        assert!(!p.contains("只读勘探"), "自由模式不应泄漏已安装专家描述");
    }

    #[test]
    fn renders_active_team_with_identity() {
        use crate::expert::{ExpertSource, ExpertSummary};
        let members = vec![ExpertSummary {
            id: "i".into(),
            source: ExpertSource::Plugin,
            name: "lead".into(),
            description: "统筹".into(),
            tools: vec![],
            model_tier: "main".into(),
            max_turns: None,
            role: "lead".into(),
            plugin_id: "plg".into(),
            team_id: String::new(),
            display_name: Some("何执舟".into()),
            profession: Some("首席策略官".into()),
            avatar: None,
            color: None,
            enabled: true,
            installed_at: "1".into(),
            catalog_id: None,
            group_id: None,
        }];
        let p = system_prompt(
            &[],
            &members,
            "",
            "normal",
            "/tmp/ws",
            Some("交易团队"),
            None,
            Some("先调研、后决策、再复盘。"),
            true,
            true, // 串行模式
            &[],
        );
        assert!(p.contains("已激活团队「交易团队」"));
        assert!(p.contains("你是这支团队的统筹者"));
        assert!(p.contains("何执舟"));
        assert!(p.contains("首席策略官"));
        assert!(p.contains("name=`lead`"));
        // 团队 SOP（lead 正文）注入。
        assert!(p.contains("团队协作指引"));
        assert!(p.contains("先调研、后决策、再复盘"));
        // 串行模式：前台 reply-based，明确不用 collect_agents / 不带 background。
        assert!(p.contains("当前为**串行**"));
        assert!(!p.contains("当前为**并行**"));
        assert!(p.contains("不要用 collect_agents"));
        assert!(p.contains("不要带 background"));
        // 并行模式（同 Some 团队分支）：后台批量 + collect。
        let pp = system_prompt(
            &[],
            &members,
            "",
            "normal",
            "/tmp/ws",
            Some("交易团队"),
            None,
            Some("先调研、后决策、再复盘。"),
            true,
            false,
            &[],
        );
        assert!(pp.contains("当前为**并行**"));
        assert!(!pp.contains("当前为**串行**"));
        assert!(pp.contains("collect_agents"));
        assert!(pp.contains("background=true"));
    }

    #[test]
    fn renders_persona_override() {
        let persona = "你现在以「资深架构师」的身份与用户对话。只谈架构、不写实现代码。";
        let p = system_prompt(
            &[],
            &[],
            "",
            "normal",
            "",
            None,
            Some(persona),
            None,
            true,
            false,
            &[],
        );
        assert!(p.contains("## 当前身份"));
        assert!(p.contains("资深架构师"));
        assert!(p.contains("只谈架构"));
    }

    #[test]
    fn child_omits_dispatch_section_keeps_scaffold() {
        // 子代理：allow_dispatch=false → 无「团队」段，但保留工作目录与人设脚手架。
        let persona = "你是检索专员，只读、须给出处。";
        let p = system_prompt(
            &[],
            &[],
            "",
            "normal",
            "/tmp/ws",
            None,
            Some(persona),
            None,
            false,
            false,
            &[],
        );
        assert!(!p.contains("## 团队"));
        assert!(p.contains("## 当前身份"));
        assert!(p.contains("检索专员"));
        assert!(p.contains("你的工作目录"));
    }
}

#[cfg(test)]
mod deferred_catalog_tests {
    use super::*;

    #[test]
    fn renders_deferred_tools_block() {
        let deferred = vec![
            ("web_fetch".to_string(), "抓取网页内容".to_string()),
            ("mcp__pan__upload".to_string(), "上传文件到网盘".to_string()),
        ];
        let p = system_prompt(&[], &[], "", "normal", "/ws", None, None, None, true, false, &deferred);
        assert!(p.contains("可用工具目录"));
        assert!(p.contains("web_fetch"));
        assert!(p.contains("上传文件到网盘"));
        assert!(p.contains("find_tools"));
    }

    #[test]
    fn empty_deferred_renders_no_block() {
        let p = system_prompt(&[], &[], "", "normal", "/ws", None, None, None, true, false, &[]);
        assert!(!p.contains("可用工具目录"));
    }
}

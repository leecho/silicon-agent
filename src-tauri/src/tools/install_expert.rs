use std::sync::Arc;

use crate::expert::ExpertService;
use crate::tools::{RiskLevel, Tool};

/// `install_expert` 工具：把对话里（在 `create-expert` 技能引导下）设计好的一个**散装专家**
/// 登记到平台（写 .md + 索引）。创建指引在技能里，本工具只负责登记这一副作用。
/// risk = High（写持久全局状态，需用户确认）。
pub struct InstallExpert {
    pub agents: Arc<ExpertService>,
}

fn str_array(v: &serde_json::Value, key: &str) -> Vec<String> {
    v.get(key)
        .and_then(|x| x.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|e| e.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default()
}

fn opt_str(v: &serde_json::Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(|x| x.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

impl Tool for InstallExpert {
    fn name(&self) -> &str {
        "install_expert"
    }

    fn disclosure(&self) -> crate::tools::Disclosure {
        crate::tools::Disclosure::Deferred
    }

    fn label(&self) -> &str {
        "创建专家"
    }

    fn description(&self) -> &str {
        "登记一个已敲定的专家（助手角色）到平台——把角色设定写入 system_prompt 后调用本工具完成创建。\
         设计怎么做由 create-expert 技能引导；本工具只执行登记，需用户确认。\
         登记后它会出现在「专家」列表，可在会话选作对话身份、被主助手派发、或编入团队。"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": "名称（唯一标识，可用中文，如 投研助手）" },
                "description": { "type": "string", "description": "一句话说明它能帮用户干什么" },
                "system_prompt": { "type": "string", "description": "角色设定正文：它是谁、行事准则、输出格式等（成为该专家的人设）" },
                "tools": { "type": "array", "items": { "type": "string" }, "description": "可用工具白名单（如 read_file, grep, web_search）；留空则默认开放全部工具" },
                "model": { "type": "string", "enum": ["aux", "main"], "description": "模型档位：main=主力模型(默认), aux=辅助模型" },
                "display_name": { "type": "string", "description": "可选显示名" },
                "profession": { "type": "string", "description": "可选职业/头衔" },
                "quick_prompts": { "type": "array", "items": { "type": "string" }, "description": "可选用户引导语：几条示范提示词，引导用户怎么用这个专家（如「帮我分析这家公司的财报」）" }
            },
            "required": ["name", "description", "system_prompt"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::High
    }

    fn execute(&self, args: &serde_json::Value) -> Result<String, String> {
        let name = opt_str(args, "name").ok_or("缺少 name")?;
        let description = opt_str(args, "description").unwrap_or_default();
        let system_prompt = opt_str(args, "system_prompt").ok_or("缺少 system_prompt")?;
        let tools = str_array(args, "tools");
        let model = opt_str(args, "model").unwrap_or_else(|| "main".into());
        let summary = self.agents.create_standalone(
            &name,
            &description,
            &system_prompt,
            tools,
            &model,
            opt_str(args, "display_name"),
            opt_str(args, "profession"),
            None,
            str_array(args, "quick_prompts"),
            None,
        )?;
        Ok(format!(
            "已创建专家「{}」：下一轮起可在会话角色选择器（👥）选用，或编入团队。",
            summary.display_name.unwrap_or(summary.name)
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool() -> InstallExpert {
        let base = std::env::temp_dir().join(format!("siw-installexpert-{}", std::process::id()));
        let db = Arc::new(crate::storage::AppDatabase::open(base.join("a.sqlite3")).unwrap());
        InstallExpert {
            agents: Arc::new(ExpertService::new(db, base.join("root"))),
        }
    }

    #[test]
    fn name_label_disclosure_risk() {
        let t = tool();
        assert_eq!(t.name(), "install_expert");
        assert_eq!(t.label(), "创建专家");
        assert_eq!(t.disclosure(), crate::tools::Disclosure::Deferred);
        assert_eq!(t.risk_level(), RiskLevel::High);
        assert!(t.requires_confirmation());
    }

    #[test]
    fn missing_name_errors() {
        let t = tool();
        let err = t.execute(&serde_json::json!({"system_prompt": "x"})).unwrap_err();
        assert!(err.contains("name"));
    }
}

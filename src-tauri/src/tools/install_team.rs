use std::sync::Arc;

use crate::team::{InlineExpert, TeamService};
use crate::tools::{RiskLevel, Tool};

/// `install_team` 工具：把对话里（在 `create-team` 技能引导下）设计好的一个**团队**
/// （主理人 + 成员）登记到平台。现场定义的主理人/成员会被造成该团队的私有专家。
/// risk = High（写持久全局状态，需用户确认）。
pub struct InstallTeam {
    pub teams: Arc<TeamService>,
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

/// 把一个专家定义对象解析成 InlineExpert。
fn parse_expert(v: &serde_json::Value) -> Result<InlineExpert, String> {
    let name = opt_str(v, "name").ok_or("成员/主理人缺少 name")?;
    let system_prompt =
        opt_str(v, "system_prompt").ok_or_else(|| format!("专家「{name}」缺少 system_prompt"))?;
    Ok(InlineExpert {
        name,
        description: opt_str(v, "description").unwrap_or_default(),
        system_prompt,
        tools: str_array(v, "tools"),
        model_tier: opt_str(v, "model").unwrap_or_else(|| "main".into()),
        display_name: opt_str(v, "display_name"),
        profession: opt_str(v, "profession"),
    })
}

fn expert_schema(desc: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "description": desc,
        "properties": {
            "name": { "type": "string", "description": "成员标识（唯一，可中文）" },
            "description": { "type": "string", "description": "一句话说明其职责" },
            "system_prompt": { "type": "string", "description": "角色设定正文（身份/准则/产出格式）" },
            "tools": { "type": "array", "items": { "type": "string" }, "description": "可用工具白名单；留空则默认开放全部工具" },
            "model": { "type": "string", "enum": ["aux", "main"], "description": "模型档位，默认 main(主力模型)" },
            "display_name": { "type": "string", "description": "可选显示名" },
            "profession": { "type": "string", "description": "可选职业/头衔" }
        },
        "required": ["name", "system_prompt"]
    })
}

impl Tool for InstallTeam {
    fn name(&self) -> &str {
        "install_team"
    }

    fn disclosure(&self) -> crate::tools::Disclosure {
        crate::tools::Disclosure::Deferred
    }

    fn label(&self) -> &str {
        "创建团队"
    }

    fn description(&self) -> &str {
        "登记一个已敲定的团队到平台。团队 = 一名主理人（统筹、决定把活分给谁）+ 若干成员（实际干活）。\
         主理人/成员在本次调用里现场定义（成为该团队的私有专家）。设计怎么做由 create-team 技能引导；\
         本工具只执行登记，需用户确认。登记后团队出现在「团队」列表、可在会话激活。"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": "团队标识（唯一，建议英文，如 content-team）" },
                "display_name": { "type": "string", "description": "团队显示名（如 内容创作团队）" },
                "description": { "type": "string", "description": "团队一句话简介" },
                "lead": expert_schema("主理人（可选）：其设定作为团队统筹说明，不直接干活、不进可派发名单"),
                "members": {
                    "type": "array",
                    "items": expert_schema("成员：实际干活、可被主助手派发"),
                    "description": "成员列表（至少一名）"
                },
                "quick_prompts": { "type": "array", "items": { "type": "string" }, "description": "开场引导语：几条示范提示词，引导用户怎么用这个团队（如「帮我做一份这周的竞品分析」）；显示在团队详情里，点一下就能带着该团队开始对话" }
            },
            "required": ["name", "members"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::High
    }

    fn execute(&self, args: &serde_json::Value) -> Result<String, String> {
        let name = opt_str(args, "name").ok_or("缺少 name")?;
        let display_name = opt_str(args, "display_name").unwrap_or_else(|| name.clone());
        let description = opt_str(args, "description").unwrap_or_default();
        let lead = match args.get("lead") {
            Some(v) if v.is_object() => Some(parse_expert(v)?),
            _ => None,
        };
        let members: Vec<InlineExpert> = args
            .get("members")
            .and_then(|x| x.as_array())
            .map(|arr| arr.iter().map(parse_expert).collect::<Result<Vec<_>, _>>())
            .transpose()?
            .unwrap_or_default();

        let quick_prompts = str_array(args, "quick_prompts");
        let summary = self.teams.create_with_members(
            &name,
            &display_name,
            &description,
            lead,
            members,
            quick_prompts,
            None,
        )?;
        Ok(format!(
            "已创建团队「{}」（{} 名成员）：下一轮起可在会话角色选择器（👥）激活。",
            summary.display_name, summary.member_count
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool() -> InstallTeam {
        let base = std::env::temp_dir().join(format!("siw-installteam-{}", std::process::id()));
        let db = Arc::new(crate::storage::AppDatabase::open(base.join("a.sqlite3")).unwrap());
        let experts = Arc::new(crate::expert::ExpertService::new(db.clone(), base.join("root")));
        InstallTeam {
            teams: Arc::new(TeamService::new(db, experts, base.join("root"))),
        }
    }

    #[test]
    fn name_label_disclosure_risk() {
        let t = tool();
        assert_eq!(t.name(), "install_team");
        assert_eq!(t.label(), "创建团队");
        assert_eq!(t.disclosure(), crate::tools::Disclosure::Deferred);
        assert_eq!(t.risk_level(), RiskLevel::High);
    }

    #[test]
    fn missing_name_errors() {
        let t = tool();
        let err = t.execute(&serde_json::json!({"members": []})).unwrap_err();
        assert!(err.contains("name"));
    }
}

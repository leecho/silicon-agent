use std::path::PathBuf;
use std::sync::Arc;

use crate::skill::SkillService;
use crate::tools::sandbox::resolve_in_workspace;
use crate::tools::{RiskLevel, Tool};

/// `install_skill` 工具：把会话工作区内已创作好的技能目录登记到平台
/// （复制进受管 skills 根 + 写 DB 索引），登记后下一轮 system prompt 即可见、可 `load_skill`。
///
/// 与 fs 工具不同：fs 工具被沙箱限制在工作区内、无法写受管 skills 根；本工具受控地执行
/// 这一"越界"特权操作，故 risk = High（写持久全局状态，需用户确认）。`skill_path` 仍要求
/// 落在工作区内（创作产物所在处）。
pub struct InstallSkill {
    pub workspace: PathBuf,
    pub skills: Arc<SkillService>,
}

impl Tool for InstallSkill {
    fn name(&self) -> &str {
        "install_skill"
    }

    fn label(&self) -> &str {
        "登记技能"
    }

    fn description(&self) -> &str {
        "把工作区内已创作好的技能目录登记到平台，使其可被发现和加载。\
         skill_path 指向工作区内含 SKILL.md 的技能目录；登记需用户确认，确认后该技能下一轮即可用。\
         迭代同名技能时传 overwrite=true（仅能覆盖用户自建技能，不能覆盖内置）。"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "skill_path": {
                    "type": "string",
                    "description": "工作区内技能目录的相对路径（含 SKILL.md），如 ./my-skill"
                },
                "overwrite": {
                    "type": "boolean",
                    "description": "同名技能是否覆盖更新（仅用户自建技能，默认 false）"
                }
            },
            "required": ["skill_path"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::High
    }

    fn execute(&self, args: &serde_json::Value) -> Result<String, String> {
        let skill_path = args
            .get("skill_path")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or("缺少 skill_path")?;
        let overwrite = args
            .get("overwrite")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // 源目录必须落在工作区内（创作产物所在处）。
        let abs = resolve_in_workspace(&self.workspace, skill_path)?;
        let abs_str = abs.to_string_lossy();

        let summary = self
            .skills
            .install_or_update_from_path(&abs_str, overwrite)?;
        Ok(format!(
            "已登记技能「{}」：下一轮起会出现在「可用技能」中，可用 load_skill(name=\"{}\") 加载其完整指引。",
            summary.name, summary.name
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool() -> InstallSkill {
        // 用一个不会被触达的 SkillService（测试只覆盖 execute 的前置校验，不落盘）。
        let base = std::env::temp_dir().join(format!("siw-installskill-{}", std::process::id()));
        let db = Arc::new(crate::storage::AppDatabase::open(base.join("a.sqlite3")).unwrap());
        InstallSkill {
            workspace: base.join("ws"),
            skills: Arc::new(SkillService::new(db, base.join("skills"))),
        }
    }

    #[test]
    fn missing_skill_path_errors() {
        let t = tool();
        let err = t.execute(&serde_json::json!({})).unwrap_err();
        assert!(err.contains("skill_path"));
    }

    #[test]
    fn path_escaping_workspace_is_rejected() {
        let t = tool();
        let err = t
            .execute(&serde_json::json!({"skill_path": "../../etc"}))
            .unwrap_err();
        assert!(err.contains("越出工作区"), "应拒绝越界 path：{err}");
    }

    #[test]
    fn risk_is_high_and_requires_confirmation() {
        let t = tool();
        assert_eq!(t.risk_level(), RiskLevel::High);
        assert!(t.requires_confirmation());
        assert_eq!(t.label(), "登记技能");
    }
}

use std::path::PathBuf;
use std::sync::Arc;

use crate::plugin::PluginService;
use crate::tools::sandbox::resolve_in_workspace;
use crate::tools::{RiskLevel, Tool};

/// `install_plugin` 工具：把会话工作区内已创作好的套件目录登记到平台
/// （复制进受管 plugins 根 + 写 plugin/skill 索引），登记后其下可见 skill 即进「可用技能」。
///
/// 与 `install_skill` 同理：受控地执行写受管目录这一特权操作，risk = High（持久全局状态，需用户确认）。
/// `plugin_path` 须落在工作区内（创作产物所在处），且含根目录 `plugin.json`（或 `.claude-plugin/plugin.json`）。
pub struct InstallPlugin {
    pub workspace: PathBuf,
    pub plugins: Arc<PluginService>,
}

impl Tool for InstallPlugin {
    fn name(&self) -> &str {
        "install_plugin"
    }

    fn disclosure(&self) -> crate::tools::Disclosure {
        crate::tools::Disclosure::Deferred
    }

    fn label(&self) -> &str {
        "安装套件"
    }

    fn description(&self) -> &str {
        "把工作区内已创作好的套件目录（角色工具箱，含根目录 plugin.json 与 skills/）登记到平台，\
         使其与其下技能可被发现和加载。plugin_path 指向工作区内的套件目录；登记需用户确认。\
         迭代同名套件时传 overwrite=true（仅能覆盖用户自建套件，不能覆盖内置）。"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "plugin_path": {
                    "type": "string",
                    "description": "工作区内套件目录的相对路径（含根目录 plugin.json），如 ./legal-assistant"
                },
                "overwrite": {
                    "type": "boolean",
                    "description": "同名套件是否覆盖更新（仅用户自建套件，默认 false）"
                }
            },
            "required": ["plugin_path"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::High
    }

    fn execute(&self, args: &serde_json::Value) -> Result<String, String> {
        let plugin_path = args
            .get("plugin_path")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or("缺少 plugin_path")?;
        let overwrite = args
            .get("overwrite")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let abs = resolve_in_workspace(&self.workspace, plugin_path)?;
        let abs_str = abs.to_string_lossy();

        let summary = self
            .plugins
            .install_or_update_from_path(&abs_str, overwrite)?;
        Ok(format!(
            "已登记套件「{}」（{} 个技能）：下一轮起其可见技能会出现在「可用技能」中，可用 load_skill 加载。",
            summary.display_name, summary.skill_count
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool() -> InstallPlugin {
        let base = std::env::temp_dir().join(format!("siw-installplugin-{}", std::process::id()));
        let db = Arc::new(crate::storage::AppDatabase::open(base.join("a.sqlite3")).unwrap());
        InstallPlugin {
            workspace: base.join("ws"),
            plugins: Arc::new(PluginService::new(
                db,
                base.join("plugins"),
                base.join("builtin-plugins"),
            )),
        }
    }

    #[test]
    fn missing_plugin_path_errors() {
        let t = tool();
        let err = t.execute(&serde_json::json!({})).unwrap_err();
        assert!(err.contains("plugin_path"));
    }

    #[test]
    fn path_escaping_workspace_is_rejected() {
        let t = tool();
        let err = t
            .execute(&serde_json::json!({"plugin_path": "../../etc"}))
            .unwrap_err();
        assert!(err.contains("越出工作区"), "应拒绝越界 path：{err}");
    }

    #[test]
    fn risk_is_high_and_requires_confirmation() {
        let t = tool();
        assert_eq!(t.risk_level(), RiskLevel::High);
        assert!(t.requires_confirmation());
        assert_eq!(t.label(), "安装套件");
    }
}

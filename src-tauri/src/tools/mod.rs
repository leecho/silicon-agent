pub mod add_artifact;
pub mod ask_user;
pub mod command_tool;
pub mod fs_tools;
pub mod install_skill;
pub mod load_skill;
pub mod propose_plan;
pub mod read_skill_file;
pub mod registry;
pub mod sandbox;
pub mod search_tools;
pub mod update_todos;
pub mod web_fetch;
pub mod web_search;

pub use registry::ToolRegistry;

/// 工具给模型的规格（name + description + JSON schema）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// 工具副作用风险级别，驱动权限闸门。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum RiskLevel {
    /// 只读、无副作用——任何模式下都直接放行。
    Safe,
    /// 写副作用、工作区内可控——auto 模式自动放行，manual 模式首次确认。
    Low,
    /// 任意命令执行——仅 full 模式放行，auto/manual 均需确认。
    High,
}

/// 工具统一接口。`execute` 返回纯文本结果（成功或失败说明）。
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    /// 面向用户的中文动作标签（如「执行命令」「搜索网页」）。用于远程 IM 进度行等
    /// 不该暴露内部工具名的场景。默认回退到 `name()`；新增工具就近覆盖此方法维护标签。
    fn label(&self) -> &str {
        self.name()
    }
    fn description(&self) -> &str;
    fn parameters(&self) -> serde_json::Value;
    /// 是否并发安全（只读、无副作用）——可与同类并行执行。默认 false。
    fn concurrency_safe(&self) -> bool {
        false
    }
    /// 工具副作用风险级别。默认 `Safe`（只读）。
    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Safe
    }
    /// 执行前是否需要用户确认——派生自风险级别（非 Safe 即有副作用）。
    /// 计划模式闸门仍用它表达"有副作用"。一般不要覆盖，覆盖 `risk_level` 即可。
    fn requires_confirmation(&self) -> bool {
        self.risk_level() != RiskLevel::Safe
    }
    fn execute(&self, args: &serde_json::Value) -> Result<String, String>;
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: self.name().into(),
            description: self.description().into(),
            parameters: self.parameters(),
        }
    }
}

#[cfg(test)]
mod risk_tests {
    use super::*;

    struct SafeTool;
    impl Tool for SafeTool {
        fn name(&self) -> &str {
            "safe"
        }
        fn description(&self) -> &str {
            ""
        }
        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({})
        }
        fn execute(&self, _: &serde_json::Value) -> Result<String, String> {
            Ok(String::new())
        }
    }

    struct LowTool;
    impl Tool for LowTool {
        fn name(&self) -> &str {
            "low"
        }
        fn description(&self) -> &str {
            ""
        }
        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({})
        }
        fn risk_level(&self) -> RiskLevel {
            RiskLevel::Low
        }
        fn execute(&self, _: &serde_json::Value) -> Result<String, String> {
            Ok(String::new())
        }
    }

    #[test]
    fn default_is_safe_and_no_confirm() {
        assert_eq!(SafeTool.risk_level(), RiskLevel::Safe);
        assert!(!SafeTool.requires_confirmation());
    }

    #[test]
    fn low_requires_confirmation_derived() {
        assert_eq!(LowTool.risk_level(), RiskLevel::Low);
        assert!(LowTool.requires_confirmation());
    }
}

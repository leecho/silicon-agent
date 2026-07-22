pub mod add_artifact;
pub mod ask_user;
pub mod browser;
pub mod computer;
pub mod collect_agents;
pub mod command_tool;
pub mod install_expert;
pub mod install_team;
pub mod dispatch_agent;
pub mod find_tools;
pub mod fs_search;
pub mod fs_tools;
pub mod install_plugin;
pub mod install_skill;
pub mod load_skill;
pub mod propose_plan;
pub mod propose_soul_update;
pub mod read_skill_file;
pub mod registry;
pub mod remember;
pub mod sandbox;
pub mod search_knowledge;
pub mod update_tasks;
pub mod update_todos;
pub mod web_fetch;
pub mod web_search;
// macOS 应用工具（T90）：依赖 apple/ 的 EventKit/osascript 后端，仅 macOS 编译。
#[cfg(target_os = "macos")]
pub mod calendar;
#[cfg(target_os = "macos")]
pub mod reminders;
#[cfg(target_os = "macos")]
pub mod notes;

pub use registry::ToolRegistry;

use crate::tools::propose_plan::PROPOSE_PLAN_TOOL;

/// 工具给模型的规格（name + description + JSON schema）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct ToolSpec {
    pub name: String,
    /// 面向用户的中文动作标签（单一真相源 = `Tool::label()`）。
    pub label: String,
    pub description: String,
    pub parameters: serde_json::Value,
    pub disclosure: Disclosure,
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

/// 工具对模型的披露级别——驱动渐进式披露（T83）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum Disclosure {
    /// 常驻：每轮都进 tools[]。
    Core,
    /// 推迟：仅进 system prompt 目录，经 find_tools 激活后才进 tools[]。
    Deferred,
}

/// 是否把某工具放进本轮 tools[]——渐进式披露 + plan 只读 + 普通模式排除 propose_plan 的合并判定（T83）。
/// 纯函数：便于单测，run_loop_inner 每轮按 spec 调用。
pub fn include_in_tools(
    disclosure: Disclosure,
    activated: bool,
    requires_confirmation: bool,
    mode: &str,
    name: &str,
) -> bool {
    // 披露闸：Core 恒在；Deferred 仅激活后在。
    if disclosure == Disclosure::Deferred && !activated {
        return false;
    }
    if mode == "plan" {
        // 计划模式仅只读工具。
        return !requires_confirmation;
    }
    // 普通模式：排除 propose_plan（仅计划模式可用）。
    name != PROPOSE_PLAN_TOOL
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
    /// 披露级别。默认 Core（安全：新工具不被意外隐藏）；长尾工具覆写为 Deferred。
    fn disclosure(&self) -> Disclosure {
        Disclosure::Core
    }
    /// 按本次调用参数动态判定风险级别（喂给 mode-aware 的 needs_confirmation）。
    /// 默认回落静态 `risk_level()`（既有工具零改动）。日历/提醒/备忘录覆写：
    /// list/get=Safe、create/update=Low、delete=High，从而 delete 跟随权限模式确认（T90）。
    fn risk_for(&self, _args: &serde_json::Value) -> RiskLevel {
        self.risk_level()
    }
    /// 单次执行超时秒数。None = 用全局默认；Some(0) = 不设超时（逃生舱，长任务工具可覆写）。
    /// 默认 None：既有工具零改动。慢工具（浏览器/命令类）按需覆写为更大值。
    fn timeout_secs(&self) -> Option<u64> {
        None
    }
    fn execute(&self, args: &serde_json::Value) -> Result<String, String>;
    /// 可取消执行：run 级取消标记穿入，进程类工具据此在轮询中 kill 子进程，实现「立即停止」
    /// 而非脱管后台跑到自然结束。默认忽略 cancel、回退 execute——既有工具零改动。
    fn execute_cancellable(
        &self,
        args: &serde_json::Value,
        _cancel: &std::sync::atomic::AtomicBool,
    ) -> Result<String, String> {
        self.execute(args)
    }
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: self.name().into(),
            label: self.label().into(),
            description: self.description().into(),
            parameters: self.parameters(),
            disclosure: self.disclosure(),
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

    #[test]
    fn disclosure_defaults_to_core() {
        assert_eq!(SafeTool.disclosure(), Disclosure::Core);
    }

    #[test]
    fn spec_carries_disclosure() {
        assert_eq!(SafeTool.spec().disclosure, Disclosure::Core);
    }

    #[test]
    fn execute_cancellable_defaults_to_execute_even_when_cancelled() {
        // 默认方法忽略 cancel、委派 execute——即使 cancel 已置位，结果与 execute 一致。
        let cancel = std::sync::atomic::AtomicBool::new(true);
        let via_cancellable = SafeTool.execute_cancellable(&serde_json::json!({}), &cancel);
        let via_execute = SafeTool.execute(&serde_json::json!({}));
        assert_eq!(via_cancellable, via_execute);
    }
}

#[cfg(test)]
mod disclosure_select_tests {
    use super::*;

    #[test]
    fn core_always_included_normal_mode() {
        assert!(include_in_tools(Disclosure::Core, false, false, "normal", "read_file"));
    }

    #[test]
    fn deferred_excluded_until_activated() {
        assert!(!include_in_tools(Disclosure::Deferred, false, true, "normal", "web_fetch"));
        assert!(include_in_tools(Disclosure::Deferred, true, true, "normal", "web_fetch"));
    }

    #[test]
    fn propose_plan_excluded_in_normal_mode() {
        assert!(!include_in_tools(Disclosure::Core, false, false, "normal", "propose_plan"));
    }

    #[test]
    fn plan_mode_drops_write_tools_even_if_core() {
        assert!(!include_in_tools(Disclosure::Core, false, true, "plan", "write_file"));
        assert!(include_in_tools(Disclosure::Core, false, false, "plan", "read_file"));
    }

    #[test]
    fn plan_mode_unactivated_deferred_still_excluded() {
        assert!(!include_in_tools(Disclosure::Deferred, false, false, "plan", "web_search"));
    }

    #[test]
    fn long_tail_builtins_are_deferred() {
        use crate::tools::add_artifact::AddArtifact;
        use crate::tools::remember::Remember;
        use crate::tools::web_fetch::WebFetch;
        use crate::tools::web_search::WebSearch;
        use crate::tools::propose_soul_update::ProposeSoulUpdate;
        use crate::tools::search_knowledge::SearchKnowledge;
        assert_eq!(AddArtifact.disclosure(), Disclosure::Deferred);
        assert_eq!(Remember.disclosure(), Disclosure::Deferred);
        assert_eq!(WebFetch.disclosure(), Disclosure::Deferred);
        assert_eq!(WebSearch::new().disclosure(), Disclosure::Deferred);
        assert_eq!(ProposeSoulUpdate.disclosure(), Disclosure::Deferred);
        assert_eq!(SearchKnowledge.disclosure(), Disclosure::Deferred);
    }
}

/// 工具叙事标签守卫（T88）：每个内置工具都必须覆写 `label()`（即 `label() != name()`），
/// 否则前端叙事会泄漏原始英文工具名（如曾经的 `调用 computer`）。
///
/// 为何在此而非 registry 级：`EngineBuilder::build_registry` 需要 `tauri::AppHandle`，
/// 集成测试无法构造（见 `tests/session_running_flag.rs` 注释），故无法在测试里调
/// `build_registry().specs()`。这里枚举 `build_registry` 注册的全部内置工具实例并逐一断言，
/// 与 registry 集合一一对应——新增内置工具时，请在此处同步登记一行，CI 才能守住。
#[cfg(test)]
mod label_guard_tests {
    use super::*;
    use std::sync::Arc;

    /// 临时 DB，供需要 service 依赖的工具构造（label() 不依赖 service 字段，仅为可构造）。
    fn temp_db() -> Arc<crate::storage::AppDatabase> {
        static C: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let seq = C.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "siw-label-guard_{}_{}",
            std::process::id(),
            seq
        ));
        Arc::new(
            crate::storage::AppDatabase::open(dir.join("app.sqlite3")).expect("temp db"),
        )
    }

    /// 返回 `build_registry` 注册的全部内置工具（含需 service 依赖者；MCP 动态工具排除——
    /// 它们由前端 `mcp__` 分支单独叙事，不走 `Tool::label()`）。新增内置工具时在此登记。
    fn builtin_tools() -> Vec<Arc<dyn Tool>> {
        use crate::tools::add_artifact::AddArtifact;
        use crate::tools::ask_user::AskUser;
        use crate::tools::collect_agents::CollectAgents;
        use crate::tools::command_tool::CommandExecute;
        use crate::tools::dispatch_agent::DispatchAgent;
        use crate::tools::find_tools::FindTools;
        use crate::tools::fs_search::{Glob, Grep};
        use crate::tools::fs_tools::{EditFile, ReadFile, WriteFile};
        use crate::tools::install_expert::InstallExpert;
        use crate::tools::install_plugin::InstallPlugin;
        use crate::tools::install_skill::InstallSkill;
        use crate::tools::install_team::InstallTeam;
        use crate::tools::load_skill::LoadSkill;
        use crate::tools::propose_plan::ProposePlan;
        use crate::tools::propose_soul_update::ProposeSoulUpdate;
        use crate::tools::read_skill_file::ReadSkillFile;
        use crate::tools::remember::Remember;
        use crate::tools::search_knowledge::SearchKnowledge;
        use crate::tools::update_tasks::UpdateTasks;
        use crate::tools::update_todos::UpdateTodos;
        use crate::tools::web_fetch::WebFetch;
        use crate::tools::web_search::WebSearch;

        let db = temp_db();
        let ws = std::env::temp_dir();
        let root = ws.join("siw-label-guard-root");
        let skills = Arc::new(crate::skill::SkillService::new(db.clone(), root.clone()));
        let experts =
            Arc::new(crate::expert::ExpertService::new(db.clone(), root.clone()));
        let teams = Arc::new(crate::team::TeamService::new(
            db.clone(),
            experts.clone(),
            root.clone(),
        ));
        let plugins = Arc::new(crate::plugin::PluginService::new(
            db.clone(),
            root.clone(),
            root.clone(),
        ));

        let mut tools: Vec<Arc<dyn Tool>> = vec![
            Arc::new(ReadFile { workspace: ws.clone() }),
            Arc::new(WriteFile { workspace: ws.clone() }),
            Arc::new(EditFile { workspace: ws.clone() }),
            Arc::new(Glob { workspace: ws.clone() }),
            Arc::new(Grep { workspace: ws.clone() }),
            Arc::new(CommandExecute { workspace: ws.clone() }),
            Arc::new(WebSearch::new()),
            Arc::new(WebFetch),
            Arc::new(SearchKnowledge),
            Arc::new(AskUser),
            Arc::new(LoadSkill),
            Arc::new(FindTools),
            Arc::new(ReadSkillFile),
            Arc::new(UpdateTodos),
            Arc::new(UpdateTasks),
            Arc::new(AddArtifact),
            Arc::new(Remember),
            Arc::new(ProposePlan),
            Arc::new(ProposeSoulUpdate),
            Arc::new(DispatchAgent),
            Arc::new(CollectAgents),
            Arc::new(InstallSkill {
                workspace: ws.clone(),
                skills,
            }),
            Arc::new(InstallPlugin {
                workspace: ws.clone(),
                plugins,
            }),
            Arc::new(InstallExpert { agents: experts }),
            Arc::new(InstallTeam { teams }),
        ];

        // 桌面操作工具仅 macOS / Windows 注册（与 build_registry 一致）。
        #[cfg(target_os = "macos")]
        {
            let backend: Arc<dyn crate::desktop::DesktopController> =
                Arc::new(crate::desktop::macos::MacosController);
            tools.push(Arc::new(crate::tools::computer::Computer::new(backend)));
            // T90 应用工具：用 mock 后端构造（label() 不依赖后端）。
            tools.push(Arc::new(crate::tools::calendar::Calendar::new(Arc::new(
                crate::apple::calendar::MockCalendar::new(),
            ))));
            tools.push(Arc::new(crate::tools::reminders::Reminders::new(Arc::new(
                crate::apple::reminders::MockReminders::new(),
            ))));
            tools.push(Arc::new(crate::tools::notes::Notes::new(Arc::new(
                crate::apple::notes::MockNotes::new(),
            ))));
        }
        #[cfg(target_os = "windows")]
        {
            let backend: Arc<dyn crate::desktop::DesktopController> =
                Arc::new(crate::desktop::windows::WindowsController);
            tools.push(Arc::new(crate::tools::computer::Computer::new(backend)));
        }

        // 浏览器工具：CDP 后端在 Task 6 才有，测试中用 MockController 构造。
        {
            use crate::browser::mock::MockController;
            use crate::browser::DomSnapshot;
            let backend: Arc<dyn crate::browser::BrowserController> =
                Arc::new(MockController::with_snapshot(DomSnapshot {
                    url: String::new(), title: String::new(), elements: vec![],
                    truncated: false, coverage_hint: 1.0,
                }));
            tools.push(Arc::new(crate::tools::browser::Browser::new(
                backend,
                std::env::temp_dir(),
                "test".to_string(),
            )));
        }

        tools
    }

    #[test]
    fn every_builtin_tool_has_real_label() {
        for tool in builtin_tools() {
            let spec = tool.spec();
            assert_ne!(
                spec.label, spec.name,
                "内置工具 `{}` 未覆写 label()（label==name），前端叙事会泄漏原始英文名；\
                 请为该工具实现 fn label() 返回中文动作标签",
                spec.name
            );
            assert!(
                !spec.label.is_empty(),
                "内置工具 `{}` 的 label 为空",
                spec.name
            );
        }
    }

    /// 不变量：并行路径（engine::execute_parallel_group）对 `concurrency_safe()==true`
    /// 的工具并发执行，并**故意跳过 plan-mode 闸与权限闸**，前提是「concurrency_safe ⇒ risk Safe」。
    /// 若将来给某个非 Safe（需确认）的工具误标 concurrency_safe，并行路径会静默绕过权限闸。
    /// 此测试把该前提钉死：通过 `builtin_tools()` 枚举生产工具（新增内置工具自动覆盖），
    /// 任一 concurrency_safe 工具若不是 risk Safe，则 CI 失败而非上线后静默绕权限。
    #[test]
    fn concurrency_safe_tools_are_all_risk_safe() {
        let null = serde_json::Value::Null;
        let mut checked = 0usize;
        for tool in builtin_tools() {
            if tool.concurrency_safe() {
                checked += 1;
                assert_eq!(
                    tool.risk_for(&null),
                    RiskLevel::Safe,
                    "concurrency_safe 工具「{}」必须是 risk Safe（否则并行路径会绕过权限闸）",
                    tool.name()
                );
                assert!(
                    !tool.requires_confirmation(),
                    "concurrency_safe 工具「{}」不得需要确认（并行路径不经权限闸）",
                    tool.name()
                );
            }
        }
        // 防止 builtin_tools() 退化为空集导致断言空跑——当前至少 ReadFile/Glob/Grep/
        // WebFetch/WebSearch/SearchKnowledge 六个 concurrency_safe 工具。
        assert!(
            checked >= 6,
            "预期至少 6 个 concurrency_safe 工具被检查，实际 {checked}；\
             builtin_tools() 是否遗漏了 concurrency_safe 工具？"
        );
    }
}

#[cfg(test)]
mod risk_for_tests {
    use super::*;

    struct DynRiskTool;
    impl Tool for DynRiskTool {
        fn name(&self) -> &str {
            "dynrisk"
        }
        fn description(&self) -> &str {
            ""
        }
        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({})
        }
        fn risk_for(&self, args: &serde_json::Value) -> RiskLevel {
            match args.get("action").and_then(|v| v.as_str()) {
                Some("delete") => RiskLevel::High,
                Some("create") | Some("update") => RiskLevel::Low,
                _ => RiskLevel::Safe,
            }
        }
        fn execute(&self, _args: &serde_json::Value) -> Result<String, String> {
            Ok(String::new())
        }
    }

    #[test]
    fn risk_for_varies_by_action() {
        let t = DynRiskTool;
        assert_eq!(t.risk_for(&serde_json::json!({"action":"list"})), RiskLevel::Safe);
        assert_eq!(t.risk_for(&serde_json::json!({"action":"create"})), RiskLevel::Low);
        assert_eq!(t.risk_for(&serde_json::json!({"action":"update"})), RiskLevel::Low);
        assert_eq!(t.risk_for(&serde_json::json!({"action":"delete"})), RiskLevel::High);
    }

    struct PlainHigh;
    impl Tool for PlainHigh {
        fn name(&self) -> &str {
            "plain"
        }
        fn description(&self) -> &str {
            ""
        }
        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({})
        }
        fn risk_level(&self) -> RiskLevel {
            RiskLevel::High
        }
        fn execute(&self, _args: &serde_json::Value) -> Result<String, String> {
            Ok(String::new())
        }
    }

    #[test]
    fn risk_for_defaults_to_risk_level() {
        // 不覆写 risk_for 的工具 → risk_for == risk_level()（既有工具零行为变化）。
        assert_eq!(PlainHigh.risk_for(&serde_json::Value::Null), RiskLevel::High);
    }
}

#[cfg(test)]
mod timeout_tests {
    use super::*;

    #[test]
    fn timeout_secs_defaults_to_none_and_can_override() {
        struct DefaultTool;
        impl Tool for DefaultTool {
            fn name(&self) -> &str { "t_default" }
            fn description(&self) -> &str { "" }
            fn parameters(&self) -> serde_json::Value { serde_json::json!({}) }
            fn execute(&self, _: &serde_json::Value) -> Result<String, String> { Ok(String::new()) }
        }
        struct SlowTool;
        impl Tool for SlowTool {
            fn name(&self) -> &str { "t_slow" }
            fn description(&self) -> &str { "" }
            fn parameters(&self) -> serde_json::Value { serde_json::json!({}) }
            fn execute(&self, _: &serde_json::Value) -> Result<String, String> { Ok(String::new()) }
            fn timeout_secs(&self) -> Option<u64> { Some(120) }
        }
        assert_eq!(DefaultTool.timeout_secs(), None);
        assert_eq!(SlowTool.timeout_secs(), Some(120));
    }
}

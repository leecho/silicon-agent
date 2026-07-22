mod artifact_preview;
pub mod scheduler;
pub use scheduler::*;

// 按命令域拆分的薄入口子模块；`pub use *` 保持 `commands::<command>` 路径稳定（lib.rs 注册）。
mod agent;
mod app_control;
mod artifact;
mod browser;
mod call_log;
mod market;
mod computer;
mod permissions;
mod group;
mod knowledge;
mod mcp;
mod memory;
mod organization;
mod plugin;
mod project;
mod provider;
mod run;
mod runtime;
mod session;
mod skill;
mod team;
mod tools;
mod tray;
mod usage;
pub use agent::*;
pub use app_control::*;
pub use artifact::*;
pub use browser::*;
pub use call_log::*;
pub use market::*;
pub use computer::*;
pub use permissions::*;
pub use group::*;
pub use knowledge::*;
pub use mcp::*;
pub use memory::*;
pub use organization::*;
pub use plugin::*;
pub use project::*;
pub use provider::*;
pub use run::*;
pub use runtime::*;
pub use session::*;
pub use skill::*;
pub use team::*;
pub use tools::*;
pub use tray::*;
pub use usage::*;

use tauri::State;

use crate::app_state::AppState;
use crate::session::AskQuestion;

/// 把每题答案格式化为回灌模型的工具结果文本。每题一行：
/// `{序号}. {标签}：{答案以、连接}`，标签优先 header、否则用问题文本；空答案=（未回答）。
pub fn format_ask_answers(questions: &[AskQuestion], answers: &[Vec<String>]) -> String {
    let mut out = String::from("用户已回答：");
    for (i, q) in questions.iter().enumerate() {
        let label = if q.header.trim().is_empty() {
            q.question.as_str()
        } else {
            q.header.as_str()
        };
        let vals = answers.get(i).cloned().unwrap_or_default();
        let answer = if vals.is_empty() {
            "（未回答）".to_string()
        } else {
            vals.join("、")
        };
        out.push_str(&format!("\n{}. {}：{}", i + 1, label, answer));
    }
    out
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppHealth {
    pub ok: bool,
    pub db_ready: bool,
    pub version: String,
}

/// 应用运行平台。前端布局需要按系统窗口控件位置调整标题栏安全区。
#[tauri::command]
pub fn app_platform() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "macos"
    }
    #[cfg(target_os = "windows")]
    {
        "windows"
    }
    #[cfg(target_os = "linux")]
    {
        "linux"
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        "unknown"
    }
}

/// 健康检查：确认 DB 可用、返回版本。前端启动时调用，验证命令链路。
#[tauri::command]
pub fn app_health(services: State<'_, AppState>) -> Result<AppHealth, String> {
    let db_ready = services
        .db
        .table_exists("schema_migrations")
        .map_err(|err| err.to_string())?;
    Ok(AppHealth {
        ok: true,
        db_ready,
        version: env!("CARGO_PKG_VERSION").into(),
    })
}

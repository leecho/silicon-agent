//! 市场命令（薄入口）：四个市场各一组「浏览 / 详情 / 安装」。
//!
//! **全部 async + spawn_blocking**：市场拉取是阻塞 HTTP，放同步命令会冻住 UI 主线程
//! （对齐 provider 的 test/fetch 命令）。
//!
//! **安装不再「先探类型再分发」**：从哪个货架点的安装，类型本来就是已知的 ——
//! 技能市场装出来的必是技能。旧入口 `install_extension_from_path` 那套按清单文件名猜类型
//! 的逻辑仍然保留，但只服务于**本地目录/zip 安装**（那里才真的不知道用户拖进来的是什么）。

use crate::app_state::AppState;
use tauri::State;

/// 建一个临时目录用于物化市场包。
fn staging_dir(tag: &str) -> Result<std::path::PathBuf, String> {
    let dir = std::env::temp_dir().join(format!(
        "siw-market-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default()
    ));
    std::fs::create_dir_all(&dir).map_err(|e| format!("建临时目录失败：{e}"))?;
    Ok(dir)
}

// ============================= 技能市场（SkillHub）=============================

/// 浏览技能市场。**服务端分页 + 搜索 + 分类筛选**——SkillHub 有 7 万+ 技能，拉不全。
#[tauri::command]
pub async fn browse_skill_market(
    services: State<'_, AppState>,
    page: u32,
    page_size: u32,
    keyword: Option<String>,
    category: Option<String>,
) -> Result<crate::market::skill_market::SkillMarketPage, String> {
    let markets = services.markets.clone();
    tauri::async_runtime::spawn_blocking(move || {
        markets
            .skill
            .browse(page, page_size, keyword.as_deref(), category.as_deref())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// SkillHub 的技能分类。**只有技能货架有分类** —— 它是 SkillHub 自己的分类体系，
/// 专家/团队/插件那三个货架没有这回事。
#[tauri::command]
pub async fn list_skill_categories(
    services: State<'_, AppState>,
) -> Result<Vec<crate::market::skill_market::SkillCategory>, String> {
    let markets = services.markets.clone();
    tauri::async_runtime::spawn_blocking(move || markets.skill.categories())
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn skill_market_detail(
    services: State<'_, AppState>,
    slug: String,
) -> Result<crate::market::skill_market::SkillMarketDetail, String> {
    let markets = services.markets.clone();
    tauri::async_runtime::spawn_blocking(move || markets.skill.detail(&slug))
        .await
        .map_err(|e| e.to_string())?
}

/// 技能正文（`SKILL.md` 原文，markdown）。**安装前预览**用 ——
/// 技能会改变模型行为，装进来之前该看得见它到底写了什么。
#[tauri::command]
pub async fn skill_market_preview(
    services: State<'_, AppState>,
    slug: String,
) -> Result<String, String> {
    let markets = services.markets.clone();
    tauri::async_runtime::spawn_blocking(move || markets.skill.preview(&slug))
        .await
        .map_err(|e| e.to_string())?
}

/// 从技能市场安装一个技能：下载 zip → 解压到临时目录 → 交给 `SkillService`。
#[tauri::command]
pub async fn install_skill_from_market(
    services: State<'_, AppState>,
    slug: String,
) -> Result<crate::skill::types::SkillSummary, String> {
    let markets = services.markets.clone();
    let staged = tauri::async_runtime::spawn_blocking(move || {
        let dir = staging_dir("skill")?;
        match markets.skill.materialize(&slug, &dir) {
            Ok(()) => Ok::<std::path::PathBuf, String>(dir),
            Err(e) => {
                let _ = std::fs::remove_dir_all(&dir);
                Err(e)
            }
        }
    })
    .await
    .map_err(|e| e.to_string())??;

    let result = services.skills.install_from_path(&staged.to_string_lossy());
    let _ = std::fs::remove_dir_all(&staged);
    result
}

// ============================= 专家市场（官方仓）=============================

#[tauri::command]
pub async fn browse_expert_market(
    services: State<'_, AppState>,
    page: u32,
    page_size: u32,
    keyword: Option<String>,
) -> Result<crate::market::expert_market::ExpertMarketPage, String> {
    let markets = services.markets.clone();
    tauri::async_runtime::spawn_blocking(move || {
        markets.expert.browse(page, page_size, keyword.as_deref())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn expert_market_detail(
    services: State<'_, AppState>,
    name: String,
) -> Result<crate::market::expert_market::ExpertMarketDetail, String> {
    let markets = services.markets.clone();
    tauri::async_runtime::spawn_blocking(move || markets.expert.detail(&name))
        .await
        .map_err(|e| e.to_string())?
}

/// 装专家包：落成散装专家 + 其 `expert_id` **私有**技能（只在选中该专家时载入）。
#[tauri::command]
pub async fn install_expert_from_market(
    services: State<'_, AppState>,
    name: String,
) -> Result<crate::expert::types::ExpertSummary, String> {
    let markets = services.markets.clone();
    let staged = tauri::async_runtime::spawn_blocking(move || {
        let dir = staging_dir("expert")?;
        match markets.expert.materialize(&name, &dir) {
            Ok(()) => Ok::<std::path::PathBuf, String>(dir),
            Err(e) => {
                let _ = std::fs::remove_dir_all(&dir);
                Err(e)
            }
        }
    })
    .await
    .map_err(|e| e.to_string())??;

    let result = crate::expert::expert::import_expert(
        &services.experts,
        &services.skills,
        &services.workspace_base,
        &staged.to_string_lossy(),
    );
    let _ = std::fs::remove_dir_all(&staged);
    result
}

// ============================= 团队市场（官方仓）=============================

#[tauri::command]
pub async fn browse_team_market(
    services: State<'_, AppState>,
    page: u32,
    page_size: u32,
    keyword: Option<String>,
) -> Result<crate::market::team_market::TeamMarketPage, String> {
    let markets = services.markets.clone();
    tauri::async_runtime::spawn_blocking(move || {
        markets.team.browse(page, page_size, keyword.as_deref())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn team_market_detail(
    services: State<'_, AppState>,
    name: String,
) -> Result<crate::market::team_market::TeamMarketDetail, String> {
    let markets = services.markets.clone();
    tauri::async_runtime::spawn_blocking(move || markets.team.detail(&name))
        .await
        .map_err(|e| e.to_string())?
}

/// 装团队包：落成团队 + 其 `team_id` **私有**成员与技能（只在激活该团队时载入）。
#[tauri::command]
pub async fn install_team_from_market(
    services: State<'_, AppState>,
    name: String,
) -> Result<crate::team::types::TeamSummary, String> {
    let markets = services.markets.clone();
    let staged = tauri::async_runtime::spawn_blocking(move || {
        let dir = staging_dir("team")?;
        match markets.team.materialize(&name, &dir) {
            Ok(()) => Ok::<std::path::PathBuf, String>(dir),
            Err(e) => {
                let _ = std::fs::remove_dir_all(&dir);
                Err(e)
            }
        }
    })
    .await
    .map_err(|e| e.to_string())??;

    let result = services.teams.import_from_path(
        &staged.to_string_lossy(),
        crate::team::model::TeamSource::Imported,
    );
    let _ = std::fs::remove_dir_all(&staged);
    result
}

// ============================= 插件市场（标准生态）=============================

#[tauri::command]
pub async fn browse_plugin_market(
    services: State<'_, AppState>,
    page: u32,
    page_size: u32,
    keyword: Option<String>,
) -> Result<crate::market::plugin_market::PluginMarketPage, String> {
    let markets = services.markets.clone();
    tauri::async_runtime::spawn_blocking(move || {
        markets.plugin.browse(page, page_size, keyword.as_deref())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn plugin_market_detail(
    services: State<'_, AppState>,
    name: String,
) -> Result<crate::market::plugin_market::PluginMarketDetail, String> {
    let markets = services.markets.clone();
    tauri::async_runtime::spawn_blocking(move || markets.plugin.detail(&name))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn install_plugin_from_market(
    services: State<'_, AppState>,
    name: String,
) -> Result<crate::plugin::types::PluginSummary, String> {
    let markets = services.markets.clone();
    let staged = tauri::async_runtime::spawn_blocking(move || {
        let dir = staging_dir("plugin")?;
        match markets.plugin.materialize(&name, &dir) {
            Ok(()) => Ok::<std::path::PathBuf, String>(dir),
            Err(e) => {
                let _ = std::fs::remove_dir_all(&dir);
                Err(e)
            }
        }
    })
    .await
    .map_err(|e| e.to_string())??;

    let result = services
        .plugins
        .install_from_path(&staged.to_string_lossy());
    let _ = std::fs::remove_dir_all(&staged);

    let summary = result?;
    // MCP 联动失败不回滚安装，仅记录（插件本体已就绪）。
    if let Err(e) = services.facade.refresh_plugin_mcp(&summary.id, true) {
        eprintln!("[plugin->mcp] 安装后摄取失败 plugin={}：{e}", summary.id);
    }
    Ok(summary)
}

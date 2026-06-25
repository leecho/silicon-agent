pub mod app_facade;
pub mod app_settings;
pub mod app_state;
pub mod aux_gen;
pub mod call_log;
pub mod commands;
pub mod context;
pub mod engine;
pub mod provider;
pub mod remote;
pub mod run;
pub mod session;
pub mod skill;
pub mod storage;
pub mod tools;
pub mod tray;
pub mod usage;

use tauri::Manager;

use crate::app_state::AppState;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let state = AppState::open(app.handle())?;
            app.manage(state);
            // 启动对账：统一收敛器把一切被中断会话收敛到可交互 idle（取代两个旧 reconcile，须在 manage 之后）。
            app.state::<AppState>().coordinator.reconcile_all();
            // 进程内看门狗：周期回收挂死租约 + 停泊孤儿 → reconcile（覆盖运行期内的挂死/孤儿，非只重启）。
            crate::run::watchdog::start(app.handle().clone());
            crate::tray::install_tray(app)?;
            // 远程接入：按 enabled 启动各 channel 的 connector 线程（缺配置/密钥的跳过，不阻断启动）。
            crate::remote::start_enabled_channels(app.handle());
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() != crate::tray::MAIN_WINDOW_LABEL {
                return;
            }
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // 关闭主窗口只隐藏到后台，后台任务和调度器继续运行。
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_platform,
            commands::app_health,
            commands::refresh_tray_menu,
            commands::list_providers,
            commands::upsert_provider,
            commands::delete_provider,
            commands::set_provider_enabled,
            commands::test_provider,
            commands::fetch_provider_models,
            commands::list_provider_models,
            commands::upsert_provider_model,
            commands::delete_provider_model,
            commands::set_model_enabled,
            commands::set_default_model,
            commands::set_fallback_model,
            commands::get_fallback_model,
            commands::list_enabled_models,
            commands::set_session_model,
            commands::list_sessions,
            commands::get_default_session,
            commands::create_session,
            commands::get_session,
            commands::set_session_role,
            commands::set_session_agent,
            commands::submit_user_message,
            commands::list_session_queue,
            commands::cancel_queued_task,
            commands::submit_permission_decision,
            commands::submit_ask_response,
            commands::cancel_ask_response,
            commands::set_session_permission_mode,
            commands::get_global_permission_mode,
            commands::set_global_permission_mode,
            commands::get_suggestions_enabled,
            commands::set_suggestions_enabled,
            commands::get_aux_model_id,
            commands::set_aux_model_id,
            commands::submit_plan_decision,
            commands::stop_session,
            commands::delete_session,
            commands::rename_session,
            commands::set_session_pinned,
            commands::set_session_group,
            commands::set_session_mode,
            commands::set_session_workspace,
            commands::open_session_workspace,
            commands::open_artifact_file,
            commands::reveal_artifact_file,
            commands::get_recent_workspaces,
            commands::list_session_workspace_files,
            commands::set_draft_content,
            commands::cleanup_empty_drafts,
            commands::attach_file,
            commands::save_attachment,
            commands::read_attachment,
            commands::read_artifact,
            commands::create_session_group,
            commands::update_session_group,
            commands::list_session_groups,
            commands::delete_session_group,
            commands::list_skills,
            commands::toggle_skill,
            commands::install_skill_from_path,
            commands::uninstall_skill,
            commands::get_skill_detail,
            commands::read_skill_file,
            commands::compact_session,
            commands::get_auto_compact_enabled,
            commands::set_auto_compact_enabled,
            commands::get_auto_compact_threshold_pct,
            commands::set_auto_compact_threshold_pct,
            commands::get_show_completed_process,
            commands::set_show_completed_process,
            commands::get_session_task_panel_default_visible,
            commands::set_session_task_panel_default_visible,
            commands::get_auto_retry_max,
            commands::set_auto_retry_max,
            commands::get_max_iterations,
            commands::set_max_iterations,
            commands::get_subagent_execution_mode,
            commands::set_subagent_execution_mode,
            commands::get_agent_persona,
            commands::set_agent_persona,
            commands::retry_session,
            commands::get_usage_analytics,
            commands::get_session_context_usage,
            commands::get_session_usage,
            commands::get_session_message_usage,
            commands::get_model_call_log_enabled,
            commands::set_model_call_log_enabled,
            commands::list_model_calls,
            commands::get_model_call,
            commands::clear_model_calls,
            commands::get_model_call_log_stats,
            remote::commands::list_remote_channels,
            remote::commands::set_remote_channel,
            remote::commands::pause_remote_channel,
            remote::commands::resume_remote_channel,
            remote::commands::disconnect_remote_channel,
            remote::commands::list_remote_allowlist,
            remote::commands::add_remote_peer,
            remote::commands::remove_remote_peer,
            remote::commands::list_remote_bindings,
            remote::commands::begin_remote_wechat_pairing,
            remote::commands::connect_remote_telegram,
            remote::commands::connect_remote_dingtalk,
            remote::commands::connect_remote_feishu
        ])
        .build(tauri::generate_context!())
        .expect("error while building silicon-agent")
        .run(|app, event| {
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen {
                has_visible_windows,
                ..
            } = event
            {
                if !has_visible_windows {
                    crate::tray::show_main_window(app);
                }
            }
        });
}

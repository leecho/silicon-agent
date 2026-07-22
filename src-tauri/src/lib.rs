pub mod agent;
pub mod app_facade;
pub mod app_settings;
pub mod app_state;
pub mod apple;
pub mod aux_gen;
pub mod browser;
pub mod call_log;
pub mod commands;
pub mod context;
pub mod desktop;
pub mod engine;
pub mod expert;
pub mod group;
pub mod hook;
pub mod http;
pub mod knowledge;
pub mod market;
pub mod mcp;
pub mod memory;
pub mod permissions;
pub mod plugin;
pub mod project;
pub mod provider;
pub mod remote;
pub mod run;
pub mod scheduler;
pub mod session;
pub mod skill;
pub mod storage;
pub mod team;
pub mod tools;
pub mod tray;
pub mod usage;
pub mod yaml_block;

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
            let scheduler = crate::scheduler::runner::Scheduler::start(app.handle().clone());
            // 保活：交给 Tauri 托管，应用退出时 drop（通知线程停止）。
            app.manage(scheduler);
            // T73：演化扫描线程（默认无伴随体开启 evolution_enabled，故空转无副作用）。
            let evolution =
                crate::agent::evolution_runner::EvolutionScanner::start(app.handle().clone());
            app.manage(evolution);
            crate::tray::install_tray(app)?;
            // MCP 子系统：注入 AppHandle（用于推状态事件）并启动已启用 server 的连接。
            app.state::<AppState>().mcp.attach_app(app.handle().clone());
            app.state::<AppState>().mcp.startup_connect_all();
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
            commands::find_child_session,
            commands::list_session_children,
            commands::set_session_role,
            commands::set_session_agent,
            commands::list_teams,
            commands::list_active_teams,
            commands::team_detail,
            commands::create_team,
            commands::toggle_team,
            commands::delete_team,
            commands::list_experts,
            commands::import_team_from_path,
            commands::import_expert_from_path,
            commands::list_standalone_experts,
            commands::list_manageable_experts,
            commands::create_expert,
            commands::toggle_expert,
            commands::delete_expert,
            commands::expert_detail,
            commands::create_agent,
            commands::list_agents,
            commands::agent_detail,
            commands::list_agent_sessions,
            commands::list_agent_tasks,
            commands::list_agent_artifacts,
            commands::list_agent_skills,
            commands::open_agent_workspace,
            commands::update_agent,
            commands::toggle_agent,
            commands::set_agent_group,
            commands::delete_agent,
            commands::set_evolution_enabled,
            commands::list_soul_versions,
            commands::approve_soul_proposal,
            commands::reject_soul_proposal,
            commands::rollback_soul_version,
            commands::browse_skill_market,
            commands::list_skill_categories,
            commands::skill_market_detail,
            commands::skill_market_preview,
            commands::install_skill_from_market,
            commands::browse_expert_market,
            commands::expert_market_detail,
            commands::install_expert_from_market,
            commands::browse_team_market,
            commands::team_market_detail,
            commands::install_team_from_market,
            commands::browse_plugin_market,
            commands::plugin_market_detail,
            commands::install_plugin_from_market,
            commands::list_groups,
            commands::create_group,
            commands::rename_group,
            commands::delete_group,
            commands::set_expert_group,
            commands::set_team_group,
            commands::set_skill_group,
            commands::list_projects,
            commands::create_project,
            commands::get_project,
            commands::delete_project,
            commands::list_project_members,
            commands::list_project_skills,
            commands::add_project_member,
            commands::remove_project_member,
            commands::import_team_member,
            commands::submit_project_draft_message,
            commands::list_project_threads,
            commands::set_project_permission_mode,
            commands::set_project_instructions,
            commands::update_project,
            commands::set_project_workspace,
            commands::list_project_child_runs,
            commands::list_project_artifacts,
            commands::list_project_tasks,
            commands::list_thread_tasks,
            commands::open_project_workspace,
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
            commands::enhance_message,
            commands::submit_plan_decision,
            commands::stop_session,
            commands::cancel_child,
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
            commands::list_project_workspace_files,
            commands::read_project_workspace_file,
            commands::open_project_workspace_file,
            commands::list_agent_workspace_files,
            commands::read_agent_workspace_file,
            commands::open_agent_workspace_file,
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
            commands::list_plugins,
            commands::install_plugin_from_path,
            commands::toggle_plugin,
            commands::uninstall_plugin,
            commands::plugin_detail,
            commands::list_memories,
            commands::add_memory,
            commands::update_memory,
            commands::delete_memory,
            commands::clear_memories,
            commands::get_memory_profile,
            commands::set_memory_profile,
            commands::set_memory_pinned,
            commands::curate_memories,
            commands::list_scoped_memories,
            commands::count_scoped_memories,
            commands::add_scoped_memory,
            commands::kb_list,
            commands::kb_create,
            commands::kb_update,
            commands::kb_delete,
            commands::kb_document_list,
            commands::kb_document_text,
            commands::kb_document_preview,
            commands::kb_document_add,
            commands::kb_document_add_url,
            commands::kb_document_delete,
            commands::kb_search,
            commands::kb_mount,
            commands::kb_unmount,
            commands::kb_mounted_ids,
            commands::kb_mount_scope,
            commands::kb_unmount_scope,
            commands::kb_scoped_mounted_ids,
            commands::kb_vector_settings,
            commands::kb_set_vector_settings,
            commands::kb_build_vector_index,
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
            commands::get_tool_timeout_secs,
            commands::set_tool_timeout_secs,
            commands::get_tool_parallelism,
            commands::set_tool_parallelism,
            commands::get_subagent_execution_mode,
            commands::set_subagent_execution_mode,
            commands::get_computer_use_enabled,
            commands::set_computer_use_enabled,
            commands::get_tool_labels,
            commands::browser_status,
            commands::get_browser_use_enabled,
            commands::set_browser_use_enabled,
            commands::get_browser_headless,
            commands::set_browser_headless,
            commands::get_browser_idle_close_min,
            commands::set_browser_idle_close_min,
            commands::browser_is_open,
            commands::browser_open,
            commands::permission_status_all,
            commands::permission_status,
            commands::permission_request,
            commands::permission_open_settings,
            commands::app_relaunch,
            commands::retry_session,
            commands::get_usage_analytics,
            commands::get_session_context_usage,
            commands::get_session_usage,
            commands::get_project_usage,
            commands::get_agent_usage,
            commands::get_session_message_usage,
            commands::get_model_call_log_enabled,
            commands::set_model_call_log_enabled,
            commands::list_model_calls,
            commands::get_model_call,
            commands::clear_model_calls,
            commands::get_model_call_log_stats,
            commands::create_scheduled_task,
            commands::update_scheduled_task,
            commands::delete_scheduled_task,
            commands::set_task_enabled,
            commands::list_scheduled_tasks,
            commands::get_scheduled_task,
            commands::list_task_executions,
            commands::run_task_now,
            commands::get_keep_system_awake,
            commands::set_keep_system_awake,
            commands::mcp_list_servers,
            commands::mcp_server_statuses,
            commands::mcp_upsert_server,
            commands::mcp_import_json,
            commands::mcp_export_json,
            commands::mcp_list_tools,
            commands::mcp_set_enabled,
            commands::mcp_set_auto_approve,
            commands::mcp_delete_server,
            commands::mcp_test_connection,
            commands::mcp_reconnect,
            commands::mcp_oauth_authorize,
            commands::mcp_oauth_revoke,
            commands::mcp_set_oauth_client_id,
            remote::commands::list_remote_channels,
            remote::commands::set_remote_channel,
            remote::commands::pause_remote_channel,
            remote::commands::resume_remote_channel,
            remote::commands::disconnect_remote_channel,
            remote::commands::list_remote_allowlist,
            remote::commands::add_remote_peer,
            remote::commands::remove_remote_peer,
            remote::commands::list_remote_bindings,
            remote::commands::switch_remote_binding_session,
            remote::commands::begin_remote_wechat_pairing,
            remote::commands::connect_remote_telegram,
            remote::commands::connect_remote_dingtalk,
            remote::commands::connect_remote_feishu
        ])
        .build(tauri::generate_context!())
        .expect("error while building silicon-worker")
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

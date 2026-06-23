pub mod menu;

use tauri::{
    image::Image,
    menu::{Menu, MenuBuilder, MenuItemBuilder, Submenu, SubmenuBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, Runtime,
};

use crate::app_state::AppState;

use self::menu::{
    parse_tray_item_id, session_item_id, split_recent_sessions, truncate_menu_title, TrayAction,
    TrayEntitySplit, TrayMenuSnapshot, TraySession,
};

pub const MAIN_WINDOW_LABEL: &str = "main";
pub const TRAY_EVENT_NEW_TASK: &str = "tray_new_task";
pub const TRAY_EVENT_OPEN_SESSION: &str = "tray_open_session";

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct TrayOpenPayload {
    id: String,
}

pub fn show_main_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn empty_snapshot() -> TrayMenuSnapshot {
    TrayMenuSnapshot {
        sessions: TrayEntitySplit {
            primary: Vec::new(),
            more: Vec::new(),
        },
    }
}

fn snapshot_from_state(state: &AppState) -> Result<TrayMenuSnapshot, String> {
    let sessions = state
        .session
        .list_sessions()?
        .into_iter()
        .map(|session| TraySession {
            id: session.id,
            title: session.title,
            updated_at: session.updated_at,
            is_draft: session.is_draft,
            origin: if session.origin.is_empty() {
                "user".to_string()
            } else {
                session.origin
            },
        })
        .collect::<Vec<_>>();

    Ok(TrayMenuSnapshot {
        sessions: split_recent_sessions(sessions),
    })
}

fn build_split_submenu(
    app: &tauri::AppHandle,
    title: &str,
    split: &TrayEntitySplit,
    id_fn: fn(&str) -> String,
    empty_label: &str,
) -> tauri::Result<Submenu<tauri::Wry>> {
    let mut builder = SubmenuBuilder::new(app, title);
    if split.primary.is_empty() {
        let empty = MenuItemBuilder::new(empty_label)
            .enabled(false)
            .build(app)?;
        return builder.item(&empty).build();
    }

    for item in &split.primary {
        let menu_item = MenuItemBuilder::with_id(id_fn(&item.id), truncate_menu_title(&item.title))
            .build(app)?;
        builder = builder.item(&menu_item);
    }

    if !split.more.is_empty() {
        let mut more = SubmenuBuilder::new(app, "更多");
        for item in &split.more {
            let menu_item =
                MenuItemBuilder::with_id(id_fn(&item.id), truncate_menu_title(&item.title))
                    .build(app)?;
            more = more.item(&menu_item);
        }
        builder = builder.item(&more.build()?);
    }
    builder.build()
}

fn build_tray_menu(
    app: &tauri::AppHandle,
    snapshot: &TrayMenuSnapshot,
) -> tauri::Result<Menu<tauri::Wry>> {
    let new_task = MenuItemBuilder::with_id(menu::TRAY_NEW_TASK_ID, "新任务").build(app)?;
    let show = MenuItemBuilder::with_id(menu::TRAY_SHOW_ID, "打开 SiliconAgent").build(app)?;
    let quit = MenuItemBuilder::with_id(menu::TRAY_QUIT_ID, "退出 SiliconAgent").build(app)?;

    let session_menu =
        build_split_submenu(app, "会话", &snapshot.sessions, session_item_id, "暂无会话")?;

    MenuBuilder::new(app)
        .item(&new_task)
        .separator()
        .item(&session_menu)
        .separator()
        .item(&show)
        .item(&quit)
        .build()
}

fn dispatch_tray_action(app: &AppHandle, action: TrayAction) {
    match action {
        TrayAction::NewTask => {
            show_main_window(app);
            let _ = app.emit(TRAY_EVENT_NEW_TASK, ());
        }
        TrayAction::ShowMainWindow => show_main_window(app),
        TrayAction::Quit => app.exit(0),
        TrayAction::OpenSession(id) => {
            show_main_window(app);
            let _ = app.emit(TRAY_EVENT_OPEN_SESSION, TrayOpenPayload { id });
        }
    }
}

pub fn refresh_tray_menu(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let snapshot = snapshot_from_state(&state)?;
    let menu = build_tray_menu(app, &snapshot).map_err(|err| err.to_string())?;
    let tray = app
        .tray_by_id("main-tray")
        .ok_or_else(|| "main tray is not installed".to_string())?;
    tray.set_menu(Some(menu)).map_err(|err| err.to_string())?;
    Ok(())
}

pub fn install_tray(app: &tauri::App) -> tauri::Result<()> {
    let state = app.state::<AppState>();
    let snapshot = snapshot_from_state(&state).unwrap_or_else(|_| empty_snapshot());
    let menu = build_tray_menu(app.handle(), &snapshot)?;
    let icon = Image::new_owned(
        include_bytes!("../../icons/tray-template.rgba").to_vec(),
        128,
        128,
    );

    TrayIconBuilder::with_id("main-tray")
        .icon(icon)
        .icon_as_template(true)
        .tooltip("SiliconAgent")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        })
        .on_menu_event(|app, event| {
            if let Some(action) = parse_tray_item_id(event.id().as_ref()) {
                dispatch_tray_action(app, action);
            }
        })
        .build(app)?;
    Ok(())
}

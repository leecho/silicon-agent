#[tauri::command]
pub fn refresh_tray_menu(app: tauri::AppHandle) -> Result<(), String> {
    crate::tray::refresh_tray_menu(&app)
}

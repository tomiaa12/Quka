use tauri::{AppHandle, State};

use crate::database::settings::{get, upsert};
use crate::shortcut::{change, register, unregister, ShortcutState, ShortcutStatus};
use crate::state::{AppError, AppState};

#[tauri::command]
pub fn get_shortcut_status(state: State<ShortcutState>) -> ShortcutStatus {
    state.status()
}

#[tauri::command]
pub fn register_global_shortcut(app: AppHandle, state: State<AppState>) -> Result<ShortcutStatus, String> {
    let shortcut = {
        let conn = state.lock_db()?;
        get(&conn).map_err(AppError::from)?.global_shortcut
    };
    register(&app, &shortcut).map_err(AppError::into)
}

#[tauri::command]
pub fn unregister_global_shortcut(app: AppHandle) -> Result<ShortcutStatus, String> {
    unregister(&app).map_err(AppError::into)
}

#[tauri::command]
pub fn change_global_shortcut(
    shortcut: String,
    app: AppHandle,
    state: State<AppState>,
) -> Result<ShortcutStatus, String> {
    let status = change(&app, &shortcut).map_err(AppError::from)?;
    let conn = state.lock_db()?;
    let mut settings = get(&conn).map_err(AppError::from)?;
    settings.global_shortcut = status.shortcut.clone();
    upsert(&conn, &settings).map_err(AppError::from)?;
    Ok(status)
}

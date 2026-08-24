use tauri::{AppHandle, Emitter, State};

use crate::database::models::Settings;
use crate::database::settings::{get, upsert};
use crate::shortcut::change;
use crate::state::{AppError, AppState};

#[tauri::command]
pub fn get_settings(state: State<AppState>) -> Result<Settings, String> {
    let conn = state.lock_db()?;
    get(&conn).map_err(AppError::into)
}

#[tauri::command]
pub fn update_settings(
    settings: Settings,
    app: AppHandle,
    state: State<AppState>,
) -> Result<(), String> {
    let conn = state.lock_db()?;
    let previous = get(&conn).map_err(AppError::from)?;
    upsert(&conn, &settings).map_err(AppError::from)?;
    drop(conn);
    if previous.result_limit != settings.result_limit
        || previous.enable_usage_ranking != settings.enable_usage_ranking
    {
        if let Ok(mut index) = state.lock_index() {
            index.set_prefs(settings.result_limit, settings.enable_usage_ranking);
        }
    }
    if previous.global_shortcut != settings.global_shortcut {
        change(&app, &settings.global_shortcut).map_err(AppError::from)?;
    }
    if previous.launch_at_startup != settings.launch_at_startup {
        crate::autostart::apply(settings.launch_at_startup).map_err(AppError::from)?;
    }
    if previous.locale != settings.locale {
        crate::app_window::apply_locale(&app, &settings.locale);
        if let Err(error) = crate::tray::refresh(&app, &settings.locale) {
            log::error!("托盘语言更新失败：{error}");
        }
    }
    let _ = app.emit("settings-updated", &settings);
    Ok(())
}

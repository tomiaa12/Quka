use tauri::State;

use crate::database::applications::{get_by_id, record_launch};
use crate::state::{AppError, AppState};

#[tauri::command]
pub fn launch_application(id: String, state: State<AppState>) -> Result<(), String> {
    let application = {
        let conn = state.lock_db()?;
        get_by_id(&conn, &id)
            .map_err(AppError::from)?
            .ok_or_else(|| AppError::Launcher("应用不存在".into()))?
    };

    crate::launcher::launch(&application).map_err(AppError::from)?;

    let timestamp = {
        let conn = state.lock_db()?;
        record_launch(&conn, &id).map_err(AppError::from)?
    };
    if let Ok(mut index) = state.lock_index() {
        index.note_launch(&id, timestamp);
    }
    Ok(())
}

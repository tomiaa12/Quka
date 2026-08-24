use tauri::State;

use crate::database::applications::{delete, get_by_id, insert, query_all, update};
use crate::database::models::{Application, DatabaseInfo};
use crate::state::{AppError, AppState};

#[tauri::command]
pub fn get_applications(state: State<AppState>) -> Result<Vec<Application>, String> {
    let conn = state.lock_db()?;
    query_all(&conn).map_err(AppError::into)
}

#[tauri::command]
pub fn insert_application(application: Application, state: State<AppState>) -> Result<(), String> {
    {
        let conn = state.lock_db()?;
        insert(&conn, &application).map_err(AppError::from)?;
    }
    state.refresh_index().map_err(AppError::from)?;
    Ok(())
}

#[tauri::command]
pub fn update_application(application: Application, state: State<AppState>) -> Result<(), String> {
    {
        let conn = state.lock_db()?;
        update(&conn, &application).map_err(AppError::from)?;
    }
    state.refresh_index().map_err(AppError::from)?;
    Ok(())
}

#[tauri::command]
pub fn delete_application(id: String, state: State<AppState>) -> Result<(), String> {
    {
        let conn = state.lock_db()?;
        delete(&conn, &id).map_err(AppError::from)?;
    }
    state.refresh_index().map_err(AppError::from)?;
    Ok(())
}

#[tauri::command]
pub fn get_application(id: String, state: State<AppState>) -> Result<Application, String> {
    let conn = state.lock_db()?;
    get_by_id(&conn, &id)
        .map_err(AppError::from)?
        .ok_or_else(|| "应用不存在".into())
}

#[tauri::command]
pub fn get_database_info(state: State<AppState>) -> Result<DatabaseInfo, String> {
    let conn = state.lock_db()?;
    let application_count = crate::database::applications::count(&conn).map_err(AppError::from)?;
    Ok(DatabaseInfo {
        application_count,
        just_initialized: state.just_initialized,
        needs_scan: crate::scanner::is_scan_supported()
            && (application_count == 0 || state.take_needs_rescan()),
        scanner: crate::scanner::current_scanner_name().into(),
    })
}

pub mod applications;
pub mod id;
pub mod migrations;
pub mod models;
pub mod search;
pub mod seed;
pub mod settings;

use std::fs;
use std::path::PathBuf;

use rusqlite::Connection;
use tauri::{AppHandle, Manager};

use crate::state::{AppError, AppState};

pub fn app_data_dir(app: &AppHandle) -> Result<PathBuf, AppError> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|error| AppError::Io(error.to_string()))?;
    fs::create_dir_all(&dir).map_err(|error| AppError::Io(error.to_string()))?;
    Ok(dir)
}

pub fn app_db_path(app: &AppHandle) -> Result<PathBuf, AppError> {
    Ok(app_data_dir(app)?.join("quka.db"))
}

pub fn open(app: &AppHandle) -> Result<AppState, AppError> {
    let path = app_db_path(app)?;
    let conn = Connection::open(&path).map_err(AppError::from)?;
    let needs_rescan = migrations::run(&conn)?;
    let settings_row = settings::get(&conn)?;
    let just_initialized = if crate::scanner::is_scan_supported() {
        false
    } else {
        seed::seed_if_empty(&conn)?
    };
    let apps = applications::query_all(&conn)?;
    let index = search::SearchIndex::build(
        apps,
        settings_row.result_limit,
        settings_row.enable_usage_ranking,
    );
    log::info!("搜索索引已加载：{} 个应用", index.len());
    Ok(AppState {
        db: std::sync::Mutex::new(conn),
        index: std::sync::Mutex::new(index),
        just_initialized,
        needs_rescan: std::sync::atomic::AtomicBool::new(needs_rescan),
    })
}

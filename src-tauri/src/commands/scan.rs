use tauri::{AppHandle, Emitter, Manager, State};

use crate::database::applications::count;
use crate::database::models::ScanResult;
use crate::icon::icons_dir;
use crate::scanner::macos::MacOsScanner;
use crate::scanner::sync::{sync_macos, sync_windows};
use crate::scanner::windows::WindowsScanner;
use crate::scanner::{current_scanner_name, ApplicationScanner};
use crate::state::{AppError, AppState};

#[tauri::command]
pub async fn rescan_applications(app: AppHandle, _state: State<'_, AppState>) -> Result<ScanResult, String> {
    run_rescan(&app).await.map_err(AppError::into)
}

pub async fn run_rescan(app: &AppHandle) -> Result<ScanResult, AppError> {
    let scanner = current_scanner_name().to_string();
    let state = app.state::<AppState>();
    if scanner == "none" {
        log::info!("跳过扫描：当前平台没有可用的应用扫描器");
        let conn = state.lock_db()?;
        let application_count = count(&conn)?;
        return Ok(ScanResult {
            application_count,
            inserted: 0,
            updated: 0,
            deleted: 0,
            supported: false,
            scanner,
        });
    }

    let home = app.path().home_dir().map_err(|error| AppError::Io(error.to_string()))?;
    let icon_dir = icons_dir(app)?;
    let scanner_kind = scanner.clone();

    log::info!("扫描开始");
    let scanned = tauri::async_runtime::spawn_blocking(move || match scanner_kind.as_str() {
        "macos" => MacOsScanner::new(home, icon_dir).scan(),
        _ => WindowsScanner::new(icon_dir).scan(),
    })
    .await
    .map_err(|error| AppError::Scanner(error.to_string()))??;

    let conn = state.lock_db()?;
    let stats = if scanner == "macos" {
        sync_macos(&conn, scanned)?
    } else {
        sync_windows(&conn, scanned)?
    };
    let application_count = count(&conn)?;
    crate::database::migrations::mark_scan_schema(&conn)?;
    drop(conn);
    state.refresh_index()?;
    state.clear_needs_rescan();
    log::info!(
        "扫描结束：共 {} 个应用，新增 {}，更新 {}，删除 {}",
        application_count,
        stats.inserted,
        stats.updated,
        stats.deleted
    );

    let result = ScanResult {
        application_count,
        inserted: stats.inserted,
        updated: stats.updated,
        deleted: stats.deleted,
        supported: true,
        scanner,
    };
    let _ = app.emit("apps-rescanned", &result);
    Ok(result)
}

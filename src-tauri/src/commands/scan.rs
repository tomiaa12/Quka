use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Emitter, Manager, State};

use crate::database::applications::count;
use crate::database::models::ScanResult;
use crate::icon::icons_dir;
use crate::scanner::macos::MacOsScanner;
use crate::scanner::sync::{sync_macos, sync_windows};
use crate::scanner::windows::WindowsScanner;
use crate::scanner::{current_scanner_name, ApplicationScanner};
use crate::state::{AppError, AppState};

static SCAN_LOCK: Mutex<()> = Mutex::new(());
static LAST_SCAN_MS: AtomicI64 = AtomicI64::new(0);

const NUDGE_AFTER_MS: i64 = 45_000;

#[tauri::command]
pub async fn rescan_applications(app: AppHandle, _state: State<'_, AppState>) -> Result<ScanResult, String> {
    run_rescan(&app).await.map_err(AppError::into)
}

pub async fn run_rescan(app: &AppHandle) -> Result<ScanResult, AppError> {
    run_rescan_with(app, false).await
}

pub fn request_auto_rescan(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        match run_rescan_with(&app, true).await {
            Ok(result) => {
                if result.inserted + result.updated + result.deleted > 0 {
                    log::info!(
                        "自动扫描：新增 {}，更新 {}，删除 {}",
                        result.inserted,
                        result.updated,
                        result.deleted
                    );
                } else {
                    log::debug!("自动扫描：应用列表无变化");
                }
            }
            Err(error) => log::error!("自动扫描失败：{error}"),
        }
    });
}

pub fn nudge_rescan(app: &AppHandle) {
    let last = LAST_SCAN_MS.load(Ordering::Relaxed);
    if last > 0 && now_ms().saturating_sub(last) < NUDGE_AFTER_MS {
        return;
    }
    request_auto_rescan(app);
}

pub async fn run_rescan_with(app: &AppHandle, silent: bool) -> Result<ScanResult, AppError> {
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
            silent,
        });
    }

    let home = app.path().home_dir().map_err(|error| AppError::Io(error.to_string()))?;
    let icon_dir = icons_dir(app)?;
    let scanner_kind = scanner.clone();
    let app_handle = app.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let _guard = SCAN_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let state = app_handle.state::<AppState>();
        log::info!("扫描开始");
        let scanned = match scanner_kind.as_str() {
            "macos" => MacOsScanner::new(home, icon_dir).scan(),
            _ => WindowsScanner::new(icon_dir).scan(),
        }?;

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
        LAST_SCAN_MS.store(now_ms(), Ordering::Relaxed);
        log::info!(
            "扫描结束：共 {} 个应用，新增 {}，更新 {}，删除 {}",
            application_count,
            stats.inserted,
            stats.updated,
            stats.deleted
        );

        let changed = stats.inserted > 0 || stats.updated > 0 || stats.deleted > 0;
        let result = ScanResult {
            application_count,
            inserted: stats.inserted,
            updated: stats.updated,
            deleted: stats.deleted,
            supported: true,
            scanner,
            silent,
        };
        if !silent || changed {
            let _ = app_handle.emit("apps-rescanned", &result);
        }
        Ok(result)
    })
    .await
    .map_err(|error| AppError::Scanner(error.to_string()))?
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

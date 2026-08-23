use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager, PhysicalPosition};

use crate::state::AppError;

static LAST_MONITOR: Mutex<Option<CachedMonitor>> = Mutex::new(None);
static IGNORE_UNFOCUS_UNTIL: AtomicI64 = AtomicI64::new(0);

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

fn arm_ignore_unfocus() {
    IGNORE_UNFOCUS_UNTIL.store(now_ms() + 400, Ordering::Relaxed);
}

pub fn should_hide_on_unfocus() -> bool {
    now_ms() >= IGNORE_UNFOCUS_UNTIL.load(Ordering::Relaxed)
}

#[derive(Clone, Copy)]
struct CachedMonitor {
    origin_x: i32,
    origin_y: i32,
    width: u32,
    height: u32,
}

impl CachedMonitor {
    fn contains(self, x: i32, y: i32) -> bool {
        x >= self.origin_x
            && y >= self.origin_y
            && x < self.origin_x + self.width as i32
            && y < self.origin_y + self.height as i32
    }
}

pub fn toggle_search_window(app: &AppHandle) -> Result<(), AppError> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| AppError::Shortcut("搜索窗口不存在".into()))?;
    if window.is_visible().unwrap_or(false) {
        hide_search_window(app)?;
        return Ok(());
    }
    show_search_window(app)
}

pub fn hide_search_window(app: &AppHandle) -> Result<(), AppError> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| AppError::Shortcut("搜索窗口不存在".into()))?;
    window
        .hide()
        .map_err(|error| AppError::Shortcut(error.to_string()))?;
    Ok(())
}

pub fn show_search_window(app: &AppHandle) -> Result<(), AppError> {
    let started = std::time::Instant::now();
    arm_ignore_unfocus();
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| AppError::Shortcut("搜索窗口不存在".into()))?;
    if let Err(error) = position_on_cursor_monitor(app, &window) {
        log::warn!("定位搜索窗口失败：{error}");
    }
    if window.is_minimized().unwrap_or(false) {
        window
            .unminimize()
            .map_err(|error| AppError::Shortcut(error.to_string()))?;
    }
    window
        .show()
        .map_err(|error| AppError::Shortcut(error.to_string()))?;
    window
        .set_focus()
        .map_err(|error| AppError::Shortcut(error.to_string()))?;
    window
        .emit("search-shown", ())
        .map_err(|error| AppError::Shortcut(error.to_string()))?;
    let elapsed = started.elapsed().as_millis();
    if elapsed >= 100 {
        log::warn!("窗口显示 {elapsed}ms");
    } else {
        log::debug!("窗口显示 {elapsed}ms");
    }
    Ok(())
}

pub fn show_settings_window(app: &AppHandle) -> Result<(), AppError> {
    if let Some(window) = app.get_webview_window("settings") {
        if window.is_minimized().unwrap_or(false) {
            window
                .unminimize()
                .map_err(|error| AppError::Shortcut(error.to_string()))?;
        }
        window
            .show()
            .map_err(|error| AppError::Shortcut(error.to_string()))?;
        window
            .set_focus()
            .map_err(|error| AppError::Shortcut(error.to_string()))?;
        return Ok(());
    }

    tauri::WebviewWindowBuilder::new(app, "settings", tauri::WebviewUrl::App("index.html".into()))
        .title("Quka 设置")
        .inner_size(760.0, 520.0)
        .resizable(false)
        .decorations(true)
        .skip_taskbar(false)
        .center()
        .build()
        .map_err(|error| AppError::Shortcut(error.to_string()))?;
    Ok(())
}

fn position_on_cursor_monitor(
    app: &AppHandle,
    window: &tauri::WebviewWindow,
) -> Result<(), AppError> {
    let cursor = app
        .cursor_position()
        .map_err(|error| AppError::Shortcut(error.to_string()))?;
    let x = cursor.x.round() as i32;
    let y = cursor.y.round() as i32;

    if let Ok(cache) = LAST_MONITOR.lock() {
        if let Some(monitor) = *cache {
            if monitor.contains(x, y) {
                apply_position(window, monitor)?;
                return Ok(());
            }
        }
    }

    let monitors = window
        .available_monitors()
        .map_err(|error| AppError::Shortcut(error.to_string()))?;
    let monitor = monitors
        .into_iter()
        .find(|item| {
            let position = item.position();
            let size = item.size();
            x >= position.x
                && y >= position.y
                && x < position.x + size.width as i32
                && y < position.y + size.height as i32
        })
        .or_else(|| window.current_monitor().ok().flatten())
        .or_else(|| window.primary_monitor().ok().flatten());

    let Some(monitor) = monitor else {
        return Ok(());
    };
    let origin = monitor.position();
    let area = monitor.size();
    let cached = CachedMonitor {
        origin_x: origin.x,
        origin_y: origin.y,
        width: area.width,
        height: area.height,
    };
    if let Ok(mut slot) = LAST_MONITOR.lock() {
        *slot = Some(cached);
    }
    apply_position(window, cached)
}

fn apply_position(window: &tauri::WebviewWindow, monitor: CachedMonitor) -> Result<(), AppError> {
    let size = window
        .outer_size()
        .map_err(|error| AppError::Shortcut(error.to_string()))?;
    let x = monitor.origin_x + (monitor.width as i32 - size.width as i32).max(0) / 2;
    let y = monitor.origin_y + ((monitor.height as i32 - size.height as i32) / 5).max(48);
    window
        .set_position(PhysicalPosition::new(x, y))
        .map_err(|error| AppError::Shortcut(error.to_string()))?;
    Ok(())
}

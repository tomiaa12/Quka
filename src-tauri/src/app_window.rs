use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager, PhysicalPosition};

use crate::i18n;
use crate::state::AppError;

fn current_locale(app: &AppHandle) -> String {
    let Some(state) = app.try_state::<crate::state::AppState>() else {
        return "system".into();
    };
    state
        .lock_db()
        .ok()
        .and_then(|conn| crate::database::settings::get(&conn).ok())
        .map(|settings| settings.locale)
        .unwrap_or_else(|| "system".into())
}

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
    #[cfg(target_os = "macos")]
    macos::set_alpha(&window, 1.0);
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
    let monitor = match cursor_monitor(app, &window) {
        Ok(monitor) => monitor,
        Err(error) => {
            log::warn!("定位搜索窗口失败：{error}");
            None
        }
    };

    // macOS 对隐藏窗的 set_position 经常不生效，show() 会先画在上次的屏幕再跳过来。
    // 先把透明度降到 0，就位后再显示。
    #[cfg(target_os = "macos")]
    macos::set_alpha(&window, 0.0);

    if window.is_minimized().unwrap_or(false) {
        window
            .unminimize()
            .map_err(|error| AppError::Shortcut(error.to_string()))?;
    }
    if let Some(monitor) = monitor {
        if let Err(error) = apply_position(&window, monitor) {
            log::warn!("预定位搜索窗口失败：{error}");
        }
    }
    window
        .show()
        .map_err(|error| AppError::Shortcut(error.to_string()))?;
    if let Some(monitor) = monitor {
        if let Err(error) = apply_position(&window, monitor) {
            log::warn!("显示后定位搜索窗口失败：{error}");
        }
    }
    #[cfg(target_os = "macos")]
    macos::set_alpha(&window, 1.0);

    window
        .set_focus()
        .map_err(|error| AppError::Shortcut(error.to_string()))?;
    window
        .emit("search-shown", ())
        .map_err(|error| AppError::Shortcut(error.to_string()))?;
    crate::commands::scan::nudge_rescan(app);
    let elapsed = started.elapsed().as_millis();
    if elapsed >= 100 {
        log::warn!("窗口显示 {elapsed}ms");
    } else {
        log::debug!("窗口显示 {elapsed}ms");
    }
    Ok(())
}

pub fn show_settings_window(app: &AppHandle) -> Result<(), AppError> {
    #[cfg(target_os = "macos")]
    {
        let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
        activate_app();
    }

    let window = if let Some(window) = app.get_webview_window("settings") {
        if window.is_minimized().unwrap_or(false) {
            window
                .unminimize()
                .map_err(|error| AppError::Shortcut(error.to_string()))?;
        }
        let _ = window.set_title(i18n::settings_title(&current_locale(app)));
        window
    } else {
        tauri::WebviewWindowBuilder::new(app, "settings", tauri::WebviewUrl::App("index.html".into()))
            .title(i18n::settings_title(&current_locale(app)))
            .inner_size(760.0, 520.0)
            .resizable(false)
            .decorations(true)
            .skip_taskbar(false)
            .visible(true)
            .focused(true)
            .center()
            .build()
            .map_err(|error| AppError::Shortcut(error.to_string()))?
    };

    window
        .show()
        .map_err(|error| AppError::Shortcut(error.to_string()))?;
    window
        .set_focus()
        .map_err(|error| AppError::Shortcut(error.to_string()))?;
    #[cfg(target_os = "macos")]
    {
        activate_app();
        refocus_settings_after_menu(app);
    }
    Ok(())
}

pub fn restore_accessory(_app: &AppHandle) {
    #[cfg(target_os = "macos")]
    {
        let _ = _app.set_activation_policy(tauri::ActivationPolicy::Accessory);
    }
}

#[cfg(target_os = "macos")]
fn activate_app() {
    use objc2_app_kit::NSApplication;
    use objc2_foundation::MainThreadMarker;
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    #[allow(deprecated)]
    app.activateIgnoringOtherApps(true);
}

#[cfg(target_os = "macos")]
fn refocus_settings_after_menu(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(80));
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || {
            activate_app();
            if let Some(window) = handle.get_webview_window("settings") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        });
    });
}

pub fn apply_locale(app: &AppHandle, locale: &str) {
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.set_title(i18n::settings_title(locale));
    }
}

pub fn watch_foreign_activation(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    macos::watch_foreign_activation(app);
    #[cfg(not(target_os = "macos"))]
    let _ = app;
}

fn cursor_monitor(
    app: &AppHandle,
    window: &tauri::WebviewWindow,
) -> Result<Option<CachedMonitor>, AppError> {
    let cursor = app
        .cursor_position()
        .map_err(|error| AppError::Shortcut(error.to_string()))?;
    let x = cursor.x.round() as i32;
    let y = cursor.y.round() as i32;

    if let Ok(cache) = LAST_MONITOR.lock() {
        if let Some(monitor) = *cache {
            if monitor.contains(x, y) {
                return Ok(Some(monitor));
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
        return Ok(None);
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
    Ok(Some(cached))
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

#[cfg(target_os = "macos")]
mod macos {
    use std::ptr::NonNull;

    use block2::RcBlock;
    use objc2_app_kit::{NSWindow, NSWorkspace, NSWorkspaceDidActivateApplicationNotification};
    use objc2_foundation::NSNotification;
    use tauri::AppHandle;

    use super::{hide_search_window, should_hide_on_unfocus};

    pub fn set_alpha(window: &tauri::WebviewWindow, alpha: f64) {
        let Ok(ptr) = window.ns_window() else {
            return;
        };
        if ptr.is_null() {
            return;
        }
        unsafe {
            let ns_window = &*ptr.cast::<NSWindow>();
            ns_window.setAnimationBehavior(objc2_app_kit::NSWindowAnimationBehavior::None);
            ns_window.setAlphaValue(alpha);
        }
    }

    pub fn watch_foreign_activation(app: &AppHandle) {
        let app = app.clone();
        let observer = unsafe {
            let workspace = NSWorkspace::sharedWorkspace();
            let center = workspace.notificationCenter();
            let block = RcBlock::new(move |_notification: NonNull<NSNotification>| {
                if !should_hide_on_unfocus() {
                    return;
                }
                if let Err(error) = hide_search_window(&app) {
                    log::debug!("其他应用激活后隐藏搜索窗失败：{error}");
                }
            });
            center.addObserverForName_object_queue_usingBlock(
                Some(NSWorkspaceDidActivateApplicationNotification),
                None,
                None,
                &block,
            )
        };
        std::mem::forget(observer);
    }
}

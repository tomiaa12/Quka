use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};

use crate::app_window::{show_search_window, show_settings_window};
use crate::commands::scan::run_rescan;

pub fn open_label() -> &'static str {
    if cfg!(target_os = "macos") {
        "搜索"
    } else {
        "打开搜索"
    }
}

pub fn rescan_label() -> &'static str {
    if cfg!(target_os = "macos") {
        "重新扫描"
    } else {
        "重新扫描应用"
    }
}

pub fn install(app: &AppHandle) -> Result<(), String> {
    let title = MenuItem::with_id(app, "title", "Quka", false, None::<&str>)
        .map_err(|error| error.to_string())?;
    let open = MenuItem::with_id(app, "open", open_label(), true, None::<&str>)
        .map_err(|error| error.to_string())?;
    let settings = MenuItem::with_id(app, "settings", "设置", true, None::<&str>)
        .map_err(|error| error.to_string())?;
    let rescan = MenuItem::with_id(app, "rescan", rescan_label(), true, None::<&str>)
        .map_err(|error| error.to_string())?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)
        .map_err(|error| error.to_string())?;
    let separator = PredefinedMenuItem::separator(app).map_err(|error| error.to_string())?;

    let menu = Menu::with_items(app, &[&title, &open, &settings, &rescan, &separator, &quit])
        .map_err(|error| error.to_string())?;

    let mut builder = TrayIconBuilder::new()
        .menu(&menu)
        .tooltip("Quka")
        .show_menu_on_left_click(cfg!(target_os = "macos"))
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => {
                if let Err(error) = show_search_window(app) {
                    log::error!("托盘打开搜索失败：{error}");
                }
            }
            "settings" => {
                if let Err(error) = show_settings_window(app) {
                    log::error!("托盘打开设置失败：{error}");
                }
            }
            "rescan" => start_rescan(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if cfg!(target_os = "macos") {
                return;
            }
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                if let Err(error) = show_search_window(tray.app_handle()) {
                    log::error!("托盘打开搜索失败：{error}");
                }
            }
        });

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }

    builder.build(app).map_err(|error| error.to_string())?;
    log::info!("系统托盘已就绪");
    Ok(())
}

fn start_rescan(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        match run_rescan(&app).await {
            Ok(_) => {}
            Err(error) => {
                log::error!("重新扫描失败：{error}");
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.emit("scan-failed", error.to_string());
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{open_label, rescan_label};

    #[test]
    fn uses_platform_menu_labels() {
        if cfg!(target_os = "macos") {
            assert_eq!(open_label(), "搜索");
            assert_eq!(rescan_label(), "重新扫描");
        } else {
            assert_eq!(open_label(), "打开搜索");
            assert_eq!(rescan_label(), "重新扫描应用");
        }
    }
}

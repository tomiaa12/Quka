use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};

use crate::app_window::{show_search_window, show_settings_window};
use crate::commands::scan::run_rescan;
use crate::i18n;

const TRAY_ID: &str = "main";

pub fn open_label(locale: &str) -> &'static str {
    i18n::tray_open(locale)
}

pub fn rescan_label(locale: &str) -> &'static str {
    i18n::tray_rescan(locale)
}

pub fn install(app: &AppHandle, locale: &str, tray_icon: &str) -> Result<(), String> {
    let style = crate::database::settings::normalize_tray_icon(tray_icon);
    let menu = build_menu(app, locale)?;
    let builder = TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu)
        .tooltip("Quka")
        .icon_as_template(is_template(&style))
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

    builder
        .icon(tray_image(&style)?)
        .build(app)
        .map_err(|error| error.to_string())?;
    log::info!("系统托盘已就绪");
    Ok(())
}

pub fn refresh(app: &AppHandle, locale: &str) -> Result<(), String> {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return Ok(());
    };
    let menu = build_menu(app, locale)?;
    tray.set_menu(Some(menu))
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn apply_icon(app: &AppHandle, tray_icon: &str) -> Result<(), String> {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return Ok(());
    };
    let style = crate::database::settings::normalize_tray_icon(tray_icon);
    tray.set_icon_with_as_template(Some(tray_image(&style)?), is_template(&style))
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn is_template(style: &str) -> bool {
    style != "color"
}

fn tray_image(style: &str) -> Result<Image<'static>, String> {
    let bytes: &[u8] = match style {
        "mono" => include_bytes!("../icons/tray/mono-64.png"),
        "search" => include_bytes!("../icons/tray/search-64.png"),
        "bolt" => include_bytes!("../icons/tray/bolt-64.png"),
        _ => include_bytes!("../icons/64x64.png"),
    };
    Image::from_bytes(bytes).map_err(|error| error.to_string())
}

fn build_menu(app: &AppHandle, locale: &str) -> Result<Menu<tauri::Wry>, String> {
    let title = MenuItem::with_id(app, "title", "Quka", false, None::<&str>)
        .map_err(|error| error.to_string())?;
    let open = MenuItem::with_id(app, "open", i18n::tray_open(locale), true, None::<&str>)
        .map_err(|error| error.to_string())?;
    let settings = MenuItem::with_id(app, "settings", i18n::tray_settings(locale), true, None::<&str>)
        .map_err(|error| error.to_string())?;
    let rescan = MenuItem::with_id(app, "rescan", i18n::tray_rescan(locale), true, None::<&str>)
        .map_err(|error| error.to_string())?;
    let quit = MenuItem::with_id(app, "quit", i18n::tray_quit(locale), true, None::<&str>)
        .map_err(|error| error.to_string())?;
    let separator = PredefinedMenuItem::separator(app).map_err(|error| error.to_string())?;
    Menu::with_items(app, &[&title, &open, &settings, &rescan, &separator, &quit])
        .map_err(|error| error.to_string())
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
            assert_eq!(open_label("zh-CN"), "搜索");
            assert_eq!(rescan_label("zh-CN"), "重新扫描");
            assert_eq!(open_label("en"), "Search");
            assert_eq!(rescan_label("en"), "Rescan");
        } else {
            assert_eq!(open_label("zh-CN"), "打开搜索");
            assert_eq!(rescan_label("zh-CN"), "重新扫描应用");
            assert_eq!(open_label("en"), "Open Search");
            assert_eq!(rescan_label("en"), "Rescan Apps");
        }
    }
}

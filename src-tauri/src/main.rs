#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(dead_code)]

mod app_window;
mod autostart;
mod commands;
mod database;
mod i18n;
mod icon;
mod launcher;
mod scanner;
mod shortcut;
mod state;
mod tray;

use commands::applications::{
    delete_application, get_application, get_applications, get_database_info, insert_application,
    update_application,
};
use commands::launch::launch_application;
use commands::scan::rescan_applications;
use commands::shortcut::{
    change_global_shortcut, get_shortcut_status, register_global_shortcut,
    unregister_global_shortcut,
};
use tauri::Manager;

use commands::search::search_applications;
use commands::settings::{get_settings, update_settings};

fn main() {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_secs()
        .try_init();

    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            let state = database::open(app.handle()).map_err(|error| error.to_string())?;
            let settings = {
                let conn = state.lock_db().map_err(|error| error.to_string())?;
                crate::database::settings::get(&conn).map_err(|error| error.to_string())?
            };
            app.manage(state);
            app.manage(shortcut::ShortcutState::new());
            if let Err(error) = shortcut::register(app.handle(), &settings.global_shortcut) {
                log::error!("快捷键注册失败：{error}");
            }
            if let Err(error) = autostart::sync(settings.launch_at_startup) {
                log::error!("{error}");
            }
            if let Err(error) = tray::install(app.handle(), &settings.locale) {
                log::error!("系统托盘创建失败：{error}");
            }
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.hide();
            }
            app_window::watch_foreign_activation(app.handle());
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }
            match event {
                tauri::WindowEvent::CloseRequested { api, .. } => {
                    api.prevent_close();
                    let _ = window.hide();
                }
                tauri::WindowEvent::Focused(false) => {
                    if app_window::should_hide_on_unfocus() {
                        let _ = window.hide();
                    }
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_applications,
            get_application,
            insert_application,
            update_application,
            delete_application,
            get_database_info,
            rescan_applications,
            launch_application,
            search_applications,
            get_settings,
            update_settings,
            get_shortcut_status,
            register_global_shortcut,
            unregister_global_shortcut,
            change_global_shortcut,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Quka")
        .run(|_app, event| {
            if let tauri::RunEvent::ExitRequested { api, code, .. } = event {
                if code.is_none() {
                    api.prevent_exit();
                }
            }
        });
}

#[cfg(test)]
mod packaging_tests {
    use std::path::PathBuf;

    #[test]
    fn bundle_targets_cover_windows_and_macos() {
        let value: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let targets: Vec<&str> = value["bundle"]["targets"]
            .as_array()
            .expect("bundle.targets")
            .iter()
            .filter_map(|item| item.as_str())
            .collect();
        assert!(targets.contains(&"nsis"), "缺少 Windows .exe (NSIS)");
        assert!(targets.contains(&"msi"), "缺少 Windows .msi");
        assert!(targets.contains(&"dmg"), "缺少 macOS .dmg");
        assert_eq!(value["bundle"]["windows"]["nsis"]["installMode"], "currentUser");
        assert_eq!(
            value["bundle"]["windows"]["wix"]["upgradeCode"],
            "9e2c1a70-4d8f-5b31-8c6a-1f0e7d5b2a44"
        );
        assert_eq!(value["bundle"]["createUpdaterArtifacts"], true);
        let endpoints = value["plugins"]["updater"]["endpoints"]
            .as_array()
            .expect("updater.endpoints");
        assert!(
            endpoints
                .iter()
                .any(|item| item.as_str().is_some_and(|url| url.contains("latest.json"))),
            "缺少 latest.json 更新地址"
        );
    }

    #[test]
    fn bundle_icons_exist() {
        let icons = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("icons");
        for name in [
            "32x32.png",
            "128x128.png",
            "128x128@2x.png",
            "icon.ico",
            "icon.icns",
        ] {
            assert!(icons.join(name).is_file(), "缺少打包图标 {name}");
        }
    }
}

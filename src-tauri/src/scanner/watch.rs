use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tauri::{AppHandle, Manager};

use super::macos;
use super::windows;

const DEBOUNCE: Duration = Duration::from_millis(1800);

pub fn start(app: &AppHandle, on_change: impl Fn() + Send + 'static) {
    if !super::is_scan_supported() {
        return;
    }
    let home = app.path().home_dir().unwrap_or_default();
    let roots = watch_roots(&home);
    if roots.is_empty() {
        log::warn!("没有可监听的应用目录");
        return;
    }
    if let Err(error) = std::thread::Builder::new()
        .name("quka-app-watch".into())
        .spawn(move || {
            if let Err(error) = run_loop(roots, on_change) {
                log::error!("应用目录监听结束：{error}");
            }
        })
    {
        log::error!("无法启动应用目录监听：{error}");
    }
}

pub fn watch_roots(home: &Path) -> Vec<(PathBuf, bool)> {
    if macos::is_supported() {
        return macos::watch_directories(home);
    }
    if windows::is_supported() {
        return windows::watch_directories();
    }
    Vec::new()
}

pub fn is_interesting_path(path: &Path) -> bool {
    if is_noise_path(path) {
        return false;
    }
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let lower = name.to_ascii_lowercase();
    if lower.starts_with('.') || lower == "thumbs.db" || lower == "desktop.ini" {
        return false;
    }
    if path.components().any(|component| {
        component
            .as_os_str()
            .to_str()
            .is_some_and(|part| part.to_ascii_lowercase().ends_with(".app"))
    }) {
        return true;
    }
    if has_ignored_extension(&lower) {
        return false;
    }
    if lower.ends_with(".lnk") || lower.ends_with(".exe") || lower.ends_with(".app") {
        return true;
    }
    if lower.ends_with(".localized") {
        return true;
    }
    path.extension().is_none()
}

pub fn event_should_rescan(event: &Event) -> bool {
    if event.need_rescan() {
        return true;
    }
    if matches!(event.kind, EventKind::Access(_) | EventKind::Other) {
        return false;
    }
    event.paths.iter().any(|path| is_interesting_path(path))
}

fn has_ignored_extension(name: &str) -> bool {
    const SKIP: &[&str] = &[
        ".tmp",
        ".temp",
        ".download",
        ".crdownload",
        ".part",
        ".partial",
        ".dll",
        ".pak",
        ".log",
        ".plist",
        ".json",
        ".strings",
        ".loctable",
        ".icns",
        ".ico",
        ".png",
        ".jpg",
        ".jpeg",
        ".gif",
        ".svg",
        ".ds_store",
    ];
    SKIP.iter().any(|ext| name.ends_with(ext))
}

fn is_noise_path(path: &Path) -> bool {
    path.components().any(|component| {
        let Some(name) = component.as_os_str().to_str() else {
            return false;
        };
        matches!(
            name.to_ascii_lowercase().as_str(),
            "node_modules"
                | ".git"
                | "winsxs"
                | "windowsapps"
                | "$recycle.bin"
                | "temp"
                | "tmp"
                | "cache"
                | "logs"
        )
    })
}

fn run_loop(roots: Vec<(PathBuf, bool)>, on_change: impl Fn()) -> notify::Result<()> {
    let (tx, rx) = mpsc::channel();
    let mut watcher = RecommendedWatcher::new(tx, notify::Config::default())?;
    let mut watching = 0usize;
    for (path, recursive) in &roots {
        if !path.is_dir() {
            log::debug!("跳过监听（目录不存在）{}", path.display());
            continue;
        }
        let mode = if *recursive {
            RecursiveMode::Recursive
        } else {
            RecursiveMode::NonRecursive
        };
        match watcher.watch(path, mode) {
            Ok(()) => {
                watching += 1;
                log::info!("已监听应用目录 {}", path.display());
            }
            Err(error) => log::warn!("无法监听 {}：{error}", path.display()),
        }
    }
    if watching == 0 {
        log::warn!("应用目录监听未启动：没有可访问的目录");
        return Ok(());
    }

    let mut pending = false;
    loop {
        let received = if pending {
            match rx.recv_timeout(DEBOUNCE) {
                Ok(event) => Some(event),
                Err(RecvTimeoutError::Timeout) => {
                    on_change();
                    pending = false;
                    None
                }
                Err(RecvTimeoutError::Disconnected) => break,
            }
        } else {
            match rx.recv() {
                Ok(event) => Some(event),
                Err(_) => break,
            }
        };
        let Some(event) = received else {
            continue;
        };
        match event {
            Ok(event) if event_should_rescan(&event) => pending = true,
            Ok(_) => {}
            Err(error) => log::warn!("应用目录监听事件异常：{error}"),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{event_should_rescan, is_interesting_path, watch_roots};
    use notify::event::{CreateKind, Event, EventKind};
    use std::path::{Path, PathBuf};

    #[test]
    fn macos_watch_roots_cover_user_apps() {
        let roots = watch_roots(Path::new("/Users/ada"));
        if cfg!(target_os = "macos") {
            let paths: Vec<_> = roots.iter().map(|(path, _)| path.clone()).collect();
            assert!(paths.contains(&PathBuf::from("/Applications")));
            assert!(paths.contains(&PathBuf::from("/Users/ada/Applications")));
            assert!(!paths.contains(&PathBuf::from("/System/Applications")));
        }
    }

    #[test]
    fn matches_app_bundles_and_localized_folders() {
        assert!(is_interesting_path(Path::new("/Applications/WeChat.app")));
        assert!(is_interesting_path(Path::new(
            "/Applications/WeChat.app/Contents/MacOS/WeChat"
        )));
        assert!(is_interesting_path(Path::new("/Applications/Lark.localized")));
        assert!(is_interesting_path(Path::new("/Users/ada/Applications/Foo.app")));
    }

    #[test]
    fn matches_windows_shortcuts_and_install_folders() {
        assert!(is_interesting_path(Path::new(
            r"C:\ProgramData\Microsoft\Windows\Start Menu\Programs\Google Chrome.lnk"
        )));
        assert!(is_interesting_path(Path::new(r"C:\Program Files\MyApp")));
        assert!(is_interesting_path(Path::new(r"C:\Program Files\MyApp\app.exe")));
        assert!(is_interesting_path(Path::new(
            r"C:\Users\ada\AppData\Local\Programs\cursor\Cursor.exe"
        )));
    }

    #[test]
    fn ignores_temp_cache_and_metadata() {
        assert!(!is_interesting_path(Path::new("/Applications/.DS_Store")));
        assert!(!is_interesting_path(Path::new(
            r"C:\Program Files\MyApp\setup.tmp"
        )));
        assert!(!is_interesting_path(Path::new(
            r"C:\Program Files\node_modules\cli.js"
        )));
        assert!(is_interesting_path(Path::new(
            "/Applications/Foo.app/Contents/Info.plist"
        )));
    }

    #[test]
    fn create_events_trigger_rescan() {
        let event = Event::new(EventKind::Create(CreateKind::Folder))
            .add_path(PathBuf::from("/Applications/Demo.app"));
        assert!(event_should_rescan(&event));
    }
}

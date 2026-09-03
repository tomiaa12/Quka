pub mod detect;
pub mod macos;
pub mod windows;

use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::app_window::toggle_search_window;
use crate::state::AppError;
use detect::{normalize_shortcut, parse_shortcut, shortcut_label};

pub struct ShortcutState {
    inner: Mutex<ShortcutRuntime>,
}

struct ShortcutRuntime {
    current: String,
    registered: bool,
    last_error: String,
    handle: Option<PlatformHandle>,
}

enum PlatformHandle {
    Windows(windows::PlatformHandle),
    MacOs(macos::PlatformHandle),
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutStatus {
    pub shortcut: String,
    pub label: String,
    pub registered: bool,
    pub error: String,
}

impl ShortcutState {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(ShortcutRuntime {
                current: detect::default_shortcut(),
                registered: false,
                last_error: String::new(),
                handle: None,
            }),
        }
    }

    pub fn clear_error(&self) {
        let mut inner = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        inner.last_error.clear();
    }

    pub fn status(&self) -> ShortcutStatus {
        let inner = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        ShortcutStatus {
            shortcut: inner.current.clone(),
            label: shortcut_label(&inner.current),
            registered: inner.registered,
            error: inner.last_error.clone(),
        }
    }
}

pub fn register(app: &AppHandle, shortcut: &str) -> Result<ShortcutStatus, AppError> {
    change(app, shortcut)
}

pub fn unregister(app: &AppHandle) -> Result<ShortcutStatus, AppError> {
    let state = app.state::<ShortcutState>();
    let mut inner = state
        .inner
        .lock()
        .map_err(|error| AppError::Shortcut(error.to_string()))?;
    stop_handle(app, inner.handle.take());
    inner.registered = false;
    inner.last_error = String::new();
    log::info!("快捷键已注销");
    Ok(ShortcutStatus {
        shortcut: inner.current.clone(),
        label: shortcut_label(&inner.current),
        registered: false,
        error: String::new(),
    })
}

pub fn change(app: &AppHandle, shortcut: &str) -> Result<ShortcutStatus, AppError> {
    let normalized = normalize_shortcut(shortcut);
    let kind = parse_shortcut(&normalized)?;
    let state = app.state::<ShortcutState>();
    let mut inner = state
        .inner
        .lock()
        .map_err(|error| AppError::Shortcut(error.to_string()))?;
    stop_handle(app, inner.handle.take());
    inner.current = normalized.clone();

    let app_handle = app.clone();
    let started = if windows::is_supported() {
        windows::start(kind, {
            let app_handle = app_handle.clone();
            move || on_shortcut_toggle(&app_handle)
        })
        .map(|handle| (PlatformHandle::Windows(handle), None))
    } else if macos::is_supported() {
        macos::start(app, kind, move || on_shortcut_toggle(&app_handle))
            .map(|(handle, warning)| (PlatformHandle::MacOs(handle), warning))
    } else {
        Err(AppError::Shortcut("当前平台暂不支持全局快捷键".into()))
    };

    match started {
        Ok((handle, warning)) => {
            inner.handle = Some(handle);
            inner.registered = true;
            inner.last_error = warning.unwrap_or_default();
            log::info!("快捷键已注册：{}", shortcut_label(&normalized));
        }
        Err(error) => {
            let error = wrap_register_error(error);
            inner.registered = false;
            inner.last_error = error.to_string();
            log::error!("{error}");
            return Err(error);
        }
    }

    Ok(ShortcutStatus {
        shortcut: inner.current.clone(),
        label: shortcut_label(&inner.current),
        registered: inner.registered,
        error: inner.last_error.clone(),
    })
}

fn wrap_register_error(error: AppError) -> AppError {
    let message = error.to_string();
    if message.contains("快捷键注册失败") {
        error
    } else {
        AppError::Shortcut(format!("快捷键注册失败：{message}"))
    }
}

fn stop_handle(app: &AppHandle, handle: Option<PlatformHandle>) {
    match handle {
        Some(PlatformHandle::Windows(handle)) => windows::stop(handle),
        Some(PlatformHandle::MacOs(handle)) => macos::stop(app, handle),
        None => {}
    }
}

fn on_shortcut_toggle(app: &AppHandle) {
    let app = app.clone();
    if let Err(error) = app.clone().run_on_main_thread(move || {
        if crate::fullscreen::should_block_shortcut(&app) {
            if let Err(error) = crate::app_window::hide_search_window(&app) {
                log::debug!("全屏时隐藏搜索窗失败：{error}");
            }
            return;
        }
        if let Err(error) = toggle_search_window(&app) {
            log::error!("快捷键呼出窗口失败：{error}");
        }
    }) {
        log::error!("快捷键回调调度失败：{error}");
    }
}

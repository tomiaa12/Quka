use tauri::{AppHandle, Manager};

use crate::database::settings;
use crate::state::AppState;

pub fn should_block_shortcut(app: &AppHandle) -> bool {
    let enabled = match app.try_state::<AppState>() {
        Some(state) => state
            .lock_db()
            .ok()
            .and_then(|conn| settings::get(&conn).ok())
            .map(|item| item.disable_on_fullscreen)
            .unwrap_or(true),
        None => true,
    };
    enabled && is_exclusive_fullscreen()
}

pub fn is_exclusive_fullscreen() -> bool {
    #[cfg(windows)]
    {
        windows::is_exclusive_fullscreen()
    }
    #[cfg(target_os = "macos")]
    {
        macos::is_exclusive_fullscreen()
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        false
    }
}

#[cfg(windows)]
mod windows {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    use windows::Win32::System::Threading::GetCurrentProcessId;
    use windows::Win32::UI::Shell::{
        SHQueryUserNotificationState, QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetClassNameW, GetForegroundWindow, GetWindowRect, GetWindowThreadProcessId,
    };

    pub fn is_exclusive_fullscreen() -> bool {
        notification_state_fullscreen() || foreground_covers_monitor()
    }

    fn notification_state_fullscreen() -> bool {
        unsafe {
            let mut state = Default::default();
            if SHQueryUserNotificationState(&mut state).is_err() {
                return false;
            }
            state == QUNS_RUNNING_D3D_FULL_SCREEN || state == QUNS_PRESENTATION_MODE
        }
    }

    fn foreground_covers_monitor() -> bool {
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.0.is_null() {
                return false;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid == GetCurrentProcessId() {
                return false;
            }
            let mut class = [0u16; 64];
            let count = GetClassNameW(hwnd, &mut class);
            if count > 0 {
                let name = String::from_utf16_lossy(&class[..count as usize]);
                if matches!(name.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd") {
                    return false;
                }
            }
            let mut rect = RECT::default();
            if GetWindowRect(hwnd, &mut rect).is_err() {
                return false;
            }
            let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
            let mut info = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            if GetMonitorInfoW(monitor, &mut info).as_bool() == false {
                return false;
            }
            let mon = info.rcMonitor;
            rect.left <= mon.left + 2
                && rect.top <= mon.top + 2
                && rect.right >= mon.right - 2
                && rect.bottom >= mon.bottom - 2
        }
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use objc2_app_kit::{NSRunningApplication, NSScreen, NSWorkspace};
    use objc2_foundation::MainThreadMarker;

    pub fn is_exclusive_fullscreen() -> bool {
        display_captured() || frontmost_fills_screen()
    }

    fn display_captured() -> bool {
        #[link(name = "CoreGraphics", kind = "framework")]
        extern "C" {
            fn CGMainDisplayID() -> u32;
            fn CGDisplayIsCaptured(display: u32) -> u8;
        }
        unsafe { CGDisplayIsCaptured(CGMainDisplayID()) != 0 }
    }

    fn frontmost_fills_screen() -> bool {
        let Some(mtm) = MainThreadMarker::new() else {
            return false;
        };
        let workspace = NSWorkspace::sharedWorkspace();
        let Some(front) = workspace.frontmostApplication() else {
            return false;
        };
        let current = NSRunningApplication::currentApplication();
        if front.processIdentifier() == current.processIdentifier() {
            return false;
        }
        NSScreen::screens(mtm).iter().any(|screen| {
            let frame = screen.frame();
            let visible = screen.visibleFrame();
            (frame.size.width - visible.size.width).abs() < 1.0
                && (frame.size.height - visible.size.height).abs() < 2.0
                && frame.size.width > 0.0
                && frame.size.height > 0.0
        })
    }
}

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
            match SHQueryUserNotificationState() {
                Ok(state) => {
                    state == QUNS_RUNNING_D3D_FULL_SCREEN || state == QUNS_PRESENTATION_MODE
                }
                Err(_) => false,
            }
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
    use core::ffi::c_void;

    use core_foundation::base::TCFType;
    use core_foundation::dictionary::{CFDictionary, CFDictionaryGetValueIfPresent, CFDictionaryRef};
    use core_foundation::number::CFNumber;
    use core_foundation::string::CFStringRef;
    use core_graphics::display::CGDisplay;
    use core_graphics::geometry::CGRect;
    use core_graphics::window::{
        copy_window_info, kCGNullWindowID, kCGWindowBounds, kCGWindowLayer,
        kCGWindowListExcludeDesktopElements, kCGWindowListOptionOnScreenOnly, kCGWindowOwnerPID,
    };
    use objc2_app_kit::{NSRunningApplication, NSWorkspace};

    /// 与 Windows 侧一致：窗口贴齐显示器边缘的容差（点）。
    const EDGE_TOLERANCE: f64 = 2.0;
    /// 普通应用窗口所在 layer；菜单栏、Dock 等系统 UI 不是 0。
    const NORMAL_WINDOW_LAYER: i64 = 0;

    pub fn is_exclusive_fullscreen() -> bool {
        display_captured() || frontmost_covers_display()
    }

    fn display_captured() -> bool {
        #[link(name = "CoreGraphics", kind = "framework")]
        extern "C" {
            fn CGMainDisplayID() -> u32;
            fn CGDisplayIsCaptured(display: u32) -> u8;
        }
        unsafe { CGDisplayIsCaptured(CGMainDisplayID()) != 0 }
    }

    fn frontmost_covers_display() -> bool {
        let workspace = NSWorkspace::sharedWorkspace();
        let Some(front) = workspace.frontmostApplication() else {
            return false;
        };
        let current = NSRunningApplication::currentApplication();
        let front_pid = i64::from(front.processIdentifier());
        if front_pid == i64::from(current.processIdentifier()) {
            return false;
        }

        let Some(windows) = copy_window_info(
            kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements,
            kCGNullWindowID,
        ) else {
            return false;
        };
        let Ok(display_ids) = CGDisplay::active_displays() else {
            return false;
        };
        let displays: Vec<CGRect> = display_ids
            .into_iter()
            .map(|id| CGDisplay::new(id).bounds())
            .filter(|rect| rect.size.width > 0.0 && rect.size.height > 0.0)
            .collect();
        if displays.is_empty() {
            return false;
        }

        windows.iter().any(|item| {
            let dict_ref = *item as CFDictionaryRef;
            if dict_ref.is_null() {
                return false;
            }
            let dict: CFDictionary = unsafe { CFDictionary::wrap_under_get_rule(dict_ref) };
            window_covers_any_display(&dict, front_pid, &displays)
        })
    }

    fn window_covers_any_display(
        dict: &CFDictionary,
        front_pid: i64,
        displays: &[CGRect],
    ) -> bool {
        if dict_i64(dict, unsafe { kCGWindowOwnerPID }) != Some(front_pid) {
            return false;
        }
        if dict_i64(dict, unsafe { kCGWindowLayer }) != Some(NORMAL_WINDOW_LAYER) {
            return false;
        }
        let Some(bounds) = dict_rect(dict, unsafe { kCGWindowBounds }) else {
            return false;
        };
        displays
            .iter()
            .any(|display| rect_covers_display(bounds, *display, EDGE_TOLERANCE))
    }

    fn dict_value(dict: &CFDictionary, key: CFStringRef) -> Option<*const c_void> {
        let mut value: *const c_void = std::ptr::null();
        let found = unsafe {
            CFDictionaryGetValueIfPresent(
                dict.as_concrete_TypeRef(),
                key as *const c_void,
                &mut value,
            )
        };
        if found != 0 && !value.is_null() {
            Some(value)
        } else {
            None
        }
    }

    fn dict_i64(dict: &CFDictionary, key: CFStringRef) -> Option<i64> {
        let value = dict_value(dict, key)?;
        let number = unsafe { CFNumber::wrap_under_get_rule(value as _) };
        number.to_i64()
    }

    fn dict_rect(dict: &CFDictionary, key: CFStringRef) -> Option<CGRect> {
        let value = dict_value(dict, key)?;
        let bounds: CFDictionary = unsafe { CFDictionary::wrap_under_get_rule(value as _) };
        CGRect::from_dict_representation(&bounds)
    }

    fn rect_covers_display(window: CGRect, display: CGRect, tolerance: f64) -> bool {
        window.origin.x <= display.origin.x + tolerance
            && window.origin.y <= display.origin.y + tolerance
            && window.origin.x + window.size.width >= display.origin.x + display.size.width - tolerance
            && window.origin.y + window.size.height
                >= display.origin.y + display.size.height - tolerance
    }

    #[cfg(test)]
    mod tests {
        use super::{rect_covers_display, EDGE_TOLERANCE};
        use core_graphics::geometry::{CGPoint, CGRect, CGSize};

        fn rect(x: f64, y: f64, w: f64, h: f64) -> CGRect {
            CGRect::new(&CGPoint::new(x, y), &CGSize::new(w, h))
        }

        #[test]
        fn detects_window_covering_display() {
            let display = rect(0.0, 0.0, 1920.0, 1080.0);
            assert!(rect_covers_display(
                rect(0.0, 0.0, 1920.0, 1080.0),
                display,
                EDGE_TOLERANCE
            ));
            assert!(rect_covers_display(
                rect(-1.0, -1.0, 1922.0, 1082.0),
                display,
                EDGE_TOLERANCE
            ));
        }

        #[test]
        fn ignores_ordinary_windowed_apps() {
            let display = rect(0.0, 0.0, 1920.0, 1080.0);
            assert!(!rect_covers_display(
                rect(100.0, 80.0, 1200.0, 800.0),
                display,
                EDGE_TOLERANCE
            ));
            // 未盖住菜单栏区域的普通最大化，不应算独占全屏
            assert!(!rect_covers_display(
                rect(0.0, 25.0, 1920.0, 1055.0),
                display,
                EDGE_TOLERANCE
            ));
        }

        #[test]
        fn does_not_treat_idle_secondary_screen_geometry_as_fullscreen() {
            // 旧逻辑用「任意一块屏 visibleFrame == frame」误判；副屏常年如此。
            let primary = rect(0.0, 0.0, 1920.0, 1080.0);
            let secondary = rect(-1920.0, 0.0, 1920.0, 1080.0);
            let finder = rect(200.0, 120.0, 900.0, 700.0);
            assert!(!rect_covers_display(finder, primary, EDGE_TOLERANCE));
            assert!(!rect_covers_display(finder, secondary, EDGE_TOLERANCE));
        }
    }
}

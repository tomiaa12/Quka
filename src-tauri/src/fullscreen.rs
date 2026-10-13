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

    use core_foundation::array::CFArray;
    use core_foundation::base::{CFTypeRef, TCFType};
    use core_foundation::boolean::CFBoolean;
    use core_foundation::string::{CFString, CFStringRef};
    use objc2::MainThreadMarker;
    use objc2_app_kit::{
        NSApplication, NSApplicationPresentationOptions, NSRunningApplication, NSWorkspace,
    };

    type AxUiElementRef = *mut c_void;
    type AxError = i32;

    const AX_ERROR_SUCCESS: AxError = 0;
    /// 未公开但系统窗口普遍支持的属性；最大化窗口为 false，绿钮全屏为 true。
    const AX_FULL_SCREEN_ATTRIBUTE: &str = "AXFullScreen";

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXUIElementCreateApplication(pid: i32) -> AxUiElementRef;
        fn AXUIElementCopyAttributeValue(
            element: AxUiElementRef,
            attribute: CFStringRef,
            value: *mut CFTypeRef,
        ) -> AxError;
        fn CFRelease(cf: CFTypeRef);
    }

    pub fn is_exclusive_fullscreen() -> bool {
        // 不按「窗口铺满显示器」判断：副屏最大化常年等同于铺满，会误伤。
        display_captured() || system_presentation_fullscreen() || frontmost_ax_fullscreen()
    }

    fn display_captured() -> bool {
        #[link(name = "CoreGraphics", kind = "framework")]
        extern "C" {
            fn CGMainDisplayID() -> u32;
            fn CGDisplayIsCaptured(display: u32) -> u8;
        }
        unsafe { CGDisplayIsCaptured(CGMainDisplayID()) != 0 }
    }

    fn system_presentation_fullscreen() -> bool {
        let Some(mtm) = MainThreadMarker::new() else {
            return false;
        };
        NSApplication::sharedApplication(mtm)
            .currentSystemPresentationOptions()
            .contains(NSApplicationPresentationOptions::FullScreen)
    }

    fn frontmost_ax_fullscreen() -> bool {
        let workspace = NSWorkspace::sharedWorkspace();
        let Some(front) = workspace.frontmostApplication() else {
            return false;
        };
        let current = NSRunningApplication::currentApplication();
        let pid = front.processIdentifier();
        if pid == current.processIdentifier() {
            return false;
        }

        let app = unsafe { AXUIElementCreateApplication(pid) };
        if app.is_null() {
            return false;
        }
        let fullscreen = ax_window_is_fullscreen(app, "AXFocusedWindow")
            || ax_window_is_fullscreen(app, "AXMainWindow")
            || ax_any_window_is_fullscreen(app);
        unsafe { CFRelease(app as CFTypeRef) };
        fullscreen
    }

    fn ax_window_is_fullscreen(app: AxUiElementRef, attribute: &str) -> bool {
        let Some(window) = ax_copy_element(app, attribute) else {
            return false;
        };
        let fullscreen = ax_read_bool(window, AX_FULL_SCREEN_ATTRIBUTE).unwrap_or(false);
        unsafe { CFRelease(window as CFTypeRef) };
        fullscreen
    }

    fn ax_any_window_is_fullscreen(app: AxUiElementRef) -> bool {
        let Some(windows) = ax_copy_array(app, "AXWindows") else {
            return false;
        };
        windows.iter().any(|item| {
            let window = *item as AxUiElementRef;
            if window.is_null() {
                return false;
            }
            ax_read_bool(window, AX_FULL_SCREEN_ATTRIBUTE).unwrap_or(false)
        })
    }

    fn ax_copy_element(element: AxUiElementRef, attribute: &str) -> Option<AxUiElementRef> {
        let key = CFString::new(attribute);
        let mut value: CFTypeRef = std::ptr::null_mut();
        let status = unsafe {
            AXUIElementCopyAttributeValue(element, key.as_concrete_TypeRef(), &mut value)
        };
        if status != AX_ERROR_SUCCESS || value.is_null() {
            return None;
        }
        Some(value as AxUiElementRef)
    }

    fn ax_copy_array(element: AxUiElementRef, attribute: &str) -> Option<CFArray> {
        let key = CFString::new(attribute);
        let mut value: CFTypeRef = std::ptr::null_mut();
        let status = unsafe {
            AXUIElementCopyAttributeValue(element, key.as_concrete_TypeRef(), &mut value)
        };
        if status != AX_ERROR_SUCCESS || value.is_null() {
            return None;
        }
        Some(unsafe { CFArray::wrap_under_create_rule(value as _) })
    }

    fn ax_read_bool(element: AxUiElementRef, attribute: &str) -> Option<bool> {
        let key = CFString::new(attribute);
        let mut value: CFTypeRef = std::ptr::null_mut();
        let status = unsafe {
            AXUIElementCopyAttributeValue(element, key.as_concrete_TypeRef(), &mut value)
        };
        if status != AX_ERROR_SUCCESS || value.is_null() {
            return None;
        }
        let boolean = unsafe { CFBoolean::wrap_under_create_rule(value as _) };
        Some(boolean.into())
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn maximized_is_not_treated_as_ax_fullscreen_signal() {
            // 探测脚本：副屏最大化窗口 AXFullScreen=false；仅绿钮全屏为 true。
            assert_eq!(super::AX_FULL_SCREEN_ATTRIBUTE, "AXFullScreen");
            assert_eq!(super::AX_ERROR_SUCCESS, 0);
        }
    }
}

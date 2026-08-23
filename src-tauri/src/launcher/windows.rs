use std::path::Path;

use super::{validate_path, ApplicationLauncher};
use crate::database::models::Application;
use crate::state::AppError;

pub struct WindowsLauncher;

pub fn is_supported() -> bool {
    cfg!(target_os = "windows")
}

impl ApplicationLauncher for WindowsLauncher {
    fn launch(&self, app: &Application) -> Result<(), AppError> {
        let path = validate_path(&app.path)?;
        log::info!("启动应用 {}", app.name);
        match shell_execute(&path) {
            Ok(()) => Ok(()),
            Err(error) => {
                log::error!("启动应用失败 {}：{error}", app.name);
                Err(error)
            }
        }
    }
}

#[cfg(windows)]
fn shell_execute(path: &Path) -> Result<(), AppError> {
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let file = wide(path.as_os_str());
    let directory = path
        .parent()
        .map(|parent| wide(parent.as_os_str()))
        .unwrap_or_default();
    let operation = wide(std::ffi::OsStr::new("open"));

    let result = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(operation.as_ptr()),
            PCWSTR(file.as_ptr()),
            PCWSTR::null(),
            if directory.is_empty() {
                PCWSTR::null()
            } else {
                PCWSTR(directory.as_ptr())
            },
            SW_SHOWNORMAL,
        )
    };

    let code = result.0 as isize;
    if code > 32 {
        Ok(())
    } else {
        Err(AppError::Launcher(format!("应用启动失败（{code}）")))
    }
}

#[cfg(not(windows))]
fn shell_execute(_path: &Path) -> Result<(), AppError> {
    Err(AppError::Launcher("当前平台不是 Windows".into()))
}

#[cfg(windows)]
fn wide(value: &std::ffi::OsStr) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    value.encode_wide().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::WindowsLauncher;
    use crate::database::models::Application;
    use crate::launcher::ApplicationLauncher;

    fn app(path: &str) -> Application {
        Application {
            id: "test".into(),
            name: "Notepad".into(),
            path: path.into(),
            bundle_id: None,
            icon: None,
            source: "system".into(),
            launch_count: 0,
            last_launch_time: None,
        }
    }

    #[test]
    fn missing_file_does_not_launch() {
        let error = WindowsLauncher
            .launch(&app(r"C:\missing-quka-launcher.exe"))
            .unwrap_err();
        assert!(error.to_string().contains("路径不存在"));
    }

    #[cfg(windows)]
    #[test]
    fn launches_notepad() {
        WindowsLauncher
            .launch(&app(r"C:\Windows\System32\notepad.exe"))
            .unwrap();
    }
}

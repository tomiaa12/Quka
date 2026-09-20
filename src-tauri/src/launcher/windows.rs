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
        log::info!("启动应用 {}", app.name);
        let result = if let Some(target) = shell_apps_folder_target(app) {
            shell_execute_str(&target)
        } else {
            let path = validate_path(&app.path)?;
            shell_execute(&path)
        };
        match result {
            Ok(()) => Ok(()),
            Err(error) => {
                log::error!("启动应用失败 {}：{error}", app.name);
                Err(error)
            }
        }
    }
}

fn shell_apps_folder_target(app: &Application) -> Option<String> {
    let path = app.path.trim();
    let lower = path.to_ascii_lowercase();
    if lower.starts_with("shell:appsfolder\\") || lower.starts_with("shell:appsfolder/") {
        return Some(path.to_string());
    }
    None
}

#[cfg(windows)]
fn shell_execute(path: &Path) -> Result<(), AppError> {
    shell_execute_str(&path.to_string_lossy())
}

#[cfg(windows)]
fn shell_execute_str(target: &str) -> Result<(), AppError> {
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let file = wide_str(target);
    let directory = Path::new(target)
        .parent()
        .filter(|parent| {
            let value = parent.to_string_lossy();
            !value.is_empty() && !value.eq_ignore_ascii_case("shell:AppsFolder")
        })
        .map(|parent| wide_str(&parent.to_string_lossy()))
        .unwrap_or_default();
    let operation = wide_str("open");

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

#[cfg(not(windows))]
fn shell_execute_str(_target: &str) -> Result<(), AppError> {
    Err(AppError::Launcher("当前平台不是 Windows".into()))
}

#[cfg(windows)]
fn wide_str(value: &str) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    std::ffi::OsStr::new(value)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{shell_apps_folder_target, WindowsLauncher};
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
            aliases: String::new(),
        }
    }

    #[test]
    fn missing_file_does_not_launch() {
        let error = WindowsLauncher
            .launch(&app(r"C:\missing-quka-launcher.exe"))
            .unwrap_err();
        assert!(error.to_string().contains("路径不存在"));
    }

    #[test]
    fn detects_shell_apps_folder_target() {
        let target = shell_apps_folder_target(&app(
            r"shell:AppsFolder\Claude_pzs8sxrjxfjjc!Claude",
        ));
        assert_eq!(
            target.as_deref(),
            Some(r"shell:AppsFolder\Claude_pzs8sxrjxfjjc!Claude")
        );
        assert!(shell_apps_folder_target(&app(r"C:\Windows\System32\notepad.exe")).is_none());
    }

    #[cfg(windows)]
    #[test]
    fn launches_notepad() {
        WindowsLauncher
            .launch(&app(r"C:\Windows\System32\notepad.exe"))
            .unwrap();
    }
}

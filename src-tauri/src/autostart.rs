use std::path::PathBuf;

use auto_launch::{AutoLaunch, AutoLaunchBuilder, MacOSLaunchMode, WindowsEnableMode};

use crate::state::AppError;

const APP_NAME: &str = "Quka";

fn launch_path() -> Result<PathBuf, AppError> {
    let exe = std::env::current_exe().map_err(|error| AppError::Io(error.to_string()))?;
    if cfg!(target_os = "macos") {
        if let Some(bundle) = exe
            .ancestors()
            .find(|path| path.extension().is_some_and(|ext| ext == "app"))
        {
            return Ok(bundle.to_path_buf());
        }
    }
    Ok(exe)
}

fn manager_for(path: &str) -> Result<AutoLaunch, AppError> {
    AutoLaunchBuilder::new()
        .set_app_name(APP_NAME)
        .set_app_path(path)
        .set_windows_enable_mode(WindowsEnableMode::CurrentUser)
        .set_macos_launch_mode(MacOSLaunchMode::AppleScript)
        .build()
        .map_err(|error| AppError::Io(format!("开机启动设置失败：{error}")))
}

fn manager() -> Result<AutoLaunch, AppError> {
    manager_for(&launch_path()?.to_string_lossy())
}

pub fn apply(enabled: bool) -> Result<(), AppError> {
    let auto = manager()?;
    if enabled {
        auto.enable()
            .map_err(|error| AppError::Io(format!("开机启动设置失败：{error}")))?;
        log::info!("已开启开机启动");
    } else {
        auto.disable()
            .map_err(|error| AppError::Io(format!("开机启动设置失败：{error}")))?;
        log::info!("已关闭开机启动");
    }
    Ok(())
}

pub fn is_enabled() -> Result<bool, AppError> {
    manager()
        .and_then(|auto| {
            auto.is_enabled()
                .map_err(|error| AppError::Io(format!("开机启动查询失败：{error}")))
        })
}

pub fn sync(enabled: bool) -> Result<(), AppError> {
    match is_enabled() {
        Ok(current) if current == enabled => Ok(()),
        _ => apply(enabled),
    }
}

#[cfg(test)]
mod tests {
    use super::{apply, is_enabled, launch_path, APP_NAME};

    #[test]
    fn launch_path_is_absolute() {
        assert!(launch_path().unwrap().is_absolute());
    }

    #[test]
    fn app_name_is_quka() {
        assert_eq!(APP_NAME, "Quka");
    }

    #[cfg(windows)]
    #[test]
    fn toggles_windows_startup() {
        apply(true).expect("enable startup");
        assert!(is_enabled().unwrap(), "Quka should be in Windows Startup");
        apply(false).expect("disable startup");
        assert!(!is_enabled().unwrap(), "Quka should be removed from Startup");
    }
}

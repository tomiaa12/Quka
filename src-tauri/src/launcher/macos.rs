use std::path::Path;

use super::{validate_path, ApplicationLauncher};
use crate::database::models::Application;
use crate::state::AppError;

pub struct MacOsLauncher;

pub fn is_supported() -> bool {
    cfg!(target_os = "macos")
}

impl ApplicationLauncher for MacOsLauncher {
    fn launch(&self, app: &Application) -> Result<(), AppError> {
        let path = validate_path(&app.path)?;
        log::info!("启动应用 {}", app.name);
        match open_application(&path) {
            Ok(()) => Ok(()),
            Err(error) => {
                log::error!("启动应用失败 {}：{error}", app.name);
                Err(error)
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn open_application(path: &Path) -> Result<(), AppError> {
    let status = std::process::Command::new("open")
        .arg(path)
        .status()
        .map_err(|error| AppError::Launcher(format!("应用启动失败：{error}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(AppError::Launcher("应用启动失败".into()))
    }
}

#[cfg(not(target_os = "macos"))]
fn open_application(_path: &Path) -> Result<(), AppError> {
    Err(AppError::Launcher("当前平台不是 macOS".into()))
}

#[cfg(test)]
mod tests {
    use super::MacOsLauncher;
    use crate::database::models::Application;
    use crate::launcher::ApplicationLauncher;

    #[test]
    fn missing_bundle_does_not_launch() {
        let error = MacOsLauncher
            .launch(&Application {
                id: "test".into(),
                name: "Missing".into(),
                path: "/Applications/Missing Quka.app".into(),
                bundle_id: None,
                icon: None,
                source: "applications".into(),
                launch_count: 0,
                last_launch_time: None,
            })
            .unwrap_err();
        assert!(error.to_string().contains("路径不存在"));
    }
}

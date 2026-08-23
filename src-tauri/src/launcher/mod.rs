pub mod macos;
pub mod windows;

use crate::database::models::Application;
use crate::state::AppError;

pub trait ApplicationLauncher {
    fn launch(&self, app: &Application) -> Result<(), AppError>;
}

pub fn validate_path(path: &str) -> Result<std::path::PathBuf, AppError> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(AppError::Launcher("应用路径不存在".into()));
    }
    let path = std::path::PathBuf::from(trimmed);
    if !path.exists() {
        return Err(AppError::Launcher(format!(
            "应用启动失败：路径不存在。{}",
            path.display()
        )));
    }
    Ok(path)
}

pub fn launch(app: &Application) -> Result<(), AppError> {
    if windows::is_supported() {
        windows::WindowsLauncher.launch(app)
    } else if macos::is_supported() {
        macos::MacOsLauncher.launch(app)
    } else {
        Err(AppError::Launcher("当前平台暂不支持启动应用".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::validate_path;

    #[test]
    fn rejects_missing_path() {
        let error = validate_path(r"C:\definitely-missing-quka-app.exe").unwrap_err();
        assert!(error.to_string().contains("路径不存在"));
    }

    #[test]
    fn rejects_empty_path() {
        let error = validate_path("   ").unwrap_err();
        assert!(error.to_string().contains("路径不存在"));
    }

    #[cfg(windows)]
    #[test]
    fn accepts_existing_system_exe() {
        let path = validate_path(r"C:\Windows\System32\notepad.exe").unwrap();
        assert!(path.is_file());
    }
}

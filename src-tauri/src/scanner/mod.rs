pub mod macos;
pub mod sync;
pub mod watch;
pub mod windows;

use crate::database::models::Application;
use crate::state::AppError;

pub trait ApplicationScanner {
    fn scan(&self) -> Result<Vec<Application>, AppError>;
}

pub fn is_scan_supported() -> bool {
    macos::is_supported() || windows::is_supported()
}

pub fn current_scanner_name() -> &'static str {
    if macos::is_supported() {
        "macos"
    } else if windows::is_supported() {
        "windows"
    } else {
        "none"
    }
}

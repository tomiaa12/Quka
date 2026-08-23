pub mod macos;
pub mod windows;

use std::fs;
use std::path::PathBuf;

use tauri::AppHandle;

use crate::state::AppError;

pub fn icons_dir(app: &AppHandle) -> Result<PathBuf, AppError> {
    let dir = crate::database::app_data_dir(app)?.join("icons");
    fs::create_dir_all(&dir).map_err(|error| AppError::Io(error.to_string()))?;
    Ok(dir)
}

pub fn sanitize_file_stem(id: &str) -> String {
    let stem: String = id
        .chars()
        .map(|char| {
            if char.is_ascii_alphanumeric() || matches!(char, '.' | '-' | '_') {
                char
            } else {
                '_'
            }
        })
        .collect();
    if stem.is_empty() {
        "app".into()
    } else {
        stem
    }
}

#[cfg(test)]
mod tests {
    use super::sanitize_file_stem;

    #[test]
    fn keeps_bundle_ids() {
        assert_eq!(sanitize_file_stem("com.google.Chrome"), "com.google.Chrome");
    }

    #[test]
    fn replaces_unsafe_characters() {
        assert_eq!(sanitize_file_stem("a/b:c*d"), "a_b_c_d");
    }
}

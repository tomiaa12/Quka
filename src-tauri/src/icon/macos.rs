use std::fs::{self, File};
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use icns::IconFamily;

use super::sanitize_file_stem;
use crate::state::AppError;

pub fn cache_app_icon(
    app_id: &str,
    app_path: &Path,
    icon_file: Option<&str>,
    icon_dir: &Path,
) -> Option<String> {
    match try_cache_app_icon(app_id, app_path, icon_file, icon_dir) {
        Ok(path) => path,
        Err(error) => {
            log::warn!("图标读取失败 {}：{error}", app_path.display());
            None
        }
    }
}

fn try_cache_app_icon(
    app_id: &str,
    app_path: &Path,
    icon_file: Option<&str>,
    icon_dir: &Path,
) -> Result<Option<String>, AppError> {
    let Some(icns_path) = find_icns(app_path, icon_file) else {
        return Ok(None);
    };

    fs::create_dir_all(icon_dir).map_err(|error| AppError::Io(error.to_string()))?;
    let dest = icon_dir.join(format!("{}.png", sanitize_file_stem(app_id)));
    if cache_is_fresh(&dest, &icns_path) {
        return Ok(Some(dest.to_string_lossy().into_owned()));
    }

    convert_icns_to_png(&icns_path, &dest)?;
    Ok(Some(dest.to_string_lossy().into_owned()))
}

fn find_icns(app_path: &Path, icon_file: Option<&str>) -> Option<PathBuf> {
    let resources = app_path.join("Contents").join("Resources");
    if let Some(name) = icon_file.map(str::trim).filter(|name| !name.is_empty()) {
        let candidates = if name.to_ascii_lowercase().ends_with(".icns") {
            vec![resources.join(name)]
        } else {
            vec![resources.join(name), resources.join(format!("{name}.icns"))]
        };
        if let Some(path) = candidates.into_iter().find(|path| path.is_file()) {
            return Some(path);
        }
    }

    let entries = fs::read_dir(&resources).ok()?;
    let mut icns: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("icns"))
        })
        .collect();
    icns.sort();
    icns.into_iter().next()
}

fn cache_is_fresh(dest: &Path, source: &Path) -> bool {
    let Ok(dest_meta) = dest.metadata() else {
        return false;
    };
    let Ok(source_meta) = source.metadata() else {
        return false;
    };
    let dest_mtime = dest_meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
    let source_mtime = source_meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
    dest_mtime >= source_mtime
}

fn convert_icns_to_png(icns_path: &Path, dest: &Path) -> Result<(), AppError> {
    let file = File::open(icns_path).map_err(|error| AppError::Io(error.to_string()))?;
    let family = IconFamily::read(BufReader::new(file))
        .map_err(|error| AppError::Io(format!("无法解析 icns：{error}")))?;

    let mut best: Option<icns::Image> = None;
    for icon_type in family.available_icons() {
        let Ok(image) = family.get_icon_with_type(icon_type) else {
            continue;
        };
        let better = best
            .as_ref()
            .is_none_or(|current| image.width() * image.height() > current.width() * current.height());
        if better {
            best = Some(image);
        }
    }

    let image = best.ok_or_else(|| AppError::Io("icns 中没有可用图标".into()))?;
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|error| AppError::Io(error.to_string()))?;
    }
    let output = File::create(dest).map_err(|error| AppError::Io(error.to_string()))?;
    image
        .write_png(output)
        .map_err(|error| AppError::Io(format!("写入图标缓存失败：{error}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{convert_icns_to_png, find_icns};
    use icns::{IconFamily, IconType, Image, PixelFormat};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("quka-icon-{name}-{nanos}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_icns(path: &std::path::Path) {
        let mut family = IconFamily::new();
        let data = vec![0u8; 32 * 32 * 4];
        let image = Image::from_data(PixelFormat::RGBA, 32, 32, data).unwrap();
        family.add_icon_with_type(&image, IconType::RGBA32_32x32).unwrap();
        family.write(fs::File::create(path).unwrap()).unwrap();
    }

    #[test]
    fn finds_icon_file_from_plist_name() {
        let root = temp_dir("find");
        let resources = root.join("Contents").join("Resources");
        fs::create_dir_all(&resources).unwrap();
        let icns = resources.join("AppIcon.icns");
        write_icns(&icns);
        assert_eq!(find_icns(&root, Some("AppIcon")), Some(icns));
    }

    #[test]
    fn converts_icns_to_png() {
        let root = temp_dir("convert");
        let icns = root.join("AppIcon.icns");
        let png = root.join("out.png");
        write_icns(&icns);
        convert_icns_to_png(&icns, &png).unwrap();
        assert!(png.is_file());
        assert!(fs::metadata(&png).unwrap().len() > 0);
    }
}

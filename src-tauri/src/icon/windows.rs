use std::fs;
use std::path::Path;
use std::time::SystemTime;

use super::sanitize_file_stem;
use crate::state::AppError;

pub fn cache_file_icon(app_id: &str, file_path: &Path, icon_dir: &Path) -> Option<String> {
    match try_cache_file_icon(app_id, file_path, icon_dir) {
        Ok(path) => path,
        Err(error) => {
            log::warn!("图标读取失败 {}：{error}", file_path.display());
            None
        }
    }
}

fn try_cache_file_icon(
    app_id: &str,
    file_path: &Path,
    icon_dir: &Path,
) -> Result<Option<String>, AppError> {
    if !file_path.is_file() {
        return Ok(None);
    }

    fs::create_dir_all(icon_dir).map_err(|error| AppError::Io(error.to_string()))?;
    let dest = icon_dir.join(format!("{}.png", sanitize_file_stem(app_id)));
    if cache_is_fresh(&dest, file_path) {
        return Ok(Some(dest.to_string_lossy().into_owned()));
    }

    extract_file_icon(file_path, &dest)?;
    if dest.is_file() {
        Ok(Some(dest.to_string_lossy().into_owned()))
    } else {
        Ok(None)
    }
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

#[cfg(windows)]
fn extract_file_icon(file_path: &Path, dest: &Path) -> Result<(), AppError> {
    windows_extract::extract_to_png(file_path, dest)
}

#[cfg(not(windows))]
fn extract_file_icon(_file_path: &Path, _dest: &Path) -> Result<(), AppError> {
    Err(AppError::Io("当前平台不支持提取 Windows 图标".into()))
}

#[cfg(windows)]
mod windows_extract {
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use std::ptr;

    use windows::core::PCWSTR;
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC,
        BITMAP, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, BI_RGB,
    };
    use windows::Win32::UI::Shell::{
        ExtractIconExW, SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON,
    };
    use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO};

    use crate::state::AppError;

    pub fn extract_to_png(file_path: &Path, dest: &Path) -> Result<(), AppError> {
        let wide: Vec<u16> = file_path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let mut large = HICON::default();
        let count = unsafe {
            ExtractIconExW(
                PCWSTR(wide.as_ptr()),
                0,
                Some(&mut large),
                None,
                1,
            )
        };
        if count == 0 || large.is_invalid() {
            large = icon_from_shell(&wide)?;
        }

        let result = hicon_to_png(large, dest);
        unsafe {
            let _ = DestroyIcon(large);
        }
        result
    }

    fn icon_from_shell(wide: &[u16]) -> Result<HICON, AppError> {
        let mut info = SHFILEINFOW::default();
        let result = unsafe {
            SHGetFileInfoW(
                PCWSTR(wide.as_ptr()),
                Default::default(),
                Some(&mut info),
                std::mem::size_of::<SHFILEINFOW>() as u32,
                SHGFI_ICON | SHGFI_LARGEICON,
            )
        };
        if result == 0 || info.hIcon.is_invalid() {
            return Err(AppError::Io("没有可提取的图标".into()));
        }
        Ok(info.hIcon)
    }

    fn hicon_to_png(icon: HICON, dest: &Path) -> Result<(), AppError> {
        unsafe {
            let mut info = ICONINFO::default();
            GetIconInfo(icon, &mut info).map_err(|error| AppError::Io(error.to_string()))?;
            if info.hbmColor.is_invalid() {
                let _ = DeleteObject(info.hbmMask.into());
                return Err(AppError::Io("图标位图无效".into()));
            }

            let mut bitmap = BITMAP::default();
            if GetObjectW(
                info.hbmColor.into(),
                std::mem::size_of::<BITMAP>() as i32,
                Some(ptr::from_mut(&mut bitmap).cast()),
            ) == 0
            {
                let _ = DeleteObject(info.hbmColor.into());
                let _ = DeleteObject(info.hbmMask.into());
                return Err(AppError::Io("读取图标位图失败".into()));
            }

            let width = bitmap.bmWidth;
            let height = bitmap.bmHeight.unsigned_abs() as i32;
            if width <= 0 || height <= 0 {
                let _ = DeleteObject(info.hbmColor.into());
                let _ = DeleteObject(info.hbmMask.into());
                return Err(AppError::Io("图标尺寸无效".into()));
            }

            let hdc_screen = GetDC(None);
            let hdc = CreateCompatibleDC(Some(hdc_screen));
            let header = BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0 as u32,
                ..Default::default()
            };
            let mut info_header = BITMAPINFO {
                bmiHeader: header,
                ..Default::default()
            };
            let mut pixels = vec![0u8; (width * height * 4) as usize];
            let copied = GetDIBits(
                hdc,
                info.hbmColor,
                0,
                height as u32,
                Some(pixels.as_mut_ptr().cast()),
                &mut info_header,
                DIB_RGB_COLORS,
            );

            let _ = DeleteDC(hdc);
            ReleaseDC(None, hdc_screen);
            let _ = DeleteObject(info.hbmColor.into());
            let _ = DeleteObject(info.hbmMask.into());

            if copied == 0 {
                return Err(AppError::Io("导出图标像素失败".into()));
            }

            for pixel in pixels.chunks_exact_mut(4) {
                pixel.swap(0, 2);
            }

            image::save_buffer(
                dest,
                &pixels,
                width as u32,
                height as u32,
                image::ExtendedColorType::Rgba8,
            )
            .map_err(|error| AppError::Io(error.to_string()))?;
            let _ = header;
            Ok(())
        }
    }
}

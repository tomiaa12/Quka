use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use lnk::encoding::WINDOWS_1252;
use lnk::ShellLink;

use super::ApplicationScanner;
use crate::database::id::{generate_id, normalize_path};
use crate::database::models::Application;
use crate::icon::windows::cache_file_icon;
use crate::state::AppError;

const MAX_PROGRAM_DEPTH: usize = 4;

const SKIP_DIR_NAMES: &[&str] = &[
    "windowsapps",
    "winsxs",
    "node_modules",
    "uninstall",
    "update",
    "updates",
    "updater",
    "crashpad",
    "redist",
    "redistributable",
    "redistributables",
    "temp",
    "tmp",
    "$recycle.bin",
    "bin",
    "tools",
    "sdk",
    "shared",
    "cli-plugins",
    "jbr",
    "testhost",
    "packages",
    "common files",
    "windows kits",
    "dotnet",
    "msbuild",
];

const SKIP_EXE_STEMS: &[&str] = &[
    "uninstall",
    "uninst",
    "unins000",
    "update",
    "updater",
    "helper",
    "crash-handler",
    "crashhandler",
    "crashpad",
    "setup",
    "installer",
];

pub const WINDOWS_SOURCES: &[&str] = &[
    "start-menu",
    "program-files",
    "local-programs",
    "desktop",
    "start-apps",
];

const BROWSER_EXE_STEMS: &[&str] = &[
    "chrome",
    "chrome_proxy",
    "msedge",
    "msedge_proxy",
    "brave",
    "brave_proxy",
    "chromium",
    "vivaldi",
    "opera",
];

/// Known-folder GUIDs that appear in Start menu AppIDs (e.g. `{7C5A40EF-...}\Adobe\...`).
const KNOWN_FOLDER_GUIDS: &[(&str, &str)] = &[
    ("{6D809377-6AF0-444B-8957-A3773F02200E}", "ProgramW6432"),
    ("{7C5A40EF-A0FB-4BFC-874A-C0F2E0B9FA8E}", "ProgramFiles(x86)"),
    ("{F38BF404-1D43-42F2-9305-67DE0B28FC23}", "windir"),
    ("{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}", "SystemRoot"),
    ("{D65231B0-B2F1-4857-A4CE-A8E7C6EA7D27}", "SystemRoot"),
];

pub struct WindowsScanner {
    icon_dir: PathBuf,
    start_menu_dirs: Vec<PathBuf>,
    program_dirs: Vec<PathBuf>,
    desktop_dirs: Vec<PathBuf>,
}

impl WindowsScanner {
    pub fn new(icon_dir: PathBuf) -> Self {
        Self {
            icon_dir,
            start_menu_dirs: default_start_menu_dirs(),
            program_dirs: default_program_dirs(),
            desktop_dirs: default_desktop_dirs(),
        }
    }

    pub fn with_directories(
        icon_dir: PathBuf,
        start_menu_dirs: Vec<PathBuf>,
        program_dirs: Vec<PathBuf>,
    ) -> Self {
        Self {
            icon_dir,
            start_menu_dirs,
            program_dirs,
            desktop_dirs: Vec::new(),
        }
    }

    pub fn with_desktop(mut self, desktop_dirs: Vec<PathBuf>) -> Self {
        self.desktop_dirs = desktop_dirs;
        self
    }

    pub fn scan_directories(&self) -> Result<Vec<Application>, AppError> {
        let mut apps = Vec::new();

        for directory in &self.start_menu_dirs {
            collect_shortcuts(directory, "start-menu", &mut apps);
        }
        for directory in &self.desktop_dirs {
            collect_shortcuts(directory, "desktop", &mut apps);
            let mut exes = Vec::new();
            collect_files(directory, 0, 1, "exe", &mut exes);
            for exe in exes {
                match read_executable(&exe, "desktop") {
                    Ok(Some(app)) => apps.push(app),
                    Ok(None) => {}
                    Err(error) => log::warn!("跳过桌面程序 {}：{error}", exe.display()),
                }
            }
        }

        let mut exes = Vec::new();
        for directory in &self.program_dirs {
            collect_files(directory, 0, MAX_PROGRAM_DEPTH, "exe", &mut exes);
        }
        for exe in pick_main_exes(exes) {
            match read_executable(&exe, source_for_program(&exe)) {
                Ok(Some(app)) => apps.push(app),
                Ok(None) => {}
                Err(error) => log::warn!("跳过程序 {}：{error}", exe.display()),
            }
        }

        match collect_start_apps(&apps) {
            Ok(extra) => apps.extend(extra),
            Err(error) => log::warn!("扫描开始菜单应用列表失败：{error}"),
        }

        Ok(attach_icons(dedup_apps(apps), &self.icon_dir))
    }
}

impl ApplicationScanner for WindowsScanner {
    fn scan(&self) -> Result<Vec<Application>, AppError> {
        self.scan_directories()
    }
}

pub fn is_supported() -> bool {
    cfg!(target_os = "windows")
}

pub fn watch_directories() -> Vec<(PathBuf, bool)> {
    default_start_menu_dirs()
        .into_iter()
        .chain(default_desktop_dirs())
        .map(|path| (path, true))
        .chain(default_program_dirs().into_iter().map(|path| (path, true)))
        .collect()
}

pub(crate) fn default_start_menu_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(appdata) = std::env::var("APPDATA") {
        dirs.push(
            PathBuf::from(appdata)
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs"),
        );
    }
    if let Ok(programdata) = std::env::var("PROGRAMDATA") {
        dirs.push(
            PathBuf::from(programdata)
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs"),
        );
    }
    dirs
}

pub(crate) fn default_program_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(program_files) = std::env::var("ProgramW6432") {
        dirs.push(PathBuf::from(program_files));
    } else if let Ok(program_files) = std::env::var("ProgramFiles") {
        dirs.push(PathBuf::from(program_files));
    } else {
        dirs.push(PathBuf::from(r"C:\Program Files"));
    }
    if let Ok(program_files_x86) = std::env::var("ProgramFiles(x86)") {
        dirs.push(PathBuf::from(program_files_x86));
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        dirs.push(PathBuf::from(local).join("Programs"));
    }
    dirs
}

pub(crate) fn default_desktop_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(user) = std::env::var("USERPROFILE") {
        dirs.push(PathBuf::from(user).join("Desktop"));
    }
    if let Ok(public) = std::env::var("PUBLIC") {
        dirs.push(PathBuf::from(public).join("Desktop"));
    }
    dirs
}

fn collect_shortcuts(directory: &Path, source: &str, apps: &mut Vec<Application>) {
    let mut links = Vec::new();
    collect_files(directory, 0, 12, "lnk", &mut links);
    for link in links {
        match read_shortcut(&link, source) {
            Ok(Some(app)) => apps.push(app),
            Ok(None) => {}
            Err(error) => log::warn!("跳过快捷方式 {}：{error}", link.display()),
        }
    }
}

fn collect_files(dir: &Path, depth: usize, max_depth: usize, extension: &str, out: &mut Vec<PathBuf>) {
    if depth > max_depth {
        return;
    }
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) => {
            log::warn!("扫描目录不存在或无法读取 {}：{error}", dir.display());
            return;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(_) => continue,
        };
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            if is_skipped_dir(&path) {
                continue;
            }
            collect_files(&path, depth + 1, max_depth, extension, out);
            continue;
        }
        if has_extension(&path, extension) {
            out.push(path);
        }
    }
}

pub fn is_skipped_dir(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| SKIP_DIR_NAMES.contains(&name.to_ascii_lowercase().as_str()))
}

pub fn is_skipped_exe(path: &Path) -> bool {
    let Some(stem) = path.file_stem().and_then(|name| name.to_str()) else {
        return true;
    };
    let stem = stem.to_ascii_lowercase();
    SKIP_EXE_STEMS
        .iter()
        .any(|item| stem == *item || stem.starts_with(&format!("{item}_")) || stem.ends_with(&format!("_{item}")))
        || stem.contains("uninstall")
        || stem.contains("unins00")
}

fn is_skipped_shortcut_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.contains("uninstall") || lower.contains("卸载")
}

fn has_extension(path: &Path, extension: &str) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case(extension))
}

fn read_shortcut(link_path: &Path, source: &str) -> Result<Option<Application>, AppError> {
    let name = link_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("Untitled")
        .to_string();
    if is_skipped_shortcut_name(&name) {
        return Ok(None);
    }

    let shortcut = match ShellLink::open(link_path, WINDOWS_1252) {
        Ok(shortcut) => shortcut,
        Err(error) => {
            log::warn!(
                "快捷方式解析失败，按文件名收录 {}：{error}",
                link_path.display()
            );
            return Ok(Some(app_from_link_file(link_path, &name, source, None)));
        }
    };
    let target = shortcut
        .link_target()
        .or_else(|| shortcut.string_data().relative_path().clone());
    let Some(target) = target.filter(|value| !value.trim().is_empty()) else {
        return Ok(Some(app_from_link_file(link_path, &name, source, None)));
    };

    let target_path = PathBuf::from(&target);
    if is_skipped_exe(&target_path) {
        return Ok(None);
    }

    let args = shortcut
        .string_data()
        .command_line_arguments()
        .clone()
        .unwrap_or_default();
    // Browser-hosted PWAs must keep the .lnk so ShellExecute preserves --app-id args.
    if is_browser_exe(&target_path) && is_pwa_arguments(&args) {
        let bundle_id = browser_pwa_bundle_id(&target_path, &args);
        return Ok(Some(app_from_link_file(
            link_path,
            &name,
            source,
            bundle_id.as_deref(),
        )));
    }

    if target_path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
        && !target_path.is_file()
    {
        return Ok(Some(app_from_link_file(link_path, &name, source, None)));
    }

    let path = if target_path.is_file() {
        target_path.to_string_lossy().into_owned()
    } else {
        link_path.to_string_lossy().into_owned()
    };

    Ok(Some(Application {
        id: generate_id(&path, None),
        name,
        path,
        bundle_id: None,
        icon: None,
        source: source.into(),
        launch_count: 0,
        last_launch_time: None,
        aliases: String::new(),
    }))
}

fn app_from_link_file(
    link_path: &Path,
    name: &str,
    source: &str,
    bundle_id: Option<&str>,
) -> Application {
    let path = link_path.to_string_lossy().into_owned();
    Application {
        id: generate_id(&path, bundle_id),
        name: name.to_string(),
        path,
        bundle_id: bundle_id.map(str::to_string),
        icon: None,
        source: source.into(),
        launch_count: 0,
        last_launch_time: None,
        aliases: String::new(),
    }
}

fn is_browser_exe(path: &Path) -> bool {
    path.file_stem()
        .and_then(|name| name.to_str())
        .is_some_and(|stem| {
            BROWSER_EXE_STEMS
                .iter()
                .any(|item| stem.eq_ignore_ascii_case(item))
        })
}

fn is_pwa_arguments(args: &str) -> bool {
    let lower = args.to_ascii_lowercase();
    lower.contains("--app-id=")
        || lower.contains("--app-id ")
        || lower.contains("--app=")
        || lower.contains("--app ")
}

fn extract_flag_value(args: &str, flag: &str) -> Option<String> {
    let lower = args.to_ascii_lowercase();
    let flag_lower = flag.to_ascii_lowercase();
    let rest = if let Some(idx) = lower.find(&format!("{flag_lower}=")) {
        &args[idx + flag.len() + 1..]
    } else if let Some(idx) = lower.find(&format!("{flag_lower} ")) {
        &args[idx + flag.len() + 1..]
    } else {
        return None;
    };
    let value = rest
        .trim_start()
        .split_whitespace()
        .next()
        .unwrap_or("")
        .trim_matches('"')
        .trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn browser_pwa_bundle_id(exe: &Path, args: &str) -> Option<String> {
    let app_id = extract_flag_value(args, "--app-id")?;
    let stem = exe
        .file_stem()
        .and_then(|name| name.to_str())?
        .to_ascii_lowercase();
    let prefix = match stem.as_str() {
        "chrome" | "chrome_proxy" | "chromium" => "Chrome._crx_",
        "msedge" | "msedge_proxy" => "MSEdge._crx_",
        "brave" | "brave_proxy" => "Brave._crx_",
        _ => return None,
    };
    Some(format!("{prefix}{app_id}"))
}

fn collect_start_apps(existing: &[Application]) -> Result<Vec<Application>, AppError> {
    #[cfg(windows)]
    {
        let entries = query_start_apps()?;
        let mut seen_paths: HashSet<String> = existing
            .iter()
            .map(|app| normalize_path(&app.path))
            .collect();
        let mut seen_ids: HashSet<String> = existing.iter().map(|app| app.id.clone()).collect();
        let mut apps = Vec::new();

        for entry in entries {
            let name = entry.name.trim();
            let app_id = entry.app_id.trim();
            if name.is_empty() || app_id.is_empty() || is_skipped_shortcut_name(name) {
                continue;
            }
            if seen_ids.contains(app_id) {
                continue;
            }

            if let Some(resolved) = resolve_start_app_path(app_id) {
                if is_skipped_exe(&resolved) {
                    continue;
                }
                let path = resolved.to_string_lossy().into_owned();
                let key = normalize_path(&path);
                if !seen_paths.insert(key) {
                    continue;
                }
                let id = generate_id(&path, None);
                if !seen_ids.insert(id.clone()) {
                    continue;
                }
                apps.push(Application {
                    id,
                    name: name.to_string(),
                    path,
                    bundle_id: None,
                    icon: None,
                    source: "start-apps".into(),
                    launch_count: 0,
                    last_launch_time: None,
                    aliases: String::new(),
                });
                continue;
            }

            // Store / UWP / PWA / unresolved Start AppIDs launch via shell:AppsFolder.
            let path = format!("shell:AppsFolder\\{app_id}");
            let key = normalize_path(&path);
            if !seen_paths.insert(key) {
                continue;
            }
            if !seen_ids.insert(app_id.to_string()) {
                continue;
            }
            apps.push(Application {
                id: generate_id(&path, Some(app_id)),
                name: name.to_string(),
                path,
                bundle_id: Some(app_id.to_string()),
                icon: None,
                source: "start-apps".into(),
                launch_count: 0,
                last_launch_time: None,
                aliases: String::new(),
            });
        }

        Ok(apps)
    }
    #[cfg(not(windows))]
    {
        let _ = existing;
        Ok(Vec::new())
    }
}

#[derive(Debug, serde::Deserialize)]
struct StartAppEntry {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "AppID")]
    app_id: String,
}

#[cfg(windows)]
fn query_start_apps() -> Result<Vec<StartAppEntry>, AppError> {
    let output = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            "Get-StartApps | ConvertTo-Json -Compress",
        ])
        .output()
        .map_err(|error| AppError::Io(format!("无法执行 Get-StartApps：{error}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::Io(format!(
            "Get-StartApps 失败：{}",
            stderr.trim()
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let trimmed = stdout.trim();
    if trimmed.is_empty() || trimmed == "null" {
        return Ok(Vec::new());
    }

    if trimmed.starts_with('[') {
        serde_json::from_str(trimmed)
            .map_err(|error| AppError::Io(format!("解析 Get-StartApps JSON 失败：{error}")))
    } else {
        let single: StartAppEntry = serde_json::from_str(trimmed)
            .map_err(|error| AppError::Io(format!("解析 Get-StartApps JSON 失败：{error}")))?;
        Ok(vec![single])
    }
}

fn is_shell_start_app_id(app_id: &str) -> bool {
    let id = app_id.trim();
    if id.is_empty() {
        return false;
    }
    if id.contains('!') || id.contains("._crx_") {
        return true;
    }
    // Hosted app / protocol-style IDs that are not filesystem paths.
    if looks_like_filesystem_app_id(id) {
        return false;
    }
    true
}

fn looks_like_filesystem_app_id(app_id: &str) -> bool {
    let id = app_id.trim();
    if id.len() >= 3 {
        let bytes = id.as_bytes();
        if bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && (bytes[2] == b'\\' || bytes[2] == b'/')
        {
            return true;
        }
    }
    id.starts_with("\\\\") || id.starts_with('{') || id.to_ascii_lowercase().ends_with(".exe")
}

fn resolve_start_app_path(app_id: &str) -> Option<PathBuf> {
    let id = app_id.trim();
    if id.is_empty() {
        return None;
    }

    if id.len() >= 3 {
        let bytes = id.as_bytes();
        if bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && (bytes[2] == b'\\' || bytes[2] == b'/')
        {
            let path = PathBuf::from(id);
            return path.is_file().then_some(path);
        }
    }
    if id.starts_with("\\\\") {
        let path = PathBuf::from(id);
        return path.is_file().then_some(path);
    }

    if let Some(path) = resolve_known_folder_app_id(id) {
        return path.is_file().then_some(path);
    }

    None
}

fn resolve_known_folder_app_id(app_id: &str) -> Option<PathBuf> {
    let end = app_id.find('}')?;
    if !app_id.starts_with('{') {
        return None;
    }
    let guid = &app_id[..=end];
    let rest = app_id[end + 1..].trim_start_matches(['\\', '/']);
    let env_key = KNOWN_FOLDER_GUIDS
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(guid))
        .map(|(_, key)| *key)?;
    let base = if env_key == "SystemRoot" || env_key == "windir" {
        std::env::var("SystemRoot")
            .or_else(|_| std::env::var("windir"))
            .ok()
            .map(PathBuf::from)?
    } else {
        std::env::var(env_key).ok().map(PathBuf::from)?
    };
    let path = if env_key == "SystemRoot" && guid.eq_ignore_ascii_case("{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}")
    {
        base.join("System32").join(rest)
    } else if env_key == "SystemRoot"
        && guid.eq_ignore_ascii_case("{D65231B0-B2F1-4857-A4CE-A8E7C6EA7D27}")
    {
        base.join("SysWOW64").join(rest)
    } else if env_key == "windir" {
        base.join(rest)
    } else {
        base.join(rest)
    };
    Some(path)
}

fn read_executable(exe_path: &Path, source: &str) -> Result<Option<Application>, AppError> {
    if is_skipped_exe(exe_path) || !exe_path.is_file() {
        return Ok(None);
    }
    let name = exe_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("Untitled")
        .to_string();
    let path = exe_path.to_string_lossy().into_owned();
    let id = generate_id(&path, None);
    Ok(Some(Application {
        id,
        name,
        path,
        bundle_id: None,
        icon: None,
        source: source.into(),
        launch_count: 0,
        last_launch_time: None,
        aliases: String::new(),
    }))
}

fn source_for_program(path: &Path) -> &'static str {
    let normalized = normalize_path(&path.to_string_lossy());
    if normalized.contains("/appdata/local/programs") {
        "local-programs"
    } else {
        "program-files"
    }
}

fn exe_matches_folder(path: &Path, folder: &str) -> bool {
    let Some(stem) = path.file_stem().and_then(|name| name.to_str()) else {
        return false;
    };
    let stem = stem.to_ascii_lowercase();
    !folder.is_empty() && (stem == folder || folder.contains(&stem) || stem.contains(folder))
}

fn pick_main_exes(exes: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut grouped: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();
    for exe in exes {
        if is_skipped_exe(&exe) {
            continue;
        }
        let parent = exe.parent().unwrap_or(Path::new("")).to_path_buf();
        grouped.entry(parent).or_default().push(exe);
    }

    let mut picked = Vec::new();
    for (parent, files) in grouped {
        let folder = parent
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let matched: Vec<PathBuf> = files
            .iter()
            .filter(|path| exe_matches_folder(path, &folder))
            .cloned()
            .collect();
        picked.extend(matched);
    }
    picked
}

fn attach_icons(mut apps: Vec<Application>, icon_dir: &Path) -> Vec<Application> {
    for app in &mut apps {
        app.icon = cache_file_icon(&app.id, Path::new(&app.path), icon_dir);
    }
    apps
}

fn dedup_apps(apps: Vec<Application>) -> Vec<Application> {
    let mut seen_paths = HashSet::new();
    let mut seen_ids = HashSet::new();
    let mut unique = Vec::new();
    for app in apps {
        let path_key = normalize_path(&app.path);
        if !seen_paths.insert(path_key) || !seen_ids.insert(app.id.clone()) {
            continue;
        }
        unique.push(app);
    }
    unique
}

#[cfg(test)]
mod tests {
    use super::{
        browser_pwa_bundle_id, collect_files, dedup_apps, extract_flag_value, is_browser_exe,
        is_pwa_arguments, is_shell_start_app_id, is_skipped_dir, is_skipped_exe,
        is_skipped_shortcut_name, looks_like_filesystem_app_id, pick_main_exes,
        resolve_known_folder_app_id, source_for_program, watch_directories, WindowsScanner,
    };
    use crate::database::models::Application;
    use crate::scanner::ApplicationScanner;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("quka-win-{name}-{nanos}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn touch(path: &Path) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, b"fake").unwrap();
    }

    #[test]
    fn skips_helper_executables() {
        assert!(is_skipped_exe(Path::new("uninstall.exe")));
        assert!(is_skipped_exe(Path::new("Unins000.exe")));
        assert!(is_skipped_exe(Path::new("update.exe")));
        assert!(is_skipped_exe(Path::new("helper.exe")));
        assert!(is_skipped_exe(Path::new("crash-handler.exe")));
        assert!(is_skipped_exe(Path::new("Chrome Uninstall.exe")));
        assert!(!is_skipped_exe(Path::new("chrome.exe")));
        assert!(!is_skipped_exe(Path::new("Code.exe")));
    }

    #[test]
    fn skips_uninstall_shortcuts() {
        assert!(is_skipped_shortcut_name("Uninstall Google Chrome"));
        assert!(is_skipped_shortcut_name("卸载微信"));
        assert!(!is_skipped_shortcut_name("Google Chrome"));
    }

    #[test]
    fn skips_internal_directories() {
        assert!(is_skipped_dir(&Path::new("App").join("uninstall")));
        assert!(is_skipped_dir(&Path::new("Program Files").join("WindowsApps")));
        assert!(!is_skipped_dir(&Path::new("Program Files").join("Google")));
    }

    #[test]
    fn detects_pwa_browser_shortcuts() {
        assert!(is_browser_exe(Path::new(
            r"C:\Program Files\Google\Chrome\Application\chrome.exe"
        )));
        assert!(is_pwa_arguments("--app-id=cadlkienfkbeoongdcgmdikeeg"));
        assert!(is_pwa_arguments("--profile-directory=Default --app=https://chatgpt.com"));
        assert!(!is_pwa_arguments("--profile-directory=Default"));
        assert_eq!(
            browser_pwa_bundle_id(
                Path::new(r"C:\Program Files\Google\Chrome\Application\chrome.exe"),
                "--app-id=cadlkienfkbeoongdcgmdikeeg"
            )
            .as_deref(),
            Some("Chrome._crx_cadlkienfkbeoongdcgmdikeeg")
        );
        assert_eq!(
            extract_flag_value("--app-id=abc123 --profile-directory=Default", "--app-id")
                .as_deref(),
            Some("abc123")
        );
    }

    #[test]
    fn classifies_start_app_ids() {
        assert!(is_shell_start_app_id("Claude_pzs8sxrjxfjjc!Claude"));
        assert!(is_shell_start_app_id("OpenAI.Codex_2p2nqsd0c76g0!App"));
        assert!(is_shell_start_app_id("Chrome._crx_cadlkienfkbeoongdcgmdikeeg"));
        assert!(!is_shell_start_app_id(r"D:\7-zip\7zFM.exe"));
        assert!(looks_like_filesystem_app_id(
            r"{7C5A40EF-A0FB-4BFC-874A-C0F2E0B9FA8E}\Adobe\Acrobat DC\Acrobat\Acrobat.exe"
        ));
    }

    #[test]
    fn resolves_program_files_guid_app_id() {
        let path = resolve_known_folder_app_id(
            r"{6D809377-6AF0-444B-8957-A3773F02200E}\Quka\Quka.exe",
        )
        .expect("guid should map");
        let rendered = path.to_string_lossy().to_ascii_lowercase();
        assert!(
            rendered.ends_with(r"\quka\quka.exe") || rendered.ends_with("/quka/quka.exe"),
            "{rendered}"
        );
    }

    #[test]
    fn collects_lnk_and_exe() {
        let root = temp_dir("collect");
        touch(&root.join("Programs").join("Chrome.lnk"));
        touch(&root.join("Programs").join("Tools").join("help.txt"));
        touch(&root.join("App").join("chrome.exe"));
        touch(&root.join("App").join("uninstall.exe"));

        let mut links = Vec::new();
        collect_files(&root.join("Programs"), 0, 4, "lnk", &mut links);
        assert_eq!(links.len(), 1);

        let mut exes = Vec::new();
        collect_files(&root.join("App"), 0, 4, "exe", &mut exes);
        assert_eq!(exes.len(), 2);
    }

    #[test]
    fn prefers_folder_named_exe() {
        let root = temp_dir("pick");
        let chrome = root.join("Chrome").join("chrome.exe");
        let helper = root.join("Chrome").join("notifier.exe");
        touch(&chrome);
        touch(&helper);
        let picked = pick_main_exes(vec![helper, chrome.clone()]);
        assert_eq!(picked, vec![chrome]);
    }

    #[test]
    fn ignores_unrelated_single_exe() {
        let root = temp_dir("tool");
        let tool = root.join("Something").join("curl.exe");
        touch(&tool);
        assert!(pick_main_exes(vec![tool]).is_empty());
    }

    #[test]
    fn dedups_same_target_path() {
        let apps = vec![
            Application {
                id: "1".into(),
                name: "Chrome".into(),
                path: r"C:\Program Files\Google\Chrome\Application\chrome.exe".into(),
                bundle_id: None,
                icon: None,
                source: "start-menu".into(),
                launch_count: 0,
                last_launch_time: None,
                aliases: String::new(),
            },
            Application {
                id: "2".into(),
                name: "chrome".into(),
                path: r"c:\program files\google\chrome\application\chrome.exe".into(),
                bundle_id: None,
                icon: None,
                source: "program-files".into(),
                launch_count: 0,
                last_launch_time: None,
                aliases: String::new(),
            },
        ];
        let unique = dedup_apps(apps);
        assert_eq!(unique.len(), 1);
        assert_eq!(unique[0].source, "start-menu");
    }

    #[test]
    fn dedups_same_bundle_id() {
        let apps = vec![
            Application {
                id: "Chrome._crx_abc".into(),
                name: "ChatGPT".into(),
                path: r"C:\Users\Ada\AppData\Roaming\Microsoft\Windows\Start Menu\Programs\ChatGPT.lnk".into(),
                bundle_id: Some("Chrome._crx_abc".into()),
                icon: None,
                source: "start-menu".into(),
                launch_count: 0,
                last_launch_time: None,
                aliases: String::new(),
            },
            Application {
                id: "Chrome._crx_abc".into(),
                name: "ChatGPT".into(),
                path: r"shell:AppsFolder\Chrome._crx_abc".into(),
                bundle_id: Some("Chrome._crx_abc".into()),
                icon: None,
                source: "start-apps".into(),
                launch_count: 0,
                last_launch_time: None,
                aliases: String::new(),
            },
        ];
        let unique = dedup_apps(apps);
        assert_eq!(unique.len(), 1);
        assert_eq!(unique[0].source, "start-menu");
    }

    #[test]
    fn classifies_local_programs() {
        assert_eq!(
            source_for_program(Path::new(
                r"C:\Users\Ada\AppData\Local\Programs\cursor\Cursor.exe"
            )),
            "local-programs"
        );
        assert_eq!(
            source_for_program(Path::new(r"C:\Program Files\App\app.exe")),
            "program-files"
        );
    }

    #[test]
    fn watch_covers_start_menu_and_program_files() {
        let dirs = watch_directories();
        assert!(dirs.iter().all(|(_, recursive)| *recursive));
        if cfg!(target_os = "windows") {
            assert!(
                dirs.iter().any(|(path, _)| path
                    .to_string_lossy()
                    .contains("Start Menu")),
                "missing Start Menu watch: {dirs:?}"
            );
        }
    }

    #[test]
    fn scan_skips_missing_directories() {
        let root = temp_dir("missing");
        let scanner = WindowsScanner::with_directories(
            root.join("icons"),
            vec![root.join("missing-start")],
            vec![root.join("missing-pf")],
        );
        let apps = ApplicationScanner::scan(&scanner).unwrap();
        // On Windows, Get-StartApps may still return installed apps.
        if cfg!(target_os = "windows") {
            assert!(apps.iter().all(|app| {
                matches!(
                    app.source.as_str(),
                    "start-apps"
                        | "start-menu"
                        | "program-files"
                        | "local-programs"
                        | "desktop"
                )
            }));
        } else {
            assert!(apps.is_empty());
        }
    }

    #[test]
    fn scan_reads_program_files_exe() {
        let root = temp_dir("exe");
        let exe = root.join("Program Files").join("QukaTest").join("QukaTest.exe");
        touch(&exe);
        let scanner = WindowsScanner::with_directories(
            root.join("icons"),
            vec![root.join("Start Menu")],
            vec![root.join("Program Files")],
        );
        let apps = scanner.scan_directories().unwrap();
        assert!(
            apps.iter()
                .any(|app| app.name == "QukaTest" && app.source == "program-files"),
            "expected QukaTest in {apps:?}"
        );
    }

    #[cfg(windows)]
    #[test]
    fn parses_powershell_shortcut() {
        let root = temp_dir("lnk");
        let link = root.join("Notepad.lnk");
        let target = r"C:\Windows\System32\notepad.exe";
        let status = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!(
                    "$s = (New-Object -ComObject WScript.Shell).CreateShortcut('{}'); $s.TargetPath = '{}'; $s.Save()",
                    link.display(),
                    target
                ),
            ])
            .status()
            .unwrap();
        assert!(status.success());
        assert!(link.is_file());

        let scanner = WindowsScanner::with_directories(root.join("icons"), vec![root.clone()], vec![]);
        let apps = scanner.scan_directories().unwrap();
        assert!(
            apps.iter().any(|app| {
                app.name == "Notepad"
                    && app.source == "start-menu"
                    && app.path.to_ascii_lowercase().ends_with("notepad.exe")
            }),
            "expected Notepad shortcut in {apps:?}"
        );
        let notepad = apps.iter().find(|app| app.name == "Notepad").unwrap();
        assert!(notepad.icon.as_ref().is_some_and(|icon| Path::new(icon).is_file()));
    }

    #[cfg(windows)]
    #[test]
    fn scans_real_start_menu_and_programs() {
        let scanner = WindowsScanner::new(temp_dir("real-scan"));
        let apps = ApplicationScanner::scan(&scanner).unwrap();
        assert!(
            !apps.is_empty(),
            "Windows scanner should find installed applications"
        );
        assert!(apps.iter().any(|app| {
            matches!(
                app.source.as_str(),
                "start-menu" | "program-files" | "local-programs" | "desktop" | "start-apps"
            )
        }));
        assert!(apps.iter().any(|app| !app.name.is_empty() && !app.path.is_empty()));
        if Path::new(r"C:\Users\Public\Desktop\微信.lnk").is_file() {
            assert!(
                apps.iter().any(|app| app.name == "微信" && app.source == "desktop"),
                "public desktop WeChat shortcut should be indexed"
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn indexes_store_and_pwa_start_apps() {
        let scanner = WindowsScanner::with_directories(
            temp_dir("start-apps"),
            vec![],
            vec![],
        );
        let apps = scanner.scan_directories().unwrap();
        let chatgpt = apps
            .iter()
            .filter(|app| app.name.eq_ignore_ascii_case("ChatGPT"))
            .collect::<Vec<_>>();
        let claude = apps
            .iter()
            .filter(|app| app.name.eq_ignore_ascii_case("Claude"))
            .collect::<Vec<_>>();
        assert!(
            !chatgpt.is_empty(),
            "ChatGPT store/PWA entry should be indexed via Get-StartApps"
        );
        assert!(
            !claude.is_empty(),
            "Claude store entry should be indexed via Get-StartApps"
        );
        assert!(chatgpt.iter().any(|app| {
            app.path.to_ascii_lowercase().contains("shell:appsfolder\\")
                || app.bundle_id.as_deref().is_some_and(|id| id.contains("._crx_") || id.contains('!'))
        }));
        assert!(claude.iter().any(|app| {
            app.bundle_id
                .as_deref()
                .is_some_and(|id| id.contains('!'))
                || app.path.to_ascii_lowercase().contains("shell:appsfolder\\")
        }));
    }

    #[test]
    fn includes_unreadable_shortcut_by_name() {
        let root = temp_dir("badlnk");
        fs::write(root.join("微信.lnk"), b"not-a-real-shortcut").unwrap();
        let scanner = WindowsScanner::with_directories(root.join("icons"), vec![root.clone()], vec![]);
        let apps = scanner.scan_directories().unwrap();
        assert!(
            apps.iter()
                .any(|app| app.name == "微信" && app.path.ends_with("微信.lnk")),
            "expected WeChat bad shortcut in {apps:?}"
        );
    }

    #[test]
    fn scans_desktop_shortcuts() {
        let root = temp_dir("desktop");
        fs::write(root.join("微信.lnk"), b"not-a-real-shortcut").unwrap();
        let scanner = WindowsScanner::with_directories(root.join("icons"), vec![], vec![])
            .with_desktop(vec![root.clone()]);
        let apps = scanner.scan_directories().unwrap();
        assert!(
            apps.iter()
                .any(|app| app.name == "微信" && app.source == "desktop"),
            "expected desktop WeChat in {apps:?}"
        );
    }

    #[cfg(windows)]
    #[test]
    fn extracts_system_exe_icon() {
        let root = temp_dir("sysicon");
        let icon = crate::icon::windows::cache_file_icon(
            "notepad",
            Path::new(r"C:\Windows\System32\notepad.exe"),
            &root,
        );
        assert!(icon.as_ref().is_some_and(|path| Path::new(path).is_file()));
    }

    #[cfg(windows)]
    #[test]
    fn extracts_wechat_shortcut_icon() {
        let link = Path::new(r"C:\Users\Public\Desktop\微信.lnk");
        if !link.is_file() {
            return;
        }
        let root = temp_dir("wechat-icon");
        let icon = crate::icon::windows::cache_file_icon("wechat-lnk", link, &root);
        assert!(
            icon.as_ref().is_some_and(|path| Path::new(path).is_file()),
            "WeChat shortcut icon should be readable via the shell"
        );
    }

    #[cfg(windows)]
    #[test]
    fn extracts_shell_apps_folder_icon() {
        let root = temp_dir("shell-icon");
        let icon = crate::icon::windows::cache_file_icon(
            "claude-shell",
            Path::new(r"shell:AppsFolder\Claude_pzs8sxrjxfjjc!Claude"),
            &root,
        );
        assert!(
            icon.as_ref().is_some_and(|path| Path::new(path).is_file()),
            "shell:AppsFolder icons should be readable via SHGetFileInfo"
        );
    }
}

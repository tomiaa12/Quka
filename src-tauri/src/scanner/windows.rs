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

pub const WINDOWS_SOURCES: &[&str] = &["start-menu", "program-files", "local-programs", "desktop"];

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

        Ok(attach_icons(dedup_by_target(apps), &self.icon_dir))
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

fn default_start_menu_dirs() -> Vec<PathBuf> {
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

fn default_program_dirs() -> Vec<PathBuf> {
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

fn default_desktop_dirs() -> Vec<PathBuf> {
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
            return Ok(Some(app_from_link_file(link_path, &name, source)));
        }
    };
    let target = shortcut
        .link_target()
        .or_else(|| shortcut.string_data().relative_path().clone());
    let Some(target) = target.filter(|value| !value.trim().is_empty()) else {
        return Ok(Some(app_from_link_file(link_path, &name, source)));
    };

    let target_path = PathBuf::from(&target);
    if is_skipped_exe(&target_path) {
        return Ok(None);
    }
    if target_path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("exe")) && !target_path.is_file()
    {
        return Ok(Some(app_from_link_file(link_path, &name, source)));
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

fn app_from_link_file(link_path: &Path, name: &str, source: &str) -> Application {
    let path = link_path.to_string_lossy().into_owned();
    Application {
        id: generate_id(&path, None),
        name: name.to_string(),
        path,
        bundle_id: None,
        icon: None,
        source: source.into(),
        launch_count: 0,
        last_launch_time: None,
        aliases: String::new(),
    }
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

fn dedup_by_target(apps: Vec<Application>) -> Vec<Application> {
    let mut seen = HashSet::new();
    let mut unique = Vec::new();
    for app in apps {
        let key = normalize_path(&app.path);
        if !seen.insert(key) {
            continue;
        }
        unique.push(app);
    }
    unique
}

#[cfg(test)]
mod tests {
    use super::{
        collect_files, dedup_by_target, is_skipped_dir, is_skipped_exe, is_skipped_shortcut_name,
        pick_main_exes, source_for_program, WindowsScanner,
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
        assert!(is_skipped_dir(Path::new(r"C:\Program Files\App\uninstall")));
        assert!(is_skipped_dir(Path::new(r"C:\Program Files\WindowsApps")));
        assert!(!is_skipped_dir(Path::new(r"C:\Program Files\Google")));
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
        let unique = dedup_by_target(apps);
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
    fn scan_skips_missing_directories() {
        let root = temp_dir("missing");
        let scanner = WindowsScanner::with_directories(
            root.join("icons"),
            vec![root.join("missing-start")],
            vec![root.join("missing-pf")],
        );
        let apps = ApplicationScanner::scan(&scanner).unwrap();
        assert!(apps.is_empty());
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
        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].name, "QukaTest");
        assert_eq!(apps[0].source, "program-files");
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
        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].name, "Notepad");
        assert_eq!(apps[0].source, "start-menu");
        assert!(apps[0]
            .path
            .to_ascii_lowercase()
            .ends_with("notepad.exe"));
        assert!(apps[0].icon.as_ref().is_some_and(|icon| Path::new(icon).is_file()));
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
                "start-menu" | "program-files" | "local-programs" | "desktop"
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

    #[test]
    fn includes_unreadable_shortcut_by_name() {
        let root = temp_dir("badlnk");
        fs::write(root.join("微信.lnk"), b"not-a-real-shortcut").unwrap();
        let scanner = WindowsScanner::with_directories(root.join("icons"), vec![root.clone()], vec![]);
        let apps = scanner.scan_directories().unwrap();
        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].name, "微信");
        assert!(apps[0].path.ends_with("微信.lnk"));
        assert_eq!(apps[0].source, "start-menu");
    }

    #[test]
    fn scans_desktop_shortcuts() {
        let root = temp_dir("desktop");
        fs::write(root.join("微信.lnk"), b"not-a-real-shortcut").unwrap();
        let scanner = WindowsScanner::with_directories(root.join("icons"), vec![], vec![])
            .with_desktop(vec![root.clone()]);
        let apps = scanner.scan_directories().unwrap();
        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].name, "微信");
        assert_eq!(apps[0].source, "desktop");
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
}

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::ApplicationScanner;
use crate::database::id::{generate_id, normalize_path};
use crate::database::models::Application;
use crate::icon::macos::cache_app_icon;
use crate::state::AppError;

#[derive(Debug, Default, Deserialize)]
struct InfoPlist {
    #[serde(rename = "CFBundleDisplayName")]
    display_name: Option<String>,
    #[serde(rename = "CFBundleName")]
    name: Option<String>,
    #[serde(rename = "CFBundleIdentifier")]
    bundle_id: Option<String>,
    #[serde(rename = "CFBundleIconFile")]
    icon_file: Option<String>,
    #[serde(rename = "CFBundleIconName")]
    icon_name: Option<String>,
}

pub struct MacOsScanner {
    home_dir: PathBuf,
    icon_dir: PathBuf,
}

impl MacOsScanner {
    pub fn new(home_dir: PathBuf, icon_dir: PathBuf) -> Self {
        Self { home_dir, icon_dir }
    }

    pub fn default_directories(&self) -> Vec<PathBuf> {
        vec![
            PathBuf::from("/Applications"),
            self.home_dir.join("Applications"),
            PathBuf::from("/System/Applications"),
        ]
    }

    pub fn scan_directories(&self, directories: &[PathBuf]) -> Result<Vec<Application>, AppError> {
        let mut bundles = Vec::new();
        for directory in directories {
            collect_app_bundles(directory, &mut bundles);
        }

        let mut apps = Vec::new();
        for bundle in bundles {
            match read_application(&bundle, &self.icon_dir, &self.home_dir) {
                Ok(app) => apps.push(app),
                Err(error) => log::warn!("跳过应用 {}：{error}", bundle.display()),
            }
        }

        Ok(dedup_applications(apps))
    }
}

impl ApplicationScanner for MacOsScanner {
    fn scan(&self) -> Result<Vec<Application>, AppError> {
        self.scan_directories(&self.default_directories())
    }
}

pub fn is_supported() -> bool {
    cfg!(target_os = "macos")
}

fn collect_app_bundles(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) => {
            log::warn!("扫描目录不存在或无法读取 {}：{error}", dir.display());
            return;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if is_app_bundle(&path) {
            out.push(path);
            continue;
        }
        if path.is_dir() {
            collect_app_bundles(&path, out);
        }
    }
}

fn is_app_bundle(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("app"))
        && path.is_dir()
}

fn read_application(app_path: &Path, icon_dir: &Path, home_dir: &Path) -> Result<Application, AppError> {
    let file_name = app_path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("Untitled")
        .to_string();
    let plist = read_info_plist(&app_path.join("Contents").join("Info.plist"));
    let bundle_id = plist.as_ref().and_then(|info| non_empty(&info.bundle_id));
    let name = plist
        .as_ref()
        .and_then(|info| non_empty(&info.display_name).or_else(|| non_empty(&info.name)))
        .unwrap_or(file_name);
    let icon_file = plist
        .as_ref()
        .and_then(|info| non_empty(&info.icon_file).or_else(|| non_empty(&info.icon_name)));
    let path = app_path.to_string_lossy().replace('\\', "/");
    let id = generate_id(&path, bundle_id.as_deref());
    let icon = cache_app_icon(&id, app_path, icon_file.as_deref(), icon_dir);

    Ok(Application {
        id,
        name,
        path,
        bundle_id,
        icon,
        source: source_for_path(app_path, home_dir),
        launch_count: 0,
        last_launch_time: None,
    })
}

fn read_info_plist(path: &Path) -> Option<InfoPlist> {
    match plist::from_file::<_, InfoPlist>(path) {
        Ok(info) => Some(info),
        Err(error) => {
            log::warn!("无法读取 Info.plist {}：{error}", path.display());
            None
        }
    }
}

fn non_empty(value: &Option<String>) -> Option<String> {
    value.as_ref().and_then(|text| {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn source_for_path(path: &Path, home_dir: &Path) -> String {
    let normalized = normalize_path(&path.to_string_lossy());
    if normalized.contains("/system/applications") {
        return "system".into();
    }
    let user_apps = normalize_path(&home_dir.join("Applications").to_string_lossy());
    if !user_apps.is_empty()
        && (normalized == user_apps || normalized.starts_with(&format!("{user_apps}/")))
    {
        return "user".into();
    }
    "applications".into()
}

fn dedup_applications(apps: Vec<Application>) -> Vec<Application> {
    let mut seen_ids = HashSet::new();
    let mut seen_paths = HashSet::new();
    let mut unique = Vec::new();

    for app in apps {
        let path_key = normalize_path(&app.path);
        if !seen_ids.insert(app.id.clone()) || !seen_paths.insert(path_key) {
            continue;
        }
        unique.push(app);
    }

    unique
}

#[cfg(test)]
mod tests {
    use super::{
        collect_app_bundles, dedup_applications, is_app_bundle, read_application, source_for_path,
        MacOsScanner,
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
        let dir = std::env::temp_dir().join(format!("quka-scan-{name}-{nanos}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_app(root: &Path, relative: &str, display: &str, name: &str, bundle_id: &str) -> PathBuf {
        let app = root.join(relative);
        let contents = app.join("Contents");
        fs::create_dir_all(contents.join("Resources")).unwrap();
        fs::write(
            contents.join("Info.plist"),
            format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDisplayName</key>
  <string>{display}</string>
  <key>CFBundleName</key>
  <string>{name}</string>
  <key>CFBundleIdentifier</key>
  <string>{bundle_id}</string>
</dict>
</plist>
"#
            ),
        )
        .unwrap();
        app
    }

    #[test]
    fn recognizes_app_bundles() {
        let root = temp_dir("bundle");
        let app = root.join("Safari.app");
        fs::create_dir_all(&app).unwrap();
        assert!(is_app_bundle(&app));
        assert!(!is_app_bundle(&root));
    }

    #[test]
    fn prefers_display_name() {
        let root = temp_dir("name");
        let app = write_app(&root, "Chrome.app", "Google Chrome", "Chrome", "com.google.Chrome");
        let icon_dir = root.join("icons");
        let scanned = read_application(&app, &icon_dir, &root.join("home")).unwrap();
        assert_eq!(scanned.name, "Google Chrome");
        assert_eq!(scanned.bundle_id.as_deref(), Some("com.google.Chrome"));
        assert_eq!(scanned.id, "com.google.Chrome");
    }

    #[test]
    fn does_not_enter_app_bundles() {
        let root = temp_dir("nested");
        write_app(&root, "Foo.app", "Foo", "Foo", "com.example.Foo");
        write_app(
            &root,
            "Foo.app/Contents/Helpers/Helper.app",
            "Helper",
            "Helper",
            "com.example.Helper",
        );
        write_app(&root, "Utilities/Bar.app", "Bar", "Bar", "com.example.Bar");

        let mut bundles = Vec::new();
        collect_app_bundles(&root, &mut bundles);
        let names: Vec<_> = bundles
            .iter()
            .filter_map(|path| path.file_name()?.to_str())
            .collect();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"Foo.app"));
        assert!(names.contains(&"Bar.app"));
        assert!(!names.contains(&"Helper.app"));
    }

    #[test]
    fn dedups_by_bundle_id() {
        let apps = vec![
            Application {
                id: "com.google.Chrome".into(),
                name: "Google Chrome".into(),
                path: "/Applications/Google Chrome.app".into(),
                bundle_id: Some("com.google.Chrome".into()),
                icon: None,
                source: "applications".into(),
                launch_count: 0,
                last_launch_time: None,
            },
            Application {
                id: "com.google.Chrome".into(),
                name: "Google Chrome".into(),
                path: "/Users/me/Applications/Google Chrome.app".into(),
                bundle_id: Some("com.google.Chrome".into()),
                icon: None,
                source: "user".into(),
                launch_count: 0,
                last_launch_time: None,
            },
        ];
        let unique = dedup_applications(apps);
        assert_eq!(unique.len(), 1);
        assert_eq!(unique[0].path, "/Applications/Google Chrome.app");
    }

    #[test]
    fn classifies_sources() {
        let home = Path::new("/Users/ada");
        assert_eq!(
            source_for_path(Path::new("/System/Applications/Safari.app"), home),
            "system"
        );
        assert_eq!(
            source_for_path(Path::new("/Users/ada/Applications/Foo.app"), home),
            "user"
        );
        assert_eq!(
            source_for_path(Path::new("/Applications/Chrome.app"), home),
            "applications"
        );
    }

    #[test]
    fn skips_missing_directories() {
        let root = temp_dir("missing");
        let scanner = MacOsScanner::new(root.join("home"), root.join("icons"));
        let apps = scanner
            .scan_directories(&[root.join("does-not-exist")])
            .unwrap();
        assert!(apps.is_empty());
    }

    #[test]
    fn scans_custom_directories() {
        let root = temp_dir("scan");
        let applications = root.join("Applications");
        write_app(
            &applications,
            "Visual Studio Code.app",
            "Visual Studio Code",
            "Code",
            "com.microsoft.VSCode",
        );
        let scanner = MacOsScanner::new(root.join("home"), root.join("icons"));
        let apps = scanner.scan_directories(&[applications]).unwrap();
        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].name, "Visual Studio Code");
        assert_eq!(apps[0].source, "applications");
    }

    #[test]
    fn trait_scan_uses_standard_paths() {
        let root = temp_dir("trait");
        let scanner = MacOsScanner::new(root.join("home"), root.join("icons"));
        let apps = ApplicationScanner::scan(&scanner).unwrap();
        assert!(apps.is_empty());
    }
}

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Application {
    pub id: String,
    pub name: String,
    pub path: String,
    pub bundle_id: Option<String>,
    pub icon: Option<String>,
    pub source: String,
    pub launch_count: i64,
    pub last_launch_time: Option<i64>,
    #[serde(default)]
    pub aliases: String,
}

impl Application {
    pub fn search_names(&self) -> Vec<&str> {
        std::iter::once(self.name.as_str())
            .chain(
                self.aliases
                    .split('\n')
                    .map(str::trim)
                    .filter(|item| !item.is_empty()),
            )
            .collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub global_shortcut: String,
    pub launch_at_startup: bool,
    pub result_limit: i64,
    pub enable_usage_ranking: bool,
    pub theme: String,
    pub locale: String,
    #[serde(default = "default_true")]
    pub disable_on_fullscreen: bool,
    #[serde(default = "default_color_icon")]
    pub tray_icon: String,
}

fn default_color_icon() -> String {
    "color".into()
}

fn default_true() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            global_shortcut: if cfg!(target_os = "macos") {
                "DoubleCommand".into()
            } else {
                "DoubleCtrl".into()
            },
            launch_at_startup: false,
            result_limit: 8,
            enable_usage_ranking: true,
            theme: "system".into(),
            locale: "system".into(),
            disable_on_fullscreen: true,
            tray_icon: "color".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseInfo {
    pub application_count: i64,
    pub just_initialized: bool,
    pub needs_scan: bool,
    pub scanner: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    pub application_count: i64,
    pub inserted: i64,
    pub updated: i64,
    pub deleted: i64,
    pub supported: bool,
    pub scanner: String,
    #[serde(default)]
    pub silent: bool,
}

use rusqlite::Connection;

use super::applications::{count, insert};
use super::models::Application;
use crate::state::AppError;

struct SeedApp {
    name: &'static str,
    path: &'static str,
    bundle_id: &'static str,
    icon: &'static str,
    source: &'static str,
    launch_count: i64,
    last_launch_offset_ms: i64,
}

const SEED_APPS: &[SeedApp] = &[
    SeedApp {
        name: "Visual Studio Code",
        path: "/Applications/Visual Studio Code.app",
        bundle_id: "com.microsoft.VSCode",
        icon: "vscode",
        source: "applications",
        launch_count: 28,
        last_launch_offset_ms: 20 * 60 * 1000,
    },
    SeedApp {
        name: "Google Chrome",
        path: "/Applications/Google Chrome.app",
        bundle_id: "com.google.Chrome",
        icon: "chrome",
        source: "applications",
        launch_count: 12,
        last_launch_offset_ms: 2 * 60 * 60 * 1000,
    },
    SeedApp {
        name: "Figma",
        path: "/Applications/Figma.app",
        bundle_id: "com.figma.Desktop",
        icon: "figma",
        source: "applications",
        launch_count: 9,
        last_launch_offset_ms: 6 * 60 * 60 * 1000,
    },
    SeedApp {
        name: "Cursor",
        path: "/Applications/Cursor.app",
        bundle_id: "com.todesktop.230313mzl4w4u92",
        icon: "cursor",
        source: "applications",
        launch_count: 41,
        last_launch_offset_ms: 8 * 60 * 1000,
    },
    SeedApp {
        name: "WeChat",
        path: "/Applications/WeChat.app",
        bundle_id: "com.tencent.xinWeChat",
        icon: "wechat",
        source: "applications",
        launch_count: 16,
        last_launch_offset_ms: 24 * 60 * 60 * 1000,
    },
    SeedApp {
        name: "Slack",
        path: "/Applications/Slack.app",
        bundle_id: "com.tinyspeck.slackmacgap",
        icon: "slack",
        source: "applications",
        launch_count: 4,
        last_launch_offset_ms: 3 * 24 * 60 * 60 * 1000,
    },
    SeedApp {
        name: "Terminal",
        path: "/System/Applications/Utilities/Terminal.app",
        bundle_id: "com.apple.Terminal",
        icon: "terminal",
        source: "system",
        launch_count: 7,
        last_launch_offset_ms: 5 * 60 * 60 * 1000,
    },
    SeedApp {
        name: "Notion",
        path: "/Applications/Notion.app",
        bundle_id: "notion.id",
        icon: "notion",
        source: "applications",
        launch_count: 3,
        last_launch_offset_ms: 2 * 24 * 60 * 60 * 1000,
    },
];

pub fn seed_if_empty(conn: &Connection) -> Result<bool, AppError> {
    if count(conn)? > 0 {
        return Ok(false);
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .map_err(|error| AppError::Io(error.to_string()))?;

    for item in SEED_APPS {
        insert(
            conn,
            &Application {
                id: super::id::generate_id(item.path, Some(item.bundle_id)),
                name: item.name.into(),
                path: item.path.into(),
                bundle_id: Some(item.bundle_id.into()),
                icon: Some(item.icon.into()),
                source: item.source.into(),
                launch_count: item.launch_count,
                last_launch_time: Some(now - item.last_launch_offset_ms),
                aliases: String::new(),
            },
        )?;
    }

    Ok(true)
}

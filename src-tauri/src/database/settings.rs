use rusqlite::{params, Connection, OptionalExtension};

use super::models::Settings;
use crate::shortcut::detect::{default_shortcut, normalize_shortcut};
use crate::state::AppError;

pub fn get(conn: &Connection) -> Result<Settings, AppError> {
    let settings = conn
        .query_row(
            "
            SELECT global_shortcut, launch_at_startup, result_limit, enable_usage_ranking, theme, locale, disable_on_fullscreen, tray_icon
            FROM settings
            WHERE id = 1
            ",
            [],
            |row| {
                Ok(Settings {
                    global_shortcut: row.get(0)?,
                    launch_at_startup: row.get::<_, i64>(1)? != 0,
                    result_limit: row.get(2)?,
                    enable_usage_ranking: row.get::<_, i64>(3)? != 0,
                    theme: row.get(4)?,
                    locale: row.get(5)?,
                    disable_on_fullscreen: row.get::<_, i64>(6)? != 0,
                    tray_icon: row.get(7)?,
                })
            },
        )
        .optional()?;

    match settings {
        Some(mut value) => {
            let normalized = normalize_shortcut(&value.global_shortcut);
            let locale = normalize_locale(&value.locale);
            let tray_icon = normalize_tray_icon(&value.tray_icon);
            if normalized != value.global_shortcut
                || locale != value.locale
                || tray_icon != value.tray_icon
            {
                value.global_shortcut = normalized;
                value.locale = locale;
                value.tray_icon = tray_icon;
                upsert(conn, &value)?;
            }
            Ok(value)
        }
        None => {
            let mut defaults = Settings::default();
            defaults.global_shortcut = default_shortcut();
            upsert(conn, &defaults)?;
            Ok(defaults)
        }
    }
}

pub fn upsert(conn: &Connection, settings: &Settings) -> Result<(), AppError> {
    conn.execute(
        "
        INSERT INTO settings (
            id, global_shortcut, launch_at_startup, result_limit, enable_usage_ranking, theme, locale, disable_on_fullscreen, tray_icon
        ) VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        ON CONFLICT(id) DO UPDATE SET
            global_shortcut = excluded.global_shortcut,
            launch_at_startup = excluded.launch_at_startup,
            result_limit = excluded.result_limit,
            enable_usage_ranking = excluded.enable_usage_ranking,
            theme = excluded.theme,
            locale = excluded.locale,
            disable_on_fullscreen = excluded.disable_on_fullscreen,
            tray_icon = excluded.tray_icon
        ",
        params![
            settings.global_shortcut,
            settings.launch_at_startup as i64,
            settings.result_limit,
            settings.enable_usage_ranking as i64,
            settings.theme,
            settings.locale,
            settings.disable_on_fullscreen as i64,
            settings.tray_icon,
        ],
    )?;
    Ok(())
}

pub fn normalize_tray_icon(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "mono" | "template" | "menu" => "mono".into(),
        "search" => "search".into(),
        "bolt" | "lightning" => "bolt".into(),
        _ => "color".into(),
    }
}

fn normalize_locale(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "zh-cn" | "zh" | "zh-hans" => "zh-CN".into(),
        "en" | "en-us" | "en-gb" => "en".into(),
        "system" => "system".into(),
        _ => "system".into(),
    }
}

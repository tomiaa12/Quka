use rusqlite::Connection;

use crate::state::AppError;

const SCAN_SCHEMA: i64 = 3;

pub fn run(conn: &Connection) -> Result<bool, AppError> {
    if let Err(error) = conn.pragma_update(None, "journal_mode", "WAL") {
        log::debug!("WAL 未启用：{error}");
    }
    let _ = conn.pragma_update(None, "synchronous", "NORMAL");
    let _ = conn.pragma_update(None, "temp_store", "MEMORY");

    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS applications (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            path TEXT NOT NULL UNIQUE,
            bundle_id TEXT,
            icon TEXT,
            source TEXT NOT NULL,
            launch_count INTEGER NOT NULL DEFAULT 0,
            last_launch_time INTEGER,
            aliases TEXT NOT NULL DEFAULT '',
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS settings (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            global_shortcut TEXT NOT NULL,
            launch_at_startup INTEGER NOT NULL,
            result_limit INTEGER NOT NULL,
            enable_usage_ranking INTEGER NOT NULL,
            theme TEXT NOT NULL,
            locale TEXT NOT NULL DEFAULT 'system',
            disable_on_fullscreen INTEGER NOT NULL DEFAULT 1,
            tray_icon TEXT NOT NULL DEFAULT 'color'
        );

        CREATE INDEX IF NOT EXISTS idx_applications_recent
            ON applications(last_launch_time DESC, launch_count DESC);
        CREATE INDEX IF NOT EXISTS idx_applications_name
            ON applications(name);
        ",
    )
    .map_err(AppError::from)?;

    ensure_column(conn, "settings", "locale", "TEXT NOT NULL DEFAULT 'system'")?;
    ensure_column(
        conn,
        "settings",
        "disable_on_fullscreen",
        "INTEGER NOT NULL DEFAULT 1",
    )?;
    ensure_column(
        conn,
        "settings",
        "tray_icon",
        "TEXT NOT NULL DEFAULT 'color'",
    )?;
    let added_aliases = ensure_column(
        conn,
        "applications",
        "aliases",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    let version = conn.pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))?;
    Ok(added_aliases || version < SCAN_SCHEMA)
}

pub fn mark_scan_schema(conn: &Connection) -> Result<(), AppError> {
    conn.pragma_update(None, "user_version", SCAN_SCHEMA)?;
    Ok(())
}

fn ensure_column(
    conn: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<bool, AppError> {
    let exists = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .and_then(|mut stmt| {
            let names = stmt.query_map([], |row| row.get::<_, String>(1))?;
            for name in names {
                if name? == column {
                    return Ok(true);
                }
            }
            Ok(false)
        })
        .unwrap_or(false);
    if exists {
        return Ok(false);
    }
    conn.execute(
        &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
        [],
    )?;
    Ok(true)
}

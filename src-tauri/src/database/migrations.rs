use rusqlite::Connection;

use crate::state::AppError;

pub fn run(conn: &Connection) -> Result<(), AppError> {
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
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS settings (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            global_shortcut TEXT NOT NULL,
            launch_at_startup INTEGER NOT NULL,
            result_limit INTEGER NOT NULL,
            enable_usage_ranking INTEGER NOT NULL,
            theme TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_applications_recent
            ON applications(last_launch_time DESC, launch_count DESC);
        CREATE INDEX IF NOT EXISTS idx_applications_name
            ON applications(name);
        ",
    )
    .map_err(AppError::from)?;

    Ok(())
}

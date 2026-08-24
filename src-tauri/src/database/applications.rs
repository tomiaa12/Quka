use rusqlite::{params, Connection, OptionalExtension};

use super::models::Application;
use crate::state::AppError;

fn now_ms() -> Result<i64, AppError> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .map_err(|error| AppError::Io(error.to_string()))
}

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Application> {
    Ok(Application {
        id: row.get(0)?,
        name: row.get(1)?,
        path: row.get(2)?,
        bundle_id: row.get(3)?,
        icon: row.get(4)?,
        source: row.get(5)?,
        launch_count: row.get(6)?,
        last_launch_time: row.get(7)?,
        aliases: row.get::<_, Option<String>>(8)?.unwrap_or_default(),
    })
}

pub fn insert(conn: &Connection, app: &Application) -> Result<(), AppError> {
    let timestamp = now_ms()?;
    conn.execute(
        "
        INSERT INTO applications (
            id, name, path, bundle_id, icon, source,
            launch_count, last_launch_time, aliases, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)
        ",
        params![
            app.id,
            app.name,
            app.path,
            app.bundle_id,
            app.icon,
            app.source,
            app.launch_count,
            app.last_launch_time,
            app.aliases,
            timestamp,
        ],
    )?;
    Ok(())
}

pub fn update(conn: &Connection, app: &Application) -> Result<(), AppError> {
    let timestamp = now_ms()?;
    let changed = conn.execute(
        "
        UPDATE applications SET
            name = ?2,
            path = ?3,
            bundle_id = ?4,
            icon = ?5,
            source = ?6,
            launch_count = ?7,
            last_launch_time = ?8,
            aliases = ?9,
            updated_at = ?10
        WHERE id = ?1
        ",
        params![
            app.id,
            app.name,
            app.path,
            app.bundle_id,
            app.icon,
            app.source,
            app.launch_count,
            app.last_launch_time,
            app.aliases,
            timestamp,
        ],
    )?;

    if changed == 0 {
        return Err(AppError::Database("应用不存在".into()));
    }
    Ok(())
}

pub fn record_launch(conn: &Connection, id: &str) -> Result<i64, AppError> {
    let timestamp = now_ms()?;
    let changed = conn.execute(
        "
        UPDATE applications SET
            launch_count = launch_count + 1,
            last_launch_time = ?2,
            updated_at = ?2
        WHERE id = ?1
        ",
        params![id, timestamp],
    )?;
    if changed == 0 {
        return Err(AppError::Database("应用不存在".into()));
    }
    Ok(timestamp)
}

pub fn delete(conn: &Connection, id: &str) -> Result<(), AppError> {
    let changed = conn.execute("DELETE FROM applications WHERE id = ?1", params![id])?;
    if changed == 0 {
        return Err(AppError::Database("应用不存在".into()));
    }
    Ok(())
}

pub fn get_by_id(conn: &Connection, id: &str) -> Result<Option<Application>, AppError> {
    let app = conn
        .query_row(
            "
            SELECT id, name, path, bundle_id, icon, source, launch_count, last_launch_time, aliases
            FROM applications
            WHERE id = ?1
            ",
            params![id],
            map_row,
        )
        .optional()?;
    Ok(app)
}

pub fn query_all(conn: &Connection) -> Result<Vec<Application>, AppError> {
    let mut statement = conn.prepare(
        "
        SELECT id, name, path, bundle_id, icon, source, launch_count, last_launch_time, aliases
        FROM applications
        ORDER BY last_launch_time DESC, launch_count DESC, name ASC
        ",
    )?;
    let rows = statement.query_map([], map_row)?;
    let mut apps = Vec::new();
    for row in rows {
        apps.push(row?);
    }
    Ok(apps)
}

pub fn count(conn: &Connection) -> Result<i64, AppError> {
    let value = conn.query_row("SELECT COUNT(*) FROM applications", [], |row| row.get(0))?;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::{insert, record_launch};
    use crate::database::migrations;
    use crate::database::models::Application;
    use rusqlite::Connection;

    fn memory_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        migrations::run(&conn).unwrap();
        conn
    }

    #[test]
    fn increments_launch_count() {
        let conn = memory_db();
        insert(
            &conn,
            &Application {
                id: "notepad".into(),
                name: "Notepad".into(),
                path: r"C:\Windows\System32\notepad.exe".into(),
                bundle_id: None,
                icon: None,
                source: "system".into(),
                launch_count: 2,
                last_launch_time: None,
                aliases: String::new(),
            },
        )
        .unwrap();
        record_launch(&conn, "notepad").unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT launch_count FROM applications WHERE id = 'notepad'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let last: Option<i64> = conn
            .query_row(
                "SELECT last_launch_time FROM applications WHERE id = 'notepad'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 3);
        assert!(last.is_some_and(|value| value > 0));
    }

    #[test]
    fn record_launch_rejects_unknown_id() {
        let conn = memory_db();
        let error = record_launch(&conn, "missing").unwrap_err();
        assert!(error.to_string().contains("应用不存在"));
    }

    #[test]
    fn query_all_under_10ms() {
        let conn = memory_db();
        for index in 0..200 {
            insert(
                &conn,
                &Application {
                    id: format!("app-{index}"),
                    name: format!("App {index}"),
                    path: format!("C:\\Apps\\app-{index}.exe"),
                    bundle_id: None,
                    icon: None,
                    source: "system".into(),
                    launch_count: index,
                    last_launch_time: Some(index),
                    aliases: String::new(),
                },
            )
            .unwrap();
        }
        let _ = super::query_all(&conn).unwrap();
        let started = std::time::Instant::now();
        let apps = super::query_all(&conn).unwrap();
        let elapsed = started.elapsed();
        assert_eq!(apps.len(), 200);
        assert!(
            elapsed.as_millis() < 10,
            "query_all 耗时 {}ms，目标 < 10ms",
            elapsed.as_millis()
        );
    }
}

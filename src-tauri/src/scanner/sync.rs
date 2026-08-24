use std::collections::HashSet;

use rusqlite::Connection;

use crate::database::applications::{delete, insert, query_all, update};
use crate::database::id::normalize_path;
use crate::database::models::Application;
use crate::state::AppError;

pub const MACOS_SOURCES: &[&str] = &["applications", "user", "system"];

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SyncStats {
    pub inserted: i64,
    pub updated: i64,
    pub deleted: i64,
}

pub fn sync_macos(conn: &Connection, scanned: Vec<Application>) -> Result<SyncStats, AppError> {
    sync_scanned(conn, scanned, MACOS_SOURCES, &[])
}

pub fn sync_windows(conn: &Connection, scanned: Vec<Application>) -> Result<SyncStats, AppError> {
    sync_scanned(
        conn,
        scanned,
        crate::scanner::windows::WINDOWS_SOURCES,
        MACOS_SOURCES,
    )
}

pub fn sync_scanned(
    conn: &Connection,
    scanned: Vec<Application>,
    managed_sources: &[&str],
    prune_extra_sources: &[&str],
) -> Result<SyncStats, AppError> {
    let existing = query_all(conn)?;
    let scanned_ids: HashSet<String> = scanned.iter().map(|app| app.id.clone()).collect();
    let mut stats = SyncStats::default();

    for app in &scanned {
        if let Some(old) = existing.iter().find(|item| item.id == app.id) {
            let changed = old.name != app.name
                || old.path != app.path
                || old.bundle_id != app.bundle_id
                || old.icon != app.icon
                || old.source != app.source
                || old.aliases != app.aliases;
            if !changed {
                continue;
            }

            let mut next = app.clone();
            next.launch_count = old.launch_count;
            next.last_launch_time = old.last_launch_time;
            update(conn, &next)?;
            stats.updated += 1;
            log::info!("更新应用 {}", next.name);
            continue;
        }

        if let Some(conflict) = existing.iter().find(|item| {
            item.id != app.id && normalize_path(&item.path) == normalize_path(&app.path)
        }) {
            delete(conn, &conflict.id)?;
            stats.deleted += 1;
            log::info!("删除应用 {}", conflict.name);
        }

        insert(conn, app)?;
        stats.inserted += 1;
        log::info!("发现应用 {}", app.name);
    }

    for old in &existing {
        let prune = (managed_sources.contains(&old.source.as_str())
            && !scanned_ids.contains(&old.id))
            || prune_extra_sources.contains(&old.source.as_str());
        if !prune {
            continue;
        }
        delete(conn, &old.id)?;
        stats.deleted += 1;
        log::info!("删除应用 {}", old.name);
    }

    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::{sync_macos, sync_scanned, sync_windows};
    use crate::database::applications::{insert, query_all};
    use crate::database::migrations;
    use crate::database::models::Application;
    use rusqlite::Connection;

    fn memory_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        migrations::run(&conn).unwrap();
        conn
    }

    fn app(id: &str, name: &str, path: &str, source: &str) -> Application {
        Application {
            id: id.into(),
            name: name.into(),
            path: path.into(),
            bundle_id: Some(id.into()),
            icon: None,
            source: source.into(),
            launch_count: 0,
            last_launch_time: None,
            aliases: String::new(),
        }
    }

    #[test]
    fn inserts_updates_and_deletes() {
        let conn = memory_db();
        let mut existing = app(
            "com.old.App",
            "Old",
            "/Applications/Old.app",
            "applications",
        );
        existing.launch_count = 9;
        insert(&conn, &existing).unwrap();
        insert(
            &conn,
            &app(
                "com.keep.App",
                "Keep",
                "/Applications/Keep.app",
                "applications",
            ),
        )
        .unwrap();

        let mut updated = app(
            "com.keep.App",
            "Keep Renamed",
            "/Applications/Keep.app",
            "applications",
        );
        updated.icon = Some("/tmp/keep.png".into());

        let stats = sync_macos(
            &conn,
            vec![
                updated,
                app(
                    "com.new.App",
                    "New",
                    "/Applications/New.app",
                    "applications",
                ),
            ],
        )
        .unwrap();

        assert_eq!(stats.inserted, 1);
        assert_eq!(stats.updated, 1);
        assert_eq!(stats.deleted, 1);

        let apps = query_all(&conn).unwrap();
        assert_eq!(apps.len(), 2);
        let keep = apps.iter().find(|item| item.id == "com.keep.App").unwrap();
        assert_eq!(keep.name, "Keep Renamed");
        assert_eq!(keep.launch_count, 0);
    }

    #[test]
    fn preserves_launch_count_on_update() {
        let conn = memory_db();
        let mut existing = app(
            "com.google.Chrome",
            "Chrome",
            "/Applications/Google Chrome.app",
            "applications",
        );
        existing.launch_count = 12;
        existing.last_launch_time = Some(123);
        insert(&conn, &existing).unwrap();

        let mut scanned = existing.clone();
        scanned.name = "Google Chrome".into();
        sync_macos(&conn, vec![scanned]).unwrap();

        let apps = query_all(&conn).unwrap();
        assert_eq!(apps[0].name, "Google Chrome");
        assert_eq!(apps[0].launch_count, 12);
        assert_eq!(apps[0].last_launch_time, Some(123));
    }

    #[test]
    fn windows_sync_removes_macos_seed() {
        let conn = memory_db();
        insert(
            &conn,
            &app(
                "com.google.Chrome",
                "Google Chrome",
                "/Applications/Google Chrome.app",
                "applications",
            ),
        )
        .unwrap();

        let scanned = app(
            "chrome-id",
            "Google Chrome",
            r"C:\Program Files\Google\Chrome\Application\chrome.exe",
            "start-menu",
        );
        let stats = sync_windows(&conn, vec![scanned]).unwrap();
        assert_eq!(stats.inserted, 1);
        assert_eq!(stats.deleted, 1);
        let apps = query_all(&conn).unwrap();
        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].source, "start-menu");
    }

    #[test]
    fn generic_sync_respects_managed_sources() {
        let conn = memory_db();
        insert(&conn, &app("keep", "Keep", r"C:\Keep.exe", "start-menu")).unwrap();
        insert(&conn, &app("gone", "Gone", r"C:\Gone.exe", "start-menu")).unwrap();
        let stats = sync_scanned(
            &conn,
            vec![app("keep", "Keep", r"C:\Keep.exe", "start-menu")],
            &["start-menu"],
            &[],
        )
        .unwrap();
        assert_eq!(stats.deleted, 1);
        assert_eq!(query_all(&conn).unwrap().len(), 1);
    }
}

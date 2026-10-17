//! WebKit keeps the app's localStorage in its own SQLite file and never sets a
//! journal size limit, so a write burst leaves a WAL that stays at its peak
//! size forever (350 MB for ~450 KB of keys was observed). Before any webview
//! opens, fold the WAL back into the database and truncate it.

use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{Connection, OpenFlags};

/// Below this the WAL is normal working space, not leftover.
const WAL_COMPACT_THRESHOLD_BYTES: u64 = 32 * 1024 * 1024;

/// `~/Library/WebKit/<identifier>/WebsiteData/Default/<origin>/<origin>/LocalStorage/localstorage.sqlite3`
fn local_storage_databases(webkit_root: &Path) -> Vec<PathBuf> {
    let default = webkit_root.join("WebsiteData").join("Default");
    let Ok(origins) = std::fs::read_dir(&default) else {
        return Vec::new();
    };
    let mut databases = Vec::new();
    for origin in origins.flatten() {
        let Ok(partitions) = std::fs::read_dir(origin.path()) else {
            continue;
        };
        for partition in partitions.flatten() {
            let database = partition
                .path()
                .join("LocalStorage")
                .join("localstorage.sqlite3");
            if database.is_file() {
                databases.push(database);
            }
        }
    }
    databases
}

fn wal_len(database: &Path) -> u64 {
    let mut wal = database.as_os_str().to_owned();
    wal.push("-wal");
    std::fs::metadata(PathBuf::from(wal))
        .map(|metadata| metadata.len())
        .unwrap_or(0)
}

/// SQLite's own locking keeps this safe if another process still has the file
/// open: the checkpoint then reports busy and the next launch tries again.
fn truncate_wal(database: &Path) -> rusqlite::Result<()> {
    let conn = Connection::open_with_flags(
        database,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    conn.busy_timeout(Duration::ZERO)?;
    conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))
}

pub fn compact_local_storage_wal(webkit_root: &Path) {
    for database in local_storage_databases(webkit_root) {
        let before = wal_len(&database);
        if before < WAL_COMPACT_THRESHOLD_BYTES {
            continue;
        }
        match truncate_wal(&database) {
            Ok(()) => eprintln!(
                "[DCC] localStorage WAL compacted: {} MB -> {} MB",
                before / (1024 * 1024),
                wal_len(&database) / (1024 * 1024)
            ),
            Err(error) => eprintln!("[DCC] localStorage WAL left as is: {error}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncates_a_leftover_wal_and_keeps_the_keys() {
        let directory = tempfile::tempdir().expect("temporary webkit root");
        let storage_dir = directory
            .path()
            .join("WebsiteData/Default/origin/origin/LocalStorage");
        std::fs::create_dir_all(&storage_dir).unwrap();
        let database = storage_dir.join("localstorage.sqlite3");

        // A writer that never checkpoints, like WebKit between launches.
        let writer = Connection::open(&database).unwrap();
        writer
            .execute_batch(
                "PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0;
                 CREATE TABLE ItemTable (key TEXT UNIQUE, value BLOB);",
            )
            .unwrap();
        let blob = "x".repeat(256 * 1024);
        for round in 0..160 {
            writer
                .execute(
                    "INSERT OR REPLACE INTO ItemTable VALUES ('dcc-query-cache', ?1)",
                    [format!("{round}{blob}")],
                )
                .unwrap();
        }
        assert!(wal_len(&database) >= WAL_COMPACT_THRESHOLD_BYTES);
        std::mem::forget(writer); // leave the WAL behind, as a killed process would

        compact_local_storage_wal(directory.path());

        assert_eq!(wal_len(&database), 0);
        let reader = Connection::open(&database).unwrap();
        let value: String = reader
            .query_row(
                "SELECT value FROM ItemTable WHERE key = 'dcc-query-cache'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(value.starts_with("159"));
    }

    #[test]
    fn ignores_a_profile_without_local_storage() {
        let directory = tempfile::tempdir().expect("temporary webkit root");
        compact_local_storage_wal(directory.path());
        assert!(local_storage_databases(directory.path()).is_empty());
    }
}

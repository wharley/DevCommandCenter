//! Giving freed pages back to the disk. Deleting rows only moves their pages
//! to SQLite's free list; the file keeps its size until a VACUUM.

use super::*;

/// Below this much free space, a full rewrite is not worth the pause.
const VACUUM_MIN_FREE_BYTES: i64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VacuumOutcome {
    pub before_bytes: i64,
    pub after_bytes: i64,
}

fn page_stats(conn: &Connection) -> rusqlite::Result<(i64, i64, i64)> {
    let page_size = conn.query_row("PRAGMA page_size", [], |row| row.get(0))?;
    let page_count = conn.query_row("PRAGMA page_count", [], |row| row.get(0))?;
    let free_pages = conn.query_row("PRAGMA freelist_count", [], |row| row.get(0))?;
    Ok((page_size, page_count, free_pages))
}

/// Rewrites a database created before incremental auto-vacuum, once, when at
/// least a quarter of it (and 64 MB) is free. It holds the write lock for a
/// few seconds, so callers run it before anything starts writing (app
/// startup). Afterwards [`SqliteSessionRepo::reclaim_free_pages_step`] keeps
/// the file small in slices.
pub fn vacuum_into_incremental_mode(conn: &Connection) -> rusqlite::Result<Option<VacuumOutcome>> {
    let mode: i64 = conn.query_row("PRAGMA auto_vacuum", [], |row| row.get(0))?;
    if mode != 0 {
        return Ok(None);
    }
    let (page_size, page_count, free_pages) = page_stats(conn)?;
    if free_pages * page_size < VACUUM_MIN_FREE_BYTES || free_pages * 4 < page_count {
        return Ok(None);
    }
    conn.execute_batch("PRAGMA auto_vacuum = INCREMENTAL; VACUUM;")?;
    // VACUUM goes through the WAL; fold it back so the file shrinks now.
    conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;
    let (page_size_after, page_count_after, _) = page_stats(conn)?;
    Ok(Some(VacuumOutcome {
        before_bytes: page_count * page_size,
        after_bytes: page_count_after * page_size_after,
    }))
}

impl SqliteSessionRepo {
    /// Returns up to `pages` free pages to the disk in one short write and
    /// reports how many stay free. Zero when the database is not in
    /// incremental mode yet (only [`vacuum_into_incremental_mode`] fixes that).
    pub fn reclaim_free_pages_step(&self, pages: u32) -> Result<i64> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let mode: i64 = conn
            .query_row("PRAGMA auto_vacuum", [], |row| row.get(0))
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        if mode != 2 {
            return Ok(0);
        }
        // Each step of this pragma frees one page; run it to the end.
        let mut statement = conn
            .prepare(&format!("PRAGMA incremental_vacuum({pages})"))
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let mut rows = statement
            .query([])
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        while rows
            .next()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?
            .is_some()
        {}
        drop(rows);
        drop(statement);
        conn.query_row("PRAGMA freelist_count", [], |row| row.get(0))
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fill_and_free(conn: &Connection, megabytes: usize) {
        conn.execute_batch("CREATE TABLE IF NOT EXISTS filler (blob TEXT)")
            .unwrap();
        let blob = "x".repeat(1024 * 1024);
        for _ in 0..megabytes {
            conn.execute("INSERT INTO filler VALUES (?1)", params![blob])
                .unwrap();
        }
        conn.execute("DELETE FROM filler", []).unwrap();
    }

    fn file_len(path: &Path) -> u64 {
        std::fs::metadata(path).unwrap().len()
    }

    #[test]
    fn converts_a_mostly_free_legacy_database_once() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("legacy.sqlite");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch("PRAGMA journal_mode = WAL").unwrap();
        fill_and_free(&conn, 80);
        conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))
            .unwrap();
        assert!(file_len(&path) > 80 * 1024 * 1024);

        let outcome = vacuum_into_incremental_mode(&conn).unwrap().unwrap();
        assert!(outcome.after_bytes < 1024 * 1024, "{outcome:?}");
        assert!(file_len(&path) < 1024 * 1024);
        let mode: i64 = conn
            .query_row("PRAGMA auto_vacuum", [], |row| row.get(0))
            .unwrap();
        assert_eq!(mode, 2);
        assert_eq!(vacuum_into_incremental_mode(&conn).unwrap(), None);
    }

    #[test]
    fn leaves_a_database_with_little_free_space_alone() {
        let conn = Connection::open_in_memory().unwrap();
        fill_and_free(&conn, 2);
        assert_eq!(vacuum_into_incremental_mode(&conn).unwrap(), None);
    }

    #[test]
    fn a_new_session_database_reclaims_free_pages_in_slices() {
        let directory = tempfile::tempdir().unwrap();
        let repo = SqliteSessionRepo::open(directory.path().join("new.sqlite")).unwrap();
        fill_and_free(&repo.conn.lock().unwrap(), 4);

        let free_before: i64 = repo
            .conn
            .lock()
            .unwrap()
            .query_row("PRAGMA freelist_count", [], |row| row.get(0))
            .unwrap();
        let first = repo.reclaim_free_pages_step(256).unwrap();
        assert_eq!(first, free_before - 256, "a slice frees exactly its size");
        let mut remaining = first;
        while remaining > 0 {
            remaining = repo.reclaim_free_pages_step(256).unwrap();
        }
        assert_eq!(remaining, 0);
    }
}

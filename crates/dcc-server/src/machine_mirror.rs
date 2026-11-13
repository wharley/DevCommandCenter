//! A task that runs on another machine, kept here as a mirror: its
//! workspace, session and thread rows and its session events are copied
//! into this DCC's database, so the list and the conversation read it as
//! they read a local task. `dcc_machine_tasks` says which tasks are
//! mirrors, of which machine, and how far their events were copied.
//!
//! The machine is the source of truth: events are written here with the
//! sequence it gave them (never renumbered), and when it merged a run of
//! them (`reset`) the mirror drops its copy and reads the session again.
//! A mirror's paths are stored as `dcc-machine://<machine>/<path>`, which
//! is never a folder of this computer, so nothing reads local files for a
//! task that lives elsewhere, even when both machines use the same path.

use std::path::Path;

use dcc_core::domain::session::{SessionEventKind, SessionEventRecord};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

pub const MACHINE_PATH_SCHEME: &str = "dcc-machine://";

/// A path on a machine, as a mirror stores it.
pub fn machine_path(machine_id: &str, path: &str) -> String {
    if path.starts_with(MACHINE_PATH_SCHEME) {
        return path.to_string();
    }
    format!(
        "{MACHINE_PATH_SCHEME}{machine_id}/{}",
        path.trim_start_matches('/')
    )
}

/// The machine's own path of a mirror's path; `None` for a local one.
pub fn remote_path(stored: &str) -> Option<String> {
    let rest = stored.strip_prefix(MACHINE_PATH_SCHEME)?;
    let (_, path) = rest.split_once('/')?;
    Some(format!("/{path}"))
}

/// A mirrored task: which machine runs it and its session there.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MirroredTask {
    pub workspace_id: String,
    pub machine_id: String,
    pub session_id: String,
    /// The highest event sequence copied so far.
    pub cursor: u64,
}

pub fn ensure_schema(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS dcc_machine_tasks (
            workspace_id  TEXT PRIMARY KEY NOT NULL,
            machine_id    TEXT NOT NULL,
            session_id    TEXT NOT NULL,
            cursor        INTEGER NOT NULL DEFAULT 0,
            created_at    TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_dcc_machine_tasks_machine
            ON dcc_machine_tasks(machine_id);
        "#,
    )
}

pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    ensure_schema(&conn)?;
    Ok(conn)
}

fn read_task(row: &rusqlite::Row<'_>) -> rusqlite::Result<MirroredTask> {
    Ok(MirroredTask {
        workspace_id: row.get(0)?,
        machine_id: row.get(1)?,
        session_id: row.get(2)?,
        cursor: row.get::<_, i64>(3)?.max(0) as u64,
    })
}

const TASK_COLUMNS: &str = "workspace_id, machine_id, session_id, cursor";

/// Marks a task as a mirror; marking it again keeps how far it was copied.
pub fn record(
    conn: &Connection,
    workspace_id: &str,
    machine_id: &str,
    session_id: &str,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO dcc_machine_tasks (workspace_id, machine_id, session_id)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(workspace_id) DO UPDATE SET
             machine_id = excluded.machine_id,
             cursor = CASE WHEN session_id = excluded.session_id THEN cursor ELSE 0 END,
             session_id = excluded.session_id",
        params![workspace_id, machine_id, session_id],
    )?;
    Ok(())
}

pub fn list(conn: &Connection) -> rusqlite::Result<Vec<MirroredTask>> {
    let mut statement = conn.prepare(&format!(
        "SELECT {TASK_COLUMNS} FROM dcc_machine_tasks ORDER BY created_at, workspace_id"
    ))?;
    let tasks = statement.query_map([], read_task)?.collect();
    tasks
}

pub fn find(conn: &Connection, workspace_id: &str) -> rusqlite::Result<Option<MirroredTask>> {
    conn.query_row(
        &format!("SELECT {TASK_COLUMNS} FROM dcc_machine_tasks WHERE workspace_id = ?1"),
        params![workspace_id],
        read_task,
    )
    .optional()
}

/// Forgets the mirrors of a machine that was removed. Their rows stay as
/// plain history: the conversations are still readable, just no longer
/// followed.
pub fn forget_machine(conn: &Connection, machine_id: &str) -> rusqlite::Result<usize> {
    conn.execute(
        "DELETE FROM dcc_machine_tasks WHERE machine_id = ?1",
        params![machine_id],
    )
}

/// What a finished turn's event carries in its own columns, as the local
/// append writes them.
fn terminal(kind: &SessionEventKind) -> Option<(&str, &'static str)> {
    match kind {
        SessionEventKind::TurnCompleted { turn_id } => Some((turn_id.0.as_str(), "completed")),
        SessionEventKind::TurnAborted { turn_id, .. } => Some((turn_id.0.as_str(), "aborted")),
        _ => None,
    }
}

/// Copies events as the machine numbered them and moves the cursor to the
/// highest one. One already here (same id, or its sequence taken) is left
/// alone. Returns how many were new.
pub fn apply_events(
    conn: &mut Connection,
    workspace_id: &str,
    events: &[SessionEventRecord],
) -> Result<usize, String> {
    let transaction = conn.transaction().map_err(|error| error.to_string())?;
    let mut added = 0;
    let mut highest = 0u64;
    for event in events {
        let kind_json = serde_json::to_string(&event.kind).map_err(|error| error.to_string())?;
        let (turn, state) = terminal(&event.kind).unzip();
        added += transaction
            .execute(
                "INSERT OR IGNORE INTO dcc_session_events
                    (event_id, session_id, sequence, occurred_at, kind_json,
                     terminal_turn_id, terminal_kind)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    event.event_id,
                    event.session_id.0,
                    event.sequence as i64,
                    event.occurred_at,
                    kind_json,
                    turn,
                    state,
                ],
            )
            .map_err(|error| error.to_string())?;
        highest = highest.max(event.sequence);
    }
    transaction
        .execute(
            "UPDATE dcc_machine_tasks SET cursor = MAX(cursor, ?2) WHERE workspace_id = ?1",
            params![workspace_id, highest as i64],
        )
        .map_err(|error| error.to_string())?;
    transaction.commit().map_err(|error| error.to_string())?;
    Ok(added)
}

/// The machine rewrote part of the session (it merges a settled turn's
/// chunks): drop the copy and start from the beginning.
pub fn reset(conn: &mut Connection, workspace_id: &str, session_id: &str) -> Result<(), String> {
    let transaction = conn.transaction().map_err(|error| error.to_string())?;
    transaction
        .execute(
            "DELETE FROM dcc_session_events WHERE session_id = ?1",
            params![session_id],
        )
        .map_err(|error| error.to_string())?;
    transaction
        .execute(
            "UPDATE dcc_machine_tasks SET cursor = 0 WHERE workspace_id = ?1",
            params![workspace_id],
        )
        .map_err(|error| error.to_string())?;
    transaction.commit().map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use dcc_core::domain::session::{SessionId, TurnId};

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             CREATE TABLE dcc_sessions (id TEXT PRIMARY KEY NOT NULL);
             CREATE TABLE dcc_session_events (
                event_id TEXT PRIMARY KEY NOT NULL, session_id TEXT NOT NULL,
                sequence INTEGER NOT NULL, occurred_at TEXT NOT NULL, kind_json TEXT NOT NULL,
                terminal_turn_id TEXT NULL, terminal_kind TEXT NULL,
                UNIQUE(session_id, sequence),
                FOREIGN KEY (session_id) REFERENCES dcc_sessions(id) ON DELETE CASCADE);
             INSERT INTO dcc_sessions (id) VALUES ('s-1');",
        )
        .unwrap();
        ensure_schema(&conn).unwrap();
        conn
    }

    fn event(id: &str, sequence: u64, kind: SessionEventKind) -> SessionEventRecord {
        SessionEventRecord {
            event_id: id.into(),
            session_id: SessionId("s-1".into()),
            sequence,
            occurred_at: "2026-11-13T10:00:00Z".into(),
            kind,
        }
    }

    fn done(turn: &str) -> SessionEventKind {
        SessionEventKind::TurnCompleted {
            turn_id: TurnId(turn.into()),
        }
    }

    #[test]
    fn a_mirrors_path_is_never_a_local_folder_and_gives_the_machines_back() {
        let stored = machine_path("m-1", "/Users/gio/vendeagora-app");
        assert_eq!(stored, "dcc-machine://m-1/Users/gio/vendeagora-app");
        assert!(!Path::new(&stored).is_dir());
        assert_eq!(machine_path("m-1", &stored), stored, "not prefixed twice");
        assert_eq!(
            remote_path(&stored).as_deref(),
            Some("/Users/gio/vendeagora-app")
        );
        assert_eq!(remote_path("/Users/me/app"), None);
    }

    #[test]
    fn events_keep_the_machines_sequence_and_the_cursor_follows() {
        let mut conn = db();
        record(&conn, "w-1", "m-1", "s-1").unwrap();
        // The machine's numbering has a gap; the copy keeps it.
        let batch = [event("e-3", 3, done("t-1")), event("e-7", 7, done("t-2"))];
        assert_eq!(apply_events(&mut conn, "w-1", &batch).unwrap(), 2);
        // Read again (the same batch, overlapping the next): only what is new.
        let again = [event("e-7", 7, done("t-2")), event("e-8", 8, done("t-3"))];
        assert_eq!(apply_events(&mut conn, "w-1", &again).unwrap(), 1);
        let task = find(&conn, "w-1").unwrap().unwrap();
        assert_eq!((task.cursor, task.machine_id.as_str()), (8, "m-1"));
        let stored: Vec<(i64, String, String)> = conn
            .prepare("SELECT sequence, terminal_turn_id, terminal_kind FROM dcc_session_events ORDER BY sequence")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(
            stored,
            [
                (3, "t-1".to_string(), "completed".to_string()),
                (7, "t-2".to_string(), "completed".to_string()),
                (8, "t-3".to_string(), "completed".to_string()),
            ]
        );
        // Marking the task again does not lose its place.
        record(&conn, "w-1", "m-1", "s-1").unwrap();
        assert_eq!(find(&conn, "w-1").unwrap().unwrap().cursor, 8);
    }

    #[test]
    fn a_reset_drops_the_copy_and_a_removed_machine_stops_being_followed() {
        let mut conn = db();
        record(&conn, "w-1", "m-1", "s-1").unwrap();
        apply_events(&mut conn, "w-1", &[event("e-1", 1, done("t-1"))]).unwrap();
        reset(&mut conn, "w-1", "s-1").unwrap();
        assert_eq!(find(&conn, "w-1").unwrap().unwrap().cursor, 0);
        let left: i64 = conn
            .query_row("SELECT COUNT(*) FROM dcc_session_events", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(left, 0);
        assert_eq!(list(&conn).unwrap().len(), 1);
        assert_eq!(forget_machine(&conn, "m-1").unwrap(), 1);
        assert!(list(&conn).unwrap().is_empty());
    }
}

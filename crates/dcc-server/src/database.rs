//! The app's database, ready to use: its storage settings, the legacy column
//! migrations, the schema, the MCP, task and session tables, then a one-time
//! compaction.
//! Whoever hosts the backend opens it once at startup.

use std::path::Path;

use dcc_infra::mcp_db::SqliteMcpRepo;
use rusqlite::{params, Connection};

/// `src-tauri/sql/schema.sql` (CREATE IF NOT EXISTS).
pub const APP_SCHEMA_SQL: &str = include_str!("../../../src-tauri/sql/schema.sql");

pub fn open_app_database(db_path: &Path) -> Result<Connection, String> {
    let conn = Connection::open(db_path).map_err(|e| e.to_string())?;
    // Before journal_mode: a new file only takes auto_vacuum while empty.
    dcc_infra::db::limit_storage_growth(&conn).map_err(|e| e.to_string())?;
    conn.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous = NORMAL;
        PRAGMA foreign_keys = ON;
        PRAGMA busy_timeout = 5000;
        ",
    )
    .map_err(|e| e.to_string())?;
    run_legacy_schema_migrations(&conn)
        .map_err(|e| format!("failed to migrate legacy schema: {e}"))?;
    conn.execute_batch(APP_SCHEMA_SQL)
        .map_err(|e| format!("failed to apply schema: {e}"))?;
    SqliteMcpRepo::apply_migrations(&conn)
        .map_err(|e| format!("failed to apply MCP schema: {e}"))?;
    // The tasks' and sessions' tables, so a first run already has every table
    // the readers expect (the menu bar's snapshot joins them).
    dcc_infra::db::SqliteWorkspaceRepo::open(db_path)
        .map_err(|e| format!("failed to apply the task schema: {e}"))?;
    dcc_infra::db::SqliteSessionRepo::open(db_path)
        .map_err(|e| format!("failed to apply the session schema: {e}"))?;
    Ok(conn)
}

/// Once per database: nothing writes yet, so the rewrite cannot make an
/// agent wait. Later frees go back in slices (the compaction worker).
pub fn compact_once(conn: &Connection) {
    match dcc_infra::db::vacuum_into_incremental_mode(conn) {
        Ok(Some(outcome)) => eprintln!(
            "[DCC] database compacted: {} MB -> {} MB",
            outcome.before_bytes / (1024 * 1024),
            outcome.after_bytes / (1024 * 1024)
        ),
        Ok(None) => {}
        Err(error) => eprintln!("[DCC] database compaction skipped: {error}"),
    }
}

fn table_has_column(conn: &Connection, table: &str, column: &str) -> Result<bool, String> {
    let pragma = format!("PRAGMA table_info({table})");
    let mut stmt = conn.prepare(&pragma).map_err(|e| e.to_string())?;
    let mut rows = stmt.query([]).map_err(|e| e.to_string())?;
    while let Some(row) = rows.next().map_err(|e| e.to_string())? {
        let name: String = row.get(1).map_err(|e| e.to_string())?;
        if name == column {
            return Ok(true);
        }
    }
    Ok(false)
}

fn table_exists(conn: &Connection, table: &str) -> Result<bool, String> {
    let mut stmt = conn
        .prepare("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1 LIMIT 1")
        .map_err(|e| e.to_string())?;
    stmt.exists(params![table]).map_err(|e| e.to_string())
}

fn ensure_column(
    conn: &Connection,
    table: &str,
    column: &str,
    sql_type: &str,
) -> Result<(), String> {
    if !table_exists(conn, table)? {
        return Ok(());
    }
    if table_has_column(conn, table, column)? {
        return Ok(());
    }
    let sql = format!("ALTER TABLE {table} ADD COLUMN {column} {sql_type}");
    conn.execute(&sql, []).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn run_legacy_schema_migrations(conn: &Connection) -> Result<(), String> {
    // Providers
    ensure_column(conn, "providers", "api_key_encrypted", "BLOB")?;
    ensure_column(conn, "providers", "cli_path", "TEXT")?;
    ensure_column(conn, "providers", "config", "TEXT")?;

    // Projects
    ensure_column(conn, "projects", "git_remote_url", "TEXT")?;
    ensure_column(conn, "projects", "repo_config", "TEXT")?;
    ensure_column(conn, "projects", "last_opened_at", "TEXT")?;

    // Combs
    ensure_column(conn, "combs", "review_targets", "TEXT")?;
    ensure_column(conn, "combs", "branch", "TEXT")?;
    ensure_column(conn, "combs", "worktree_path", "TEXT")?;
    ensure_column(conn, "combs", "status", "TEXT NOT NULL DEFAULT 'active'")?;
    ensure_column(conn, "combs", "last_opened_at", "TEXT")?;
    ensure_column(conn, "combs", "is_pinned", "INTEGER DEFAULT 0")?;
    ensure_column(conn, "combs", "pinned_at", "TEXT")?;
    ensure_column(conn, "combs", "last_git_activity_at", "TEXT")?;
    ensure_column(conn, "combs", "forge_link", "TEXT")?;

    // Panes
    ensure_column(conn, "panes", "pty_owner_key", "TEXT")?;
    ensure_column(conn, "panes", "last_activity_at", "TEXT")?;

    Ok(())
}

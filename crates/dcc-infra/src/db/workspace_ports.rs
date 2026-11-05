//! Each task gets a block of ports for what it runs (`DCC_PORT` up to
//! `DCC_PORT + 9`), so two tasks of one project can serve side by side.
//!
//! The block is taken the first time the task needs it, not when the task
//! is created, so creating a task costs nothing. It stays with the task
//! between launches and goes back when the task row is deleted (the foreign
//! key cascades); a completed or archived task's block can be taken by a new
//! task, and the old one gets a fresh block if it is ever reopened.

use super::*;

pub const PORT_BLOCK_START: u16 = 41_000;
pub const PORT_BLOCK_SIZE: u16 = 10;
const PORT_BLOCK_END: u16 = 60_000;

pub(super) const WORKSPACE_PORTS_TABLE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS dcc_workspace_ports (
    workspace_id TEXT PRIMARY KEY NOT NULL,
    port_base INTEGER NOT NULL UNIQUE,
    FOREIGN KEY (workspace_id) REFERENCES dcc_workspaces(id) ON DELETE CASCADE
);
"#;

fn repository_error(error: impl ToString) -> dcc_core::CoreError {
    dcc_core::CoreError::Repository(error.to_string())
}

/// Every port of the block can be bound on this machine right now.
pub fn port_block_is_free(base: u16) -> bool {
    (base..base + PORT_BLOCK_SIZE)
        .all(|port| std::net::TcpListener::bind(("127.0.0.1", port)).is_ok())
}

impl SqliteWorkspaceRepo {
    /// The task's block, taken now if it has none.
    pub fn workspace_port_base(&self, id: &WorkspaceId) -> Result<u16> {
        self.workspace_port_base_with(id, port_block_is_free)
    }

    pub(crate) fn workspace_port_base_with(
        &self,
        id: &WorkspaceId,
        is_free: impl Fn(u16) -> bool,
    ) -> Result<u16> {
        let conn = self.conn.lock().map_err(repository_error)?;
        let held: Option<u16> = conn
            .query_row(
                "SELECT port_base FROM dcc_workspace_ports WHERE workspace_id = ?1",
                [&id.0],
                |row| row.get(0),
            )
            .optional()
            .map_err(repository_error)?;
        if let Some(base) = held {
            return Ok(base);
        }
        let mut statement = conn
            .prepare(
                "SELECT p.port_base FROM dcc_workspace_ports p
                 JOIN dcc_workspaces w ON w.id = p.workspace_id
                 WHERE w.state NOT IN ('completed', 'archived')",
            )
            .map_err(repository_error)?;
        let taken = statement
            .query_map([], |row| row.get::<_, u16>(0))
            .map_err(repository_error)?
            .collect::<rusqlite::Result<BTreeSet<u16>>>()
            .map_err(repository_error)?;
        drop(statement);
        let base = (PORT_BLOCK_START..PORT_BLOCK_END)
            .step_by(PORT_BLOCK_SIZE as usize)
            .find(|base| !taken.contains(base) && is_free(*base))
            .ok_or_else(|| repository_error("no free port block left"))?;
        // A finished task may still hold it; the new task takes it over.
        conn.execute(
            "DELETE FROM dcc_workspace_ports WHERE port_base = ?1",
            [base],
        )
        .map_err(repository_error)?;
        conn.execute(
            "INSERT INTO dcc_workspace_ports (workspace_id, port_base) VALUES (?1, ?2)",
            params![id.0, base],
        )
        .map_err(repository_error)?;
        Ok(base)
    }
}

#[cfg(test)]
#[path = "workspace_ports_tests.rs"]
mod tests;

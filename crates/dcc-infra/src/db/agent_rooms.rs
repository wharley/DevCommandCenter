//! The room an agent's conversation belongs to (`docs/PLANO_SALA_POR_PROJETO`):
//! every conversation of an agent runs in its home; the ones of a project's
//! room are marked here with the project, the rest are the Geral room's.

use super::*;

pub(super) const AGENT_ROOM_TABLE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS dcc_agent_rooms (
    session_id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL,
    created_at TEXT NOT NULL
);
-- Who answers in a room when nothing else says who; '' is the Geral room.
CREATE TABLE IF NOT EXISTS dcc_room_leads (
    room TEXT PRIMARY KEY NOT NULL,
    agent_id TEXT NOT NULL
);
"#;

fn repository_error(error: impl ToString) -> dcc_core::CoreError {
    dcc_core::CoreError::Repository(error.to_string())
}

impl SqliteSessionRepo {
    /// The agent the person set to answer in `room` (none: Geral) when
    /// nothing else says who.
    pub fn room_lead(&self, room: Option<&str>) -> Result<Option<String>> {
        let conn = self.conn.lock().map_err(repository_error)?;
        let found = conn.query_row(
            "SELECT agent_id FROM dcc_room_leads WHERE room = ?1",
            params![room.unwrap_or_default()],
            |row| row.get(0),
        );
        match found.optional() {
            Ok(lead) => Ok(lead),
            // A database from before the table: none set.
            Err(_) => Ok(None),
        }
    }

    /// Sets who answers in `room` by default; none: back to DCC's own pick.
    pub fn set_room_lead(&self, room: Option<&str>, agent_id: Option<&str>) -> Result<()> {
        let conn = self.conn.lock().map_err(repository_error)?;
        let room = room.unwrap_or_default();
        match agent_id.map(str::trim).filter(|id| !id.is_empty()) {
            Some(agent_id) => conn.execute(
                "INSERT INTO dcc_room_leads (room, agent_id) VALUES (?1, ?2)
                 ON CONFLICT(room) DO UPDATE SET agent_id = excluded.agent_id",
                params![room, agent_id],
            ),
            None => conn.execute("DELETE FROM dcc_room_leads WHERE room = ?1", params![room]),
        }
        .map_err(repository_error)?;
        Ok(())
    }

    /// Marks `session_id` as the conversation of `project_id`'s room.
    pub fn set_session_room(&self, session_id: &SessionId, project_id: &str) -> Result<()> {
        let project_id = project_id.trim();
        if project_id.is_empty() {
            return Ok(());
        }
        let conn = self.conn.lock().map_err(repository_error)?;
        let now = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
        conn.execute(
            "INSERT INTO dcc_agent_rooms (session_id, project_id, created_at) VALUES (?1, ?2, ?3) ON CONFLICT(session_id) DO NOTHING",
            params![session_id.0, project_id, now],
        )
        .map_err(repository_error)?;
        Ok(())
    }

    /// The project whose room `session_id` is in; none: the Geral room.
    pub fn session_room(&self, session_id: &SessionId) -> Result<Option<String>> {
        let conn = self.conn.lock().map_err(repository_error)?;
        conn.query_row(
            "SELECT project_id FROM dcc_agent_rooms WHERE session_id = ?1",
            params![session_id.0],
            |row| row.get(0),
        )
        .optional()
        .map_err(repository_error)
    }

    /// The project's name (as the sidebar shows it) and folder.
    pub fn project_name_and_path(&self, project_id: &str) -> Option<(String, String)> {
        let conn = self.conn.lock().ok()?;
        conn.query_row(
            "SELECT COALESCE(NULLIF(display_name, ''), name), root_path FROM dcc_repositories
             WHERE project_id = ?1 ORDER BY updated_at DESC LIMIT 1",
            params![project_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .ok()
    }

    /// When `session_id`'s last turn ended (completed or stopped), without
    /// reading its history.
    pub fn last_turn_end(&self, session_id: &SessionId) -> Result<Option<String>> {
        let conn = self.conn.lock().map_err(repository_error)?;
        conn.query_row(
            "SELECT occurred_at FROM dcc_session_events
              WHERE session_id = ?1
                AND json_extract(kind_json, '$.type') IN ('turn_completed', 'turn_aborted')
              ORDER BY sequence DESC LIMIT 1",
            params![session_id.0],
            |row| row.get(0),
        )
        .optional()
        .map_err(repository_error)
    }

    /// A task's name, as the sidebar shows it.
    pub fn workspace_name(&self, workspace_id: &str) -> Option<String> {
        let conn = self.conn.lock().ok()?;
        conn.query_row(
            "SELECT name FROM dcc_workspaces WHERE id = ?1",
            params![workspace_id],
            |row| row.get::<_, String>(0),
        )
        .ok()
        .filter(|name| !name.trim().is_empty())
    }

    /// What tells a conversation's history changed, without reading it:
    /// how many events, the highest sequence and the newest row (a rewind
    /// that drops events and writes new ones moves the last). The phone's
    /// rooms keep a conversation's lines until this moves.
    pub fn session_event_stamp(&self, session_id: &SessionId) -> Result<(i64, i64, i64)> {
        let conn = self.conn.lock().map_err(repository_error)?;
        conn.query_row(
            "SELECT COUNT(*), COALESCE(MAX(sequence), 0), COALESCE(MAX(rowid), 0)
               FROM dcc_session_events WHERE session_id = ?1",
            params![session_id.0],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(repository_error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_conversation_keeps_its_room() {
        let repo = SqliteSessionRepo::from_connection(Arc::new(Mutex::new(
            Connection::open_in_memory().unwrap(),
        )))
        .unwrap();
        let session = SessionId("s-1".into());
        assert_eq!(repo.session_room(&session).unwrap(), None);
        repo.set_session_room(&session, "checkout").unwrap();
        repo.set_session_room(&session, "other").unwrap();
        assert_eq!(
            repo.session_room(&session).unwrap().as_deref(),
            Some("checkout")
        );
        // A blank project is the Geral room: nothing kept.
        repo.set_session_room(&SessionId("s-2".into()), " ")
            .unwrap();
        assert_eq!(repo.session_room(&SessionId("s-2".into())).unwrap(), None);
        assert_eq!(repo.project_name_and_path("nope"), None);
        assert_eq!(repo.session_event_stamp(&session).unwrap(), (0, 0, 0));
        assert_eq!(repo.last_turn_end(&session).unwrap(), None);
    }
}

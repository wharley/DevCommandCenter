//! Whether a session is a resident agent's own conversation
//! (`dcc_core::domain::agent_conversation`): its project is that agent's home.

use super::*;
use dcc_core::domain::agent_conversation::home_project_id;

impl SqliteSessionRepo {
    /// True when `session_id` runs in `agent_id`'s home.
    pub fn is_agent_home_session(&self, session_id: &SessionId, agent_id: &str) -> Result<bool> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM dcc_sessions WHERE id = ?1 AND project_id = ?2)",
            params![session_id.0, home_project_id(agent_id)],
            |row| row.get(0),
        )
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))
    }
}

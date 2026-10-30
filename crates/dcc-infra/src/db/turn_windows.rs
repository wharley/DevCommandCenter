//! When a conversation's turns ran: each `turn_started` with the time its
//! turn ended (completed or aborted), or none while it has not.

use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TurnWindow {
    pub session_id: SessionId,
    /// RFC 3339, as the events store it.
    pub started_at: String,
    pub ended_at: Option<String>,
}

impl SqliteSessionRepo {
    /// Every turn of these sessions, oldest first per session.
    pub fn turn_windows(&self, session_ids: &[SessionId]) -> Result<Vec<TurnWindow>> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let mut statement = conn
            .prepare(
                "SELECT s.occurred_at,
                        (SELECT t.occurred_at FROM dcc_session_events t
                          WHERE t.session_id = s.session_id
                            AND t.terminal_turn_id = json_extract(s.kind_json, '$.turnId')
                          LIMIT 1)
                   FROM dcc_session_events s
                  WHERE s.session_id = ?1
                    AND json_extract(s.kind_json, '$.type') = 'turn_started'
                  ORDER BY s.sequence",
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let mut windows = Vec::new();
        for session_id in session_ids {
            let rows = statement
                .query_map([&session_id.0], |row| {
                    Ok(TurnWindow {
                        session_id: session_id.clone(),
                        started_at: row.get(0)?,
                        ended_at: row.get(1)?,
                    })
                })
                .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
            for row in rows {
                windows
                    .push(row.map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?);
            }
        }
        Ok(windows)
    }
}

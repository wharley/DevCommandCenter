//! When a running session last showed a sign of life, for the agents'
//! check-in (a task with a turn open and no event for a while is stalled).
//! One indexed read per running session: `(session_id, sequence)`.

use super::*;

impl SqliteSessionRepo {
    /// `occurred_at` of the session's newest event, if it has any.
    pub fn last_event_at_sync(&self, session_id: &SessionId) -> Result<Option<String>> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        conn.query_row(
            "SELECT occurred_at FROM dcc_session_events WHERE session_id = ?1 ORDER BY sequence DESC LIMIT 1",
            params![session_id.0.clone()],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_without_events_has_no_last_sign_of_life() {
        let repo = SqliteSessionRepo::from_connection(Arc::new(Mutex::new(
            Connection::open_in_memory().unwrap(),
        )))
        .unwrap();
        assert_eq!(
            repo.last_event_at_sync(&SessionId("none".into())).unwrap(),
            None
        );
    }
}

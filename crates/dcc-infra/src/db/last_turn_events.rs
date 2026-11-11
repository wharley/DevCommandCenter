//! A conversation's last turn only, for a notice about it (the native
//! room's `room_notices`): its `turn_started` and every event after it, so a
//! long conversation costs no more to read than its last turn.

use super::*;

/// Events read at most, past the last turn's start.
const LAST_TURN_LIMIT: usize = 50_000;

impl SqliteSessionRepo {
    /// The events from the last `turn_started` on, oldest first; all of them
    /// when the session has no turn yet.
    pub fn last_turn_events_sync(&self, session_id: &SessionId) -> Result<Vec<SessionEventRecord>> {
        let start: Option<i64> = {
            let conn = self
                .conn
                .lock()
                .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
            conn.query_row(
                r#"SELECT MAX(sequence) FROM dcc_session_events
                    WHERE session_id = ?1 AND kind_json LIKE '%"type":"turn_started"%'"#,
                params![session_id.0.clone()],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?
            .flatten()
        };
        let after = start.map_or(0, |sequence| sequence.saturating_sub(1).max(0) as u64);
        self.list_events_by_session_after_sync(session_id, after, LAST_TURN_LIMIT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_last_turn_is_read() {
        let repo = SqliteSessionRepo::from_connection(Arc::new(Mutex::new(
            Connection::open_in_memory().unwrap(),
        )))
        .unwrap();
        let session = SessionId("s".into());
        assert!(repo.last_turn_events_sync(&session).unwrap().is_empty());
        let kinds = [
            r#"{"type":"turn_started","turnId":"t1","prompt":"a","planMode":false,"model":null,"evidence":null,"retryOfTurnId":null,"approvalPolicy":"ask"}"#,
            r#"{"type":"turn_completed","turnId":"t1"}"#,
            r#"{"type":"turn_started","turnId":"t2","prompt":"b","planMode":false,"model":null,"evidence":null,"retryOfTurnId":null,"approvalPolicy":"ask"}"#,
            r#"{"type":"turn_completed","turnId":"t2"}"#,
        ];
        {
            let conn = repo.conn.lock().unwrap();
            conn.execute_batch("PRAGMA foreign_keys = OFF").unwrap();
            for (index, kind) in kinds.iter().enumerate() {
                conn.execute(
                    "INSERT INTO dcc_session_events (event_id, session_id, sequence, occurred_at, kind_json) VALUES (?1, 's', ?2, 't', ?3)",
                    params![format!("e{index}"), index as i64 + 1, kind],
                )
                .unwrap();
            }
        }
        let events = repo.last_turn_events_sync(&session).unwrap();
        let sequences: Vec<u64> = events.iter().map(|event| event.sequence).collect();
        assert_eq!(sequences, vec![3, 4]);
    }
}

//! Streaming providers persist a turn as thousands of tiny deltas (91% of the
//! rows on a real database). Once a turn has settled, adjacent deltas of the
//! same item are merged into one row: the rule the timeline projection already
//! applies when it renders, so what the conversation shows does not change.
//!
//! Safety rules, each tied to a reader:
//! - only runs that end before their turn's terminal event, so the session's
//!   highest sequence (the next append's base and every client cursor's
//!   ceiling) never drops;
//! - only turns settled for [`DELTA_COMPACTION_SETTLE_MINUTES`], after the
//!   desktop's live buffer let go of their chunks;
//! - merged content stays under [`MERGED_CONTENT_MAX_BYTES`], far below the
//!   live snapshot's per-record limit;
//! - the kept row records the last sequence it absorbed (`compacted_through`),
//!   so a client whose cursor sits inside a merged run is told to reload
//!   instead of silently missing the rest ([`SqliteSessionRepo::cursor_inside_compacted_run_sync`]).

use super::*;

pub const DELTA_COMPACTION_SETTLE_MINUTES: i64 = 10;
const MERGED_CONTENT_MAX_BYTES: usize = 128 * 1024;

pub(super) const DELTA_COMPACTION_TABLE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS dcc_session_delta_compaction (
	session_id TEXT PRIMARY KEY NOT NULL,
	scanned_through INTEGER NOT NULL,
	FOREIGN KEY (session_id) REFERENCES dcc_sessions(id) ON DELETE CASCADE
);
"#;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct DeltaCompactionStep {
    pub merged_rows: usize,
    pub deleted_rows: usize,
    /// The session still has work; call again.
    pub more: bool,
}

struct StoredEvent {
    event_id: String,
    sequence: i64,
    kind_json: String,
    compacted_through: Option<i64>,
    kind: Option<SessionEventKind>,
}

/// What makes two deltas mergeable: same kind, turn and item.
fn delta_target(kind: &SessionEventKind) -> Option<(&'static str, &str, &str, &str)> {
    match kind {
        SessionEventKind::TurnDelta { turn_id, content } => {
            Some(("turn_delta", turn_id.0.as_str(), "", content.as_str()))
        }
        SessionEventKind::TurnAssistantMessageDelta {
            turn_id,
            message_id,
            content,
        } => Some((
            "turn_assistant_message_delta",
            turn_id.0.as_str(),
            message_id.as_str(),
            content.as_str(),
        )),
        SessionEventKind::TurnReasoningDelta {
            turn_id,
            reasoning_id,
            content,
        } => Some((
            "turn_reasoning_delta",
            turn_id.0.as_str(),
            reasoning_id.as_str(),
            content.as_str(),
        )),
        SessionEventKind::TurnToolCallDelta {
            turn_id,
            tool_call_id,
            content,
        } => Some((
            "turn_tool_call_delta",
            turn_id.0.as_str(),
            tool_call_id.as_str(),
            content.as_str(),
        )),
        _ => None,
    }
}

fn with_content(kind: &SessionEventKind, merged: String) -> SessionEventKind {
    let mut kind = kind.clone();
    match &mut kind {
        SessionEventKind::TurnDelta { content, .. }
        | SessionEventKind::TurnAssistantMessageDelta { content, .. }
        | SessionEventKind::TurnReasoningDelta { content, .. }
        | SessionEventKind::TurnToolCallDelta { content, .. } => *content = merged,
        _ => {}
    }
    kind
}

/// The turn an event belongs to, for deciding whether it may still change.
fn turn_of(kind: &SessionEventKind) -> Option<&str> {
    kind.turn_id().map(|turn_id| turn_id.0.as_str())
}

struct Group {
    rows: Vec<usize>,
    content: String,
}

fn repository_error(error: impl ToString) -> dcc_core::CoreError {
    dcc_core::CoreError::Repository(error.to_string())
}

impl SqliteSessionRepo {
    /// Sessions with events the compaction has not settled yet.
    pub fn list_sessions_pending_delta_compaction_sync(&self) -> Result<Vec<SessionId>> {
        let conn = self.conn.lock().map_err(repository_error)?;
        let mut statement = conn
            .prepare(
                r#"
                SELECT s.id
                  FROM dcc_sessions s
                  LEFT JOIN dcc_session_delta_compaction p ON p.session_id = s.id
                 WHERE (SELECT MAX(e.sequence) FROM dcc_session_events e WHERE e.session_id = s.id)
                       > COALESCE(p.scanned_through, 0)
                 ORDER BY s.created_at ASC
                "#,
            )
            .map_err(repository_error)?;
        let ids = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(repository_error)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(repository_error)?;
        Ok(ids.into_iter().map(SessionId).collect())
    }

    /// Merges settled delta runs of one session, deleting at most about
    /// `max_deleted` rows in one short write transaction.
    pub fn compact_session_deltas_step(
        &self,
        session_id: &SessionId,
        now: DateTime<Utc>,
        max_deleted: usize,
    ) -> Result<DeltaCompactionStep> {
        let mut conn = self.conn.lock().map_err(repository_error)?;
        let scanned_through: i64 = conn
            .query_row(
                "SELECT scanned_through FROM dcc_session_delta_compaction WHERE session_id = ?1",
                params![session_id.0],
                |row| row.get(0),
            )
            .optional()
            .map_err(repository_error)?
            .unwrap_or(0);

        // Reading needs no write lock: appends only add higher sequences, and
        // every write below re-checks the row it read.
        let rows = {
            let mut statement = conn
                .prepare(
                    r#"
                    SELECT event_id, sequence, kind_json, compacted_through
                      FROM dcc_session_events
                     WHERE session_id = ?1 AND sequence > ?2
                     ORDER BY sequence ASC
                    "#,
                )
                .map_err(repository_error)?;
            let rows = statement
                .query_map(params![session_id.0, scanned_through], |row| {
                    let kind_json = row.get::<_, String>(2)?;
                    Ok(StoredEvent {
                        event_id: row.get(0)?,
                        sequence: row.get(1)?,
                        kind: from_str::<SessionEventKind>(&kind_json).ok(),
                        kind_json,
                        compacted_through: row.get(3)?,
                    })
                })
                .map_err(repository_error)?
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(repository_error)?;
            rows
        };
        if rows.is_empty() {
            return Ok(DeltaCompactionStep::default());
        }

        // Settled turns: the sequence of their terminal event.
        let settled_before = now - Duration::minutes(DELTA_COMPACTION_SETTLE_MINUTES);
        let mut terminal_sequence = HashMap::<String, i64>::new();
        let mut settled = HashMap::<String, bool>::new();
        {
            let mut statement = conn
                .prepare(
                    r#"
                    SELECT terminal_turn_id, sequence, occurred_at
                      FROM dcc_session_events
                     WHERE session_id = ?1 AND terminal_turn_id IS NOT NULL
                    "#,
                )
                .map_err(repository_error)?;
            let terminals = statement
                .query_map(params![session_id.0], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })
                .map_err(repository_error)?
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(repository_error)?;
            for (turn_id, sequence, occurred_at) in terminals {
                let is_settled = DateTime::parse_from_rfc3339(&occurred_at)
                    .map(|at| at.with_timezone(&Utc) <= settled_before)
                    .unwrap_or(false);
                terminal_sequence.insert(turn_id.clone(), sequence);
                settled.insert(turn_id, is_settled);
            }
        }
        let mergeable = |row: &StoredEvent| -> bool {
            let Some(kind) = &row.kind else { return false };
            let Some((_, turn_id, _, _)) = delta_target(kind) else {
                return false;
            };
            settled.get(turn_id).copied().unwrap_or(false)
                && terminal_sequence
                    .get(turn_id)
                    .is_some_and(|terminal| row.sequence < *terminal)
        };

        // Everything before the first event of a turn that may still change
        // is final; the next pass starts there.
        let open_from = rows.iter().find(|row| {
            row.kind
                .as_ref()
                .and_then(turn_of)
                .is_some_and(|turn_id| !settled.get(turn_id).copied().unwrap_or(false))
        });
        let boundary = open_from
            .map(|row| row.sequence - 1)
            .unwrap_or_else(|| rows.last().map(|row| row.sequence).unwrap_or(0));

        let mut groups = Vec::<Group>::new();
        let mut current: Option<(Group, (&'static str, String, String))> = None;
        let mut planned_deletes = 0usize;
        let mut truncated = false;
        for (index, row) in rows.iter().enumerate() {
            let target = row
                .kind
                .as_ref()
                .filter(|_| mergeable(row))
                .and_then(delta_target);
            let Some((tag, turn_id, item_id, content)) = target else {
                if let Some((group, _)) = current.take() {
                    groups.push(group);
                }
                continue;
            };
            let key = (tag, turn_id.to_string(), item_id.to_string());
            match &mut current {
                Some((group, current_key))
                    if *current_key == key
                        && group.content.len() + content.len() <= MERGED_CONTENT_MAX_BYTES =>
                {
                    group.rows.push(index);
                    group.content.push_str(content);
                }
                _ => {
                    if let Some((group, _)) = current.take() {
                        groups.push(group);
                    }
                    if planned_deletes >= max_deleted {
                        truncated = true;
                        break;
                    }
                    current = Some((
                        Group {
                            rows: vec![index],
                            content: content.to_string(),
                        },
                        key,
                    ));
                }
            }
            if let Some((group, _)) = &current {
                if group.rows.len() > 1 {
                    planned_deletes += 1;
                }
            }
        }
        if let Some((group, _)) = current.take() {
            groups.push(group);
        }
        groups.retain(|group| group.rows.len() > 1);

        let transaction = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(repository_error)?;
        let mut step = DeltaCompactionStep::default();
        for group in &groups {
            let first = &rows[group.rows[0]];
            let last_sequence = rows[*group.rows.last().unwrap()]
                .sequence
                .max(first.compacted_through.unwrap_or(0));
            let kind = first.kind.as_ref().expect("grouped rows are parsed deltas");
            let merged_json =
                to_string(&with_content(kind, group.content.clone())).map_err(repository_error)?;
            let updated = transaction
                .execute(
                    r#"
                    UPDATE dcc_session_events
                       SET kind_json = ?1, compacted_through = ?2
                     WHERE event_id = ?3 AND kind_json = ?4
                    "#,
                    params![merged_json, last_sequence, first.event_id, first.kind_json],
                )
                .map_err(repository_error)?;
            if updated != 1 {
                // The row changed under us; leave everything for the next pass.
                return Ok(DeltaCompactionStep::default());
            }
            for &index in &group.rows[1..] {
                let row = &rows[index];
                transaction
                    .execute(
                        "DELETE FROM dcc_session_events WHERE event_id = ?1 AND kind_json = ?2",
                        params![row.event_id, row.kind_json],
                    )
                    .map_err(repository_error)?;
                step.deleted_rows += 1;
            }
            step.merged_rows += 1;
        }
        if truncated {
            step.more = true;
        } else {
            transaction
                .execute(
                    r#"
                    INSERT INTO dcc_session_delta_compaction (session_id, scanned_through)
                    VALUES (?1, ?2)
                    ON CONFLICT(session_id) DO UPDATE SET scanned_through = excluded.scanned_through
                    "#,
                    params![session_id.0, boundary.max(scanned_through)],
                )
                .map_err(repository_error)?;
        }
        transaction.commit().map_err(repository_error)?;
        Ok(step)
    }

    /// True when `after` points inside a run that was merged into an earlier
    /// row: a client holding that cursor never saw the rest of the run and
    /// must reload the session instead of asking for what follows.
    pub fn cursor_inside_compacted_run_sync(
        &self,
        session_id: &SessionId,
        after: u64,
    ) -> Result<bool> {
        let after = i64::try_from(after).map_err(repository_error)?;
        let conn = self.conn.lock().map_err(repository_error)?;
        conn.query_row(
            r#"
            SELECT EXISTS (
                SELECT 1 FROM dcc_session_events
                 WHERE session_id = ?1 AND sequence <= ?2 AND compacted_through > ?2
            )
            "#,
            params![session_id.0, after],
            |row| row.get::<_, bool>(0),
        )
        .map_err(repository_error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dcc_core::ports::{SessionEventRepo, SessionRepo};

    const SESSION: &str = "compaction-session";

    fn repo() -> SqliteSessionRepo {
        let repo = SqliteSessionRepo::from_connection(Arc::new(Mutex::new(
            Connection::open_in_memory().unwrap(),
        )))
        .unwrap();
        futures::executor::block_on(repo.save_session(&Session {
            id: SessionId(SESSION.to_owned()),
            project_id: ProjectId("project".to_owned()),
            workspace_id: WorkspaceId("workspace".to_owned()),
            additional_workspace_ids: Vec::new(),
            provider_id: "fixture".to_owned(),
            model: None,
            provider_runtime: None,
            working_directory_override: None,
            state: SessionState::Active,
            created_at: "t0".to_owned(),
            updated_at: "t0".to_owned(),
        }))
        .unwrap();
        repo
    }

    fn append(repo: &SqliteSessionRepo, occurred_at: &str, kind: SessionEventKind) {
        futures::executor::block_on(repo.append_event(&SessionEventRecord {
            event_id: uuid::Uuid::new_v4().to_string(),
            session_id: SessionId(SESSION.to_owned()),
            sequence: 0,
            occurred_at: occurred_at.to_owned(),
            kind,
        }))
        .unwrap();
    }

    fn message(turn: &str, id: &str, content: &str) -> SessionEventKind {
        SessionEventKind::TurnAssistantMessageDelta {
            turn_id: TurnId(turn.to_owned()),
            message_id: id.to_owned(),
            content: content.to_owned(),
        }
    }

    fn tool(turn: &str, id: &str, content: &str) -> SessionEventKind {
        SessionEventKind::TurnToolCallDelta {
            turn_id: TurnId(turn.to_owned()),
            tool_call_id: id.to_owned(),
            content: content.to_owned(),
        }
    }

    fn completed(turn: &str) -> SessionEventKind {
        SessionEventKind::TurnCompleted {
            turn_id: TurnId(turn.to_owned()),
        }
    }

    fn events(repo: &SqliteSessionRepo) -> Vec<(u64, serde_json::Value)> {
        repo.list_events_by_session_sync(&SessionId(SESSION.to_owned()))
            .unwrap()
            .into_iter()
            .map(|event| (event.sequence, j(event.kind)))
            .collect()
    }

    fn j(kind: SessionEventKind) -> serde_json::Value {
        serde_json::to_value(kind).unwrap()
    }

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-10-16T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    const OLD: &str = "2026-10-16T11:00:00Z";
    const RECENT: &str = "2026-10-16T11:55:00Z";

    #[test]
    fn merges_adjacent_deltas_of_settled_turns_only() {
        let repo = repo();
        append(&repo, OLD, message("t1", "m1", "Hel"));
        append(&repo, OLD, message("t1", "m1", "lo"));
        append(&repo, OLD, tool("t1", "c1", "out"));
        append(&repo, OLD, tool("t1", "c1", "put"));
        append(&repo, OLD, message("t1", "m1", "!"));
        append(&repo, OLD, completed("t1"));
        append(&repo, RECENT, message("t2", "m2", "still "));
        append(&repo, RECENT, message("t2", "m2", "fresh"));
        append(&repo, RECENT, completed("t2"));

        let session = SessionId(SESSION.to_owned());
        let step = repo
            .compact_session_deltas_step(&session, now(), 1000)
            .unwrap();
        assert_eq!(
            step,
            DeltaCompactionStep {
                merged_rows: 2,
                deleted_rows: 2,
                more: false
            }
        );
        assert_eq!(
            events(&repo),
            vec![
                (1, j(message("t1", "m1", "Hello"))),
                (3, j(tool("t1", "c1", "output"))),
                (5, j(message("t1", "m1", "!"))),
                (6, j(completed("t1"))),
                (7, j(message("t2", "m2", "still "))),
                (8, j(message("t2", "m2", "fresh"))),
                (9, j(completed("t2"))),
            ]
        );

        // A cursor inside a merged run must reload; one past it may continue.
        assert!(repo.cursor_inside_compacted_run_sync(&session, 1).unwrap());
        assert!(!repo.cursor_inside_compacted_run_sync(&session, 2).unwrap());
        assert!(!repo.cursor_inside_compacted_run_sync(&session, 8).unwrap());

        // Once t2 settles the next pass picks it up from where t1 ended.
        let later = now() + Duration::minutes(DELTA_COMPACTION_SETTLE_MINUTES);
        assert_eq!(
            repo.list_sessions_pending_delta_compaction_sync().unwrap(),
            vec![session.clone()]
        );
        let step = repo
            .compact_session_deltas_step(&session, later, 1000)
            .unwrap();
        assert_eq!(step.deleted_rows, 1);
        assert_eq!(events(&repo)[4], (7, j(message("t2", "m2", "still fresh"))));
        assert!(repo
            .list_sessions_pending_delta_compaction_sync()
            .unwrap()
            .is_empty());
    }

    #[test]
    fn never_touches_the_terminal_row_or_late_deltas() {
        let repo = repo();
        append(&repo, OLD, message("t1", "m1", "a"));
        append(&repo, OLD, completed("t1"));
        append(&repo, OLD, message("t1", "m1", "late "));
        append(&repo, OLD, message("t1", "m1", "chunk"));

        let session = SessionId(SESSION.to_owned());
        let step = repo
            .compact_session_deltas_step(&session, now(), 1000)
            .unwrap();
        assert_eq!(step.deleted_rows, 0);
        assert_eq!(repo.last_event_sequence_sync(&session).unwrap(), 4);
    }

    #[test]
    fn caps_merged_content_and_resumes_after_a_bounded_step() {
        let repo = repo();
        let chunk = "x".repeat(MERGED_CONTENT_MAX_BYTES / 2);
        for _ in 0..5 {
            append(&repo, OLD, tool("t1", "c1", &chunk));
        }
        for index in 0..6 {
            append(&repo, OLD, message("t1", "m1", &index.to_string()));
        }
        append(&repo, OLD, completed("t1"));

        let session = SessionId(SESSION.to_owned());
        let first = repo
            .compact_session_deltas_step(&session, now(), 2)
            .unwrap();
        assert!(first.more);
        assert!(first.deleted_rows <= 3);
        while repo
            .compact_session_deltas_step(&session, now(), 2)
            .unwrap()
            .more
        {}

        let remaining = events(&repo);
        let tool_rows: Vec<_> = remaining
            .iter()
            .filter_map(|(_, kind)| match kind {
                kind if kind["type"] == "turn_tool_call_delta" => {
                    kind["content"].as_str().map(str::len)
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            tool_rows,
            vec![chunk.len() * 2, chunk.len() * 2, chunk.len()]
        );
        assert!(remaining.contains(&(6, j(message("t1", "m1", "012345")))));
    }
}

//! Changes an agent proposes to what exists
//! (`dcc_core::domain::agent_change`): a small table the app reads with the
//! sidebar, each waiting until the person applies or discards it; answered
//! ones go after a week. Applying writes only the fields the change names.

use super::*;
use dcc_core::domain::agent_change::{AgentChange, Change, MAX_WAITING_CHANGES};

pub(super) const AGENT_CHANGE_TABLE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS dcc_agent_changes (
    id TEXT PRIMARY KEY NOT NULL,
    agent_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    change_json TEXT NOT NULL,
    reason TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL,
    answered_at TEXT NULL
);
CREATE INDEX IF NOT EXISTS idx_dcc_agent_changes_waiting
    ON dcc_agent_changes(created_at) WHERE answered_at IS NULL;
"#;

fn repository_error(error: impl ToString) -> dcc_core::CoreError {
    dcc_core::CoreError::Repository(error.to_string())
}

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

impl SqliteSessionRepo {
    /// A change `agent_id` proposes. One waiting for the same source or
    /// agent is replaced; at most `MAX_WAITING_CHANGES` wait at once.
    pub fn add_agent_change(
        &self,
        agent_id: &str,
        session_id: &str,
        change: &Change,
        reason: &str,
    ) -> Result<AgentChange> {
        let same: Vec<String> = self
            .waiting_agent_changes()?
            .into_iter()
            .filter(|waiting| waiting.change.same_target(change))
            .map(|waiting| waiting.id)
            .collect();
        let conn = self.conn.lock().map_err(repository_error)?;
        for id in &same {
            conn.execute(
                "UPDATE dcc_agent_changes SET answered_at = ?2 WHERE id = ?1",
                params![id, now()],
            )
            .map_err(repository_error)?;
        }
        let waiting: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM dcc_agent_changes WHERE answered_at IS NULL",
                [],
                |row| row.get(0),
            )
            .map_err(repository_error)?;
        if waiting as usize >= MAX_WAITING_CHANGES {
            return Err(dcc_core::CoreError::InvalidInput(format!(
                "{MAX_WAITING_CHANGES} changes already wait for the person; let them answer those first"
            )));
        }
        let proposed = AgentChange {
            id: uuid::Uuid::new_v4().to_string(),
            agent_id: agent_id.to_string(),
            session_id: session_id.to_string(),
            change: change.clone(),
            reason: reason.to_string(),
            created_at: now(),
        };
        conn.execute(
            "INSERT INTO dcc_agent_changes (id, agent_id, session_id, change_json, reason, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                proposed.id,
                proposed.agent_id,
                proposed.session_id,
                serde_json::to_string(&proposed.change).map_err(repository_error)?,
                proposed.reason,
                proposed.created_at
            ],
        )
        .map_err(repository_error)?;
        Ok(proposed)
    }

    /// The changes the person has not answered, oldest first.
    pub fn waiting_agent_changes(&self) -> Result<Vec<AgentChange>> {
        let conn = self.conn.lock().map_err(repository_error)?;
        let mut statement = conn
            .prepare(
                "SELECT id, agent_id, session_id, change_json, reason, created_at FROM dcc_agent_changes WHERE answered_at IS NULL ORDER BY created_at, id",
            )
            .map_err(repository_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })
            .and_then(|rows| rows.collect::<rusqlite::Result<Vec<_>>>())
            .map_err(repository_error)?;
        // A row this version cannot read stays out of the room.
        Ok(rows
            .into_iter()
            .filter_map(|(id, agent_id, session_id, change, reason, created_at)| {
                Some(AgentChange {
                    id,
                    agent_id,
                    session_id,
                    change: serde_json::from_str(&change).ok()?,
                    reason,
                    created_at,
                })
            })
            .collect())
    }

    /// The person answered `id`; it comes back if it was waiting. Answered
    /// ones older than a week leave.
    pub fn answer_agent_change(&self, id: &str) -> Result<Option<AgentChange>> {
        let waiting = self
            .waiting_agent_changes()?
            .into_iter()
            .find(|change| change.id == id);
        let conn = self.conn.lock().map_err(repository_error)?;
        let at = Utc::now();
        conn.execute(
            "UPDATE dcc_agent_changes SET answered_at = ?2 WHERE id = ?1 AND answered_at IS NULL",
            params![id, at.to_rfc3339_opts(SecondsFormat::Millis, true)],
        )
        .map_err(repository_error)?;
        let week_ago =
            (at - chrono::Duration::days(7)).to_rfc3339_opts(SecondsFormat::Millis, true);
        conn.execute(
            "DELETE FROM dcc_agent_changes WHERE answered_at IS NOT NULL AND answered_at < ?1",
            params![week_ago],
        )
        .map_err(repository_error)?;
        Ok(waiting)
    }

    /// Writes `change`: only what it names. Whether the source or the agent
    /// was still there.
    pub fn apply_agent_change(&self, change: &Change) -> Result<bool> {
        let conn = self.conn.lock().map_err(repository_error)?;
        let changed = match change {
            Change::Source {
                source_id,
                agent_id,
                room,
                every_minutes,
                enabled,
                ..
            } => {
                let found: bool = conn
                    .query_row(
                        "SELECT EXISTS(SELECT 1 FROM dcc_agent_sources WHERE id = ?1 AND agent_id = ?2)",
                        params![source_id, agent_id],
                        |row| row.get(0),
                    )
                    .map_err(repository_error)?;
                if !found {
                    return Ok(false);
                }
                // What it had seen stays: it reads the same thing, and only
                // where it tells or how often changes.
                if let Some((_, after)) = room {
                    conn.execute(
                        "UPDATE dcc_agent_sources SET room_project_id = ?2 WHERE id = ?1",
                        params![source_id, after],
                    )
                    .map_err(repository_error)?;
                }
                if let Some((_, after)) = every_minutes {
                    conn.execute(
                        "UPDATE dcc_agent_sources SET every_minutes = ?2 WHERE id = ?1",
                        params![source_id, after],
                    )
                    .map_err(repository_error)?;
                }
                if let Some((_, after)) = enabled {
                    conn.execute(
                        "UPDATE dcc_agent_sources SET enabled = ?2 WHERE id = ?1",
                        params![source_id, after],
                    )
                    .map_err(repository_error)?;
                }
                1
            }
            Change::Projects {
                agent_id, after, ..
            } => {
                // Every project is stored as nothing, like the editor does.
                let projects = (!after.is_empty())
                    .then(|| serde_json::to_string(after))
                    .transpose()
                    .map_err(repository_error)?;
                conn.execute(
                    "UPDATE dcc_agents SET projects_json = ?2, updated_at = ?3 WHERE id = ?1 AND deleted_at IS NULL",
                    params![agent_id, projects, now()],
                )
                .map_err(repository_error)?
            }
        };
        Ok(changed > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dcc_core::domain::agent::ReviewerPresetText;
    use dcc_core::domain::agent_source::AgentSource;

    #[test]
    fn a_change_waits_replaces_its_like_and_writes_only_what_it_names() {
        let repo = SqliteSessionRepo::from_connection(Arc::new(Mutex::new(
            Connection::open_in_memory().unwrap(),
        )))
        .unwrap();
        let reviewer = repo
            .list_resident_agents(&ReviewerPresetText::default())
            .unwrap()
            .remove(0);
        let source = repo
            .propose_agent_source(&AgentSource {
                id: String::new(),
                agent_id: reviewer.id.clone(),
                connector_id: "c".into(),
                connector: "Slack".into(),
                watch: "menções".into(),
                tool: "search".into(),
                arguments: "{}".into(),
                every_minutes: 15,
                enabled: false,
                proposed: true,
                last_checked_at: None,
                created_at: String::new(),
                room: None,
            })
            .unwrap();
        repo.set_agent_source(&reviewer.id, &source.id, true)
            .unwrap();
        repo.record_agent_source_read(&source.id, &["a".to_string()], "2026-11-16T10:00:00Z")
            .unwrap();
        let moved = |every: Option<(u32, u32)>| Change::Source {
            source_id: source.id.clone(),
            agent_id: reviewer.id.clone(),
            connector: "Slack".into(),
            watch: "menções".into(),
            room: Some((None, Some("app".into()))),
            every_minutes: every,
            enabled: None,
        };
        let first = repo
            .add_agent_change(&reviewer.id, "s", &moved(None), "está na Geral")
            .unwrap();
        // The same source again replaces the card; another target is its own.
        let second = repo
            .add_agent_change(&reviewer.id, "s", &moved(Some((15, 5))), "")
            .unwrap();
        let projects = Change::Projects {
            agent_id: reviewer.id.clone(),
            before: Vec::new(),
            after: vec!["app".into()],
        };
        repo.add_agent_change(&reviewer.id, "s", &projects, "")
            .unwrap();
        let waiting = repo.waiting_agent_changes().unwrap();
        assert_eq!(waiting.len(), 2);
        assert!(waiting.iter().all(|change| change.id != first.id));

        assert!(repo.apply_agent_change(&second.change).unwrap());
        let (stored, seen) = repo.agent_source_with_seen(&source.id).unwrap().unwrap();
        assert_eq!(stored.room.as_deref(), Some("app"));
        assert_eq!(stored.every_minutes, 5);
        // What it reads did not change: on, and what it had seen stays.
        assert!(stored.enabled);
        assert_eq!(seen, ["a"]);
        assert!(repo.apply_agent_change(&projects).unwrap());
        let agent = repo.load_resident_agent(&reviewer.id).unwrap().unwrap();
        assert_eq!(agent.projects, ["app"]);

        // Answered once; a source that is gone is not written.
        assert!(repo.answer_agent_change(&second.id).unwrap().is_some());
        assert!(repo.answer_agent_change(&second.id).unwrap().is_none());
        assert_eq!(repo.waiting_agent_changes().unwrap().len(), 1);
        repo.delete_agent_source(&reviewer.id, &source.id).unwrap();
        assert!(!repo.apply_agent_change(&second.change).unwrap());
    }
}

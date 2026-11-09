//! An agent's sources (`dcc_core::domain::agent_source`): a small table the
//! app reads once a minute, with what each source already showed (its
//! seen ids) so only news wakes the agent.

use super::*;
use dcc_core::domain::agent_source::{AgentSource, MAX_SOURCES_PER_AGENT};

pub(super) const AGENT_SOURCE_TABLE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS dcc_agent_sources (
    id TEXT PRIMARY KEY NOT NULL,
    agent_id TEXT NOT NULL,
    connector_id TEXT NOT NULL,
    connector TEXT NOT NULL,
    watch TEXT NOT NULL,
    tool TEXT NOT NULL,
    arguments TEXT NOT NULL,
    every_minutes INTEGER NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    proposed INTEGER NOT NULL DEFAULT 0,
    seen_json TEXT NULL,
    last_checked_at TEXT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (agent_id) REFERENCES dcc_agents(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_dcc_agent_sources_agent ON dcc_agent_sources(agent_id);
"#;

fn repository_error(error: impl ToString) -> dcc_core::CoreError {
    dcc_core::CoreError::Repository(error.to_string())
}

const COLUMNS: &str = "s.id, s.agent_id, s.connector_id, s.connector, s.watch, s.tool, s.arguments, s.every_minutes, s.enabled, s.proposed, s.last_checked_at, s.created_at";

fn source_from_row(row: &Row<'_>) -> rusqlite::Result<AgentSource> {
    Ok(AgentSource {
        id: row.get(0)?,
        agent_id: row.get(1)?,
        connector_id: row.get(2)?,
        connector: row.get(3)?,
        watch: row.get(4)?,
        tool: row.get(5)?,
        arguments: row.get(6)?,
        every_minutes: row.get(7)?,
        enabled: row.get::<_, i64>(8)? != 0,
        proposed: row.get::<_, i64>(9)? != 0,
        last_checked_at: row.get(10)?,
        created_at: row.get(11)?,
    })
}

impl SqliteSessionRepo {
    /// Every live agent's sources, oldest first.
    pub fn list_agent_sources(&self) -> Result<Vec<AgentSource>> {
        let conn = self.conn.lock().map_err(repository_error)?;
        let mut statement = conn
            .prepare(&format!(
                "SELECT {COLUMNS} FROM dcc_agent_sources s JOIN dcc_agents a ON a.id = s.agent_id
                 WHERE a.deleted_at IS NULL ORDER BY s.created_at, s.id"
            ))
            .map_err(repository_error)?;
        statement
            .query_map([], source_from_row)
            .and_then(|rows| rows.collect::<rusqlite::Result<Vec<_>>>())
            .map_err(repository_error)
    }

    /// A source an agent proposes; it waits for the person.
    pub fn propose_agent_source(&self, source: &AgentSource) -> Result<AgentSource> {
        let conn = self.conn.lock().map_err(repository_error)?;
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM dcc_agent_sources WHERE agent_id = ?1",
                params![source.agent_id],
                |row| row.get(0),
            )
            .map_err(repository_error)?;
        if count as usize >= MAX_SOURCES_PER_AGENT {
            return Err(dcc_core::CoreError::InvalidInput(format!(
                "an agent watches at most {MAX_SOURCES_PER_AGENT} sources"
            )));
        }
        let mut stored = source.clone();
        stored.id = uuid::Uuid::new_v4().to_string();
        stored.proposed = true;
        stored.enabled = true;
        stored.last_checked_at = None;
        stored.created_at = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
        conn.execute(
            "INSERT INTO dcc_agent_sources (id, agent_id, connector_id, connector, watch, tool, arguments, every_minutes, enabled, proposed, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 1, 1, ?9)",
            params![
                stored.id,
                stored.agent_id,
                stored.connector_id,
                stored.connector,
                stored.watch,
                stored.tool,
                stored.arguments,
                stored.every_minutes,
                stored.created_at
            ],
        )
        .map_err(repository_error)?;
        Ok(stored)
    }

    /// "Criar" on a proposal; or the person's switch.
    pub fn set_agent_source(&self, agent_id: &str, id: &str, enabled: bool) -> Result<bool> {
        let conn = self.conn.lock().map_err(repository_error)?;
        let changed = conn
            .execute(
                "UPDATE dcc_agent_sources SET proposed = 0, enabled = ?3 WHERE id = ?1 AND agent_id = ?2",
                params![id, agent_id, enabled],
            )
            .map_err(repository_error)?;
        Ok(changed > 0)
    }

    pub fn delete_agent_source(&self, agent_id: &str, id: &str) -> Result<bool> {
        let conn = self.conn.lock().map_err(repository_error)?;
        let changed = conn
            .execute(
                "DELETE FROM dcc_agent_sources WHERE id = ?1 AND agent_id = ?2",
                params![id, agent_id],
            )
            .map_err(repository_error)?;
        Ok(changed > 0)
    }

    /// The source and what it already showed.
    pub fn agent_source_with_seen(&self, id: &str) -> Result<Option<(AgentSource, Vec<String>)>> {
        let conn = self.conn.lock().map_err(repository_error)?;
        conn.query_row(
            &format!("SELECT {COLUMNS}, s.seen_json FROM dcc_agent_sources s WHERE s.id = ?1"),
            params![id],
            |row| {
                let seen: Option<String> = row.get(12)?;
                Ok((
                    source_from_row(row)?,
                    seen.and_then(|json| from_str(&json).ok())
                        .unwrap_or_default(),
                ))
            },
        )
        .optional()
        .map_err(repository_error)
    }

    /// A read happened at `at`, leaving `seen` remembered.
    pub fn record_agent_source_read(&self, id: &str, seen: &[String], at: &str) -> Result<()> {
        let conn = self.conn.lock().map_err(repository_error)?;
        conn.execute(
            "UPDATE dcc_agent_sources SET seen_json = ?2, last_checked_at = ?3 WHERE id = ?1",
            params![id, to_string(seen).map_err(repository_error)?, at],
        )
        .map_err(repository_error)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dcc_core::domain::agent::{ResidentAgentDraft, ReviewerPresetText};

    #[test]
    fn sources_are_proposed_created_read_and_removed() {
        let repo = SqliteSessionRepo::from_connection(Arc::new(Mutex::new(
            Connection::open_in_memory().unwrap(),
        )))
        .unwrap();
        let mut draft = ResidentAgentDraft::reviewer(&ReviewerPresetText::default());
        draft.name = "Suporte".into();
        draft.role = "Vigie.".into();
        let agent = repo.save_resident_agent(None, draft).unwrap();
        let source = AgentSource {
            id: String::new(),
            agent_id: agent.id.clone(),
            connector_id: "slack".into(),
            connector: "Slack".into(),
            watch: "menções a mim".into(),
            tool: "slack_search_public".into(),
            arguments: "{\"query\":\"@me\"}".into(),
            every_minutes: 15,
            enabled: true,
            proposed: false,
            last_checked_at: None,
            created_at: String::new(),
        };
        let proposed = repo.propose_agent_source(&source).unwrap();
        assert!(proposed.proposed);
        assert!(repo
            .set_agent_source(&agent.id, &proposed.id, true)
            .unwrap());
        let listed = repo.list_agent_sources().unwrap();
        assert_eq!(listed.len(), 1);
        assert!(!listed[0].proposed && listed[0].enabled);
        repo.record_agent_source_read(&proposed.id, &["a".into()], "2026-11-09T10:00:00Z")
            .unwrap();
        let (read, seen) = repo.agent_source_with_seen(&proposed.id).unwrap().unwrap();
        assert_eq!(seen, ["a"]);
        assert_eq!(
            read.last_checked_at.as_deref(),
            Some("2026-11-09T10:00:00Z")
        );
        assert!(repo.delete_agent_source(&agent.id, &proposed.id).unwrap());
        assert!(repo.list_agent_sources().unwrap().is_empty());
    }
}

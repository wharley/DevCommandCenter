//! Replies agents propose (`dcc_core::domain::agent_reply`): kept until the
//! person sends or discards them; a sent one keeps when it left.

use super::*;
use dcc_core::domain::agent_reply::{AgentReply, ReplyState, MAX_PENDING_PER_AGENT};

pub(super) const AGENT_REPLY_TABLE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS dcc_agent_replies (
    id TEXT PRIMARY KEY NOT NULL,
    agent_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    connector_id TEXT NOT NULL,
    connector TEXT NOT NULL,
    tool TEXT NOT NULL,
    arguments TEXT NOT NULL,
    text_argument TEXT NOT NULL,
    destination TEXT NOT NULL,
    text TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT 'pending',
    error TEXT NULL,
    created_at TEXT NOT NULL,
    sent_at TEXT NULL,
    room_project_id TEXT NULL,
    FOREIGN KEY (agent_id) REFERENCES dcc_agents(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_dcc_agent_replies_state ON dcc_agent_replies(state, created_at);
"#;

fn repository_error(error: impl ToString) -> dcc_core::CoreError {
    dcc_core::CoreError::Repository(error.to_string())
}

const COLUMNS: &str = "r.id, r.agent_id, r.session_id, r.connector_id, r.connector, r.tool, r.arguments, r.text_argument, r.destination, r.text, r.state, r.error, r.created_at, r.sent_at, r.room_project_id";

fn reply_from_row(row: &Row<'_>) -> rusqlite::Result<AgentReply> {
    Ok(AgentReply {
        id: row.get(0)?,
        agent_id: row.get(1)?,
        session_id: row.get(2)?,
        connector_id: row.get(3)?,
        connector: row.get(4)?,
        tool: row.get(5)?,
        arguments: row.get(6)?,
        text_argument: row.get(7)?,
        destination: row.get(8)?,
        text: row.get(9)?,
        state: ReplyState::parse(&row.get::<_, String>(10)?),
        error: row.get(11)?,
        created_at: row.get(12)?,
        sent_at: row.get(13)?,
        room: row.get(14)?,
    })
}

impl SqliteSessionRepo {
    /// The replies waiting for the person, oldest first, of live agents.
    pub fn list_pending_agent_replies(&self) -> Result<Vec<AgentReply>> {
        let conn = self.conn.lock().map_err(repository_error)?;
        let mut statement = conn
            .prepare(&format!(
                "SELECT {COLUMNS} FROM dcc_agent_replies r JOIN dcc_agents a ON a.id = r.agent_id
                 WHERE a.deleted_at IS NULL AND r.state = 'pending' ORDER BY r.created_at, r.id"
            ))
            .map_err(repository_error)?;
        statement
            .query_map([], reply_from_row)
            .and_then(|rows| rows.collect::<rusqlite::Result<Vec<_>>>())
            .map_err(repository_error)
    }

    pub fn agent_reply(&self, id: &str) -> Result<Option<AgentReply>> {
        let conn = self.conn.lock().map_err(repository_error)?;
        conn.query_row(
            &format!("SELECT {COLUMNS} FROM dcc_agent_replies r WHERE r.id = ?1"),
            params![id],
            reply_from_row,
        )
        .optional()
        .map_err(repository_error)
    }

    /// A reply an agent proposes; past the limit its oldest waiting one is
    /// set aside, so the room never piles up.
    pub fn propose_agent_reply(&self, reply: &AgentReply) -> Result<AgentReply> {
        let conn = self.conn.lock().map_err(repository_error)?;
        let mut stored = reply.clone();
        stored.id = uuid::Uuid::new_v4().to_string();
        stored.state = ReplyState::Pending;
        stored.error = None;
        stored.sent_at = None;
        stored.created_at = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
        conn.execute(
            "INSERT INTO dcc_agent_replies (id, agent_id, session_id, connector_id, connector, tool, arguments, text_argument, destination, text, state, created_at, room_project_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'pending', ?11, ?12)",
            params![
                stored.id,
                stored.agent_id,
                stored.session_id,
                stored.connector_id,
                stored.connector,
                stored.tool,
                stored.arguments,
                stored.text_argument,
                stored.destination,
                stored.text,
                stored.created_at,
                stored.room
            ],
        )
        .map_err(repository_error)?;
        conn.execute(
            "UPDATE dcc_agent_replies SET state = 'discarded' WHERE agent_id = ?1 AND state = 'pending' AND id NOT IN (
                SELECT id FROM dcc_agent_replies WHERE agent_id = ?1 AND state = 'pending'
                ORDER BY created_at DESC, id DESC LIMIT ?2)",
            params![stored.agent_id, MAX_PENDING_PER_AGENT as i64],
        )
        .map_err(repository_error)?;
        Ok(stored)
    }

    /// The person's "Descartar"; false when it was no longer waiting.
    pub fn discard_agent_reply(&self, id: &str) -> Result<bool> {
        let conn = self.conn.lock().map_err(repository_error)?;
        let changed = conn
            .execute(
                "UPDATE dcc_agent_replies SET state = 'discarded' WHERE id = ?1 AND state = 'pending'",
                params![id],
            )
            .map_err(repository_error)?;
        Ok(changed > 0)
    }

    /// The text as sent, at `at`.
    pub fn mark_agent_reply_sent(&self, id: &str, text: &str, at: &str) -> Result<bool> {
        let conn = self.conn.lock().map_err(repository_error)?;
        let changed = conn
            .execute(
                "UPDATE dcc_agent_replies SET state = 'sent', text = ?2, sent_at = ?3, error = NULL WHERE id = ?1 AND state = 'pending'",
                params![id, text, at],
            )
            .map_err(repository_error)?;
        Ok(changed > 0)
    }

    /// Why "Enviar" failed, with the person's text kept for the next try.
    pub fn record_agent_reply_error(&self, id: &str, text: &str, error: &str) -> Result<()> {
        let conn = self.conn.lock().map_err(repository_error)?;
        conn.execute(
            "UPDATE dcc_agent_replies SET text = ?2, error = ?3 WHERE id = ?1 AND state = 'pending'",
            params![id, text, error],
        )
        .map_err(repository_error)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dcc_core::domain::agent::{ResidentAgentDraft, ReviewerPresetText};

    fn reply(agent_id: &str, text: &str) -> AgentReply {
        AgentReply {
            id: String::new(),
            agent_id: agent_id.into(),
            session_id: "s-1".into(),
            connector_id: "slack".into(),
            connector: "Slack".into(),
            tool: "slack_send_message".into(),
            arguments: "{\"channel_id\":\"C1\"}".into(),
            text_argument: "message".into(),
            destination: "#suporte · thread do Guilherme".into(),
            text: text.into(),
            state: ReplyState::Pending,
            error: None,
            created_at: String::new(),
            sent_at: None,
            room: Some("checkout".into()),
        }
    }

    #[test]
    fn replies_wait_then_are_sent_or_discarded_and_never_pile_up() {
        let repo = SqliteSessionRepo::from_connection(Arc::new(Mutex::new(
            Connection::open_in_memory().unwrap(),
        )))
        .unwrap();
        let mut draft = ResidentAgentDraft::reviewer(&ReviewerPresetText::default());
        draft.name = "Suporte".into();
        draft.role = "Responde.".into();
        let agent = repo.save_resident_agent(None, draft).unwrap();
        let first = repo
            .propose_agent_reply(&reply(&agent.id, "Foi na 374."))
            .unwrap();
        assert_eq!(repo.list_pending_agent_replies().unwrap().len(), 1);

        repo.record_agent_reply_error(&first.id, "Foi na loja 374.", "channel_not_found")
            .unwrap();
        let failed = repo.agent_reply(&first.id).unwrap().unwrap();
        assert_eq!(failed.text, "Foi na loja 374.");
        assert_eq!(failed.error.as_deref(), Some("channel_not_found"));
        assert!(repo
            .mark_agent_reply_sent(&first.id, "Foi na loja 374.", "2026-11-11T22:00:00Z")
            .unwrap());
        assert!(!repo.discard_agent_reply(&first.id).unwrap());
        let sent = repo.agent_reply(&first.id).unwrap().unwrap();
        assert_eq!(sent.state, ReplyState::Sent);
        assert!(sent.error.is_none());
        assert!(repo.list_pending_agent_replies().unwrap().is_empty());

        for index in 0..MAX_PENDING_PER_AGENT + 2 {
            repo.propose_agent_reply(&reply(&agent.id, &format!("r{index}")))
                .unwrap();
        }
        let pending = repo.list_pending_agent_replies().unwrap();
        assert_eq!(pending.len(), MAX_PENDING_PER_AGENT);
        assert!(repo.discard_agent_reply(&pending[0].id).unwrap());
        assert_eq!(
            repo.list_pending_agent_replies().unwrap().len(),
            MAX_PENDING_PER_AGENT - 1
        );
    }
}

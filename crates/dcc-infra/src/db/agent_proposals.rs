//! Agents an agent proposes (`dcc_core::domain::agent_proposal`): a small
//! table the app reads with the sidebar, each proposal waiting until the
//! person creates, edits or discards it; answered ones go after a week.

use super::*;
use dcc_core::domain::agent::ResidentAgentDraft;
use dcc_core::domain::agent_proposal::{AgentProposal, MAX_WAITING_PROPOSALS};

pub(super) const AGENT_PROPOSAL_TABLE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS dcc_agent_proposals (
    id TEXT PRIMARY KEY NOT NULL,
    agent_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    draft_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    answered_at TEXT NULL
);
CREATE INDEX IF NOT EXISTS idx_dcc_agent_proposals_waiting
    ON dcc_agent_proposals(created_at) WHERE answered_at IS NULL;
"#;

fn repository_error(error: impl ToString) -> dcc_core::CoreError {
    dcc_core::CoreError::Repository(error.to_string())
}

impl SqliteSessionRepo {
    /// A proposal of `agent_id`; at most `MAX_WAITING_PROPOSALS` wait at once.
    pub fn add_agent_proposal(
        &self,
        agent_id: &str,
        session_id: &str,
        draft: &ResidentAgentDraft,
    ) -> Result<AgentProposal> {
        let conn = self.conn.lock().map_err(repository_error)?;
        let waiting: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM dcc_agent_proposals WHERE answered_at IS NULL",
                [],
                |row| row.get(0),
            )
            .map_err(repository_error)?;
        if waiting as usize >= MAX_WAITING_PROPOSALS {
            return Err(dcc_core::CoreError::InvalidInput(format!(
                "{MAX_WAITING_PROPOSALS} agents already wait for the person; let them answer those first"
            )));
        }
        let proposal = AgentProposal {
            id: uuid::Uuid::new_v4().to_string(),
            agent_id: agent_id.to_string(),
            session_id: session_id.to_string(),
            draft: draft.clone(),
            created_at: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
        };
        conn.execute(
            "INSERT INTO dcc_agent_proposals (id, agent_id, session_id, draft_json, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                proposal.id,
                proposal.agent_id,
                proposal.session_id,
                serde_json::to_string(&proposal.draft).map_err(repository_error)?,
                proposal.created_at
            ],
        )
        .map_err(repository_error)?;
        Ok(proposal)
    }

    /// The proposals the person has not answered, oldest first.
    pub fn waiting_agent_proposals(&self) -> Result<Vec<AgentProposal>> {
        let conn = self.conn.lock().map_err(repository_error)?;
        let mut statement = conn
            .prepare(
                "SELECT id, agent_id, session_id, draft_json, created_at FROM dcc_agent_proposals WHERE answered_at IS NULL ORDER BY created_at, id",
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
                ))
            })
            .and_then(|rows| rows.collect::<rusqlite::Result<Vec<_>>>())
            .map_err(repository_error)?;
        // A row this version cannot read stays out of the room.
        Ok(rows
            .into_iter()
            .filter_map(|(id, agent_id, session_id, draft, created_at)| {
                Some(AgentProposal {
                    id,
                    agent_id,
                    session_id,
                    draft: serde_json::from_str(&draft).ok()?,
                    created_at,
                })
            })
            .collect())
    }

    /// The person answered `id`; its draft comes back if it was waiting.
    /// Answered ones older than a week leave.
    pub fn answer_agent_proposal(&self, id: &str) -> Result<Option<AgentProposal>> {
        let waiting = self
            .waiting_agent_proposals()?
            .into_iter()
            .find(|proposal| proposal.id == id);
        let conn = self.conn.lock().map_err(repository_error)?;
        let now = Utc::now();
        conn.execute(
            "UPDATE dcc_agent_proposals SET answered_at = ?2 WHERE id = ?1 AND answered_at IS NULL",
            params![id, now.to_rfc3339_opts(SecondsFormat::Millis, true)],
        )
        .map_err(repository_error)?;
        let week_ago =
            (now - chrono::Duration::days(7)).to_rfc3339_opts(SecondsFormat::Millis, true);
        conn.execute(
            "DELETE FROM dcc_agent_proposals WHERE answered_at IS NOT NULL AND answered_at < ?1",
            params![week_ago],
        )
        .map_err(repository_error)?;
        Ok(waiting)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dcc_core::domain::agent_proposal::proposal_draft;

    #[test]
    fn proposals_wait_capped_until_answered_once() {
        let repo = SqliteSessionRepo::from_connection(Arc::new(Mutex::new(
            Connection::open_in_memory().unwrap(),
        )))
        .unwrap();
        let draft = |name: &str| {
            proposal_draft(
                name,
                "Teste.",
                "Comece.",
                None,
                Vec::new(),
                Default::default(),
            )
            .unwrap()
        };
        for name in ["A", "B", "C"] {
            repo.add_agent_proposal("reviewer", "s-1", &draft(name))
                .unwrap();
        }
        assert!(repo
            .add_agent_proposal("reviewer", "s-1", &draft("D"))
            .is_err());
        let waiting = repo.waiting_agent_proposals().unwrap();
        assert_eq!(waiting.len(), 3);
        let answered = repo.answer_agent_proposal(&waiting[0].id).unwrap().unwrap();
        assert_eq!(answered.draft, waiting[0].draft);
        assert!(repo
            .answer_agent_proposal(&waiting[0].id)
            .unwrap()
            .is_none());
        assert_eq!(repo.waiting_agent_proposals().unwrap().len(), 2);
    }
}

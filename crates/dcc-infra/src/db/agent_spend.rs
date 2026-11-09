//! What a resident agent spent (`dcc_core::domain::agent_budget`): the
//! tokens and the reported cost of every turn of the sessions bound to it
//! since a moment, summed in one read over the usage table's time index.
//! Read only when it matters: before a turn of an agent with a limit, and
//! when its page or the team room shows the month.

use super::*;
use dcc_core::domain::agent_budget::{counted_tokens, AgentSpend};

impl SqliteSessionRepo {
    /// The agent's turns recorded at or after `since` (RFC 3339).
    pub fn agent_spend_since(&self, agent_id: &str, since: &str) -> Result<AgentSpend> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let mut statement = conn
            .prepare(
                "SELECT u.total_tokens, u.cached_input_tokens, u.cost_usd, u.turn_id
                 FROM dcc_turn_model_usage u
                 JOIN dcc_session_agents a ON a.session_id = u.session_id
                 WHERE a.agent_id = ?1 AND u.recorded_at >= ?2",
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let rows = statement
            .query_map(params![agent_id, since], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Option<f64>>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })
            .and_then(|rows| rows.collect::<rusqlite::Result<Vec<_>>>())
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let mut spend = AgentSpend::default();
        let mut turns = std::collections::HashSet::new();
        for (total, cached, cost, turn) in rows {
            spend.tokens += counted_tokens(total.max(0) as u64, cached.max(0) as u64);
            if let Some(cost) = cost {
                *spend.cost_usd.get_or_insert(0.0) += cost;
            }
            turns.insert(turn);
        }
        spend.turns = turns.len() as u64;
        Ok(spend)
    }

    /// "Continuar mesmo assim": the agent may go past its limit in `month`.
    pub fn let_agent_past_limit(&self, agent_id: &str, month: &str) -> Result<bool> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let changed = conn
            .execute(
                "UPDATE dcc_agents SET limit_override_month = ?2 WHERE id = ?1 AND deleted_at IS NULL",
                params![agent_id, month],
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Ok(changed > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dcc_core::domain::agent::{ResidentAgentDraft, ReviewerPresetText};

    #[test]
    fn an_agent_spends_what_its_sessions_used_since_a_moment() {
        let repo = SqliteSessionRepo::from_connection(Arc::new(Mutex::new(
            Connection::open_in_memory().unwrap(),
        )))
        .unwrap();
        let mut draft = ResidentAgentDraft::reviewer(&ReviewerPresetText::default());
        draft.name = "QA".into();
        draft.role = "Teste.".into();
        draft.monthly_token_limit = Some(2_000_000);
        let qa = repo.save_resident_agent(None, draft).unwrap();
        assert_eq!(qa.monthly_token_limit, Some(2_000_000));
        {
            let conn = repo.conn.lock().unwrap();
            conn.execute_batch(
                "INSERT INTO dcc_sessions (id, project_id, workspace_id, provider_id, state, created_at, updated_at) VALUES ('s-1', 'p', 'w', 'codex', 'active', 't', 't');",
            )
            .unwrap();
            conn.execute(
                "INSERT INTO dcc_session_agents (session_id, agent_id, created_at) VALUES ('s-1', ?1, 't')",
                params![qa.id],
            )
            .unwrap();
            for (turn, model, total, cached, cost, at) in [
                (
                    "t-1",
                    "a",
                    1_000,
                    900,
                    Some(0.5),
                    "2026-11-02T10:00:00.000Z",
                ),
                ("t-1", "b", 100, 0, None, "2026-11-02T10:00:00.000Z"),
                ("t-2", "a", 50, 0, Some(0.25), "2026-10-30T10:00:00.000Z"),
            ] {
                conn.execute(
                    "INSERT INTO dcc_turn_model_usage (session_id, turn_id, model, total_tokens, cached_input_tokens, cost_usd, recorded_at) VALUES ('s-1', ?1, ?2, ?3, ?4, ?5, ?6)",
                    params![turn, model, total, cached, cost, at],
                )
                .unwrap();
            }
        }
        let spend = repo
            .agent_spend_since(&qa.id, "2026-11-01T00:00:00.000Z")
            .unwrap();
        assert_eq!(spend.tokens, 190 + 100);
        assert_eq!(spend.cost_usd, Some(0.5));
        assert_eq!(spend.turns, 1);
        assert!(repo.let_agent_past_limit(&qa.id, "2026-11").unwrap());
        let stored = repo.load_resident_agent(&qa.id).unwrap().unwrap();
        assert_eq!(stored.limit_override_month.as_deref(), Some("2026-11"));
        // Saving the agent keeps its limit and the month it was let through.
        let mut again = ResidentAgentDraft::reviewer(&ReviewerPresetText::default());
        again.name = "QA".into();
        again.role = "Teste.".into();
        again.monthly_token_limit = Some(5_000_000);
        let saved = repo.save_resident_agent(Some(&qa.id), again).unwrap();
        assert_eq!(saved.monthly_token_limit, Some(5_000_000));
        assert_eq!(saved.limit_override_month.as_deref(), Some("2026-11"));
        // What it may use is kept with it.
        let mut narrowed = ResidentAgentDraft::reviewer(&ReviewerPresetText::default());
        narrowed.name = "QA".into();
        narrowed.role = "Teste.".into();
        narrowed.tool_access.groups.insert(
            "browser".into(),
            dcc_core::domain::agent_access::AccessLevel::Read,
        );
        let saved = repo.save_resident_agent(Some(&qa.id), narrowed).unwrap();
        assert_eq!(
            saved.tool_access.group("browser"),
            dcc_core::domain::agent_access::AccessLevel::Read
        );
    }
}

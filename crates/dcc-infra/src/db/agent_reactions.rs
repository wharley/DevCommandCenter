//! 👍 / 👎 on the agents' answers (`dcc_core::domain::agent_reaction`): one
//! row per answer, read per conversation for the room and per week for the
//! bulletin (by its time index).

use super::*;
use dcc_core::domain::agent_reaction::Reaction;

pub(super) const AGENT_REACTION_TABLE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS dcc_agent_reactions (
    line_key TEXT PRIMARY KEY NOT NULL,
    agent_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    reaction TEXT NOT NULL,
    reacted_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_dcc_agent_reactions_session ON dcc_agent_reactions(session_id);
CREATE INDEX IF NOT EXISTS idx_dcc_agent_reactions_at ON dcc_agent_reactions(reacted_at);
"#;

fn repository_error(error: impl ToString) -> dcc_core::CoreError {
    dcc_core::CoreError::Repository(error.to_string())
}

impl SqliteSessionRepo {
    /// The person's reaction to an answer; `None` takes it back.
    pub fn set_agent_reaction(
        &self,
        agent_id: &str,
        session_id: &str,
        line_key: &str,
        reaction: Option<Reaction>,
    ) -> Result<()> {
        let conn = self.conn.lock().map_err(repository_error)?;
        match reaction {
            Some(reaction) => conn.execute(
                "INSERT INTO dcc_agent_reactions (line_key, agent_id, session_id, reaction, reacted_at) VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(line_key) DO UPDATE SET reaction = excluded.reaction, reacted_at = excluded.reacted_at",
                params![
                    line_key,
                    agent_id,
                    session_id,
                    reaction.key(),
                    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
                ],
            ),
            None => conn.execute(
                "DELETE FROM dcc_agent_reactions WHERE line_key = ?1",
                params![line_key],
            ),
        }
        .map_err(repository_error)?;
        Ok(())
    }

    /// A conversation's reactions, by answer.
    pub fn session_reactions(&self, session_id: &str) -> Result<Vec<(String, Reaction)>> {
        let conn = self.conn.lock().map_err(repository_error)?;
        let mut statement = conn
            .prepare("SELECT line_key, reaction FROM dcc_agent_reactions WHERE session_id = ?1")
            .map_err(repository_error)?;
        let rows = statement
            .query_map(params![session_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .and_then(|rows| rows.collect::<rusqlite::Result<Vec<_>>>())
            .map_err(repository_error)?;
        Ok(rows
            .into_iter()
            .filter_map(|(key, reaction)| Some((key, Reaction::from_key(&reaction)?)))
            .collect())
    }

    /// 👍 and 👎 by agent between `from` and `to`.
    pub(super) fn reactions_between(
        conn: &Connection,
        from: &str,
        to: &str,
    ) -> Result<Vec<(String, Reaction)>> {
        let mut statement = conn
            .prepare(
                "SELECT agent_id, reaction FROM dcc_agent_reactions WHERE reacted_at >= ?1 AND reacted_at < ?2",
            )
            .map_err(repository_error)?;
        let rows = statement
            .query_map(params![from, to], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .and_then(|rows| rows.collect::<rusqlite::Result<Vec<_>>>())
            .map_err(repository_error)?;
        Ok(rows
            .into_iter()
            .filter_map(|(agent, reaction)| Some((agent, Reaction::from_key(&reaction)?)))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_reaction_per_answer_changed_or_taken_back() {
        let repo = SqliteSessionRepo::from_connection(Arc::new(Mutex::new(
            Connection::open_in_memory().unwrap(),
        )))
        .unwrap();
        repo.set_agent_reaction("rev", "s-1", "s-1:t-1", Some(Reaction::Up))
            .unwrap();
        repo.set_agent_reaction("rev", "s-1", "s-1:t-2", Some(Reaction::Up))
            .unwrap();
        repo.set_agent_reaction("rev", "s-1", "s-1:t-2", Some(Reaction::Down))
            .unwrap();
        let mut found = repo.session_reactions("s-1").unwrap();
        found.sort_by(|left, right| left.0.cmp(&right.0));
        assert_eq!(
            found,
            [
                ("s-1:t-1".to_string(), Reaction::Up),
                ("s-1:t-2".to_string(), Reaction::Down)
            ]
        );
        repo.set_agent_reaction("rev", "s-1", "s-1:t-1", None)
            .unwrap();
        assert_eq!(repo.session_reactions("s-1").unwrap().len(), 1);
        let conn = repo.conn.lock().unwrap();
        let week = SqliteSessionRepo::reactions_between(&conn, "2000-01-01", "2999-01-01").unwrap();
        assert_eq!(week, [("rev".to_string(), Reaction::Down)]);
    }
}

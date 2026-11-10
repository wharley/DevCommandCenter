//! The last usage Claude Code reported per session and model
//! (`dcc_core::domain::claude_usage`): its reports are the session's totals,
//! so a turn's share is measured against this. A session with no mark yet
//! starts from its latest stored turn, which before the marks held the
//! session's totals too.

use super::*;
use dcc_core::domain::usage::ModelTokenUsage;

pub(super) const USAGE_MARK_TABLE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS dcc_usage_marks (
    session_id TEXT NOT NULL,
    model TEXT NOT NULL,
    input_tokens INTEGER NOT NULL,
    output_tokens INTEGER NOT NULL,
    cached_input_tokens INTEGER NOT NULL,
    cache_write_input_tokens INTEGER NOT NULL,
    reasoning_output_tokens INTEGER NOT NULL,
    cost_usd REAL NULL,
    PRIMARY KEY (session_id, model)
);
"#;

fn repository_error(error: impl ToString) -> dcc_core::CoreError {
    dcc_core::CoreError::Repository(error.to_string())
}

fn usage_from_row(row: &Row<'_>) -> rusqlite::Result<ModelTokenUsage> {
    let input_tokens: u64 = row.get(0)?;
    let output_tokens: u64 = row.get(1)?;
    let cached_input_tokens: u64 = row.get(2)?;
    let cache_write_input_tokens: u64 = row.get(3)?;
    Ok(ModelTokenUsage {
        model: None,
        input_tokens,
        output_tokens,
        cached_input_tokens,
        cache_write_input_tokens,
        reasoning_output_tokens: row.get(4)?,
        total_tokens: input_tokens + output_tokens + cached_input_tokens + cache_write_input_tokens,
        cost_usd: row.get(5)?,
    })
}

impl SqliteSessionRepo {
    /// The session's last report for `model`, if it has one.
    pub fn usage_mark(&self, session_id: &str, model: &str) -> Result<Option<ModelTokenUsage>> {
        let conn = self.conn.lock().map_err(repository_error)?;
        let mark = conn
            .query_row(
                "SELECT input_tokens, output_tokens, cached_input_tokens, cache_write_input_tokens, reasoning_output_tokens, cost_usd FROM dcc_usage_marks WHERE session_id = ?1 AND model = ?2",
                params![session_id, model],
                usage_from_row,
            )
            .optional()
            .map_err(repository_error)?;
        if mark.is_some() {
            return Ok(mark);
        }
        conn.query_row(
            "SELECT input_tokens, output_tokens, cached_input_tokens, cache_write_input_tokens, reasoning_output_tokens, cost_usd FROM dcc_turn_model_usage WHERE session_id = ?1 AND model = ?2 ORDER BY recorded_at DESC LIMIT 1",
            params![session_id, model],
            usage_from_row,
        )
        .optional()
        .map_err(repository_error)
    }

    /// Keeps `usage` as the session's last report for its model.
    pub fn set_usage_mark(
        &self,
        session_id: &str,
        model: &str,
        usage: &ModelTokenUsage,
    ) -> Result<()> {
        let conn = self.conn.lock().map_err(repository_error)?;
        conn.execute(
            "INSERT INTO dcc_usage_marks (session_id, model, input_tokens, output_tokens, cached_input_tokens, cache_write_input_tokens, reasoning_output_tokens, cost_usd) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(session_id, model) DO UPDATE SET input_tokens = ?3, output_tokens = ?4, cached_input_tokens = ?5, cache_write_input_tokens = ?6, reasoning_output_tokens = ?7, cost_usd = ?8",
            params![
                session_id,
                model,
                usage.input_tokens,
                usage.output_tokens,
                usage.cached_input_tokens,
                usage.cache_write_input_tokens,
                usage.reasoning_output_tokens,
                usage.cost_usd
            ],
        )
        .map_err(repository_error)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mark_is_kept_and_starts_from_the_latest_turn() {
        let repo = SqliteSessionRepo::from_connection(Arc::new(Mutex::new(
            Connection::open_in_memory().unwrap(),
        )))
        .unwrap();
        assert_eq!(repo.usage_mark("s", "m").unwrap(), None);
        {
            let conn = repo.conn.lock().unwrap();
            // A usage row without its session, for this test only.
            conn.execute_batch("PRAGMA foreign_keys = OFF;").unwrap();
            conn.execute(
                "INSERT INTO dcc_turn_model_usage (session_id, turn_id, model, input_tokens, output_tokens, cached_input_tokens, cache_write_input_tokens, reasoning_output_tokens, total_tokens, recorded_at) VALUES ('s', 't-1', 'm', 10, 20, 300, 40, 0, 370, '2026-11-10T10:00:00Z')",
                [],
            )
            .unwrap();
        }
        assert_eq!(
            repo.usage_mark("s", "m")
                .unwrap()
                .unwrap()
                .cached_input_tokens,
            300
        );
        let usage = ModelTokenUsage {
            input_tokens: 11,
            output_tokens: 25,
            cached_input_tokens: 400,
            cache_write_input_tokens: 41,
            ..Default::default()
        };
        repo.set_usage_mark("s", "m", &usage).unwrap();
        repo.set_usage_mark("s", "m", &usage).unwrap();
        assert_eq!(
            repo.usage_mark("s", "m")
                .unwrap()
                .unwrap()
                .cached_input_tokens,
            400
        );
    }
}

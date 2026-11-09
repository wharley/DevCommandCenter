//! Pull requests an agent asked DCC to review
//! (`dcc_core::domain::pr_review_request`): a small table the app reads with
//! the sidebar and empties as it takes each one, so a request made while
//! the app was closed still runs when it opens. Taken rows go after a day.

use super::*;
use dcc_core::domain::pr_review_request::PrReviewRequest;

pub(super) const PR_REVIEW_REQUEST_TABLE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS dcc_pr_review_requests (
    id TEXT PRIMARY KEY NOT NULL,
    agent_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    url TEXT NOT NULL,
    repository TEXT NOT NULL,
    number INTEGER NOT NULL,
    created_at TEXT NOT NULL,
    taken_at TEXT NULL
);
CREATE INDEX IF NOT EXISTS idx_dcc_pr_review_requests_waiting
    ON dcc_pr_review_requests(created_at) WHERE taken_at IS NULL;
"#;

fn repository_error(error: rusqlite::Error) -> dcc_core::CoreError {
    dcc_core::CoreError::Repository(error.to_string())
}

impl SqliteSessionRepo {
    /// The requests of `agent_id` from `session_id`, one per pull request;
    /// one still waiting for the same pull request is not repeated.
    pub fn add_pr_review_requests(
        &self,
        agent_id: &str,
        session_id: &str,
        pulls: &[(String, String, u64)],
    ) -> Result<usize> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let now = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
        let mut added = 0;
        for (url, repository, number) in pulls {
            let waiting: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM dcc_pr_review_requests WHERE taken_at IS NULL AND lower(repository) = lower(?1) AND number = ?2",
                    params![repository, *number as i64],
                    |row| row.get(0),
                )
                .map_err(repository_error)?;
            if waiting > 0 {
                continue;
            }
            conn.execute(
                "INSERT INTO dcc_pr_review_requests (id, agent_id, session_id, url, repository, number, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![uuid::Uuid::new_v4().to_string(), agent_id, session_id, url, repository, *number as i64, now],
            )
            .map_err(repository_error)?;
            added += 1;
        }
        Ok(added)
    }

    /// The requests no one took yet, oldest first.
    pub fn waiting_pr_review_requests(&self) -> Result<Vec<PrReviewRequest>> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let mut statement = conn
            .prepare(
                "SELECT id, agent_id, session_id, url, repository, number, created_at FROM dcc_pr_review_requests WHERE taken_at IS NULL ORDER BY created_at, id",
            )
            .map_err(repository_error)?;
        statement
            .query_map([], |row| {
                Ok(PrReviewRequest {
                    id: row.get(0)?,
                    agent_id: row.get(1)?,
                    session_id: row.get(2)?,
                    url: row.get(3)?,
                    repository: row.get(4)?,
                    number: row.get::<_, i64>(5)? as u64,
                    created_at: row.get(6)?,
                })
            })
            .and_then(|rows| rows.collect::<rusqlite::Result<Vec<_>>>())
            .map_err(repository_error)
    }

    /// The app took `id`; taken ones older than a day leave.
    pub fn take_pr_review_request(&self, id: &str) -> Result<bool> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let now = Utc::now();
        let changed = conn
            .execute(
                "UPDATE dcc_pr_review_requests SET taken_at = ?2 WHERE id = ?1 AND taken_at IS NULL",
                params![id, now.to_rfc3339_opts(SecondsFormat::Millis, true)],
            )
            .map_err(repository_error)?;
        let day_ago =
            (now - chrono::Duration::days(1)).to_rfc3339_opts(SecondsFormat::Millis, true);
        conn.execute(
            "DELETE FROM dcc_pr_review_requests WHERE taken_at IS NOT NULL AND taken_at < ?1",
            params![day_ago],
        )
        .map_err(repository_error)?;
        Ok(changed > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_wait_once_per_pull_request_until_taken() {
        let repo = SqliteSessionRepo::from_connection(Arc::new(Mutex::new(
            Connection::open_in_memory().unwrap(),
        )))
        .unwrap();
        let pull = |number: u64| {
            (
                format!("https://github.com/acme/app/pull/{number}"),
                "acme/app".to_string(),
                number,
            )
        };
        assert_eq!(
            repo.add_pr_review_requests("reviewer", "s-1", &[pull(1), pull(2), pull(1)])
                .unwrap(),
            2
        );
        assert_eq!(
            repo.add_pr_review_requests("reviewer", "s-1", &[pull(2)])
                .unwrap(),
            0,
            "already waiting"
        );
        // Added in the same millisecond, they may come in either order.
        let mut waiting = repo.waiting_pr_review_requests().unwrap();
        waiting.sort_by_key(|request| request.number);
        assert_eq!(
            waiting
                .iter()
                .map(|request| request.number)
                .collect::<Vec<_>>(),
            [1, 2]
        );
        assert!(repo.take_pr_review_request(&waiting[0].id).unwrap());
        assert!(!repo.take_pr_review_request(&waiting[0].id).unwrap());
        assert_eq!(repo.waiting_pr_review_requests().unwrap().len(), 1);
        // Taken, it may be asked again.
        assert_eq!(
            repo.add_pr_review_requests("reviewer", "s-1", &[pull(1)])
                .unwrap(),
            1
        );
    }
}

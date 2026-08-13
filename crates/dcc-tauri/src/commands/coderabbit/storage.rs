use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};

use super::{
    WorkspaceCodeRabbitReviewHistoryEntry, WorkspaceCodeRabbitReviewHistoryOutput,
    WorkspaceCodeRabbitReviewOutput, WorkspaceCodeRabbitStoredReviewOutput,
};

pub(crate) fn load_review(
    db_path: &Path,
    workspace_root: String,
) -> Result<WorkspaceCodeRabbitStoredReviewOutput, String> {
    let conn = open_coderabbit_reviews_db(db_path)?;
    let row = conn
        .query_row(
            "SELECT review_json, updated_at FROM workspace_coderabbit_reviews WHERE workspace_root = ?1",
            params![&workspace_root],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let Some((review_json, updated_at)) = row else {
        return Ok(WorkspaceCodeRabbitStoredReviewOutput {
            workspace_root,
            review: None,
            updated_at: None,
        });
    };
    let review = serde_json::from_str::<WorkspaceCodeRabbitReviewOutput>(&review_json)
        .map_err(|error| error.to_string())?;
    Ok(WorkspaceCodeRabbitStoredReviewOutput {
        workspace_root,
        review: Some(review),
        updated_at,
    })
}

pub(crate) fn save_review(
    db_path: &Path,
    workspace_root: String,
    review: WorkspaceCodeRabbitReviewOutput,
) -> Result<WorkspaceCodeRabbitStoredReviewOutput, String> {
    let conn = open_coderabbit_reviews_db(db_path)?;
    let review_json = serde_json::to_string(&review).map_err(|error| error.to_string())?;
    conn.execute(
        "INSERT INTO workspace_coderabbit_reviews (
            workspace_root, review_json, fingerprint_hash, completed_at, updated_at
         ) VALUES (?1, ?2, ?3, ?4, datetime('now'))
         ON CONFLICT(workspace_root) DO UPDATE SET
            review_json = excluded.review_json,
            fingerprint_hash = excluded.fingerprint_hash,
            completed_at = excluded.completed_at,
            updated_at = datetime('now')",
        params![
            &workspace_root,
            &review_json,
            &review.fingerprint.combined_hash,
            &review.completed_at
        ],
    )
    .map_err(|error| error.to_string())?;
    conn.execute(
        "INSERT INTO workspace_coderabbit_review_history (
            review_id, workspace_root, review_json, review_type, success,
            findings_count, fingerprint_hash, completed_at, saved_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, datetime('now'))",
        params![
            format!("cr-review-{}", uuid::Uuid::new_v4().simple()),
            &workspace_root,
            &review_json,
            review.review_type.as_cli_value(),
            if review.success { 1 } else { 0 },
            review.findings.len() as i64,
            &review.fingerprint.combined_hash,
            &review.completed_at
        ],
    )
    .map_err(|error| error.to_string())?;
    let updated_at = conn
        .query_row(
            "SELECT updated_at FROM workspace_coderabbit_reviews WHERE workspace_root = ?1",
            params![&workspace_root],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .flatten();
    Ok(WorkspaceCodeRabbitStoredReviewOutput {
        workspace_root,
        review: Some(review),
        updated_at,
    })
}

pub(crate) fn review_history(
    db_path: &Path,
    workspace_root: String,
    limit: u32,
) -> Result<WorkspaceCodeRabbitReviewHistoryOutput, String> {
    let conn = open_coderabbit_reviews_db(db_path)?;
    let mut stmt = conn
        .prepare(
            "SELECT review_id, workspace_root, review_json, review_type, success,
                    findings_count, fingerprint_hash, completed_at, saved_at
             FROM workspace_coderabbit_review_history
             WHERE workspace_root = ?1
             ORDER BY saved_at DESC
             LIMIT ?2",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![&workspace_root, limit as i64], |row| {
            let review_json: String = row.get(2)?;
            let review = serde_json::from_str::<WorkspaceCodeRabbitReviewOutput>(&review_json)
                .map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        2,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?;
            Ok(WorkspaceCodeRabbitReviewHistoryEntry {
                review_id: row.get(0)?,
                workspace_root: row.get(1)?,
                review,
                review_type: row.get(3)?,
                success: row.get::<_, i64>(4)? != 0,
                findings_count: row.get::<_, i64>(5)? as u32,
                fingerprint_hash: row.get(6)?,
                completed_at: row.get(7)?,
                saved_at: row.get(8)?,
            })
        })
        .map_err(|error| error.to_string())?;
    let entries = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    Ok(WorkspaceCodeRabbitReviewHistoryOutput {
        workspace_root,
        entries,
    })
}

pub(crate) fn clear_review(db_path: &Path, workspace_root: String) -> Result<(), String> {
    let conn = open_coderabbit_reviews_db(db_path)?;
    conn.execute(
        "DELETE FROM workspace_coderabbit_reviews WHERE workspace_root = ?1",
        params![&workspace_root],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub(crate) fn clear_workspace_artifacts(
    db_path: &Path,
    workspace_root: String,
) -> Result<(), String> {
    let conn = open_coderabbit_reviews_db(db_path)?;
    conn.execute(
        "DELETE FROM workspace_coderabbit_reviews WHERE workspace_root = ?1",
        params![&workspace_root],
    )
    .map_err(|error| error.to_string())?;
    conn.execute(
        "DELETE FROM workspace_coderabbit_review_history WHERE workspace_root = ?1",
        params![&workspace_root],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn open_coderabbit_reviews_db(db_path: &Path) -> Result<Connection, String> {
    let conn = Connection::open(db_path).map_err(|error| error.to_string())?;
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS workspace_coderabbit_reviews (
          workspace_root TEXT PRIMARY KEY,
          review_json TEXT NOT NULL,
          fingerprint_hash TEXT,
          completed_at TEXT,
          created_at TEXT DEFAULT (datetime('now')),
          updated_at TEXT DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_workspace_coderabbit_reviews_updated
          ON workspace_coderabbit_reviews(updated_at DESC);
        CREATE TABLE IF NOT EXISTS workspace_coderabbit_review_history (
          review_id TEXT PRIMARY KEY,
          workspace_root TEXT NOT NULL,
          review_json TEXT NOT NULL,
          review_type TEXT,
          success INTEGER DEFAULT 0,
          findings_count INTEGER DEFAULT 0,
          fingerprint_hash TEXT,
          completed_at TEXT,
          saved_at TEXT DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_workspace_coderabbit_review_history_workspace
          ON workspace_coderabbit_review_history(workspace_root, saved_at DESC);
        CREATE TRIGGER IF NOT EXISTS update_workspace_coderabbit_reviews_timestamp
        AFTER UPDATE ON workspace_coderabbit_reviews
        BEGIN
          UPDATE workspace_coderabbit_reviews
          SET updated_at = datetime('now')
          WHERE workspace_root = NEW.workspace_root;
        END;
        ",
    )
    .map_err(|error| error.to_string())?;
    Ok(conn)
}

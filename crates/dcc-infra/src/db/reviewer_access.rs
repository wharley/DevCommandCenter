//! The Reviewer reviews and does not open tasks (`agent_access`, group
//! `tasks`): a Reviewer seeded before that default gets it once; what the
//! person sets afterwards stays.

use super::*;
use dcc_core::domain::agent::REVIEWER_PRESET;
use dcc_core::domain::agent_access::{AccessLevel, ToolAccess};

const REVIEWER_NO_TASKS_MARKER: (&str, i64) = ("reviewer_no_tasks", 1);

impl SqliteSessionRepo {
    pub(super) fn reviewer_without_tasks(conn: &Connection) -> Result<()> {
        if Self::schema_marker_applied(conn, REVIEWER_NO_TASKS_MARKER)? {
            return Ok(());
        }
        let rows: Vec<(String, Option<String>)> = {
            let mut statement = conn
                .prepare("SELECT id, tool_access_json FROM dcc_agents WHERE preset = ?1")
                .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
            let rows = statement
                .query_map(params![REVIEWER_PRESET], |row| {
                    Ok((row.get(0)?, row.get(1)?))
                })
                .and_then(|rows| rows.collect::<rusqlite::Result<Vec<_>>>())
                .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
            rows
        };
        for (id, json) in rows {
            let mut access: ToolAccess = json
                .as_deref()
                .and_then(|json| from_str(json).ok())
                .unwrap_or_default();
            access
                .groups
                .entry("tasks".to_string())
                .or_insert(AccessLevel::None);
            let json = to_string(&access.normalized())
                .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
            conn.execute(
                "UPDATE dcc_agents SET tool_access_json = ?2 WHERE id = ?1",
                params![id, json],
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        }
        Self::record_schema_marker(conn, REVIEWER_NO_TASKS_MARKER)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dcc_core::domain::agent::ReviewerPresetText;

    #[test]
    fn the_reviewer_stops_opening_tasks_once_and_the_persons_choice_stays() {
        let repo = SqliteSessionRepo::from_connection(Arc::new(Mutex::new(
            Connection::open_in_memory().unwrap(),
        )))
        .unwrap();
        let reviewer = &repo
            .list_resident_agents(&ReviewerPresetText::default())
            .unwrap()[0];
        assert_eq!(reviewer.tool_access.group("tasks"), AccessLevel::None);
        // An older Reviewer with everything gets the default once…
        let conn = repo.conn.lock().unwrap();
        conn.execute(
            "UPDATE dcc_agents SET tool_access_json = NULL WHERE preset = ?1",
            params![REVIEWER_PRESET],
        )
        .unwrap();
        conn.execute(
            "DELETE FROM dcc_schema_markers WHERE name = 'reviewer_no_tasks'",
            [],
        )
        .unwrap();
        SqliteSessionRepo::reviewer_without_tasks(&conn).unwrap();
        let stored: String = conn
            .query_row(
                "SELECT tool_access_json FROM dcc_agents WHERE preset = ?1",
                params![REVIEWER_PRESET],
                |row| row.get(0),
            )
            .unwrap();
        assert!(stored.contains("\"tasks\":\"none\""), "{stored}");
        // …and never again over the person's choice.
        conn.execute(
            "UPDATE dcc_agents SET tool_access_json = NULL WHERE preset = ?1",
            params![REVIEWER_PRESET],
        )
        .unwrap();
        SqliteSessionRepo::reviewer_without_tasks(&conn).unwrap();
        let kept: Option<String> = conn
            .query_row(
                "SELECT tool_access_json FROM dcc_agents WHERE preset = ?1",
                params![REVIEWER_PRESET],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(kept, None);
    }
}

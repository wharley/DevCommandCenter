//! A resident agent's routines (`dcc_core::domain::agent_routine`), kept in
//! DCC's database because the agent proposes one with a tool the person
//! then creates: a small table the app reads once a minute to see what is
//! due.

use super::*;
use dcc_core::domain::agent_routine::{Routine, RoutineDraft, MAX_ROUTINES};

pub(super) const AGENT_ROUTINE_TABLE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS dcc_agent_routines (
    id TEXT PRIMARY KEY NOT NULL,
    agent_id TEXT NOT NULL,
    days TEXT NOT NULL,
    hour INTEGER NOT NULL,
    minute INTEGER NOT NULL,
    prompt TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    proposed INTEGER NOT NULL DEFAULT 0,
    last_run_at TEXT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (agent_id) REFERENCES dcc_agents(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_dcc_agent_routines_agent ON dcc_agent_routines(agent_id);
"#;

fn repository(error: rusqlite::Error) -> dcc_core::CoreError {
    dcc_core::CoreError::Repository(error.to_string())
}

fn days_text(days: &[u8]) -> String {
    days.iter().map(u8::to_string).collect::<Vec<_>>().join(",")
}

fn routine_from_row(row: &Row<'_>) -> rusqlite::Result<Routine> {
    let days: String = row.get(2)?;
    Ok(Routine {
        id: row.get(0)?,
        agent_id: row.get(1)?,
        days: days.split(',').filter_map(|day| day.parse().ok()).collect(),
        hour: row.get(3)?,
        minute: row.get(4)?,
        prompt: row.get(5)?,
        enabled: row.get::<_, i64>(6)? != 0,
        proposed: row.get::<_, i64>(7)? != 0,
        last_run_at: row.get(8)?,
        created_at: row.get(9)?,
    })
}

impl SqliteSessionRepo {
    /// Every live agent's routines, oldest first.
    pub fn list_agent_routines(&self) -> Result<Vec<Routine>> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let mut statement = conn
            .prepare(
                "SELECT r.id, r.agent_id, r.days, r.hour, r.minute, r.prompt, r.enabled, r.proposed, r.last_run_at, r.created_at
                 FROM dcc_agent_routines r JOIN dcc_agents a ON a.id = r.agent_id
                 WHERE a.deleted_at IS NULL ORDER BY r.created_at, r.id",
            )
            .map_err(repository)?;
        statement
            .query_map([], routine_from_row)
            .and_then(|rows| rows.collect::<rusqlite::Result<Vec<_>>>())
            .map_err(repository)
    }

    /// A routine of `agent_id`, new (`id` none) or changed; a proposed one
    /// waits for the person. At most `MAX_ROUTINES` per agent.
    pub fn save_agent_routine(
        &self,
        agent_id: &str,
        id: Option<&str>,
        draft: RoutineDraft,
        enabled: bool,
        proposed: bool,
    ) -> Result<Routine> {
        let draft = draft
            .normalized()
            .map_err(dcc_core::CoreError::InvalidInput)?;
        let now = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
        let id = {
            let conn = self
                .conn
                .lock()
                .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
            match id {
                Some(id) => {
                    let changed = conn
                        .execute(
                            "UPDATE dcc_agent_routines SET days = ?3, hour = ?4, minute = ?5, prompt = ?6, enabled = ?7, updated_at = ?8 WHERE id = ?1 AND agent_id = ?2",
                            params![id, agent_id, days_text(&draft.days), draft.hour, draft.minute, draft.prompt, enabled, now],
                        )
                        .map_err(repository)?;
                    if changed == 0 {
                        return Err(dcc_core::CoreError::InvalidInput(
                            "routine not found".into(),
                        ));
                    }
                    id.to_string()
                }
                None => {
                    let count: i64 = conn
                        .query_row(
                            "SELECT COUNT(*) FROM dcc_agent_routines WHERE agent_id = ?1",
                            params![agent_id],
                            |row| row.get(0),
                        )
                        .map_err(repository)?;
                    if count as usize >= MAX_ROUTINES {
                        return Err(dcc_core::CoreError::InvalidInput(format!(
                            "an agent has at most {MAX_ROUTINES} routines"
                        )));
                    }
                    let id = uuid::Uuid::new_v4().to_string();
                    conn.execute(
                        "INSERT INTO dcc_agent_routines (id, agent_id, days, hour, minute, prompt, enabled, proposed, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)",
                        params![id, agent_id, days_text(&draft.days), draft.hour, draft.minute, draft.prompt, enabled, proposed, now],
                    )
                    .map_err(repository)?;
                    id
                }
            }
        };
        self.list_agent_routines()?
            .into_iter()
            .find(|routine| routine.id == id)
            .ok_or_else(|| dcc_core::CoreError::InvalidInput("routine not found".into()))
    }

    pub fn delete_agent_routine(&self, agent_id: &str, id: &str) -> Result<bool> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let changed = conn
            .execute(
                "DELETE FROM dcc_agent_routines WHERE id = ?1 AND agent_id = ?2",
                params![id, agent_id],
            )
            .map_err(repository)?;
        Ok(changed > 0)
    }

    /// "Criar" on a proposal: it runs from now on.
    pub fn accept_agent_routine(&self, agent_id: &str, id: &str) -> Result<bool> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let now = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
        let changed = conn
            .execute(
                "UPDATE dcc_agent_routines SET proposed = 0, enabled = 1, created_at = ?3, updated_at = ?3 WHERE id = ?1 AND agent_id = ?2",
                params![id, agent_id, now],
            )
            .map_err(repository)?;
        Ok(changed > 0)
    }

    /// A routine ran (or was held) at `at`: it waits for its next time.
    pub fn mark_agent_routine_ran(&self, agent_id: &str, id: &str, at: &str) -> Result<bool> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let changed = conn
            .execute(
                "UPDATE dcc_agent_routines SET last_run_at = ?3 WHERE id = ?1 AND agent_id = ?2",
                params![id, agent_id, at],
            )
            .map_err(repository)?;
        Ok(changed > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dcc_core::domain::agent::{ResidentAgentDraft, ReviewerPresetText};

    #[test]
    fn routines_are_saved_capped_proposed_accepted_and_marked() {
        let repo = SqliteSessionRepo::from_connection(Arc::new(Mutex::new(
            Connection::open_in_memory().unwrap(),
        )))
        .unwrap();
        let mut draft = ResidentAgentDraft::reviewer(&ReviewerPresetText::default());
        draft.name = "QA".into();
        draft.role = "Teste.".into();
        let qa = repo.save_resident_agent(None, draft).unwrap();
        let routine = |prompt: &str| RoutineDraft {
            days: vec![0, 2],
            hour: 9,
            minute: 0,
            prompt: prompt.into(),
        };
        let first = repo
            .save_agent_routine(&qa.id, None, routine("Resuma a semana"), true, false)
            .unwrap();
        assert_eq!(first.days, [0, 2]);
        assert!(first.enabled && !first.proposed);
        let proposed = repo
            .save_agent_routine(&qa.id, None, routine("Olhe o CI"), false, true)
            .unwrap();
        assert!(proposed.proposed);
        assert!(repo.accept_agent_routine(&qa.id, &proposed.id).unwrap());
        assert!(repo
            .mark_agent_routine_ran(&qa.id, &first.id, "2026-11-09T12:00:00.000Z")
            .unwrap());
        let listed = repo.list_agent_routines().unwrap();
        assert_eq!(listed.len(), 2);
        // Both may share a millisecond: found by id, not by place.
        let by_id = |id: &str| listed.iter().find(|routine| routine.id == id).unwrap();
        let accepted = by_id(&proposed.id);
        assert!(!accepted.proposed && accepted.enabled);
        assert_eq!(
            by_id(&first.id).last_run_at.as_deref(),
            Some("2026-11-09T12:00:00.000Z")
        );
        for index in 0..3 {
            repo.save_agent_routine(&qa.id, None, routine(&format!("r{index}")), true, false)
                .unwrap();
        }
        assert!(repo
            .save_agent_routine(&qa.id, None, routine("demais"), true, false)
            .is_err());
        assert!(repo.delete_agent_routine(&qa.id, &first.id).unwrap());
        assert_eq!(repo.list_agent_routines().unwrap().len(), 4);
    }
}

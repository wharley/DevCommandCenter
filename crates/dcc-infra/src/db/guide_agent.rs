//! The Guide (`dcc_core::domain::agent_guide`) is made once, by the app
//! when it starts, and never again, even after the person deletes it (its
//! row stays, deleted). Not when the database opens: every reader beside
//! the app opens it too, and a read should write nothing.

use super::*;
use dcc_core::domain::agent_guide::{guide_draft, GUIDE_PRESET};

/// Its row's id: the same everywhere, so two processes opening the database
/// at once make one Guide.
const GUIDE_ID: &str = "dcc-guide";

/// Makes the Guide when no row of it exists, live or deleted.
fn seed_guide(conn: &rusqlite::Connection) -> Result<()> {
    let repository = |error: rusqlite::Error| dcc_core::CoreError::Repository(error.to_string());
    let seeded: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM dcc_agents WHERE preset = ?1)",
            params![GUIDE_PRESET],
            |row| row.get(0),
        )
        .map_err(repository)?;
    if seeded {
        return Ok(());
    }
    let draft = guide_draft()
        .normalized()
        .map_err(dcc_core::CoreError::InvalidInput)?;
    let access = serde_json::to_string(&draft.tool_access)
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
    let now = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    conn.execute(
        "INSERT OR IGNORE INTO dcc_agents (id, name, role, kickoff_prompt, provider_id, model, avatar_color, avatar_arms, avatar_eyes, preset, created_at, updated_at, offer_prompt, avatar_shape, avatar_accessory, tool_access_json) VALUES (?1, ?2, ?3, '', NULL, NULL, ?4, ?5, ?6, ?7, ?8, ?8, '', ?9, ?10, ?11)",
        params![
            GUIDE_ID,
            draft.name,
            draft.role,
            draft.avatar.color,
            draft.avatar.arms,
            draft.avatar.eyes,
            GUIDE_PRESET,
            now,
            draft.avatar.shape,
            draft.avatar.accessory,
            access,
        ],
    )
    .map_err(repository)?;
    Ok(())
}

impl SqliteSessionRepo {
    /// The app starting: the Guide is on the team, unless it was ever made.
    pub fn ensure_guide(&self) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        seed_guide(&conn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dcc_core::domain::agent::ReviewerPresetText;
    use dcc_core::domain::agent_guide::GUIDE_ROLE;

    #[test]
    fn the_guide_is_made_once_and_stays_gone_when_deleted() {
        let repo = SqliteSessionRepo::from_connection(Arc::new(Mutex::new(
            Connection::open_in_memory().unwrap(),
        )))
        .unwrap();
        let text = ReviewerPresetText::default();
        // Opening the database makes nobody.
        assert!(repo.list_stored_resident_agents(&text).unwrap().is_empty());
        repo.ensure_guide().unwrap();
        repo.ensure_guide().unwrap();
        let agents = repo.list_resident_agents(&text).unwrap();
        let presets: Vec<_> = agents.iter().map(|agent| agent.preset.as_deref()).collect();
        // The house's agents first: the Reviewer, then the Guide.
        assert_eq!(presets, [Some("reviewer"), Some("guide")]);
        let guide = agents[1].clone();
        assert_eq!((guide.id.as_str(), guide.name.as_str()), (GUIDE_ID, "Guia"));
        assert_eq!(guide.role, GUIDE_ROLE);
        assert_eq!(guide.avatar.shape, "cat");
        // Renamed by the person, its role stays DCC's.
        let mut draft = dcc_core::domain::agent_guide::guide_draft();
        draft.name = "Tuca".to_string();
        draft.role = "anything".to_string();
        repo.save_resident_agent(Some(&guide.id), draft).unwrap();
        let saved = repo.load_resident_agent(&guide.id).unwrap().unwrap();
        assert_eq!(saved.name, "Tuca");
        // Deleted, it is not made again.
        assert!(repo.delete_resident_agent(&guide.id).unwrap());
        repo.ensure_guide().unwrap();
        assert_eq!(repo.list_resident_agents(&text).unwrap().len(), 1);
    }
}

//! A resident agent's memory (`dcc_core::domain::agent_memory`): its notes,
//! the person's or one project's, and a version of the whole memory after
//! each change, so the person can bring an earlier one back. Kept in DCC's
//! own database: the notes are rules the agent follows in every turn, not
//! the project's ai-memory recall, which a turn gets as evidence only.

use super::*;
use dcc_core::domain::agent_conversation::home_agent_id;
use dcc_core::domain::agent_memory::{
    clean_note, fits, AgentMemoryNote, AgentMemoryVersion, MemoryChange, NoteSource,
    MAX_MEMORY_CHARS, MAX_VERSIONS,
};

pub(super) const AGENT_MEMORY_TABLE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS dcc_agent_memory_notes (
    id TEXT PRIMARY KEY NOT NULL,
    agent_id TEXT NOT NULL,
    body TEXT NOT NULL,
    source TEXT NOT NULL,
    project_id TEXT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (agent_id) REFERENCES dcc_agents(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_dcc_agent_memory_notes_agent
    ON dcc_agent_memory_notes(agent_id, created_at);
CREATE TABLE IF NOT EXISTS dcc_agent_memory_versions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    agent_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    change TEXT NOT NULL,
    summary TEXT NOT NULL,
    notes_json TEXT NOT NULL,
    FOREIGN KEY (agent_id) REFERENCES dcc_agents(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_dcc_agent_memory_versions_agent
    ON dcc_agent_memory_versions(agent_id, id);
"#;

fn repository_error(error: impl ToString) -> dcc_core::CoreError {
    dcc_core::CoreError::Repository(error.to_string())
}

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// Adds the layer column to a memory created before layers existed.
pub(super) fn migrate_agent_memory(conn: &Connection) -> Result<()> {
    SqliteWorkspaceRepo::ensure_column(conn, "dcc_agent_memory_notes", "project_id", "TEXT NULL")
}

fn notes_of(conn: &Connection, agent_id: &str) -> Result<Vec<AgentMemoryNote>> {
    let mut statement = conn
        .prepare(
            "SELECT id, body, source, created_at, updated_at, project_id FROM dcc_agent_memory_notes
             WHERE agent_id = ?1 ORDER BY created_at, id",
        )
        .map_err(repository_error)?;
    let notes = statement
        .query_map(params![agent_id], |row| {
            Ok(AgentMemoryNote {
                id: row.get(0)?,
                body: row.get(1)?,
                source: NoteSource::parse(&row.get::<_, String>(2)?),
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
                project_id: row.get(5)?,
            })
        })
        .map_err(repository_error)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(repository_error)?;
    Ok(notes)
}

fn ensure_live_agent(conn: &Connection, agent_id: &str) -> Result<()> {
    let live: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM dcc_agents WHERE id = ?1 AND deleted_at IS NULL)",
            params![agent_id],
            |row| row.get(0),
        )
        .map_err(repository_error)?;
    if live {
        Ok(())
    } else {
        Err(dcc_core::CoreError::InvalidInput(format!(
            "agent {agent_id} does not exist"
        )))
    }
}

/// Records the memory as it is now, and drops versions past the limit.
fn record_version(
    conn: &Connection,
    agent_id: &str,
    change: MemoryChange,
    summary: &str,
) -> Result<()> {
    let notes = notes_of(conn, agent_id)?;
    conn.execute(
        "INSERT INTO dcc_agent_memory_versions (agent_id, created_at, change, summary, notes_json)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            agent_id,
            now(),
            change.as_str(),
            summary,
            to_string(&notes).map_err(repository_error)?
        ],
    )
    .map_err(repository_error)?;
    conn.execute(
        "DELETE FROM dcc_agent_memory_versions WHERE agent_id = ?1 AND id NOT IN (
             SELECT id FROM dcc_agent_memory_versions WHERE agent_id = ?1
             ORDER BY id DESC LIMIT ?2)",
        params![agent_id, MAX_VERSIONS as i64],
    )
    .map_err(repository_error)?;
    Ok(())
}

fn insert_note(conn: &Connection, agent_id: &str, note: &AgentMemoryNote) -> Result<()> {
    conn.execute(
        "INSERT INTO dcc_agent_memory_notes (id, agent_id, body, source, created_at, updated_at, project_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            note.id,
            agent_id,
            note.body,
            note.source.as_str(),
            note.created_at,
            note.updated_at,
            note.project_id
        ],
    )
    .map_err(repository_error)?;
    Ok(())
}

fn full() -> dcc_core::CoreError {
    dcc_core::CoreError::InvalidInput(format!(
        "The memory is full ({MAX_MEMORY_CHARS} characters); edit or delete a note first."
    ))
}

impl SqliteSessionRepo {
    /// The agent's notes, oldest first.
    pub fn agent_memory(&self, agent_id: &str) -> Result<Vec<AgentMemoryNote>> {
        let conn = self.conn.lock().map_err(repository_error)?;
        notes_of(&conn, agent_id)
    }

    /// Saves a note in the person's layer (`project_id` `None`) or a
    /// project's. The same note again in that layer (ignoring case) returns
    /// the one already there.
    pub fn add_agent_memory_note(
        &self,
        agent_id: &str,
        body: &str,
        source: NoteSource,
        project_id: Option<&str>,
    ) -> Result<AgentMemoryNote> {
        let body = clean_note(body).map_err(dcc_core::CoreError::InvalidInput)?;
        let mut conn = self.conn.lock().map_err(repository_error)?;
        let tx = conn.transaction().map_err(repository_error)?;
        ensure_live_agent(&tx, agent_id)?;
        let notes = notes_of(&tx, agent_id)?;
        if let Some(same) = notes.iter().find(|note| {
            note.in_layer(project_id) && note.body.to_lowercase() == body.to_lowercase()
        }) {
            return Ok(same.clone());
        }
        if !fits(&notes, project_id, None, &body) {
            return Err(full());
        }
        let stamp = now();
        let note = AgentMemoryNote {
            id: uuid::Uuid::new_v4().to_string(),
            body,
            source,
            project_id: project_id.map(str::to_string),
            created_at: stamp.clone(),
            updated_at: stamp,
        };
        insert_note(&tx, agent_id, &note)?;
        let change = match source {
            NoteSource::Learned => MemoryChange::Learned,
            NoteSource::Person => MemoryChange::Added,
        };
        record_version(&tx, agent_id, change, &note.body)?;
        tx.commit().map_err(repository_error)?;
        Ok(note)
    }

    /// Rewrites a note; it becomes the person's.
    pub fn edit_agent_memory_note(
        &self,
        agent_id: &str,
        note_id: &str,
        body: &str,
    ) -> Result<AgentMemoryNote> {
        let body = clean_note(body).map_err(dcc_core::CoreError::InvalidInput)?;
        let mut conn = self.conn.lock().map_err(repository_error)?;
        let tx = conn.transaction().map_err(repository_error)?;
        let notes = notes_of(&tx, agent_id)?;
        let Some(note) = notes.iter().find(|note| note.id == note_id) else {
            return Err(dcc_core::CoreError::InvalidInput(
                "That note no longer exists.".to_string(),
            ));
        };
        if note.body == body {
            return Ok(note.clone());
        }
        if !fits(&notes, note.project_id.as_deref(), Some(note_id), &body) {
            return Err(full());
        }
        let stamp = now();
        tx.execute(
            "UPDATE dcc_agent_memory_notes SET body = ?1, source = ?2, updated_at = ?3
             WHERE id = ?4 AND agent_id = ?5",
            params![body, NoteSource::Person.as_str(), stamp, note_id, agent_id],
        )
        .map_err(repository_error)?;
        record_version(&tx, agent_id, MemoryChange::Edited, &body)?;
        tx.commit().map_err(repository_error)?;
        Ok(AgentMemoryNote {
            body,
            source: NoteSource::Person,
            updated_at: stamp,
            ..note.clone()
        })
    }

    pub fn remove_agent_memory_note(&self, agent_id: &str, note_id: &str) -> Result<()> {
        let mut conn = self.conn.lock().map_err(repository_error)?;
        let tx = conn.transaction().map_err(repository_error)?;
        let body: Option<String> = tx
            .query_row(
                "SELECT body FROM dcc_agent_memory_notes WHERE id = ?1 AND agent_id = ?2",
                params![note_id, agent_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(repository_error)?;
        let Some(body) = body else {
            return Ok(());
        };
        tx.execute(
            "DELETE FROM dcc_agent_memory_notes WHERE id = ?1 AND agent_id = ?2",
            params![note_id, agent_id],
        )
        .map_err(repository_error)?;
        record_version(&tx, agent_id, MemoryChange::Removed, &body)?;
        tx.commit().map_err(repository_error)?;
        Ok(())
    }

    /// The memory's versions, newest first.
    pub fn agent_memory_versions(&self, agent_id: &str) -> Result<Vec<AgentMemoryVersion>> {
        let conn = self.conn.lock().map_err(repository_error)?;
        let mut statement = conn
            .prepare(
                "SELECT id, created_at, change, summary, notes_json FROM dcc_agent_memory_versions
                 WHERE agent_id = ?1 ORDER BY id DESC",
            )
            .map_err(repository_error)?;
        let rows = statement
            .query_map(params![agent_id], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })
            .map_err(repository_error)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(repository_error)?;
        Ok(rows
            .into_iter()
            .filter_map(|(id, created_at, change, summary, notes)| {
                Some(AgentMemoryVersion {
                    id,
                    created_at,
                    change: MemoryChange::parse(&change)?,
                    summary,
                    notes: from_str(&notes).ok()?,
                })
            })
            .collect())
    }

    /// Brings the memory back to `version_id`; the restore is a version too.
    pub fn restore_agent_memory_version(
        &self,
        agent_id: &str,
        version_id: i64,
    ) -> Result<Vec<AgentMemoryNote>> {
        let mut conn = self.conn.lock().map_err(repository_error)?;
        let tx = conn.transaction().map_err(repository_error)?;
        ensure_live_agent(&tx, agent_id)?;
        let version: Option<(String, String)> = tx
            .query_row(
                "SELECT created_at, notes_json FROM dcc_agent_memory_versions
                 WHERE id = ?1 AND agent_id = ?2",
                params![version_id, agent_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(repository_error)?;
        let Some((created_at, notes_json)) = version else {
            return Err(dcc_core::CoreError::InvalidInput(
                "That version no longer exists.".to_string(),
            ));
        };
        let notes: Vec<AgentMemoryNote> = from_str(&notes_json).map_err(repository_error)?;
        tx.execute(
            "DELETE FROM dcc_agent_memory_notes WHERE agent_id = ?1",
            params![agent_id],
        )
        .map_err(repository_error)?;
        for note in &notes {
            insert_note(&tx, agent_id, note)?;
        }
        record_version(&tx, agent_id, MemoryChange::Restored, &created_at)?;
        tx.commit().map_err(repository_error)?;
        Ok(notes)
    }

    /// The project whose notes a turn of `session_id` carries: its task's
    /// project; `None` in an agent's own conversation.
    pub fn memory_project_of_session(&self, session_id: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().map_err(repository_error)?;
        let project: Option<String> = conn
            .query_row(
                "SELECT project_id FROM dcc_sessions WHERE id = ?1",
                params![session_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(repository_error)?;
        Ok(project.filter(|project| home_agent_id(project).is_none()))
    }

    /// The project of a checkout (a repository's root or a task's worktree),
    /// for a pull request review run there.
    pub fn memory_project_of_root(&self, root: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().map_err(repository_error)?;
        conn.query_row(
            "SELECT project_id FROM dcc_repositories WHERE root_path = ?1
             UNION ALL SELECT project_id FROM dcc_workspaces WHERE root_path = ?1
             LIMIT 1",
            params![root],
            |row| row.get(0),
        )
        .optional()
        .map_err(repository_error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dcc_core::domain::agent::ReviewerPresetText;

    fn repo_with_agent() -> (SqliteSessionRepo, String) {
        let repo = SqliteSessionRepo::from_connection(Arc::new(Mutex::new(
            Connection::open_in_memory().unwrap(),
        )))
        .unwrap();
        let agents = repo
            .list_resident_agents(&ReviewerPresetText {
                name: "Revisor".to_string(),
                ..ReviewerPresetText::default()
            })
            .unwrap();
        let id = agents[0].id.clone();
        (repo, id)
    }

    #[test]
    fn notes_keep_a_version_after_each_change() {
        let (repo, agent) = repo_with_agent();
        let learned = repo
            .add_agent_memory_note(&agent, "Responder em pt-BR", NoteSource::Learned, None)
            .unwrap();
        // The same note again is the one already there.
        let again = repo
            .add_agent_memory_note(&agent, "responder em PT-BR", NoteSource::Person, None)
            .unwrap();
        assert_eq!(again.id, learned.id);
        let mine = repo
            .add_agent_memory_note(&agent, "Nunca fazer merge", NoteSource::Person, None)
            .unwrap();
        let edited = repo
            .edit_agent_memory_note(&agent, &learned.id, "Sempre responder em pt-BR")
            .unwrap();
        assert_eq!(edited.source, NoteSource::Person);
        repo.remove_agent_memory_note(&agent, &mine.id).unwrap();

        let notes = repo.agent_memory(&agent).unwrap();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].body, "Sempre responder em pt-BR");

        let versions = repo.agent_memory_versions(&agent).unwrap();
        let changes: Vec<_> = versions.iter().map(|version| version.change).collect();
        assert_eq!(
            changes,
            [
                MemoryChange::Removed,
                MemoryChange::Edited,
                MemoryChange::Added,
                MemoryChange::Learned
            ]
        );
        // Back to before the edit and the delete.
        let restored = repo
            .restore_agent_memory_version(&agent, versions[2].id)
            .unwrap();
        assert_eq!(restored.len(), 2);
        assert_eq!(repo.agent_memory(&agent).unwrap(), restored);
        assert_eq!(
            repo.agent_memory_versions(&agent).unwrap()[0].change,
            MemoryChange::Restored
        );
    }

    #[test]
    fn the_memory_refuses_what_does_not_fit() {
        let (repo, agent) = repo_with_agent();
        for index in 0..13 {
            repo.add_agent_memory_note(
                &agent,
                &format!("{index:03}{}", "x".repeat(297)),
                NoteSource::Person,
                None,
            )
            .unwrap();
        }
        let error = repo
            .add_agent_memory_note(&agent, &"y".repeat(200), NoteSource::Learned, None)
            .unwrap_err();
        assert!(error.to_string().contains("full"), "{error}");
        // A project's layer has its own room.
        assert!(repo
            .add_agent_memory_note(
                &agent,
                &"y".repeat(200),
                NoteSource::Learned,
                Some("checkout")
            )
            .is_ok());
        assert!(repo
            .add_agent_memory_note("missing", "a rule", NoteSource::Person, None)
            .is_err());
    }

    #[test]
    fn only_the_latest_versions_stay() {
        let (repo, agent) = repo_with_agent();
        let note = repo
            .add_agent_memory_note(&agent, "rule 0", NoteSource::Person, None)
            .unwrap();
        for index in 1..=MAX_VERSIONS + 5 {
            repo.edit_agent_memory_note(&agent, &note.id, &format!("rule {index}"))
                .unwrap();
        }
        assert_eq!(
            repo.agent_memory_versions(&agent).unwrap().len(),
            MAX_VERSIONS
        );
    }

    #[test]
    fn each_note_keeps_its_layer() {
        let (repo, agent) = repo_with_agent();
        let mine = repo
            .add_agent_memory_note(&agent, "Usar pnpm", NoteSource::Person, None)
            .unwrap();
        // The same text in a project is a note of that project.
        let project = repo
            .add_agent_memory_note(&agent, "usar PNPM", NoteSource::Learned, Some("checkout"))
            .unwrap();
        assert_ne!(mine.id, project.id);
        assert_eq!(project.project_id.as_deref(), Some("checkout"));
        let edited = repo
            .edit_agent_memory_note(&agent, &project.id, "Usar pnpm 9")
            .unwrap();
        assert_eq!(edited.project_id.as_deref(), Some("checkout"));
        let versions = repo.agent_memory_versions(&agent).unwrap();
        repo.remove_agent_memory_note(&agent, &project.id).unwrap();
        let restored = repo
            .restore_agent_memory_version(&agent, versions[0].id)
            .unwrap();
        // Both notes may share a millisecond: the project's is found, not
        // taken by its place.
        assert!(restored
            .iter()
            .any(|note| note.project_id.as_deref() == Some("checkout")));
        assert_eq!(repo.agent_memory(&agent).unwrap(), restored);
    }
}

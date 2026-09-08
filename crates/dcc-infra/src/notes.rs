//! Project notes own their text and context. Retiring a workspace only detaches links.
use crate::db::SqliteSessionRepo;
use chrono::Utc;
use rusqlite::{params, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

pub(crate) const NOTES_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS dcc_project_notes (
 id TEXT PRIMARY KEY NOT NULL,
 project_id TEXT NOT NULL,
 project_name TEXT NOT NULL,
 title TEXT NOT NULL,
 content TEXT NOT NULL,
 context_snapshot TEXT NOT NULL,
 source_session_id TEXT REFERENCES dcc_sessions(id) ON DELETE SET NULL,
 source_workspace_id TEXT REFERENCES dcc_workspaces(id) ON DELETE SET NULL,
 source_task_title TEXT NOT NULL,
 implementation_task_id TEXT REFERENCES dcc_workspaces(id) ON DELETE SET NULL,
 status TEXT NOT NULL DEFAULT 'open' CHECK(status IN ('open', 'completed')),
 color TEXT NOT NULL DEFAULT 'amber' CHECK(color IN ('amber', 'mint', 'violet', 'sky')),
 pinned INTEGER NOT NULL DEFAULT 0,
 revision INTEGER NOT NULL DEFAULT 1,
 created_at TEXT NOT NULL,
 updated_at TEXT NOT NULL,
 completed_at TEXT
);
CREATE INDEX IF NOT EXISTS idx_dcc_notes_project_status ON dcc_project_notes(project_id, status, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_dcc_notes_implementation ON dcc_project_notes(implementation_task_id);
"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectNote {
    pub id: String,
    pub project_id: String,
    pub project_name: String,
    pub title: String,
    pub content: String,
    pub context_snapshot: String,
    pub source_session_id: Option<String>,
    pub source_workspace_id: Option<String>,
    pub source_task_title: String,
    pub implementation_task_id: Option<String>,
    pub status: String,
    pub color: String,
    pub pinned: bool,
    pub revision: i64,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateProjectNote {
    pub project_id: String,
    pub project_name: String,
    pub title: String,
    pub content: String,
    pub context_snapshot: String,
    pub source_session_id: Option<String>,
    pub source_workspace_id: Option<String>,
    pub source_task_title: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProjectNote {
    pub id: String,
    pub revision: i64,
    pub title: String,
    pub content: String,
    pub status: String,
    pub color: String,
    pub pinned: bool,
    pub implementation_task_id: Option<String>,
}

fn read_note(row: &Row<'_>) -> rusqlite::Result<ProjectNote> {
    Ok(ProjectNote {
        id: row.get("id")?,
        project_id: row.get("project_id")?,
        project_name: row.get("project_name")?,
        title: row.get("title")?,
        content: row.get("content")?,
        context_snapshot: row.get("context_snapshot")?,
        source_session_id: row.get("source_session_id")?,
        source_workspace_id: row.get("source_workspace_id")?,
        source_task_title: row.get("source_task_title")?,
        implementation_task_id: row.get("implementation_task_id")?,
        status: row.get("status")?,
        color: row.get("color")?,
        pinned: row.get("pinned")?,
        revision: row.get("revision")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
        completed_at: row.get("completed_at")?,
    })
}

fn bounded(value: &str, max: usize) -> Result<(), String> {
    if value.chars().count() > max {
        Err("note_too_large".into())
    } else {
        Ok(())
    }
}

impl SqliteSessionRepo {
    pub fn list_project_notes(&self) -> Result<Vec<ProjectNote>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut statement = conn
            .prepare("SELECT * FROM dcc_project_notes ORDER BY updated_at DESC, id")
            .map_err(|e| e.to_string())?;
        let notes = statement
            .query_map([], read_note)
            .map_err(|e| e.to_string())?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|e| e.to_string())?;
        Ok(notes)
    }

    pub fn create_project_note(&self, input: CreateProjectNote) -> Result<ProjectNote, String> {
        if input.project_id.trim().is_empty() {
            return Err("note_project_required".into());
        }
        bounded(&input.project_id, 256)?;
        bounded(&input.project_name, 300)?;
        bounded(&input.source_task_title, 300)?;
        bounded(&input.title, 160)?;
        bounded(&input.content, 20_000)?;
        bounded(&input.context_snapshot, 12_000)?;
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        // A source may disappear between capture and persistence. Keep the snapshot.
        let source_workspace: Option<String> = conn
            .query_row(
                "SELECT id FROM dcc_workspaces WHERE id = ?1 AND project_id = ?2",
                params![input.source_workspace_id, input.project_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        let source_session: Option<String> = conn.query_row(
            "SELECT id FROM dcc_sessions WHERE id = ?1 AND project_id = ?2 AND workspace_id = ?3",
            params![input.source_session_id, input.project_id, source_workspace], |row| row.get(0),
        ).optional().map_err(|e| e.to_string())?;
        let id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        conn.execute("INSERT INTO dcc_project_notes (id, project_id, project_name, title, content, context_snapshot, source_session_id, source_workspace_id, source_task_title, created_at, updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?10)",
            params![id, input.project_id, input.project_name, input.title, input.content, input.context_snapshot, source_session, source_workspace, input.source_task_title, now]).map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT * FROM dcc_project_notes WHERE id = ?1",
            [id],
            read_note,
        )
        .map_err(|e| e.to_string())
    }

    pub fn update_project_note(&self, input: UpdateProjectNote) -> Result<ProjectNote, String> {
        bounded(&input.title, 160)?;
        bounded(&input.content, 20_000)?;
        if !["open", "completed"].contains(&input.status.as_str())
            || !["amber", "mint", "violet", "sky"].contains(&input.color.as_str())
        {
            return Err("note_invalid_state".into());
        }
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let now = Utc::now().to_rfc3339();
        // Resolve the link in SQL: stale UI cannot reintroduce a deleted workspace.
        let count = conn.execute("UPDATE dcc_project_notes SET title=?1, content=?2, status=?3, color=?4, pinned=?5, implementation_task_id=(SELECT id FROM dcc_workspaces WHERE id=?6), updated_at=?7, completed_at=CASE WHEN ?3='completed' THEN COALESCE(completed_at,?7) ELSE NULL END, revision=revision+1 WHERE id=?8 AND revision=?9",
            params![input.title, input.content, input.status, input.color, input.pinned, input.implementation_task_id, now, input.id, input.revision]).map_err(|e| e.to_string())?;
        if count == 0 {
            return Err("note_conflict".into());
        }
        conn.query_row(
            "SELECT * FROM dcc_project_notes WHERE id=?1",
            [input.id],
            read_note,
        )
        .map_err(|e| e.to_string())
    }

    pub fn delete_project_notes(&self, ids: &[String]) -> Result<(), String> {
        if ids.len() > 10_000 {
            return Err("note_too_large".into());
        }
        let mut conn = self.conn.lock().map_err(|e| e.to_string())?;
        let transaction = conn.transaction().map_err(|e| e.to_string())?;
        for id in ids {
            transaction
                .execute("DELETE FROM dcc_project_notes WHERE id=?1", [id])
                .map_err(|e| e.to_string())?;
        }
        transaction.commit().map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dcc_core::domain::workspace::WorkspaceId;

    fn input() -> CreateProjectNote {
        CreateProjectNote {
            project_id: "project".into(),
            project_name: "Project".into(),
            title: "Sessions".into(),
            content: "Revoke a device".into(),
            context_snapshot: "Independent context".into(),
            source_session_id: Some("session".into()),
            source_workspace_id: Some("workspace".into()),
            source_task_title: "Login".into(),
        }
    }
    fn update(note: &ProjectNote) -> UpdateProjectNote {
        UpdateProjectNote {
            id: note.id.clone(),
            revision: note.revision,
            title: note.title.clone(),
            content: note.content.clone(),
            status: "completed".into(),
            color: "mint".into(),
            pinned: false,
            implementation_task_id: Some("workspace".into()),
        }
    }

    #[test]
    fn note_survives_real_history_cleanup_and_database_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notes.sqlite");
        let repo = SqliteSessionRepo::open(&path).unwrap();
        repo.conn.lock().unwrap().execute_batch("INSERT INTO dcc_workspaces (id,project_id,root_path,base_branch,state,created_at,updated_at) VALUES ('workspace','project','/tmp/notes','main','ready','t0','t0'); INSERT INTO dcc_sessions (id,project_id,workspace_id,provider_id,state,created_at,updated_at) VALUES ('session','project','workspace','fixture','active','t0','t0');").unwrap();
        let note = repo.create_project_note(input()).unwrap();
        assert_eq!(note.source_session_id.as_deref(), Some("session"));
        let note = repo.update_project_note(update(&note)).unwrap();
        repo.delete_workspace_history(&[WorkspaceId("workspace".into())])
            .unwrap();
        repo.conn
            .lock()
            .unwrap()
            .execute("DELETE FROM dcc_workspaces WHERE id='workspace'", [])
            .unwrap();
        drop(repo);
        let repo = SqliteSessionRepo::open(path).unwrap();
        let saved = repo.list_project_notes().unwrap().remove(0);
        assert_eq!(saved.id, note.id);
        assert_eq!(saved.content, "Revoke a device");
        assert_eq!(saved.context_snapshot, "Independent context");
        assert_eq!(saved.source_task_title, "Login");
        assert!(
            saved.source_session_id.is_none()
                && saved.source_workspace_id.is_none()
                && saved.implementation_task_id.is_none()
        );
        assert_eq!(saved.status, "completed");
        assert!(saved.completed_at.is_some());
        let mut reopen = update(&saved);
        reopen.status = "open".into();
        assert!(repo
            .update_project_note(reopen)
            .unwrap()
            .completed_at
            .is_none());
    }

    #[test]
    fn concurrent_edits_do_not_overwrite_and_deleted_notes_cannot_resurrect() {
        let repo = SqliteSessionRepo::open(":memory:").unwrap();
        let note = repo.create_project_note(input()).unwrap();
        repo.update_project_note(update(&note)).unwrap();
        assert_eq!(
            repo.update_project_note(update(&note)).unwrap_err(),
            "note_conflict"
        );
        repo.delete_project_notes(&[note.id.clone()]).unwrap();
        assert_eq!(
            repo.update_project_note(update(&note)).unwrap_err(),
            "note_conflict"
        );
        assert!(repo.list_project_notes().unwrap().is_empty());
    }

    #[test]
    fn oversized_context_is_rejected_without_silent_truncation() {
        let repo = SqliteSessionRepo::open(":memory:").unwrap();
        let mut draft = input();
        draft.context_snapshot = "é".repeat(12_001);
        assert_eq!(
            repo.create_project_note(draft).unwrap_err(),
            "note_too_large"
        );
        assert!(repo.list_project_notes().unwrap().is_empty());
    }
}

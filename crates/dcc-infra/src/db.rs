use std::sync::{Arc, Mutex};
use std::{fmt, path::Path};

use async_trait::async_trait;
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde_json::{from_str, to_string};

use dcc_core::{
    domain::{
        project::ProjectId,
        repository::{Repository, RepositoryId},
        session::{
            Session, SessionEventKind, SessionEventRecord, SessionId, SessionProjection,
            SessionState, WorkspaceSessionSummary,
        },
        thread::{Thread, ThreadId},
        workspace::{Workspace, WorkspaceId, WorkspaceState},
    },
    ports::{RepositoryRepo, SessionEventRepo, SessionRepo, ThreadRepo, WorkspaceRepo},
    Result,
};

const WORKSPACE_TABLE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS dcc_workspaces (
	id TEXT PRIMARY KEY NOT NULL,
	project_id TEXT NOT NULL,
	name TEXT NULL,
	root_path TEXT NOT NULL,
	base_branch TEXT NOT NULL,
	worktree_path TEXT NULL,
	state TEXT NOT NULL,
	setup_report_json TEXT NULL,
	created_at TEXT NOT NULL,
	updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_dcc_workspaces_project_id
ON dcc_workspaces(project_id);
"#;

const REPOSITORY_TABLE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS dcc_repositories (
	id TEXT PRIMARY KEY NOT NULL,
	project_id TEXT NOT NULL,
	name TEXT NOT NULL,
	root_path TEXT NOT NULL UNIQUE,
	base_branch TEXT NOT NULL,
	remote TEXT NULL,
	remote_url TEXT NULL,
	forge_provider TEXT NULL,
	forge_login TEXT NULL,
	created_at TEXT NOT NULL,
	updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_dcc_repositories_project_id
ON dcc_repositories(project_id);

CREATE INDEX IF NOT EXISTS idx_dcc_repositories_updated_at
ON dcc_repositories(updated_at DESC);
"#;

const FORGE_LOGIN_PREFERENCE_TABLE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS dcc_forge_login_preferences (
	provider TEXT NOT NULL,
	host TEXT NOT NULL,
	login TEXT NOT NULL,
	PRIMARY KEY (provider, host)
);
"#;

const SESSION_TABLE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS dcc_sessions (
	id TEXT PRIMARY KEY NOT NULL,
	project_id TEXT NOT NULL,
	workspace_id TEXT NOT NULL,
	provider_id TEXT NOT NULL,
	model TEXT NULL,
	provider_runtime_json TEXT NULL,
	state TEXT NOT NULL,
	created_at TEXT NOT NULL,
	updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_dcc_sessions_workspace_id
	ON dcc_sessions(workspace_id);

CREATE TABLE IF NOT EXISTS dcc_threads (
	id TEXT PRIMARY KEY NOT NULL,
	project_id TEXT NOT NULL,
	session_id TEXT NULL UNIQUE,
	title TEXT NOT NULL,
	archived_at TEXT NULL,
	FOREIGN KEY (session_id) REFERENCES dcc_sessions(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_dcc_threads_session_id
	ON dcc_threads(session_id);

CREATE INDEX IF NOT EXISTS idx_dcc_threads_archived_at
	ON dcc_threads(archived_at);

CREATE TABLE IF NOT EXISTS dcc_session_events (
	event_id TEXT PRIMARY KEY NOT NULL,
	session_id TEXT NOT NULL,
	sequence INTEGER NOT NULL,
	occurred_at TEXT NOT NULL,
	kind_json TEXT NOT NULL,
	UNIQUE(session_id, sequence),
	FOREIGN KEY (session_id) REFERENCES dcc_sessions(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_dcc_session_events_session_sequence
	ON dcc_session_events(session_id, sequence);
"#;

#[derive(Clone)]
pub struct SqliteWorkspaceRepo {
    conn: Arc<Mutex<Connection>>,
}

#[derive(Debug, Clone)]
pub struct ForgeBoundRepositoryRecord {
    pub id: RepositoryId,
    pub login: String,
}

impl SqliteWorkspaceRepo {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path.as_ref())
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let repo = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        repo.ensure_schema()?;
        Ok(repo)
    }

    pub fn from_connection(conn: Arc<Mutex<Connection>>) -> Result<Self> {
        let repo = Self { conn };
        repo.ensure_schema()?;
        Ok(repo)
    }

    fn ensure_schema(&self) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        conn.execute_batch(&format!(
            "PRAGMA foreign_keys = ON;\n{WORKSPACE_TABLE_SQL}\n{REPOSITORY_TABLE_SQL}\n{FORGE_LOGIN_PREFERENCE_TABLE_SQL}"
        ))
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Self::ensure_column(&conn, "dcc_workspaces", "setup_report_json", "TEXT NULL")?;
        Self::ensure_column(&conn, "dcc_repositories", "remote", "TEXT NULL")?;
        Self::ensure_column(&conn, "dcc_repositories", "remote_url", "TEXT NULL")?;
        Self::ensure_column(&conn, "dcc_repositories", "forge_provider", "TEXT NULL")?;
        Self::ensure_column(&conn, "dcc_repositories", "forge_login", "TEXT NULL")?;
        Ok(())
    }

    pub fn get_forge_login_preference(&self, provider: &str, host: &str) -> Result<Option<String>> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        conn.query_row(
            "SELECT login FROM dcc_forge_login_preferences WHERE provider = ?1 AND host = ?2",
            params![provider, host],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))
    }

    pub fn set_forge_login_preference(
        &self,
        provider: &str,
        host: &str,
        login: Option<&str>,
    ) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let normalized_login = login.map(str::trim).filter(|login| !login.is_empty());
        if let Some(login) = normalized_login {
            conn.execute(
                "INSERT INTO dcc_forge_login_preferences (provider, host, login)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(provider, host) DO UPDATE SET login = excluded.login",
                params![provider, host, login],
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        } else {
            conn.execute(
                "DELETE FROM dcc_forge_login_preferences WHERE provider = ?1 AND host = ?2",
                params![provider, host],
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        }
        Ok(())
    }

    pub fn update_repository_forge_login(
        &self,
        repository_id: &RepositoryId,
        login: Option<&str>,
    ) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        conn.execute(
            "UPDATE dcc_repositories SET forge_login = ?1, updated_at = datetime('now') WHERE id = ?2",
            params![login.map(str::trim).filter(|value| !value.is_empty()), repository_id.0.clone()],
        )
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Ok(())
    }

    pub fn list_repositories_needing_forge_binding(&self) -> Result<Vec<RepositoryId>> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let mut stmt = conn
            .prepare(
                r#"
                SELECT id
                  FROM dcc_repositories
                 WHERE forge_login IS NULL
                   AND remote_url IS NOT NULL
                   AND forge_provider IN ('github', 'gitlab')
                 ORDER BY updated_at DESC, created_at DESC
                "#,
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let rows = stmt
            .query_map([], |row| Ok(RepositoryId(row.get::<_, String>(0)?)))
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;

        let mut repository_ids = Vec::new();
        for row in rows {
            repository_ids
                .push(row.map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?);
        }
        Ok(repository_ids)
    }

    pub fn list_forge_bound_repositories(&self) -> Result<Vec<ForgeBoundRepositoryRecord>> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let mut stmt = conn
            .prepare(
                r#"
                SELECT id, forge_login
                  FROM dcc_repositories
                 WHERE forge_login IS NOT NULL
                   AND remote_url IS NOT NULL
                   AND forge_provider IN ('github', 'gitlab')
                 ORDER BY updated_at DESC, created_at DESC
                "#,
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(ForgeBoundRepositoryRecord {
                    id: RepositoryId(row.get::<_, String>(0)?),
                    login: row.get::<_, String>(1)?,
                })
            })
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;

        let mut repositories = Vec::new();
        for row in rows {
            repositories
                .push(row.map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?);
        }
        Ok(repositories)
    }

    fn ensure_column(conn: &Connection, table: &str, column: &str, sql_type: &str) -> Result<()> {
        let pragma = format!("PRAGMA table_info({table})");
        let mut stmt = conn
            .prepare(&pragma)
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let existing_columns = stmt
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        if existing_columns.iter().any(|existing| existing == column) {
            return Ok(());
        }

        let sql = format!("ALTER TABLE {table} ADD COLUMN {column} {sql_type}");
        conn.execute(&sql, [])
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Ok(())
    }

    fn workspace_state_as_str(state: &WorkspaceState) -> &'static str {
        match state {
            WorkspaceState::Initializing => "initializing",
            WorkspaceState::SetupPending => "setup_pending",
            WorkspaceState::Ready => "ready",
            WorkspaceState::Archived => "archived",
        }
    }

    fn workspace_from_row(row: &Row<'_>) -> rusqlite::Result<Workspace> {
        let state = row.get::<_, String>(6)?;
        let state = match state.as_str() {
            "initializing" => WorkspaceState::Initializing,
            "setup_pending" => WorkspaceState::SetupPending,
            "ready" => WorkspaceState::Ready,
            "archived" => WorkspaceState::Archived,
            other => {
                return Err(rusqlite::Error::FromSqlConversionFailure(
                    6,
                    rusqlite::types::Type::Text,
                    Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("unknown workspace state: {other}"),
                    )),
                ))
            }
        };

        let setup_report_json = row.get::<_, Option<String>>(7)?;
        let setup_report = setup_report_json
            .as_deref()
            .map(|json| {
                from_str(json).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        7,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })
            })
            .transpose()?;

        Ok(Workspace {
            id: WorkspaceId(row.get::<_, String>(0)?),
            project_id: ProjectId(row.get::<_, String>(1)?),
            name: row.get::<_, Option<String>>(2)?,
            root_path: row.get::<_, String>(3)?,
            base_branch: row.get::<_, String>(4)?,
            worktree_path: row.get::<_, Option<String>>(5)?,
            state,
            setup_report,
            created_at: row.get::<_, String>(8)?,
            updated_at: row.get::<_, String>(9)?,
        })
    }

    fn repository_from_row(row: &Row<'_>) -> rusqlite::Result<Repository> {
        Ok(Repository {
            id: RepositoryId(row.get::<_, String>(0)?),
            project_id: ProjectId(row.get::<_, String>(1)?),
            name: row.get::<_, String>(2)?,
            root_path: row.get::<_, String>(3)?,
            base_branch: row.get::<_, String>(4)?,
            remote: row.get::<_, Option<String>>(5)?,
            remote_url: row.get::<_, Option<String>>(6)?,
            forge_provider: row.get::<_, Option<String>>(7)?,
            forge_login: row.get::<_, Option<String>>(8)?,
            created_at: row.get::<_, String>(9)?,
            updated_at: row.get::<_, String>(10)?,
        })
    }
}

#[derive(Clone)]
pub struct SqliteSessionRepo {
    conn: Arc<Mutex<Connection>>,
}

impl fmt::Debug for SqliteSessionRepo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SqliteSessionRepo").finish_non_exhaustive()
    }
}

impl SqliteSessionRepo {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path.as_ref())
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let repo = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        repo.ensure_schema()?;
        Ok(repo)
    }

    pub fn from_connection(conn: Arc<Mutex<Connection>>) -> Result<Self> {
        let repo = Self { conn };
        repo.ensure_schema()?;
        Ok(repo)
    }

    fn ensure_schema(&self) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        conn.execute_batch(&format!("PRAGMA foreign_keys = ON;\n{SESSION_TABLE_SQL}"))
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Ok(())
    }

    fn session_state_as_str(state: &SessionState) -> &'static str {
        match state {
            SessionState::Draft => "draft",
            SessionState::Active => "active",
            SessionState::Completed => "completed",
            SessionState::Aborted => "aborted",
        }
    }

    fn session_state_from_str(state: &str, column: usize) -> rusqlite::Result<SessionState> {
        match state {
            "draft" => Ok(SessionState::Draft),
            "active" => Ok(SessionState::Active),
            "completed" => Ok(SessionState::Completed),
            "aborted" => Ok(SessionState::Aborted),
            other => Err(rusqlite::Error::FromSqlConversionFailure(
                column,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("unknown session state: {other}"),
                )),
            )),
        }
    }

    fn session_from_row(row: &Row<'_>) -> rusqlite::Result<Session> {
        let provider_runtime_json = row.get::<_, Option<String>>(5)?;
        let provider_runtime = provider_runtime_json
            .as_deref()
            .map(|json| {
                from_str(json).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        5,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })
            })
            .transpose()?;

        Ok(Session {
            id: SessionId(row.get::<_, String>(0)?),
            project_id: ProjectId(row.get::<_, String>(1)?),
            workspace_id: WorkspaceId(row.get::<_, String>(2)?),
            provider_id: row.get::<_, String>(3)?,
            model: row.get::<_, Option<String>>(4)?,
            provider_runtime,
            state: Self::session_state_from_str(&row.get::<_, String>(6)?, 6)?,
            created_at: row.get::<_, String>(7)?,
            updated_at: row.get::<_, String>(8)?,
        })
    }

    fn thread_from_row(row: &Row<'_>) -> rusqlite::Result<Thread> {
        Ok(Thread {
            id: ThreadId(row.get::<_, String>(0)?),
            project_id: ProjectId(row.get::<_, String>(1)?),
            session_id: row.get::<_, Option<String>>(2)?.map(SessionId),
            title: row.get::<_, String>(3)?,
            archived_at: row.get::<_, Option<String>>(4)?,
        })
    }

    fn event_from_row(row: &Row<'_>) -> rusqlite::Result<SessionEventRecord> {
        let kind_json = row.get::<_, String>(3)?;
        let kind = from_str::<SessionEventKind>(&kind_json).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                3,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;

        Ok(SessionEventRecord {
            event_id: row.get::<_, String>(0)?,
            session_id: SessionId(row.get::<_, String>(1)?),
            sequence: row.get::<_, u64>(2)?,
            occurred_at: row.get::<_, String>(4)?,
            kind,
        })
    }

    fn list_events_by_session_sync(
        &self,
        session_id: &SessionId,
    ) -> Result<Vec<SessionEventRecord>> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let mut stmt = conn
            .prepare(
                r#"
				SELECT event_id, session_id, sequence, kind_json, occurred_at
				  FROM dcc_session_events
				 WHERE session_id = ?1
				 ORDER BY sequence ASC
				"#,
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let rows = stmt
            .query_map(params![session_id.0.clone()], Self::event_from_row)
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;

        let mut events = Vec::new();
        for row in rows {
            events.push(row.map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?);
        }

        Ok(events)
    }

    fn build_summary(&self, session: Session, thread: Thread) -> Result<WorkspaceSessionSummary> {
        let events = self.list_events_by_session_sync(&session.id)?;
        let mut last_turn_prompt = None;
        let mut last_turn_state = None;
        for event in &events {
            match &event.kind {
                SessionEventKind::TurnStarted { prompt, .. } => {
                    last_turn_prompt = Some(prompt.clone());
                    last_turn_state = Some("running".to_string());
                }
                SessionEventKind::TurnCompleted { .. } => {
                    last_turn_state = Some("completed".to_string());
                }
                SessionEventKind::TurnAborted { .. } => {
                    last_turn_state = Some("aborted".to_string());
                }
                SessionEventKind::SessionCompleted => {
                    if last_turn_state.is_none() {
                        last_turn_state = Some("completed".to_string());
                    }
                }
                SessionEventKind::SessionAborted { .. } => {
                    if last_turn_state.is_none() {
                        last_turn_state = Some("aborted".to_string());
                    }
                }
                _ => {}
            }
        }

        let projection = SessionProjection::fold(&events).unwrap_or_else(|| {
            SessionProjection::new(
                session.id.clone(),
                session.project_id.clone(),
                session.workspace_id.clone(),
                session.provider_id.clone(),
                session.model.clone(),
                session.created_at.clone(),
            )
        });

        Ok(WorkspaceSessionSummary {
            session,
            thread,
            projection,
            last_turn_prompt,
            last_turn_state,
        })
    }

    pub fn list_workspace_sessions(
        &self,
        workspace_id: &WorkspaceId,
    ) -> Result<Vec<WorkspaceSessionSummary>> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let mut stmt = conn
            .prepare(
                r#"
				SELECT
					s.id, s.project_id, s.workspace_id, s.provider_id, s.model,
					s.provider_runtime_json, s.state, s.created_at, s.updated_at,
					t.id, t.project_id, t.session_id, t.title, t.archived_at
				  FROM dcc_sessions s
				  JOIN dcc_threads t ON t.session_id = s.id
				 WHERE s.workspace_id = ?1
				 ORDER BY
				   CASE WHEN t.archived_at IS NULL THEN 0 ELSE 1 END ASC,
				   s.created_at DESC,
				   t.title DESC
				"#,
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;

        let rows = stmt
            .query_map(params![workspace_id.0.clone()], |row| {
                let session = Session {
                    id: SessionId(row.get::<_, String>(0)?),
                    project_id: ProjectId(row.get::<_, String>(1)?),
                    workspace_id: WorkspaceId(row.get::<_, String>(2)?),
                    provider_id: row.get::<_, String>(3)?,
                    model: row.get::<_, Option<String>>(4)?,
                    provider_runtime: row
                        .get::<_, Option<String>>(5)?
                        .as_deref()
                        .map(|json| {
                            from_str(json).map_err(|error| {
                                rusqlite::Error::FromSqlConversionFailure(
                                    5,
                                    rusqlite::types::Type::Text,
                                    Box::new(error),
                                )
                            })
                        })
                        .transpose()?,
                    state: Self::session_state_from_str(&row.get::<_, String>(6)?, 6)?,
                    created_at: row.get::<_, String>(7)?,
                    updated_at: row.get::<_, String>(8)?,
                };
                let thread = Thread {
                    id: ThreadId(row.get::<_, String>(9)?),
                    project_id: ProjectId(row.get::<_, String>(10)?),
                    session_id: row.get::<_, Option<String>>(11)?.map(SessionId),
                    title: row.get::<_, String>(12)?,
                    archived_at: row.get::<_, Option<String>>(13)?,
                };
                Ok((session, thread))
            })
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;

        let pairs = rows
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        drop(stmt);
        drop(conn);

        pairs
            .into_iter()
            .map(|(session, thread)| self.build_summary(session, thread))
            .collect()
    }
}

#[async_trait]
impl WorkspaceRepo for SqliteWorkspaceRepo {
    async fn save_workspace(&self, workspace: &Workspace) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;

        conn.execute(
            r#"
			INSERT INTO dcc_workspaces (
				id, project_id, name, root_path, base_branch, worktree_path,
				state, setup_report_json, created_at, updated_at
			) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
			ON CONFLICT(id) DO UPDATE SET
				project_id = excluded.project_id,
				name = excluded.name,
				root_path = excluded.root_path,
				base_branch = excluded.base_branch,
				worktree_path = excluded.worktree_path,
				state = excluded.state,
				setup_report_json = excluded.setup_report_json,
				created_at = excluded.created_at,
				updated_at = excluded.updated_at
			"#,
            params![
                workspace.id.0.clone(),
                workspace.project_id.0.clone(),
                workspace.name.clone(),
                workspace.root_path.clone(),
                workspace.base_branch.clone(),
                workspace.worktree_path.clone(),
                Self::workspace_state_as_str(&workspace.state),
                workspace
                    .setup_report
                    .as_ref()
                    .map(to_string)
                    .transpose()
                    .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?,
                workspace.created_at.clone(),
                workspace.updated_at.clone(),
            ],
        )
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;

        Ok(())
    }

    async fn get_workspace(&self, id: &WorkspaceId) -> Result<Option<Workspace>> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let workspace = conn
            .query_row(
                r#"
				SELECT id, project_id, name, root_path, base_branch, worktree_path,
				       state, setup_report_json, created_at, updated_at
				  FROM dcc_workspaces
				 WHERE id = ?1
				"#,
                params![id.0.clone()],
                Self::workspace_from_row,
            )
            .optional()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;

        Ok(workspace)
    }

    async fn list_workspaces(&self) -> Result<Vec<Workspace>> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let mut stmt = conn
            .prepare(
                r#"
				SELECT id, project_id, name, root_path, base_branch, worktree_path,
				       state, setup_report_json, created_at, updated_at
				  FROM dcc_workspaces
				 ORDER BY updated_at DESC, created_at DESC
				"#,
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let rows = stmt
            .query_map([], Self::workspace_from_row)
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;

        let mut workspaces = Vec::new();
        for row in rows {
            workspaces
                .push(row.map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?);
        }

        Ok(workspaces)
    }

    async fn delete_workspace(&self, id: &WorkspaceId) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        conn.execute(
            "DELETE FROM dcc_workspaces WHERE id = ?1",
            params![id.0.clone()],
        )
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Ok(())
    }
}

#[async_trait]
impl RepositoryRepo for SqliteWorkspaceRepo {
    async fn save_repository(&self, repository: &Repository) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;

        conn.execute(
            r#"
			INSERT INTO dcc_repositories (
				id, project_id, name, root_path, base_branch, remote, remote_url, forge_provider, forge_login, created_at, updated_at
			) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
			ON CONFLICT(root_path) DO UPDATE SET
				id = excluded.id,
				project_id = excluded.project_id,
				name = excluded.name,
				base_branch = excluded.base_branch,
				remote = excluded.remote,
				remote_url = excluded.remote_url,
				forge_provider = excluded.forge_provider,
				forge_login = excluded.forge_login,
				created_at = excluded.created_at,
				updated_at = excluded.updated_at
			"#,
            params![
                repository.id.0.clone(),
                repository.project_id.0.clone(),
                repository.name.clone(),
                repository.root_path.clone(),
                repository.base_branch.clone(),
                repository.remote.clone(),
                repository.remote_url.clone(),
                repository.forge_provider.clone(),
                repository.forge_login.clone(),
                repository.created_at.clone(),
                repository.updated_at.clone(),
            ],
        )
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;

        Ok(())
    }

    async fn get_repository(&self, id: &RepositoryId) -> Result<Option<Repository>> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let repository = conn
            .query_row(
                r#"
				SELECT id, project_id, name, root_path, base_branch, remote, remote_url, forge_provider, forge_login, created_at, updated_at
				  FROM dcc_repositories
				 WHERE id = ?1
				"#,
                params![id.0.clone()],
                Self::repository_from_row,
            )
            .optional()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;

        Ok(repository)
    }

    async fn list_repositories(&self) -> Result<Vec<Repository>> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let mut stmt = conn
            .prepare(
                r#"
				SELECT id, project_id, name, root_path, base_branch, remote, remote_url, forge_provider, forge_login, created_at, updated_at
				  FROM dcc_repositories
				 ORDER BY updated_at DESC, created_at DESC, name ASC
				"#,
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let rows = stmt
            .query_map([], Self::repository_from_row)
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;

        let mut repositories = Vec::new();
        for row in rows {
            repositories
                .push(row.map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?);
        }

        Ok(repositories)
    }

    async fn delete_repository(&self, id: &RepositoryId) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        conn.execute(
            "DELETE FROM dcc_workspaces WHERE root_path = ?1",
            params![id.0.clone()],
        )
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        conn.execute(
            "DELETE FROM dcc_repositories WHERE id = ?1",
            params![id.0.clone()],
        )
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Ok(())
    }
}

#[async_trait]
impl SessionRepo for SqliteSessionRepo {
    async fn save_session(&self, session: &Session) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let provider_runtime_json = session
            .provider_runtime
            .as_ref()
            .map(|runtime| {
                to_string(runtime)
                    .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))
            })
            .transpose()?;

        conn.execute(
            r#"
			INSERT INTO dcc_sessions (
				id, project_id, workspace_id, provider_id, model,
				provider_runtime_json, state, created_at, updated_at
			) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
			ON CONFLICT(id) DO UPDATE SET
				project_id = excluded.project_id,
				workspace_id = excluded.workspace_id,
				provider_id = excluded.provider_id,
				model = excluded.model,
				provider_runtime_json = excluded.provider_runtime_json,
				state = excluded.state,
				created_at = excluded.created_at,
				updated_at = excluded.updated_at
			"#,
            params![
                session.id.0.clone(),
                session.project_id.0.clone(),
                session.workspace_id.0.clone(),
                session.provider_id.clone(),
                session.model.clone(),
                provider_runtime_json,
                Self::session_state_as_str(&session.state),
                session.created_at.clone(),
                session.updated_at.clone(),
            ],
        )
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;

        Ok(())
    }

    async fn get_session(&self, id: &SessionId) -> Result<Option<Session>> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        conn.query_row(
            r#"
			SELECT id, project_id, workspace_id, provider_id, model,
			       provider_runtime_json, state, created_at, updated_at
			  FROM dcc_sessions
			 WHERE id = ?1
			"#,
            params![id.0.clone()],
            Self::session_from_row,
        )
        .optional()
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))
    }

    async fn delete_session(&self, id: &SessionId) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        conn.execute(
            "DELETE FROM dcc_sessions WHERE id = ?1",
            params![id.0.clone()],
        )
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Ok(())
    }
}

#[async_trait]
impl ThreadRepo for SqliteSessionRepo {
    async fn save_thread(&self, thread: &Thread) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        conn.execute(
            r#"
			INSERT INTO dcc_threads (id, project_id, session_id, title, archived_at)
			VALUES (?1, ?2, ?3, ?4, ?5)
			ON CONFLICT(id) DO UPDATE SET
				project_id = excluded.project_id,
				session_id = excluded.session_id,
				title = excluded.title,
				archived_at = excluded.archived_at
			"#,
            params![
                thread.id.0.clone(),
                thread.project_id.0.clone(),
                thread
                    .session_id
                    .as_ref()
                    .map(|session_id| session_id.0.clone()),
                thread.title.clone(),
                thread.archived_at.clone(),
            ],
        )
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Ok(())
    }

    async fn get_thread(&self, id: &ThreadId) -> Result<Option<Thread>> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        conn.query_row(
            r#"
			SELECT id, project_id, session_id, title, archived_at
			  FROM dcc_threads
			 WHERE id = ?1
			"#,
            params![id.0.clone()],
            Self::thread_from_row,
        )
        .optional()
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))
    }

    async fn find_thread_by_session_id(&self, session_id: &SessionId) -> Result<Option<Thread>> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        conn.query_row(
            r#"
			SELECT id, project_id, session_id, title, archived_at
			  FROM dcc_threads
			 WHERE session_id = ?1
			"#,
            params![session_id.0.clone()],
            Self::thread_from_row,
        )
        .optional()
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))
    }

    async fn delete_thread(&self, id: &ThreadId) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        conn.execute(
            "DELETE FROM dcc_threads WHERE id = ?1",
            params![id.0.clone()],
        )
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Ok(())
    }
}

#[async_trait]
impl SessionEventRepo for SqliteSessionRepo {
    async fn append_event(&self, event: &SessionEventRecord) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let kind_json = to_string(&event.kind)
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        conn.execute(
            r#"
			INSERT INTO dcc_session_events (
				event_id, session_id, sequence, occurred_at, kind_json
			) VALUES (?1, ?2, ?3, ?4, ?5)
			"#,
            params![
                event.event_id.clone(),
                event.session_id.0.clone(),
                event.sequence,
                event.occurred_at.clone(),
                kind_json,
            ],
        )
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Ok(())
    }

    async fn list_events_by_session(
        &self,
        session_id: &SessionId,
    ) -> Result<Vec<SessionEventRecord>> {
        self.list_events_by_session_sync(session_id)
    }

    async fn delete_events_by_session(&self, session_id: &SessionId) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        conn.execute(
            "DELETE FROM dcc_session_events WHERE session_id = ?1",
            params![session_id.0.clone()],
        )
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dcc_core::{
        domain::{
            repository::{Repository, RepositoryId},
            session::{SessionEventKind, SessionState},
            workspace::{
                WorkspaceId, WorkspaceSetupReport, WorkspaceSetupStatus, WorkspaceSetupStepReport,
            },
        },
        ports::{RepositoryRepo, SessionEventRepo, SessionRepo, ThreadRepo, WorkspaceRepo},
    };

    fn in_memory_conn() -> Arc<Mutex<Connection>> {
        Arc::new(Mutex::new(
            Connection::open_in_memory().expect("open in-memory sqlite"),
        ))
    }

    #[test]
    fn sqlite_session_repo_persists_session_thread_and_events() {
        let repo = SqliteSessionRepo::from_connection(in_memory_conn()).expect("create repo");
        let session = Session {
            id: SessionId("session-1".to_string()),
            project_id: ProjectId("project-1".to_string()),
            workspace_id: WorkspaceId("workspace-1".to_string()),
            provider_id: "codex".to_string(),
            model: Some("gpt-5".to_string()),
            provider_runtime: None,
            state: SessionState::Active,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };
        let thread = Thread {
            id: ThreadId("thread-1".to_string()),
            project_id: ProjectId("project-1".to_string()),
            session_id: Some(SessionId("session-1".to_string())),
            title: "Thread".to_string(),
            archived_at: None,
        };
        let event = SessionEventRecord {
            event_id: "event-1".to_string(),
            session_id: SessionId("session-1".to_string()),
            sequence: 1,
            occurred_at: "2026-01-01T00:00:00Z".to_string(),
            kind: SessionEventKind::SessionStarted {
                workspace_id: WorkspaceId("workspace-1".to_string()),
                project_id: ProjectId("project-1".to_string()),
                provider_id: "codex".to_string(),
                model: Some("gpt-5".to_string()),
            },
        };

        futures::executor::block_on(repo.save_session(&session)).expect("save session");
        futures::executor::block_on(repo.save_thread(&thread)).expect("save thread");
        futures::executor::block_on(repo.append_event(&event)).expect("append event");

        let summary = repo
            .list_workspace_sessions(&WorkspaceId("workspace-1".to_string()))
            .expect("list summaries");
        assert_eq!(summary.len(), 1);
        assert_eq!(summary[0].session.id.0, "session-1");
        assert_eq!(summary[0].thread.id.0, "thread-1");
        assert_eq!(summary[0].projection.state, SessionState::Active);
    }

    #[test]
    fn sqlite_workspace_repo_persists_repositories_and_deletes_linked_workspaces() {
        let repo = SqliteWorkspaceRepo::from_connection(in_memory_conn()).expect("create repo");
        let repository = Repository {
            id: RepositoryId("/tmp/repo".to_string()),
            project_id: ProjectId("project-1".to_string()),
            name: "repo".to_string(),
            root_path: "/tmp/repo".to_string(),
            base_branch: "main".to_string(),
            remote: Some("origin".to_string()),
            remote_url: Some("git@github.com:acme/repo.git".to_string()),
            forge_provider: Some("github".to_string()),
            forge_login: Some("octocat".to_string()),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };
        let workspace = Workspace {
            id: WorkspaceId("workspace-1".to_string()),
            project_id: ProjectId("project-1".to_string()),
            name: Some("Workspace".to_string()),
            root_path: "/tmp/repo".to_string(),
            base_branch: "main".to_string(),
            worktree_path: Some("/tmp/repo/.dcc-worktrees/main".to_string()),
            state: WorkspaceState::Ready,
            setup_report: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };

        futures::executor::block_on(repo.save_repository(&repository)).expect("save repository");
        futures::executor::block_on(repo.save_workspace(&workspace)).expect("save workspace");

        let repositories =
            futures::executor::block_on(repo.list_repositories()).expect("list repositories");
        assert_eq!(repositories.len(), 1);
        assert_eq!(repositories[0].root_path, "/tmp/repo");
        assert_eq!(repositories[0].remote.as_deref(), Some("origin"));
        assert_eq!(repositories[0].forge_login.as_deref(), Some("octocat"));

        futures::executor::block_on(repo.delete_repository(&RepositoryId("/tmp/repo".to_string())))
            .expect("delete repository");

        let repositories =
            futures::executor::block_on(repo.list_repositories()).expect("list repositories");
        assert!(repositories.is_empty());
        let workspaces =
            futures::executor::block_on(repo.list_workspaces()).expect("list workspaces");
        assert!(workspaces.is_empty());
    }

    #[test]
    fn sqlite_workspace_repo_lists_and_updates_forge_bindings() {
        let repo = SqliteWorkspaceRepo::from_connection(in_memory_conn()).expect("create repo");
        let repository = Repository {
            id: RepositoryId("/tmp/repo".to_string()),
            project_id: ProjectId("project-1".to_string()),
            name: "repo".to_string(),
            root_path: "/tmp/repo".to_string(),
            base_branch: "main".to_string(),
            remote: Some("origin".to_string()),
            remote_url: Some("git@github.com:acme/repo.git".to_string()),
            forge_provider: Some("github".to_string()),
            forge_login: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };

        futures::executor::block_on(repo.save_repository(&repository)).expect("save repository");

        let unbound = repo
            .list_repositories_needing_forge_binding()
            .expect("list unbound repositories");
        assert_eq!(unbound, vec![RepositoryId("/tmp/repo".to_string())]);

        repo.update_repository_forge_login(&RepositoryId("/tmp/repo".to_string()), Some("octocat"))
            .expect("update forge login");
        let bound = repo
            .list_forge_bound_repositories()
            .expect("list bound repositories");
        assert_eq!(bound.len(), 1);
        assert_eq!(bound[0].id.0, "/tmp/repo");
        assert_eq!(bound[0].login, "octocat");
    }

    #[test]
    fn sqlite_workspace_repo_roundtrips_setup_report() {
        let repo = SqliteWorkspaceRepo::from_connection(in_memory_conn()).expect("create repo");
        let workspace = Workspace {
            id: WorkspaceId("workspace-setup".to_string()),
            project_id: ProjectId("project-1".to_string()),
            name: Some("Workspace".to_string()),
            root_path: "/tmp/repo".to_string(),
            base_branch: "main".to_string(),
            worktree_path: Some("/tmp/repo/.dcc-worktrees/main".to_string()),
            state: WorkspaceState::SetupPending,
            setup_report: Some(WorkspaceSetupReport {
                status: WorkspaceSetupStatus::Warning,
                steps: vec![WorkspaceSetupStepReport {
                    label: "Install dependencies".to_string(),
                    command: "pnpm install".to_string(),
                    source_path: "/tmp/repo/package.json".to_string(),
                    status: WorkspaceSetupStatus::Warning,
                    detail: Some("pnpm: command not found".to_string()),
                }],
                message: Some("Workspace was created, but setup needs attention.".to_string()),
            }),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };

        futures::executor::block_on(repo.save_workspace(&workspace)).expect("save workspace");
        let workspaces =
            futures::executor::block_on(repo.list_workspaces()).expect("list workspaces");

        assert_eq!(workspaces.len(), 1);
        let restored = &workspaces[0];
        assert_eq!(restored.state, WorkspaceState::SetupPending);
        let setup_report = restored
            .setup_report
            .as_ref()
            .expect("workspace setup report should persist");
        assert_eq!(setup_report.status, WorkspaceSetupStatus::Warning);
        assert_eq!(setup_report.steps.len(), 1);
        assert_eq!(setup_report.steps[0].command, "pnpm install");
        assert_eq!(
            setup_report.steps[0].detail.as_deref(),
            Some("pnpm: command not found")
        );
    }
}

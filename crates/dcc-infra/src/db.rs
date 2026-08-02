use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, Mutex};
use std::{fmt, path::Path};

use async_trait::async_trait;
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde_json::{from_str, to_string};

use dcc_core::{
    domain::{
        delegation::{
            Delegation, DelegationBudget, DelegationContextPolicy, DelegationId, DelegationMode,
            DelegationStatus,
        },
        project::ProjectId,
        repository::{Repository, RepositoryId},
        session::{
            Session, SessionEventKind, SessionEventRecord, SessionId, SessionProjection,
            SessionSearchResult, SessionState, TurnId, WorkspaceSessionSummary,
        },
        thread::{Thread, ThreadId},
        workspace::{Workspace, WorkspaceId, WorkspaceSource, WorkspaceState},
        workspace_bundle::{
            WorkspaceBundle, WorkspaceBundleId, WorkspaceBundleMember, WorkspaceBundleState,
            WorkspaceBundleSummary,
        },
    },
    ports::{
        DelegationRepo, RepositoryRepo, SessionEventRepo, SessionRepo, ThreadRepo,
        WorkspaceBundleRepo, WorkspaceRepo,
    },
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
	source_json TEXT NULL,
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
	display_name TEXT NULL,
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

const WORKSPACE_BUNDLE_TABLE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS dcc_workspace_bundles (
	id TEXT PRIMARY KEY NOT NULL,
	name TEXT NOT NULL,
	primary_workspace_id TEXT NOT NULL,
	state TEXT NOT NULL,
	created_at TEXT NOT NULL,
	updated_at TEXT NOT NULL,
	FOREIGN KEY (primary_workspace_id) REFERENCES dcc_workspaces(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_dcc_workspace_bundles_updated_at
ON dcc_workspace_bundles(updated_at DESC);

CREATE TABLE IF NOT EXISTS dcc_workspace_bundle_members (
	bundle_id TEXT NOT NULL,
	workspace_id TEXT NOT NULL UNIQUE,
	created_for_bundle INTEGER NOT NULL DEFAULT 1,
	position INTEGER NOT NULL DEFAULT 0,
	-- Compatibility columns retained for databases created by the previous prototype.
	role TEXT NOT NULL DEFAULT 'contributor',
	allow_analysis INTEGER NOT NULL DEFAULT 1,
	allow_implementation INTEGER NOT NULL DEFAULT 1,
	workspace_state_before_archive TEXT NULL,
	PRIMARY KEY (bundle_id, workspace_id),
	FOREIGN KEY (bundle_id) REFERENCES dcc_workspace_bundles(id) ON DELETE CASCADE,
	FOREIGN KEY (workspace_id) REFERENCES dcc_workspaces(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_dcc_workspace_bundle_members_bundle_position
ON dcc_workspace_bundle_members(bundle_id, position);
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
	working_directory_override TEXT NULL,
	additional_workspace_ids_json TEXT NOT NULL DEFAULT '[]',
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

CREATE VIRTUAL TABLE IF NOT EXISTS dcc_session_search USING fts5(
	session_id UNINDEXED,
	workspace_id UNINDEXED,
	project_id UNINDEXED,
	thread_title,
	search_text,
	provider_id UNINDEXED,
	model UNINDEXED,
	archived_at UNINDEXED,
	created_at UNINDEXED,
	updated_at UNINDEXED,
	tokenize = 'unicode61'
);
"#;

const DELEGATION_TABLE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS dcc_delegations (
	id TEXT PRIMARY KEY NOT NULL,
	parent_session_id TEXT NOT NULL,
	parent_turn_id TEXT NULL,
	child_session_id TEXT NULL,
	workspace_id TEXT NOT NULL,
	target_provider_id TEXT NOT NULL,
	mode TEXT NOT NULL,
	status TEXT NOT NULL,
	prompt TEXT NOT NULL,
	context_policy_json TEXT NOT NULL,
	budget_json TEXT NOT NULL,
	result_summary TEXT NULL,
	touched_files_json TEXT NOT NULL DEFAULT '[]',
	diff_summary TEXT NULL,
	validation_summary TEXT NULL,
	created_at TEXT NOT NULL,
	updated_at TEXT NOT NULL,
	FOREIGN KEY (parent_session_id) REFERENCES dcc_sessions(id) ON DELETE CASCADE,
	FOREIGN KEY (child_session_id) REFERENCES dcc_sessions(id) ON DELETE SET NULL,
	FOREIGN KEY (workspace_id) REFERENCES dcc_workspaces(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_dcc_delegations_workspace_id
	ON dcc_delegations(workspace_id);

CREATE INDEX IF NOT EXISTS idx_dcc_delegations_parent_session_id
	ON dcc_delegations(parent_session_id);

CREATE INDEX IF NOT EXISTS idx_dcc_delegations_child_session_id
	ON dcc_delegations(child_session_id);

CREATE INDEX IF NOT EXISTS idx_dcc_delegations_status
	ON dcc_delegations(status);
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
            "PRAGMA foreign_keys = ON;\n{WORKSPACE_TABLE_SQL}\n{REPOSITORY_TABLE_SQL}\n{WORKSPACE_BUNDLE_TABLE_SQL}\n{FORGE_LOGIN_PREFERENCE_TABLE_SQL}"
        ))
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Self::ensure_column(&conn, "dcc_workspaces", "setup_report_json", "TEXT NULL")?;
        Self::ensure_column(&conn, "dcc_workspaces", "source_json", "TEXT NULL")?;
        Self::ensure_column(
            &conn,
            "dcc_workspace_bundle_members",
            "workspace_state_before_archive",
            "TEXT NULL",
        )?;
        Self::ensure_column(
            &conn,
            "dcc_workspace_bundle_members",
            "role",
            "TEXT NOT NULL DEFAULT 'contributor'",
        )?;
        Self::ensure_column(
            &conn,
            "dcc_workspace_bundle_members",
            "allow_analysis",
            "INTEGER NOT NULL DEFAULT 1",
        )?;
        Self::ensure_column(
            &conn,
            "dcc_workspace_bundle_members",
            "allow_implementation",
            "INTEGER NOT NULL DEFAULT 1",
        )?;
        Self::ensure_column(&conn, "dcc_repositories", "remote", "TEXT NULL")?;
        Self::ensure_column(&conn, "dcc_repositories", "remote_url", "TEXT NULL")?;
        Self::ensure_column(&conn, "dcc_repositories", "forge_provider", "TEXT NULL")?;
        Self::ensure_column(&conn, "dcc_repositories", "forge_login", "TEXT NULL")?;
        Self::ensure_column(&conn, "dcc_repositories", "display_name", "TEXT NULL")?;
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

    pub fn update_repository_display_name(
        &self,
        repository_id: &RepositoryId,
        display_name: Option<&str>,
    ) -> Result<bool> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let changed = conn
            .execute(
                "UPDATE dcc_repositories SET display_name = ?1, updated_at = datetime('now') WHERE id = ?2",
                params![
                    display_name.map(str::trim).filter(|value| !value.is_empty()),
                    repository_id.0.clone()
                ],
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Ok(changed > 0)
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
            WorkspaceState::Completed => "completed",
        }
    }

    fn workspace_bundle_state_as_str(state: &WorkspaceBundleState) -> &'static str {
        match state {
            WorkspaceBundleState::Ready => "ready",
            WorkspaceBundleState::Archived => "archived",
            WorkspaceBundleState::Completed => "completed",
        }
    }

    fn workspace_bundle_state_from_str(
        value: &str,
        column: usize,
    ) -> rusqlite::Result<WorkspaceBundleState> {
        match value {
            "ready" => Ok(WorkspaceBundleState::Ready),
            "archived" => Ok(WorkspaceBundleState::Archived),
            "completed" => Ok(WorkspaceBundleState::Completed),
            other => Err(rusqlite::Error::FromSqlConversionFailure(
                column,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("unknown workspace bundle state: {other}"),
                )),
            )),
        }
    }

    fn workspace_bundle_from_row(row: &Row<'_>) -> rusqlite::Result<WorkspaceBundle> {
        Ok(WorkspaceBundle {
            id: WorkspaceBundleId(row.get::<_, String>(0)?),
            name: row.get::<_, String>(1)?,
            primary_workspace_id: WorkspaceId(row.get::<_, String>(2)?),
            state: Self::workspace_bundle_state_from_str(&row.get::<_, String>(3)?, 3)?,
            created_at: row.get::<_, String>(4)?,
            updated_at: row.get::<_, String>(5)?,
        })
    }

    fn workspace_bundle_members(
        conn: &Connection,
        bundle_id: &WorkspaceBundleId,
    ) -> Result<Vec<WorkspaceBundleMember>> {
        let mut stmt = conn
            .prepare(
                r#"
				SELECT bundle_id, workspace_id, created_for_bundle, position
				  FROM dcc_workspace_bundle_members
				 WHERE bundle_id = ?1
				 ORDER BY position ASC, workspace_id ASC
				"#,
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let rows = stmt
            .query_map(params![bundle_id.0.clone()], |row| {
                Ok(WorkspaceBundleMember {
                    bundle_id: WorkspaceBundleId(row.get::<_, String>(0)?),
                    workspace_id: WorkspaceId(row.get::<_, String>(1)?),
                    created_for_bundle: row.get::<_, bool>(2)?,
                    position: row.get::<_, u32>(3)?,
                })
            })
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;

        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))
    }

    fn workspace_from_row(row: &Row<'_>) -> rusqlite::Result<Workspace> {
        let source_json = row.get::<_, Option<String>>(6)?;
        let source = source_json
            .as_deref()
            .map(|json| {
                from_str::<WorkspaceSource>(json).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        6,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })
            })
            .transpose()?;
        let state = row.get::<_, String>(7)?;
        let state = match state.as_str() {
            "initializing" => WorkspaceState::Initializing,
            "setup_pending" => WorkspaceState::SetupPending,
            "ready" => WorkspaceState::Ready,
            "archived" => WorkspaceState::Archived,
            "completed" => WorkspaceState::Completed,
            other => {
                return Err(rusqlite::Error::FromSqlConversionFailure(
                    7,
                    rusqlite::types::Type::Text,
                    Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("unknown workspace state: {other}"),
                    )),
                ))
            }
        };

        let setup_report_json = row.get::<_, Option<String>>(8)?;
        let setup_report = setup_report_json
            .as_deref()
            .map(|json| {
                from_str(json).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        8,
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
            source,
            state,
            setup_report,
            created_at: row.get::<_, String>(9)?,
            updated_at: row.get::<_, String>(10)?,
        })
    }

    fn repository_from_row(row: &Row<'_>) -> rusqlite::Result<Repository> {
        Ok(Repository {
            id: RepositoryId(row.get::<_, String>(0)?),
            project_id: ProjectId(row.get::<_, String>(1)?),
            name: row.get::<_, String>(2)?,
            display_name: row.get::<_, Option<String>>(3)?,
            root_path: row.get::<_, String>(4)?,
            base_branch: row.get::<_, String>(5)?,
            remote: row.get::<_, Option<String>>(6)?,
            remote_url: row.get::<_, Option<String>>(7)?,
            forge_provider: row.get::<_, Option<String>>(8)?,
            forge_login: row.get::<_, Option<String>>(9)?,
            created_at: row.get::<_, String>(10)?,
            updated_at: row.get::<_, String>(11)?,
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
        conn.execute_batch(&format!(
            "PRAGMA foreign_keys = ON;\n{WORKSPACE_TABLE_SQL}\n{SESSION_TABLE_SQL}\n{DELEGATION_TABLE_SQL}"
        ))
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        SqliteWorkspaceRepo::ensure_column(
            &conn,
            "dcc_delegations",
            "result_summary",
            "TEXT NULL",
        )?;
        SqliteWorkspaceRepo::ensure_column(
            &conn,
            "dcc_delegations",
            "touched_files_json",
            "TEXT NOT NULL DEFAULT '[]'",
        )?;
        SqliteWorkspaceRepo::ensure_column(&conn, "dcc_delegations", "diff_summary", "TEXT NULL")?;
        SqliteWorkspaceRepo::ensure_column(
            &conn,
            "dcc_delegations",
            "validation_summary",
            "TEXT NULL",
        )?;
        SqliteWorkspaceRepo::ensure_column(
            &conn,
            "dcc_sessions",
            "working_directory_override",
            "TEXT NULL",
        )?;
        SqliteWorkspaceRepo::ensure_column(
            &conn,
            "dcc_sessions",
            "additional_workspace_ids_json",
            "TEXT NOT NULL DEFAULT '[]'",
        )?;
        // Compatibility cleanup for bundles removed before their sessions were
        // deleted explicitly. Keep orphaned single-workspace history unchanged;
        // only multi-root sessions can be identified safely here.
        conn.execute(
            r#"
            DELETE FROM dcc_sessions
             WHERE TRIM(additional_workspace_ids_json) NOT IN ('', '[]', 'null')
               AND NOT EXISTS (
                   SELECT 1
                     FROM dcc_workspaces w
                    WHERE w.id = dcc_sessions.workspace_id
               )
            "#,
            [],
        )
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Self::rebuild_search_index_sync(&conn)?;
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

    fn delegation_mode_as_str(mode: &DelegationMode) -> &'static str {
        match mode {
            DelegationMode::Review => "review",
            DelegationMode::Implement => "implement",
            DelegationMode::Explain => "explain",
            DelegationMode::Test => "test",
            DelegationMode::Research => "research",
        }
    }

    fn delegation_mode_from_str(mode: &str, column: usize) -> rusqlite::Result<DelegationMode> {
        match mode {
            "review" => Ok(DelegationMode::Review),
            "implement" => Ok(DelegationMode::Implement),
            "explain" => Ok(DelegationMode::Explain),
            "test" => Ok(DelegationMode::Test),
            "research" => Ok(DelegationMode::Research),
            other => Err(rusqlite::Error::FromSqlConversionFailure(
                column,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("unknown delegation mode: {other}"),
                )),
            )),
        }
    }

    fn delegation_status_as_str(status: &DelegationStatus) -> &'static str {
        match status {
            DelegationStatus::Draft => "draft",
            DelegationStatus::Queued => "queued",
            DelegationStatus::Running => "running",
            DelegationStatus::ReviewPending => "review_pending",
            DelegationStatus::Completed => "completed",
            DelegationStatus::Failed => "failed",
            DelegationStatus::Cancelled => "cancelled",
        }
    }

    fn delegation_status_from_str(
        status: &str,
        column: usize,
    ) -> rusqlite::Result<DelegationStatus> {
        match status {
            "draft" => Ok(DelegationStatus::Draft),
            "queued" => Ok(DelegationStatus::Queued),
            "running" => Ok(DelegationStatus::Running),
            "review_pending" => Ok(DelegationStatus::ReviewPending),
            "completed" => Ok(DelegationStatus::Completed),
            "failed" => Ok(DelegationStatus::Failed),
            "cancelled" => Ok(DelegationStatus::Cancelled),
            other => Err(rusqlite::Error::FromSqlConversionFailure(
                column,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("unknown delegation status: {other}"),
                )),
            )),
        }
    }

    fn delegation_from_row(row: &Row<'_>) -> rusqlite::Result<Delegation> {
        let context_policy_json = row.get::<_, String>(9)?;
        let context_policy =
            from_str::<DelegationContextPolicy>(&context_policy_json).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    9,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?;
        let budget_json = row.get::<_, String>(10)?;
        let budget = from_str::<DelegationBudget>(&budget_json).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                10,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;
        let touched_files_json = row.get::<_, String>(12)?;
        let touched_files = from_str::<Vec<String>>(&touched_files_json).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                12,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;

        Ok(Delegation {
            id: DelegationId(row.get::<_, String>(0)?),
            parent_session_id: SessionId(row.get::<_, String>(1)?),
            parent_turn_id: row.get::<_, Option<String>>(2)?.map(TurnId),
            child_session_id: row.get::<_, Option<String>>(3)?.map(SessionId),
            workspace_id: WorkspaceId(row.get::<_, String>(4)?),
            target_provider_id: dcc_core::domain::provider::ProviderId(row.get::<_, String>(5)?),
            mode: Self::delegation_mode_from_str(&row.get::<_, String>(6)?, 6)?,
            status: Self::delegation_status_from_str(&row.get::<_, String>(7)?, 7)?,
            prompt: row.get::<_, String>(8)?,
            context_policy,
            budget,
            result_summary: row.get::<_, Option<String>>(11)?,
            touched_files,
            diff_summary: row.get::<_, Option<String>>(13)?,
            validation_summary: row.get::<_, Option<String>>(14)?,
            created_at: row.get::<_, String>(15)?,
            updated_at: row.get::<_, String>(16)?,
        })
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
        let additional_workspace_ids = from_str(&row.get::<_, String>(7)?).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                7,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;

        Ok(Session {
            id: SessionId(row.get::<_, String>(0)?),
            project_id: ProjectId(row.get::<_, String>(1)?),
            workspace_id: WorkspaceId(row.get::<_, String>(2)?),
            provider_id: row.get::<_, String>(3)?,
            model: row.get::<_, Option<String>>(4)?,
            provider_runtime,
            working_directory_override: row.get::<_, Option<String>>(6)?,
            additional_workspace_ids,
            state: Self::session_state_from_str(&row.get::<_, String>(8)?, 8)?,
            created_at: row.get::<_, String>(9)?,
            updated_at: row.get::<_, String>(10)?,
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

    fn delete_search_row_sync(conn: &Connection, session_id: &SessionId) -> Result<()> {
        conn.execute(
            "DELETE FROM dcc_session_search WHERE session_id = ?1",
            params![session_id.0.clone()],
        )
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Ok(())
    }

    fn normalize_search_query(query: &str) -> Option<String> {
        let tokens = query
            .split(|character: char| !character.is_alphanumeric())
            .map(str::trim)
            .filter(|token| !token.is_empty())
            .take(8)
            .map(|token| format!("{token}*"))
            .collect::<Vec<_>>();
        if tokens.is_empty() {
            return None;
        }
        Some(tokens.join(" "))
    }

    fn build_search_text(events: &[SessionEventRecord]) -> String {
        #[derive(Default)]
        struct ToolCallBuffer {
            label: String,
            content: String,
            failure_reason: Option<String>,
        }

        let mut fragments = Vec::new();
        let mut assistant_by_turn = HashMap::<String, String>::new();
        let mut reasoning_by_turn = HashMap::<String, BTreeMap<String, String>>::new();
        let mut tool_calls_by_turn = HashMap::<String, BTreeMap<String, ToolCallBuffer>>::new();

        fn push_fragment(fragments: &mut Vec<String>, value: String) {
            let normalized = value.trim();
            if !normalized.is_empty() {
                fragments.push(normalized.to_string());
            }
        }

        fn flush_turn(
            fragments: &mut Vec<String>,
            turn_id: &str,
            assistant_by_turn: &mut HashMap<String, String>,
            reasoning_by_turn: &mut HashMap<String, BTreeMap<String, String>>,
            tool_calls_by_turn: &mut HashMap<String, BTreeMap<String, ToolCallBuffer>>,
        ) {
            if let Some(content) = assistant_by_turn.remove(turn_id) {
                push_fragment(fragments, format!("Assistant: {content}"));
            }

            if let Some(reasoning) = reasoning_by_turn.remove(turn_id) {
                for content in reasoning.into_values() {
                    push_fragment(fragments, format!("Reasoning: {content}"));
                }
            }

            if let Some(tool_calls) = tool_calls_by_turn.remove(turn_id) {
                for tool_call in tool_calls.into_values() {
                    let mut text = tool_call.label;
                    if !tool_call.content.trim().is_empty() {
                        text.push(' ');
                        text.push_str(tool_call.content.trim());
                    }
                    if let Some(reason) = tool_call.failure_reason {
                        if !reason.trim().is_empty() {
                            text.push(' ');
                            text.push_str(reason.trim());
                        }
                    }
                    push_fragment(fragments, text);
                }
            }
        }

        for event in events {
            match &event.kind {
                SessionEventKind::TurnStarted { prompt, .. } => {
                    push_fragment(&mut fragments, format!("User: {prompt}"));
                }
                SessionEventKind::TurnSteered { prompt, .. } => {
                    push_fragment(&mut fragments, format!("User guidance: {prompt}"));
                }
                SessionEventKind::TurnQueued { queued_turn } => {
                    push_fragment(
                        &mut fragments,
                        format!("Queued follow-up: {}", queued_turn.prompt),
                    );
                }
                SessionEventKind::TurnDelta { turn_id, content } => {
                    assistant_by_turn
                        .entry(turn_id.0.clone())
                        .or_default()
                        .push_str(content);
                }
                SessionEventKind::TurnReasoningDelta {
                    turn_id,
                    reasoning_id,
                    content,
                } => {
                    reasoning_by_turn
                        .entry(turn_id.0.clone())
                        .or_default()
                        .entry(reasoning_id.clone())
                        .or_default()
                        .push_str(content);
                }
                SessionEventKind::TurnToolCallStarted {
                    turn_id,
                    tool_call_id,
                    action,
                    command,
                    file,
                } => {
                    let mut label = format!("Tool: {action}");
                    if let Some(command) =
                        command.as_deref().filter(|value| !value.trim().is_empty())
                    {
                        label.push(' ');
                        label.push_str(command.trim());
                    }
                    if let Some(file) = file.as_deref().filter(|value| !value.trim().is_empty()) {
                        label.push(' ');
                        label.push_str(file.trim());
                    }
                    tool_calls_by_turn
                        .entry(turn_id.0.clone())
                        .or_default()
                        .entry(tool_call_id.clone())
                        .or_insert_with(|| ToolCallBuffer {
                            label,
                            content: String::new(),
                            failure_reason: None,
                        });
                }
                SessionEventKind::TurnToolCallDelta {
                    turn_id,
                    tool_call_id,
                    content,
                } => {
                    tool_calls_by_turn
                        .entry(turn_id.0.clone())
                        .or_default()
                        .entry(tool_call_id.clone())
                        .or_insert_with(|| ToolCallBuffer {
                            label: "Tool: output".to_string(),
                            content: String::new(),
                            failure_reason: None,
                        })
                        .content
                        .push_str(content);
                }
                SessionEventKind::TurnToolCallFailed {
                    turn_id,
                    tool_call_id,
                    reason,
                } => {
                    if let Some(tool_call) = tool_calls_by_turn
                        .entry(turn_id.0.clone())
                        .or_default()
                        .get_mut(tool_call_id)
                    {
                        tool_call.failure_reason = reason.clone();
                    }
                }
                SessionEventKind::TurnUserInputRequested { questions, .. } => {
                    for question in questions {
                        push_fragment(
                            &mut fragments,
                            format!("User input requested: {}", question.question),
                        );
                    }
                }
                SessionEventKind::TurnUserInputResolved { answers, .. } => {
                    for answer in answers {
                        push_fragment(
                            &mut fragments,
                            format!("User input: {} {}", answer.question, answer.answer),
                        );
                    }
                }
                SessionEventKind::TurnPermissionRequested {
                    tool_name,
                    title,
                    description,
                    command,
                    file,
                    ..
                } => {
                    let mut text = format!("Permission request: {tool_name}");
                    for value in [
                        title.as_deref(),
                        description.as_deref(),
                        command.as_deref(),
                        file.as_deref(),
                    ] {
                        if let Some(value) = value.filter(|candidate| !candidate.trim().is_empty())
                        {
                            text.push(' ');
                            text.push_str(value.trim());
                        }
                    }
                    push_fragment(&mut fragments, text);
                }
                SessionEventKind::TurnPermissionResolved { behavior, .. } => {
                    push_fragment(&mut fragments, format!("Permission resolved: {behavior}"));
                }
                SessionEventKind::TurnCompleted { turn_id }
                | SessionEventKind::TurnAborted { turn_id, .. } => {
                    flush_turn(
                        &mut fragments,
                        &turn_id.0,
                        &mut assistant_by_turn,
                        &mut reasoning_by_turn,
                        &mut tool_calls_by_turn,
                    );
                }
                SessionEventKind::SessionAborted { reason } => {
                    if let Some(reason) = reason.as_deref().filter(|value| !value.trim().is_empty())
                    {
                        push_fragment(&mut fragments, format!("Session aborted: {reason}"));
                    }
                }
                SessionEventKind::CheckpointCreated { label, .. } => {
                    push_fragment(&mut fragments, format!("Checkpoint: {label}"));
                }
                SessionEventKind::SessionStarted { .. }
                | SessionEventKind::QueuedTurnRemoved { .. }
                | SessionEventKind::TurnQueueReordered { .. }
                | SessionEventKind::QueuedTurnDispatched { .. }
                | SessionEventKind::TurnReasoningStarted { .. }
                | SessionEventKind::TurnReasoningCompleted { .. }
                | SessionEventKind::TurnToolCallCompleted { .. }
                | SessionEventKind::SessionCompleted
                | SessionEventKind::DelegationRequested { .. }
                | SessionEventKind::DelegationStarted { .. }
                | SessionEventKind::DelegationDelta { .. }
                | SessionEventKind::DelegationCompleted { .. }
                | SessionEventKind::DelegationFailed { .. }
                | SessionEventKind::DelegationCancelled { .. }
                | SessionEventKind::PlanApproved { .. }
                | SessionEventKind::PlanHandedOff { .. }
                | SessionEventKind::SessionResumed => {}
            }
        }

        let pending_turns = assistant_by_turn
            .keys()
            .chain(reasoning_by_turn.keys())
            .chain(tool_calls_by_turn.keys())
            .cloned()
            .collect::<BTreeSet<_>>();
        for turn_id in pending_turns {
            flush_turn(
                &mut fragments,
                &turn_id,
                &mut assistant_by_turn,
                &mut reasoning_by_turn,
                &mut tool_calls_by_turn,
            );
        }

        fragments.join("\n\n")
    }

    fn rebuild_search_index_sync(conn: &Connection) -> Result<()> {
        let mut stmt = conn
            .prepare("SELECT id FROM dcc_sessions ORDER BY created_at ASC")
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let session_ids = rows
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        drop(stmt);

        conn.execute("DELETE FROM dcc_session_search", [])
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        for session_id in session_ids {
            Self::reindex_session_sync(conn, &SessionId(session_id))?;
        }
        Ok(())
    }

    fn reindex_session_sync(conn: &Connection, session_id: &SessionId) -> Result<()> {
        let session_and_thread = conn
            .query_row(
                r#"
                SELECT
                    s.id,
                    s.project_id,
                    s.workspace_id,
                    s.provider_id,
                    s.model,
                    s.created_at,
                    s.updated_at,
                    t.title,
                    t.archived_at
                  FROM dcc_sessions s
                  JOIN dcc_threads t ON t.session_id = s.id
                 WHERE s.id = ?1
                "#,
                params![session_id.0.clone()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, String>(7)?,
                        row.get::<_, Option<String>>(8)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;

        let Some((
            session_id_value,
            project_id,
            workspace_id,
            provider_id,
            model,
            created_at,
            updated_at,
            thread_title,
            archived_at,
        )) = session_and_thread
        else {
            return Self::delete_search_row_sync(conn, session_id);
        };

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
            .query_map(params![session_id_value.clone()], Self::event_from_row)
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let events = rows
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        drop(stmt);

        let search_text = Self::build_search_text(&events);
        Self::delete_search_row_sync(conn, session_id)?;
        conn.execute(
            r#"
            INSERT INTO dcc_session_search (
                session_id,
                workspace_id,
                project_id,
                thread_title,
                search_text,
                provider_id,
                model,
                archived_at,
                created_at,
                updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            "#,
            params![
                session_id_value,
                workspace_id,
                project_id,
                thread_title,
                search_text,
                provider_id,
                model,
                archived_at,
                created_at,
                updated_at,
            ],
        )
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Ok(())
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
        let mut last_turn_id = None;
        let mut last_turn_started_at = None;
        let mut last_turn_completed_at = None;
        for event in &events {
            match &event.kind {
                SessionEventKind::TurnStarted {
                    turn_id, prompt, ..
                } => {
                    last_turn_id = Some(turn_id.clone());
                    last_turn_prompt = Some(prompt.clone());
                    last_turn_state = Some("running".to_string());
                    last_turn_started_at = Some(event.occurred_at.clone());
                    last_turn_completed_at = None;
                }
                SessionEventKind::TurnCompleted { turn_id }
                    if last_turn_id.as_ref() == Some(turn_id) =>
                {
                    last_turn_state = Some("completed".to_string());
                    last_turn_completed_at = Some(event.occurred_at.clone());
                }
                SessionEventKind::TurnAborted { turn_id, .. }
                    if last_turn_id.as_ref() == Some(turn_id) =>
                {
                    last_turn_state = Some("aborted".to_string());
                    last_turn_completed_at = Some(event.occurred_at.clone());
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
            last_turn_started_at,
            last_turn_completed_at,
        })
    }

    fn session_search_result_from_row(row: &Row<'_>) -> rusqlite::Result<SessionSearchResult> {
        Ok(SessionSearchResult {
            session_id: SessionId(row.get::<_, String>(0)?),
            workspace_id: WorkspaceId(row.get::<_, String>(1)?),
            project_id: ProjectId(row.get::<_, String>(2)?),
            thread_title: row.get::<_, String>(3)?,
            workspace_name: row.get::<_, Option<String>>(4)?,
            workspace_branch: row.get::<_, Option<String>>(5)?,
            workspace_root_path: row.get::<_, Option<String>>(6)?,
            provider_id: row.get::<_, String>(7)?,
            model: row.get::<_, Option<String>>(8)?,
            archived_at: row.get::<_, Option<String>>(9)?,
            created_at: row.get::<_, String>(10)?,
            updated_at: row.get::<_, String>(11)?,
            snippet: row.get::<_, String>(12)?,
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
					s.provider_runtime_json, s.working_directory_override,
					s.additional_workspace_ids_json, s.state, s.created_at, s.updated_at,
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
                    working_directory_override: row.get::<_, Option<String>>(6)?,
                    additional_workspace_ids: from_str(&row.get::<_, String>(7)?).map_err(
                        |error| {
                            rusqlite::Error::FromSqlConversionFailure(
                                7,
                                rusqlite::types::Type::Text,
                                Box::new(error),
                            )
                        },
                    )?,
                    state: Self::session_state_from_str(&row.get::<_, String>(8)?, 8)?,
                    created_at: row.get::<_, String>(9)?,
                    updated_at: row.get::<_, String>(10)?,
                };
                let thread = Thread {
                    id: ThreadId(row.get::<_, String>(11)?),
                    project_id: ProjectId(row.get::<_, String>(12)?),
                    session_id: row.get::<_, Option<String>>(13)?.map(SessionId),
                    title: row.get::<_, String>(14)?,
                    archived_at: row.get::<_, Option<String>>(15)?,
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

    pub fn search_sessions(&self, query: &str, limit: usize) -> Result<Vec<SessionSearchResult>> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let safe_limit = limit.clamp(1, 100) as i64;
        let normalized_query = Self::normalize_search_query(query);
        let sql = if normalized_query.is_some() {
            r#"
            SELECT
                dcc_session_search.session_id,
                dcc_session_search.workspace_id,
                dcc_session_search.project_id,
                dcc_session_search.thread_title,
                w.name,
                w.base_branch,
                w.root_path,
                dcc_session_search.provider_id,
                dcc_session_search.model,
                dcc_session_search.archived_at,
                dcc_session_search.created_at,
                dcc_session_search.updated_at,
                snippet(dcc_session_search, 4, '', '', ' … ', 20) AS snippet
              FROM dcc_session_search
              LEFT JOIN dcc_workspaces w ON w.id = dcc_session_search.workspace_id
             WHERE dcc_session_search MATCH ?1
             ORDER BY bm25(dcc_session_search, 12.0, 1.0), dcc_session_search.updated_at DESC
             LIMIT ?2
            "#
        } else {
            r#"
            SELECT
                dcc_session_search.session_id,
                dcc_session_search.workspace_id,
                dcc_session_search.project_id,
                dcc_session_search.thread_title,
                w.name,
                w.base_branch,
                w.root_path,
                dcc_session_search.provider_id,
                dcc_session_search.model,
                dcc_session_search.archived_at,
                dcc_session_search.created_at,
                dcc_session_search.updated_at,
                substr(dcc_session_search.search_text, 1, 240) AS snippet
              FROM dcc_session_search
              LEFT JOIN dcc_workspaces w ON w.id = dcc_session_search.workspace_id
             ORDER BY dcc_session_search.updated_at DESC
             LIMIT ?1
            "#
        };
        let mut stmt = conn
            .prepare(sql)
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let rows = if let Some(normalized_query) = normalized_query {
            stmt.query_map(
                params![normalized_query, safe_limit],
                Self::session_search_result_from_row,
            )
        } else {
            stmt.query_map(params![safe_limit], Self::session_search_result_from_row)
        }
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;

        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))
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
				source_json, state, setup_report_json, created_at, updated_at
			) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
			ON CONFLICT(id) DO UPDATE SET
				project_id = excluded.project_id,
				name = excluded.name,
				root_path = excluded.root_path,
				base_branch = excluded.base_branch,
				worktree_path = excluded.worktree_path,
				source_json = excluded.source_json,
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
                workspace
                    .source
                    .as_ref()
                    .map(to_string)
                    .transpose()
                    .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?,
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
				       source_json, state, setup_report_json, created_at, updated_at
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
				       source_json, state, setup_report_json, created_at, updated_at
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
impl WorkspaceBundleRepo for SqliteWorkspaceRepo {
    async fn save_workspace_bundle(
        &self,
        bundle: &WorkspaceBundle,
        members: &[WorkspaceBundleMember],
    ) -> Result<()> {
        if bundle.name.trim().is_empty() {
            return Err(dcc_core::CoreError::InvalidInput(
                "workspace bundle name cannot be empty".to_string(),
            ));
        }
        if members.len() < 2 {
            return Err(dcc_core::CoreError::InvalidInput(
                "workspace bundle requires at least two members".to_string(),
            ));
        }
        let mut workspace_ids = BTreeSet::new();
        for member in members {
            if member.bundle_id != bundle.id {
                return Err(dcc_core::CoreError::InvalidInput(
                    "workspace bundle member references a different bundle".to_string(),
                ));
            }
            if !workspace_ids.insert(member.workspace_id.0.clone()) {
                return Err(dcc_core::CoreError::InvalidInput(format!(
                    "workspace bundle contains duplicate member: {}",
                    member.workspace_id.0
                )));
            }
        }
        if !workspace_ids.contains(&bundle.primary_workspace_id.0) {
            return Err(dcc_core::CoreError::InvalidInput(
                "workspace bundle primary workspace must be a member".to_string(),
            ));
        }

        let mut conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let tx = conn
            .transaction()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        tx.execute(
            r#"
			INSERT INTO dcc_workspace_bundles (
				id, name, primary_workspace_id, state, created_at, updated_at
			) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
			ON CONFLICT(id) DO UPDATE SET
				name = excluded.name,
				primary_workspace_id = excluded.primary_workspace_id,
				state = excluded.state,
				created_at = excluded.created_at,
				updated_at = excluded.updated_at
			"#,
            params![
                bundle.id.0.clone(),
                bundle.name.clone(),
                bundle.primary_workspace_id.0.clone(),
                Self::workspace_bundle_state_as_str(&bundle.state),
                bundle.created_at.clone(),
                bundle.updated_at.clone(),
            ],
        )
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        tx.execute(
            "DELETE FROM dcc_workspace_bundle_members WHERE bundle_id = ?1",
            params![bundle.id.0.clone()],
        )
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        for member in members {
            tx.execute(
                r#"
				INSERT INTO dcc_workspace_bundle_members (
					bundle_id, workspace_id, created_for_bundle, position,
					role, allow_analysis, allow_implementation
				) VALUES (?1, ?2, ?3, ?4, ?5, 1, 1)
				"#,
                params![
                    member.bundle_id.0.clone(),
                    member.workspace_id.0.clone(),
                    member.created_for_bundle,
                    member.position,
                    if member.workspace_id == bundle.primary_workspace_id {
                        "primary"
                    } else {
                        "contributor"
                    },
                ],
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        }
        tx.commit()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Ok(())
    }

    async fn get_workspace_bundle(
        &self,
        id: &WorkspaceBundleId,
    ) -> Result<Option<WorkspaceBundleSummary>> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let bundle = conn
            .query_row(
                r#"
				SELECT id, name, primary_workspace_id, state, created_at, updated_at
				  FROM dcc_workspace_bundles
				 WHERE id = ?1
				"#,
                params![id.0.clone()],
                Self::workspace_bundle_from_row,
            )
            .optional()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let Some(bundle) = bundle else {
            return Ok(None);
        };
        let members = Self::workspace_bundle_members(&conn, &bundle.id)?;
        Ok(Some(WorkspaceBundleSummary { bundle, members }))
    }

    async fn get_workspace_bundle_for_workspace(
        &self,
        workspace_id: &WorkspaceId,
    ) -> Result<Option<WorkspaceBundleSummary>> {
        let bundle_id = {
            let conn = self
                .conn
                .lock()
                .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
            conn.query_row(
                "SELECT bundle_id FROM dcc_workspace_bundle_members WHERE workspace_id = ?1",
                params![workspace_id.0.clone()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?
        };
        let Some(bundle_id) = bundle_id else {
            return Ok(None);
        };
        self.get_workspace_bundle(&WorkspaceBundleId(bundle_id))
            .await
    }

    async fn list_workspace_bundles(&self) -> Result<Vec<WorkspaceBundleSummary>> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let bundles = {
            let mut stmt = conn
                .prepare(
                    r#"
					SELECT id, name, primary_workspace_id, state, created_at, updated_at
					  FROM dcc_workspace_bundles
					 ORDER BY updated_at DESC, created_at DESC
					"#,
                )
                .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
            let rows = stmt
                .query_map([], Self::workspace_bundle_from_row)
                .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
            rows.collect::<rusqlite::Result<Vec<_>>>()
                .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?
        };
        bundles
            .into_iter()
            .map(|bundle| {
                let members = Self::workspace_bundle_members(&conn, &bundle.id)?;
                Ok(WorkspaceBundleSummary { bundle, members })
            })
            .collect()
    }

    async fn set_workspace_bundle_state(
        &self,
        id: &WorkspaceBundleId,
        state: WorkspaceBundleState,
        updated_at: String,
    ) -> Result<Option<WorkspaceBundleSummary>> {
        {
            let mut conn = self
                .conn
                .lock()
                .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
            let tx = conn
                .transaction()
                .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
            let updated = tx
                .execute(
                    "UPDATE dcc_workspace_bundles SET state = ?1, updated_at = ?2 WHERE id = ?3",
                    params![
                        Self::workspace_bundle_state_as_str(&state),
                        updated_at.clone(),
                        id.0.clone()
                    ],
                )
                .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
            if updated == 0 {
                return Ok(None);
            }
            match state {
                WorkspaceBundleState::Archived => {
                    tx.execute(
                        r#"
						UPDATE dcc_workspace_bundle_members
						   SET workspace_state_before_archive = (
						       SELECT state
						         FROM dcc_workspaces
						        WHERE id = dcc_workspace_bundle_members.workspace_id
						   )
						 WHERE bundle_id = ?1
						   AND workspace_state_before_archive IS NULL
						"#,
                        params![id.0.clone()],
                    )
                    .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
                    tx.execute(
                        r#"
						UPDATE dcc_workspaces
						   SET state = 'archived', updated_at = ?1
						 WHERE id IN (
						       SELECT workspace_id
						         FROM dcc_workspace_bundle_members
						        WHERE bundle_id = ?2
						 )
						"#,
                        params![updated_at.clone(), id.0.clone()],
                    )
                    .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
                }
                WorkspaceBundleState::Completed => {
                    tx.execute(
                        r#"
						UPDATE dcc_workspaces
						   SET state = 'completed', updated_at = ?1
						 WHERE id IN (
						       SELECT workspace_id
						         FROM dcc_workspace_bundle_members
						        WHERE bundle_id = ?2
						 )
						"#,
                        params![updated_at.clone(), id.0.clone()],
                    )
                    .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
                }
                WorkspaceBundleState::Ready => {
                    tx.execute(
                        r#"
						UPDATE dcc_workspaces
						   SET state = COALESCE((
						       SELECT workspace_state_before_archive
						         FROM dcc_workspace_bundle_members
						        WHERE bundle_id = ?2
						          AND workspace_id = dcc_workspaces.id
						   ), 'ready'),
						       updated_at = ?1
						 WHERE id IN (
						       SELECT workspace_id
						         FROM dcc_workspace_bundle_members
						        WHERE bundle_id = ?2
						 )
						"#,
                        params![updated_at.clone(), id.0.clone()],
                    )
                    .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
                    tx.execute(
                        r#"
						UPDATE dcc_workspace_bundle_members
						   SET workspace_state_before_archive = NULL
						 WHERE bundle_id = ?1
						"#,
                        params![id.0.clone()],
                    )
                    .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
                }
            }
            tx.commit()
                .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        }
        self.get_workspace_bundle(id).await
    }

    async fn delete_workspace_bundle(&self, id: &WorkspaceBundleId) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        conn.execute(
            "DELETE FROM dcc_workspace_bundles WHERE id = ?1",
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
				id, project_id, name, display_name, root_path, base_branch, remote, remote_url, forge_provider, forge_login, created_at, updated_at
			) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
			ON CONFLICT(root_path) DO UPDATE SET
				id = excluded.id,
				project_id = excluded.project_id,
				name = excluded.name,
				display_name = COALESCE(excluded.display_name, dcc_repositories.display_name),
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
                repository.display_name.clone(),
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
				SELECT id, project_id, name, display_name, root_path, base_branch, remote, remote_url, forge_provider, forge_login, created_at, updated_at
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
				SELECT id, project_id, name, display_name, root_path, base_branch, remote, remote_url, forge_provider, forge_login, created_at, updated_at
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
        let additional_workspace_ids_json = to_string(&session.additional_workspace_ids)
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;

        conn.execute(
            r#"
			INSERT INTO dcc_sessions (
				id, project_id, workspace_id, provider_id, model,
				provider_runtime_json, working_directory_override, additional_workspace_ids_json,
				state, created_at, updated_at
			) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
			ON CONFLICT(id) DO UPDATE SET
				project_id = excluded.project_id,
				workspace_id = excluded.workspace_id,
				provider_id = excluded.provider_id,
				model = excluded.model,
				provider_runtime_json = excluded.provider_runtime_json,
				working_directory_override = excluded.working_directory_override,
				additional_workspace_ids_json = excluded.additional_workspace_ids_json,
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
                session.working_directory_override.clone(),
                additional_workspace_ids_json,
                Self::session_state_as_str(&session.state),
                session.created_at.clone(),
                session.updated_at.clone(),
            ],
        )
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Self::reindex_session_sync(&conn, &session.id)?;

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
			       provider_runtime_json, working_directory_override,
			       additional_workspace_ids_json, state, created_at, updated_at
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
        Self::delete_search_row_sync(&conn, id)?;
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
        if let Some(session_id) = thread.session_id.as_ref() {
            Self::reindex_session_sync(&conn, session_id)?;
        }
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
        let session_id = conn
            .query_row(
                "SELECT session_id FROM dcc_threads WHERE id = ?1",
                params![id.0.clone()],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?
            .flatten();
        conn.execute(
            "DELETE FROM dcc_threads WHERE id = ?1",
            params![id.0.clone()],
        )
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        if let Some(session_id) = session_id {
            Self::delete_search_row_sync(&conn, &SessionId(session_id))?;
        }
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
        Self::reindex_session_sync(&conn, &event.session_id)?;
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
        Self::reindex_session_sync(&conn, session_id)?;
        Ok(())
    }
}

#[async_trait]
impl DelegationRepo for SqliteSessionRepo {
    async fn save_delegation(&self, delegation: &Delegation) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let context_policy_json = to_string(&delegation.context_policy)
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let budget_json = to_string(&delegation.budget)
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let touched_files_json = to_string(&delegation.touched_files)
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;

        conn.execute(
            r#"
			INSERT INTO dcc_delegations (
				id, parent_session_id, parent_turn_id, child_session_id, workspace_id,
				target_provider_id, mode, status, prompt, context_policy_json, budget_json,
				result_summary, touched_files_json, diff_summary, validation_summary, created_at, updated_at
			) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
			ON CONFLICT(id) DO UPDATE SET
				parent_session_id = excluded.parent_session_id,
				parent_turn_id = excluded.parent_turn_id,
				child_session_id = excluded.child_session_id,
				workspace_id = excluded.workspace_id,
				target_provider_id = excluded.target_provider_id,
				mode = excluded.mode,
				status = excluded.status,
				prompt = excluded.prompt,
				context_policy_json = excluded.context_policy_json,
				budget_json = excluded.budget_json,
				result_summary = excluded.result_summary,
				touched_files_json = excluded.touched_files_json,
				diff_summary = excluded.diff_summary,
				validation_summary = excluded.validation_summary,
				created_at = excluded.created_at,
				updated_at = excluded.updated_at
			"#,
            params![
                delegation.id.0.clone(),
                delegation.parent_session_id.0.clone(),
                delegation
                    .parent_turn_id
                    .as_ref()
                    .map(|turn_id| turn_id.0.clone()),
                delegation
                    .child_session_id
                    .as_ref()
                    .map(|session_id| session_id.0.clone()),
                delegation.workspace_id.0.clone(),
                delegation.target_provider_id.0.clone(),
                Self::delegation_mode_as_str(&delegation.mode),
                Self::delegation_status_as_str(&delegation.status),
                delegation.prompt.clone(),
                context_policy_json,
                budget_json,
                delegation.result_summary.clone(),
                touched_files_json,
                delegation.diff_summary.clone(),
                delegation.validation_summary.clone(),
                delegation.created_at.clone(),
                delegation.updated_at.clone(),
            ],
        )
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Ok(())
    }

    async fn get_delegation(&self, id: &DelegationId) -> Result<Option<Delegation>> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        conn.query_row(
            r#"
			SELECT id, parent_session_id, parent_turn_id, child_session_id, workspace_id,
			       target_provider_id, mode, status, prompt, context_policy_json, budget_json,
			       result_summary, touched_files_json, diff_summary, validation_summary, created_at, updated_at
			  FROM dcc_delegations
			 WHERE id = ?1
			"#,
            params![id.0.clone()],
            Self::delegation_from_row,
        )
        .optional()
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))
    }

    async fn list_delegations(
        &self,
        workspace_id: Option<&WorkspaceId>,
        parent_session_id: Option<&SessionId>,
    ) -> Result<Vec<Delegation>> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let base_sql = r#"
			SELECT id, parent_session_id, parent_turn_id, child_session_id, workspace_id,
			       target_provider_id, mode, status, prompt, context_policy_json, budget_json,
			       result_summary, touched_files_json, diff_summary, validation_summary, created_at, updated_at
			  FROM dcc_delegations
		"#;
        let order_sql = " ORDER BY updated_at DESC, created_at DESC";
        let rows = match (workspace_id, parent_session_id) {
            (Some(workspace_id), Some(parent_session_id)) => {
                let sql = format!(
                    "{base_sql} WHERE workspace_id = ?1 AND parent_session_id = ?2{order_sql}"
                );
                let mut stmt = conn
                    .prepare(&sql)
                    .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
                let rows = stmt
                    .query_map(
                        params![workspace_id.0.clone(), parent_session_id.0.clone()],
                        Self::delegation_from_row,
                    )
                    .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?
                    .collect::<rusqlite::Result<Vec<_>>>();
                rows
            }
            (Some(workspace_id), None) => {
                let sql = format!("{base_sql} WHERE workspace_id = ?1{order_sql}");
                let mut stmt = conn
                    .prepare(&sql)
                    .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
                let rows = stmt
                    .query_map(params![workspace_id.0.clone()], Self::delegation_from_row)
                    .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?
                    .collect::<rusqlite::Result<Vec<_>>>();
                rows
            }
            (None, Some(parent_session_id)) => {
                let sql = format!("{base_sql} WHERE parent_session_id = ?1{order_sql}");
                let mut stmt = conn
                    .prepare(&sql)
                    .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
                let rows = stmt
                    .query_map(
                        params![parent_session_id.0.clone()],
                        Self::delegation_from_row,
                    )
                    .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?
                    .collect::<rusqlite::Result<Vec<_>>>();
                rows
            }
            (None, None) => {
                let sql = format!("{base_sql}{order_sql}");
                let mut stmt = conn
                    .prepare(&sql)
                    .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
                let rows = stmt
                    .query_map([], Self::delegation_from_row)
                    .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?
                    .collect::<rusqlite::Result<Vec<_>>>();
                rows
            }
        }
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        Ok(rows)
    }

    async fn update_delegation_status(
        &self,
        id: &DelegationId,
        status: DelegationStatus,
        updated_at: String,
    ) -> Result<Option<Delegation>> {
        {
            let conn = self
                .conn
                .lock()
                .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
            conn.execute(
                "UPDATE dcc_delegations SET status = ?1, updated_at = ?2 WHERE id = ?3",
                params![
                    Self::delegation_status_as_str(&status),
                    updated_at,
                    id.0.clone()
                ],
            )
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        }
        self.get_delegation(id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dcc_core::{
        domain::{
            delegation::{
                Delegation, DelegationBudget, DelegationContextPolicy, DelegationId,
                DelegationMode, DelegationStatus,
            },
            provider::ProviderId,
            repository::{Repository, RepositoryId},
            session::{SessionEventKind, SessionState, TurnId},
            workspace::{
                WorkspaceId, WorkspacePushTarget, WorkspaceSetupReport, WorkspaceSetupStatus,
                WorkspaceSetupStepReport, WorkspaceSource, WorkspaceSourceKind, WorkspaceState,
            },
            workspace_bundle::{
                WorkspaceBundle, WorkspaceBundleId, WorkspaceBundleMember, WorkspaceBundleState,
            },
        },
        ports::{
            DelegationRepo, RepositoryRepo, SessionEventRepo, SessionRepo, ThreadRepo,
            WorkspaceBundleRepo, WorkspaceRepo,
        },
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
            additional_workspace_ids: vec![WorkspaceId("workspace-2".to_string())],
            provider_id: "codex".to_string(),
            model: Some("gpt-5".to_string()),
            provider_runtime: None,
            working_directory_override: None,
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
        let events = [
            SessionEventRecord {
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
            },
            SessionEventRecord {
                event_id: "event-2".to_string(),
                session_id: SessionId("session-1".to_string()),
                sequence: 2,
                occurred_at: "2026-01-01T00:00:02Z".to_string(),
                kind: SessionEventKind::TurnStarted {
                    turn_id: TurnId("turn-1".to_string()),
                    prompt: "Implement the workspace recap".to_string(),
                    plan_mode: Some(false),
                },
            },
            SessionEventRecord {
                event_id: "event-3".to_string(),
                session_id: SessionId("session-1".to_string()),
                sequence: 3,
                occurred_at: "2026-01-01T00:00:09Z".to_string(),
                kind: SessionEventKind::TurnCompleted {
                    turn_id: TurnId("turn-1".to_string()),
                },
            },
        ];

        futures::executor::block_on(repo.save_session(&session)).expect("save session");
        futures::executor::block_on(repo.save_thread(&thread)).expect("save thread");
        for event in &events {
            futures::executor::block_on(repo.append_event(event)).expect("append event");
        }

        let summary = repo
            .list_workspace_sessions(&WorkspaceId("workspace-1".to_string()))
            .expect("list summaries");
        assert_eq!(summary.len(), 1);
        assert_eq!(summary[0].session.id.0, "session-1");
        assert_eq!(
            summary[0].session.additional_workspace_ids,
            vec![WorkspaceId("workspace-2".to_string())]
        );
        assert_eq!(summary[0].thread.id.0, "thread-1");
        assert_eq!(summary[0].projection.state, SessionState::Active);
        assert_eq!(
            summary[0].last_turn_started_at.as_deref(),
            Some("2026-01-01T00:00:02Z")
        );
        assert_eq!(
            summary[0].last_turn_completed_at.as_deref(),
            Some("2026-01-01T00:00:09Z")
        );
    }

    #[test]
    fn sqlite_session_repo_cleans_only_orphaned_multi_workspace_sessions() {
        let conn = in_memory_conn();
        let session_repo =
            SqliteSessionRepo::from_connection(conn.clone()).expect("create session repo");
        let workspace_repo =
            SqliteWorkspaceRepo::from_connection(conn.clone()).expect("create workspace repo");
        let workspace = Workspace {
            id: WorkspaceId("removed-workspace".to_string()),
            project_id: ProjectId("project-1".to_string()),
            name: Some("Removed Workspace".to_string()),
            root_path: "/tmp/removed-workspace".to_string(),
            base_branch: "main".to_string(),
            worktree_path: Some("/tmp/removed-workspace".to_string()),
            source: None,
            state: WorkspaceState::Ready,
            setup_report: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };
        futures::executor::block_on(workspace_repo.save_workspace(&workspace))
            .expect("save workspace");

        let session = |id: &str, additional_workspace_ids: Vec<WorkspaceId>| Session {
            id: SessionId(id.to_string()),
            project_id: ProjectId("project-1".to_string()),
            workspace_id: workspace.id.clone(),
            additional_workspace_ids,
            provider_id: "codex".to_string(),
            model: Some("gpt-5".to_string()),
            provider_runtime: None,
            working_directory_override: None,
            state: SessionState::Completed,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };
        let multi_session = session(
            "orphaned-multi",
            vec![WorkspaceId("secondary-workspace".to_string())],
        );
        let single_session = session("orphaned-single", Vec::new());
        futures::executor::block_on(session_repo.save_session(&multi_session))
            .expect("save multi session");
        futures::executor::block_on(session_repo.save_session(&single_session))
            .expect("save single session");
        futures::executor::block_on(workspace_repo.delete_workspace(&workspace.id))
            .expect("delete workspace");

        let reopened = SqliteSessionRepo::from_connection(conn).expect("reopen session repo");
        assert!(
            futures::executor::block_on(reopened.get_session(&multi_session.id))
                .expect("read multi session")
                .is_none()
        );
        assert!(
            futures::executor::block_on(reopened.get_session(&single_session.id))
                .expect("read single session")
                .is_some()
        );
    }

    #[test]
    fn sqlite_session_repo_supports_full_text_search_over_past_sessions() {
        let conn = in_memory_conn();
        let repo = SqliteSessionRepo::from_connection(conn.clone()).expect("create session repo");
        let workspace_repo =
            SqliteWorkspaceRepo::from_connection(conn).expect("create workspace repo");
        let workspace = Workspace {
            id: WorkspaceId("workspace-1".to_string()),
            project_id: ProjectId("project-1".to_string()),
            name: Some("Searchable Workspace".to_string()),
            root_path: "/tmp/searchable".to_string(),
            base_branch: "main".to_string(),
            worktree_path: Some("/tmp/searchable".to_string()),
            source: None,
            state: WorkspaceState::Ready,
            setup_report: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };
        let session = Session {
            id: SessionId("session-search".to_string()),
            project_id: ProjectId("project-1".to_string()),
            workspace_id: WorkspaceId("workspace-1".to_string()),
            additional_workspace_ids: Vec::new(),
            provider_id: "codex".to_string(),
            model: Some("gpt-5".to_string()),
            provider_runtime: None,
            working_directory_override: None,
            state: SessionState::Active,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };
        let thread = Thread {
            id: ThreadId("thread-search".to_string()),
            project_id: ProjectId("project-1".to_string()),
            session_id: Some(session.id.clone()),
            title: "Authentication bugfix".to_string(),
            archived_at: Some("2026-01-01T00:10:00Z".to_string()),
        };
        let events = vec![
            SessionEventRecord {
                event_id: "event-1".to_string(),
                session_id: session.id.clone(),
                sequence: 1,
                occurred_at: "2026-01-01T00:00:00Z".to_string(),
                kind: SessionEventKind::SessionStarted {
                    workspace_id: WorkspaceId("workspace-1".to_string()),
                    project_id: ProjectId("project-1".to_string()),
                    provider_id: "codex".to_string(),
                    model: Some("gpt-5".to_string()),
                },
            },
            SessionEventRecord {
                event_id: "event-2".to_string(),
                session_id: session.id.clone(),
                sequence: 2,
                occurred_at: "2026-01-01T00:00:05Z".to_string(),
                kind: SessionEventKind::TurnStarted {
                    turn_id: TurnId("turn-1".to_string()),
                    prompt: "Find the authentication race condition in login".to_string(),
                    plan_mode: Some(false),
                },
            },
            SessionEventRecord {
                event_id: "event-3".to_string(),
                session_id: session.id.clone(),
                sequence: 3,
                occurred_at: "2026-01-01T00:00:10Z".to_string(),
                kind: SessionEventKind::TurnDelta {
                    turn_id: TurnId("turn-1".to_string()),
                    content: "The login handler drops the session token during retry.".to_string(),
                },
            },
            SessionEventRecord {
                event_id: "event-4".to_string(),
                session_id: session.id.clone(),
                sequence: 4,
                occurred_at: "2026-01-01T00:00:15Z".to_string(),
                kind: SessionEventKind::TurnCompleted {
                    turn_id: TurnId("turn-1".to_string()),
                },
            },
        ];

        futures::executor::block_on(workspace_repo.save_workspace(&workspace))
            .expect("save workspace");
        futures::executor::block_on(repo.save_session(&session)).expect("save session");
        futures::executor::block_on(repo.save_thread(&thread)).expect("save thread");
        for event in &events {
            futures::executor::block_on(repo.append_event(event)).expect("append event");
        }

        let matches = repo
            .search_sessions("authentication retry", 10)
            .expect("search sessions");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].session_id.0, "session-search");
        assert_eq!(matches[0].thread_title, "Authentication bugfix");
        assert_eq!(
            matches[0].workspace_name.as_deref(),
            Some("Searchable Workspace")
        );
        assert_eq!(
            matches[0].archived_at.as_deref(),
            Some("2026-01-01T00:10:00Z")
        );
        assert!(matches[0].snippet.to_lowercase().contains("authentication"));

        let recents = repo.search_sessions("", 10).expect("recent sessions");
        assert_eq!(recents.len(), 1);
        assert_eq!(recents[0].session_id.0, "session-search");
    }

    #[test]
    fn sqlite_session_repo_persists_delegations_and_status_updates() {
        let conn = in_memory_conn();
        let repo = SqliteSessionRepo::from_connection(conn.clone()).expect("create session repo");
        let workspace_repo =
            SqliteWorkspaceRepo::from_connection(conn).expect("create workspace repo");
        let workspace = Workspace {
            id: WorkspaceId("workspace-1".to_string()),
            project_id: ProjectId("project-1".to_string()),
            name: Some("Delegation Workspace".to_string()),
            root_path: "/tmp/delegation".to_string(),
            base_branch: "main".to_string(),
            worktree_path: Some("/tmp/delegation".to_string()),
            source: None,
            state: WorkspaceState::Ready,
            setup_report: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };
        let parent_session = Session {
            id: SessionId("parent-session".to_string()),
            project_id: ProjectId("project-1".to_string()),
            workspace_id: workspace.id.clone(),
            additional_workspace_ids: Vec::new(),
            provider_id: "codex".to_string(),
            model: Some("gpt-5".to_string()),
            provider_runtime: None,
            working_directory_override: None,
            state: SessionState::Active,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };
        let child_session = Session {
            id: SessionId("child-session".to_string()),
            project_id: ProjectId("project-1".to_string()),
            workspace_id: workspace.id.clone(),
            additional_workspace_ids: Vec::new(),
            provider_id: "gemini".to_string(),
            model: None,
            provider_runtime: None,
            working_directory_override: None,
            state: SessionState::Draft,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };

        futures::executor::block_on(workspace_repo.save_workspace(&workspace))
            .expect("save workspace");
        futures::executor::block_on(repo.save_session(&parent_session))
            .expect("save parent session");
        futures::executor::block_on(repo.save_session(&child_session)).expect("save child session");

        let delegation = Delegation {
            id: DelegationId("delegation-1".to_string()),
            parent_session_id: parent_session.id.clone(),
            parent_turn_id: Some(TurnId("turn-1".to_string())),
            child_session_id: Some(child_session.id.clone()),
            workspace_id: workspace.id.clone(),
            target_provider_id: ProviderId("gemini".to_string()),
            mode: DelegationMode::Review,
            status: DelegationStatus::Draft,
            prompt: "Review the current diff".to_string(),
            context_policy: DelegationContextPolicy::ReviewCurrentDiff,
            budget: DelegationBudget {
                turn_limit: Some(1),
                timeout_seconds: Some(300),
                allow_file_edits: false,
            },
            result_summary: Some("No blocking issues.".to_string()),
            touched_files: vec!["src/lib.rs".to_string()],
            diff_summary: Some("1 file changed".to_string()),
            validation_summary: Some("cargo test -p dcc-core".to_string()),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };

        futures::executor::block_on(repo.save_delegation(&delegation)).expect("save delegation");
        let fetched = futures::executor::block_on(repo.get_delegation(&delegation.id))
            .expect("get delegation")
            .expect("delegation exists");
        assert_eq!(fetched.parent_session_id.0, "parent-session");
        assert_eq!(
            fetched.child_session_id.as_ref().map(|id| id.0.as_str()),
            Some("child-session")
        );
        assert_eq!(
            fetched.context_policy,
            DelegationContextPolicy::ReviewCurrentDiff
        );
        assert_eq!(
            fetched.result_summary.as_deref(),
            Some("No blocking issues.")
        );
        assert_eq!(fetched.touched_files, vec!["src/lib.rs".to_string()]);
        assert_eq!(fetched.diff_summary.as_deref(), Some("1 file changed"));
        assert_eq!(
            fetched.validation_summary.as_deref(),
            Some("cargo test -p dcc-core")
        );

        let listed = futures::executor::block_on(
            repo.list_delegations(Some(&workspace.id), Some(&parent_session.id)),
        )
        .expect("list delegations");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id.0, "delegation-1");

        let cancelled = futures::executor::block_on(repo.update_delegation_status(
            &delegation.id,
            DelegationStatus::Cancelled,
            "2026-01-01T00:01:00Z".to_string(),
        ))
        .expect("cancel delegation")
        .expect("delegation exists after update");
        assert_eq!(cancelled.status, DelegationStatus::Cancelled);
        assert_eq!(cancelled.updated_at, "2026-01-01T00:01:00Z");
    }

    #[test]
    fn sqlite_workspace_repo_persists_repositories_and_deletes_linked_workspaces() {
        let repo = SqliteWorkspaceRepo::from_connection(in_memory_conn()).expect("create repo");
        let repository = Repository {
            id: RepositoryId("/tmp/repo".to_string()),
            project_id: ProjectId("project-1".to_string()),
            name: "repo".to_string(),
            display_name: None,
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
            source: None,
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

        assert!(repo
            .update_repository_display_name(
                &RepositoryId("/tmp/repo".to_string()),
                Some("Customer Portal"),
            )
            .expect("rename project"));
        let renamed = futures::executor::block_on(
            repo.get_repository(&RepositoryId("/tmp/repo".to_string())),
        )
        .expect("read renamed project")
        .expect("renamed project exists");
        assert_eq!(renamed.display_name.as_deref(), Some("Customer Portal"));

        futures::executor::block_on(repo.save_repository(&repository))
            .expect("re-register repository");
        let rediscovered = futures::executor::block_on(
            repo.get_repository(&RepositoryId("/tmp/repo".to_string())),
        )
        .expect("read rediscovered project")
        .expect("rediscovered project exists");
        assert_eq!(
            rediscovered.display_name.as_deref(),
            Some("Customer Portal")
        );

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
    fn sqlite_workspace_repo_round_trips_imported_source_context() {
        let repo = SqliteWorkspaceRepo::from_connection(in_memory_conn()).expect("create repo");
        let workspace = Workspace {
            id: WorkspaceId("workspace-source".to_string()),
            project_id: ProjectId("project-1".to_string()),
            name: Some("Review #42".to_string()),
            root_path: "/tmp/repo".to_string(),
            base_branch: "main".to_string(),
            worktree_path: Some("/tmp/repo/.dcc-worktrees/review-42".to_string()),
            source: Some(WorkspaceSource {
                kind: WorkspaceSourceKind::PullRequest,
                url: "https://github.com/acme/widgets/pull/42".to_string(),
                provider: "github".to_string(),
                remote_name: "origin".to_string(),
                head_branch: "feature/review".to_string(),
                head_sha: "abc123".to_string(),
                base_branch: "main".to_string(),
                change_request_number: Some(42),
                title: Some("Improve review flow".to_string()),
                author: Some("octocat".to_string()),
                source_repository: Some("acme/widgets".to_string()),
                push_target: Some(WorkspacePushTarget {
                    remote_name: "origin".to_string(),
                    branch_name: "feature/review".to_string(),
                    remote_url: None,
                    remote_created: false,
                }),
            }),
            state: WorkspaceState::Ready,
            setup_report: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };

        futures::executor::block_on(repo.save_workspace(&workspace)).expect("save workspace");
        let restored = futures::executor::block_on(repo.get_workspace(&workspace.id))
            .expect("read workspace")
            .expect("workspace exists");

        assert_eq!(
            restored
                .source
                .as_ref()
                .map(|source| source.head_branch.as_str()),
            Some("feature/review")
        );
        assert_eq!(
            restored
                .source
                .as_ref()
                .and_then(|source| source.change_request_number),
            Some(42)
        );
        assert_eq!(
            restored
                .source
                .as_ref()
                .and_then(|source| source.push_target.as_ref())
                .map(|target| target.remote_name.as_str()),
            Some("origin")
        );

        let legacy_source = serde_json::json!({
            "kind": "pull_request",
            "url": "https://github.com/acme/widgets/pull/42",
            "provider": "github",
            "remoteName": "origin",
            "headBranch": "feature/review",
            "headSha": "abc123",
            "baseBranch": "main",
            "changeRequestNumber": 42,
            "title": "Improve review flow",
            "author": "octocat"
        });
        repo.conn
            .lock()
            .expect("workspace database lock")
            .execute(
                "UPDATE dcc_workspaces SET source_json = ?2 WHERE id = ?1",
                params![&workspace.id.0, legacy_source.to_string()],
            )
            .expect("write legacy source");
        let restored_legacy = futures::executor::block_on(repo.get_workspace(&workspace.id))
            .expect("read legacy workspace")
            .expect("legacy workspace exists");
        assert!(restored_legacy
            .source
            .as_ref()
            .and_then(|source| source.push_target.as_ref())
            .is_none());
    }

    #[test]
    fn sqlite_workspace_repo_lists_and_updates_forge_bindings() {
        let repo = SqliteWorkspaceRepo::from_connection(in_memory_conn()).expect("create repo");
        let repository = Repository {
            id: RepositoryId("/tmp/repo".to_string()),
            project_id: ProjectId("project-1".to_string()),
            name: "repo".to_string(),
            display_name: None,
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
            source: None,
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

    #[test]
    fn sqlite_workspace_repo_roundtrips_workspace_bundle_and_members() {
        let repo = SqliteWorkspaceRepo::from_connection(in_memory_conn()).expect("create repo");
        let workspaces = [
            Workspace {
                id: WorkspaceId("workspace-backend".to_string()),
                project_id: ProjectId("backend".to_string()),
                name: Some("Backend".to_string()),
                root_path: "/tmp/backend".to_string(),
                base_branch: "main".to_string(),
                worktree_path: Some("/tmp/backend/.dcc-worktrees/main-a".to_string()),
                source: None,
                state: WorkspaceState::SetupPending,
                setup_report: None,
                created_at: "2026-01-01T00:00:00Z".to_string(),
                updated_at: "2026-01-01T00:00:00Z".to_string(),
            },
            Workspace {
                id: WorkspaceId("workspace-frontend".to_string()),
                project_id: ProjectId("frontend".to_string()),
                name: Some("Frontend".to_string()),
                root_path: "/tmp/frontend".to_string(),
                base_branch: "main".to_string(),
                worktree_path: Some("/tmp/frontend/.dcc-worktrees/main-b".to_string()),
                source: None,
                state: WorkspaceState::Ready,
                setup_report: None,
                created_at: "2026-01-01T00:00:00Z".to_string(),
                updated_at: "2026-01-01T00:00:00Z".to_string(),
            },
        ];
        for workspace in &workspaces {
            futures::executor::block_on(repo.save_workspace(workspace)).expect("save workspace");
        }

        let bundle = WorkspaceBundle {
            id: WorkspaceBundleId("bundle-1".to_string()),
            name: "Checkout flow".to_string(),
            primary_workspace_id: workspaces[0].id.clone(),
            state: WorkspaceBundleState::Ready,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };
        let members = workspaces
            .iter()
            .enumerate()
            .map(|(position, workspace)| WorkspaceBundleMember {
                bundle_id: bundle.id.clone(),
                workspace_id: workspace.id.clone(),
                created_for_bundle: true,
                position: position as u32,
            })
            .collect::<Vec<_>>();
        futures::executor::block_on(repo.save_workspace_bundle(&bundle, &members))
            .expect("save workspace bundle");

        let restored = futures::executor::block_on(repo.get_workspace_bundle(&bundle.id))
            .expect("get workspace bundle")
            .expect("workspace bundle exists");
        assert_eq!(restored.bundle.name, "Checkout flow");
        assert_eq!(restored.members.len(), 2);
        assert_eq!(restored.members[0].workspace_id, workspaces[0].id);
        assert_eq!(restored.members[1].workspace_id, workspaces[1].id);
        let found_by_member =
            futures::executor::block_on(repo.get_workspace_bundle_for_workspace(&workspaces[1].id))
                .expect("get workspace bundle by member")
                .expect("workspace bundle exists for member");
        assert_eq!(found_by_member.bundle.id, bundle.id);

        let listed = futures::executor::block_on(repo.list_workspace_bundles())
            .expect("list workspace bundles");
        assert_eq!(listed.len(), 1);

        let archived = futures::executor::block_on(repo.set_workspace_bundle_state(
            &bundle.id,
            WorkspaceBundleState::Archived,
            "2026-01-01T00:01:00Z".to_string(),
        ))
        .expect("archive workspace bundle")
        .expect("workspace bundle exists");
        assert_eq!(archived.bundle.state, WorkspaceBundleState::Archived);
        let archived_workspaces =
            futures::executor::block_on(repo.list_workspaces()).expect("list workspaces");
        assert!(archived_workspaces
            .iter()
            .all(|workspace| workspace.state == WorkspaceState::Archived));

        let ready = futures::executor::block_on(repo.set_workspace_bundle_state(
            &bundle.id,
            WorkspaceBundleState::Ready,
            "2026-01-01T00:02:00Z".to_string(),
        ))
        .expect("restore workspace bundle")
        .expect("workspace bundle exists");
        assert_eq!(ready.bundle.state, WorkspaceBundleState::Ready);
        let restored_workspaces =
            futures::executor::block_on(repo.list_workspaces()).expect("list restored workspaces");
        let restored_states = restored_workspaces
            .into_iter()
            .map(|workspace| (workspace.id.0, workspace.state))
            .collect::<BTreeMap<_, _>>();
        assert_eq!(
            restored_states.get("workspace-backend"),
            Some(&WorkspaceState::SetupPending)
        );
        assert_eq!(
            restored_states.get("workspace-frontend"),
            Some(&WorkspaceState::Ready)
        );

        let completed = futures::executor::block_on(repo.set_workspace_bundle_state(
            &bundle.id,
            WorkspaceBundleState::Completed,
            "2026-01-01T00:03:00Z".to_string(),
        ))
        .expect("complete workspace bundle")
        .expect("workspace bundle exists");
        assert_eq!(completed.bundle.state, WorkspaceBundleState::Completed);
        let completed_workspaces =
            futures::executor::block_on(repo.list_workspaces()).expect("list workspaces");
        assert!(completed_workspaces
            .iter()
            .all(|workspace| workspace.state == WorkspaceState::Completed));

        futures::executor::block_on(repo.delete_workspace(&workspaces[0].id))
            .expect("delete primary workspace");
        let deleted = futures::executor::block_on(repo.get_workspace_bundle(&bundle.id))
            .expect("get deleted workspace bundle");
        assert!(deleted.is_none());
    }
}

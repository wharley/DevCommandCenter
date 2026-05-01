use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use rusqlite::{params, Connection, OptionalExtension, Row};

use dcc_core::{
	domain::{
		project::ProjectId,
		workspace::{Workspace, WorkspaceId, WorkspaceState},
	},
	ports::WorkspaceRepo,
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
	created_at TEXT NOT NULL,
	updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_dcc_workspaces_project_id
	ON dcc_workspaces(project_id);
"#;

#[derive(Clone)]
pub struct SqliteWorkspaceRepo {
	conn: Arc<Mutex<Connection>>,
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
		conn.execute_batch(WORKSPACE_TABLE_SQL)
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

		Ok(Workspace {
			id: WorkspaceId(row.get::<_, String>(0)?),
			project_id: ProjectId(row.get::<_, String>(1)?),
			name: row.get::<_, Option<String>>(2)?,
			root_path: row.get::<_, String>(3)?,
			base_branch: row.get::<_, String>(4)?,
			worktree_path: row.get::<_, Option<String>>(5)?,
			state,
			created_at: row.get::<_, String>(7)?,
			updated_at: row.get::<_, String>(8)?,
		})
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
				state, created_at, updated_at
			) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
			ON CONFLICT(id) DO UPDATE SET
				project_id = excluded.project_id,
				name = excluded.name,
				root_path = excluded.root_path,
				base_branch = excluded.base_branch,
				worktree_path = excluded.worktree_path,
				state = excluded.state,
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
				       state, created_at, updated_at
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
				       state, created_at, updated_at
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
			workspaces.push(row.map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?);
		}

		Ok(workspaces)
	}
}

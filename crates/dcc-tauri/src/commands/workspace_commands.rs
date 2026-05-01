use tauri::{AppHandle, State};
use serde::{Deserialize, Serialize};
use specta::Type;

use dcc_core::{
	application::{create_workspace_for_repo as run_create_workspace_for_repo, CreateWorkspaceForRepoInput},
	domain::workspace::Workspace,
};
use dcc_infra::{db::SqliteWorkspaceRepo, git::CommandGitOps};

use crate::{
	events::TauriEventBus,
	state::WorkspaceCommandState,
};

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CreateWorkspaceForRepoOutput {
	pub workspace: Workspace,
}

#[tauri::command]
pub async fn create_workspace_for_repo(
	state: State<'_, WorkspaceCommandState>,
	app: AppHandle,
	input: CreateWorkspaceForRepoInput,
) -> Result<CreateWorkspaceForRepoOutput, String> {
	let repo = SqliteWorkspaceRepo::open(&state.db_path).map_err(|error| error.to_string())?;
	let git = CommandGitOps::new();
	let events = TauriEventBus::new(app);

	let finalized = run_create_workspace_for_repo(&repo, &git, &events, input)
		.await
		.map_err(|error| error.to_string())?;

	Ok(CreateWorkspaceForRepoOutput {
		workspace: finalized.workspace,
	})
}

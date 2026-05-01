use tauri::{AppHandle, State};

use dcc_core::application::CreateWorkspaceForRepoInput;
use dcc_tauri::{
	commands::workspace_commands::CreateWorkspaceForRepoOutput,
	state::WorkspaceCommandState,
};

#[tauri::command]
pub async fn create_workspace_for_repo(
	state: State<'_, WorkspaceCommandState>,
	app: AppHandle,
	input: CreateWorkspaceForRepoInput,
) -> Result<CreateWorkspaceForRepoOutput, String> {
	dcc_tauri::commands::workspace_commands::create_workspace_for_repo(state, app, input).await
}

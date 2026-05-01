use std::path::Path;
use std::process::Command;

use tauri::{AppHandle, State};
use serde::{Deserialize, Serialize};
use specta::Type;

use dcc_core::{
	application::{
		create_workspace_for_repo as run_create_workspace_for_repo,
		create_workspace_from_url as run_create_workspace_from_url,
		CreateWorkspaceForRepoInput,
		CreateWorkspaceFromUrlInput,
	},
	domain::workspace::Workspace,
	ports::WorkspaceRepo,
};
use dcc_infra::{db::SqliteWorkspaceRepo, git::{list_local_branch_names, CommandGitOps}};

use crate::{
	events::TauriEventBus,
	state::WorkspaceCommandState,
};

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CreateWorkspaceForRepoOutput {
	pub workspace: Workspace,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CreateWorkspaceFromUrlOutput {
	pub workspace: Workspace,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ListWorkspacesOutput {
	pub workspaces: Vec<Workspace>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ListLocalBranchesInput {
	pub workspace_root: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ListLocalBranchesOutput {
	pub branches: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ListGitTrackedFilesInput {
	pub workspace_root: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ListGitTrackedFilesOutput {
	pub paths: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ListChildDirectoriesInput {
	pub path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ListChildDirectoriesOutput {
	pub paths: Vec<String>,
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

#[tauri::command]
pub async fn create_workspace_from_url(
	state: State<'_, WorkspaceCommandState>,
	app: AppHandle,
	input: CreateWorkspaceFromUrlInput,
) -> Result<CreateWorkspaceFromUrlOutput, String> {
	let repo = SqliteWorkspaceRepo::open(&state.db_path).map_err(|error| error.to_string())?;
	let git = CommandGitOps::new();
	let events = TauriEventBus::new(app);

	let finalized = run_create_workspace_from_url(&repo, &git, &events, input)
		.await
		.map_err(|error| error.to_string())?;

	Ok(CreateWorkspaceFromUrlOutput {
		workspace: finalized.workspace,
	})
}

#[tauri::command]
pub async fn list_workspaces(state: State<'_, WorkspaceCommandState>) -> Result<ListWorkspacesOutput, String> {
	let repo = SqliteWorkspaceRepo::open(&state.db_path).map_err(|error| error.to_string())?;
	let workspaces = repo.list_workspaces().await.map_err(|error| error.to_string())?;

	Ok(ListWorkspacesOutput { workspaces })
}

#[tauri::command]
pub async fn list_local_branches(
	input: ListLocalBranchesInput,
) -> Result<ListLocalBranchesOutput, String> {
	let branches = list_local_branch_names(&input.workspace_root).map_err(|error| error.to_string())?;
	Ok(ListLocalBranchesOutput { branches })
}

/// Paths tracked by git (`git ls-files`), repo-relative forward slashes.
/// Empty vec if not a git worktree or git fails.
#[tauri::command]
pub async fn list_git_tracked_files(
	input: ListGitTrackedFilesInput,
) -> Result<ListGitTrackedFilesOutput, String> {
	let root = input.workspace_root.trim();
	if root.is_empty() {
		return Ok(ListGitTrackedFilesOutput { paths: Vec::new() });
	}

	let output = Command::new("git")
		.arg("-C")
		.arg(root)
		.arg("ls-files")
		.output()
		.map_err(|error| error.to_string())?;

	if !output.status.success() {
		return Ok(ListGitTrackedFilesOutput { paths: Vec::new() });
	}

	let stdout = String::from_utf8_lossy(&output.stdout);
	let mut paths: Vec<String> = stdout
		.lines()
		.map(|line| line.trim().to_string())
		.filter(|line| !line.is_empty())
		.collect();
	paths.sort();
	paths.dedup();

	Ok(ListGitTrackedFilesOutput { paths })
}

/// Immediate child directories of `path` (absolute paths), sorted.
#[tauri::command]
pub async fn list_child_directories(
	input: ListChildDirectoriesInput,
) -> Result<ListChildDirectoriesOutput, String> {
	let root = Path::new(input.path.trim());
	if !root.is_dir() {
		return Ok(ListChildDirectoriesOutput { paths: Vec::new() });
	}

	let mut paths: Vec<String> = Vec::new();
	for entry in std::fs::read_dir(root).map_err(|e| e.to_string())? {
		let entry = entry.map_err(|e| e.to_string())?;
		let p = entry.path();
		if p.is_dir() {
			paths.push(p.to_string_lossy().to_string());
		}
	}
	paths.sort();
	Ok(ListChildDirectoriesOutput { paths })
}

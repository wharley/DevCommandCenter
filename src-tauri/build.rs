use std::{env, fs, path::PathBuf};

	use dcc_core::{
	application::{
		AbortRunInput, AbortRunOutput, CreateWorkspaceForRepoInput, CreateWorkspaceFromUrlInput,
		ResumeSessionInput, ResumeSessionOutput, SendTurnInput, SendTurnOutput, StartThreadInput,
		StartThreadOutput,
	},
		domain::{
			project::ProjectId,
			provider::{ProviderCatalog, ProviderDescriptor},
		session::{
			Checkpoint, CheckpointId, Session, SessionEventKind, SessionEventRecord,
			SessionId, SessionProjection, SessionState, Turn, TurnId, TurnState,
		},
		workspace::{Workspace, WorkspaceId, WorkspaceState},
	},
	ports::events::CoreEvent,
};
	use dcc_tauri::commands::{
	provider_commands::ListProvidersOutput,
	workspace_commands::{
		CreateWorkspaceForRepoOutput, CreateWorkspaceFromUrlOutput, ListChildDirectoriesInput,
		ListChildDirectoriesOutput, ListGitTrackedFilesInput, ListGitTrackedFilesOutput,
		ListLocalBranchesInput, ListLocalBranchesOutput, ListWorkspacesOutput,
		WorkspaceGitChangeEntry, WorkspaceGitCommitPushInput, WorkspaceGitPathInput,
		WorkspaceGitPushInput, WorkspaceGitStatusInput, WorkspaceGitStatusOutput,
	},
};
use serde::{Deserialize, Serialize};
use specta::Type;
use specta_typescript::Typescript;
use tauri_specta::Builder;

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
	struct WorkspaceMethods {
	create_workspace_for_repo: String,
	create_workspace_from_url: String,
	list_local_branches: String,
	list_git_tracked_files: String,
	list_child_directories: String,
	list_workspaces: String,
	workspace_gh_pr_create_fill: String,
	workspace_gh_pr_view_web: String,
	workspace_git_commit_push: String,
	workspace_git_discard_file: String,
	workspace_git_push: String,
	workspace_git_stage_file: String,
	workspace_git_status: String,
	workspace_git_unstage_file: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
struct SessionMethods {
	start_thread: String,
	send_turn: String,
	abort_run: String,
	resume_session: String,
	list_thread_events: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
struct ProviderMethods {
	list_providers: String,
}

fn main() {
	tauri_build::build();

	let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("missing manifest dir"));
	let output_path = manifest_dir.join("../packages/contracts/src/generated/bindings.ts");
	if let Some(parent) = output_path.parent() {
		fs::create_dir_all(parent).expect("failed to create contracts directory");
	}

	let builder = Builder::<tauri::Wry>::new()
		.typ::<WorkspaceId>()
		.typ::<ProjectId>()
		.typ::<WorkspaceState>()
		.typ::<Workspace>()
		.typ::<SessionId>()
		.typ::<TurnId>()
		.typ::<CheckpointId>()
		.typ::<ProviderCatalog>()
		.typ::<ProviderDescriptor>()
		.typ::<dcc_core::domain::provider::HealthStatus>()
		.typ::<SessionState>()
		.typ::<TurnState>()
		.typ::<Turn>()
		.typ::<Checkpoint>()
		.typ::<Session>()
		.typ::<SessionEventKind>()
		.typ::<SessionEventRecord>()
		.typ::<SessionProjection>()
		.typ::<CreateWorkspaceForRepoInput>()
		.typ::<CreateWorkspaceForRepoOutput>()
		.typ::<CreateWorkspaceFromUrlInput>()
		.typ::<CreateWorkspaceFromUrlOutput>()
		.typ::<ListLocalBranchesInput>()
		.typ::<ListLocalBranchesOutput>()
		.typ::<ListGitTrackedFilesInput>()
		.typ::<ListGitTrackedFilesOutput>()
		.typ::<ListChildDirectoriesInput>()
		.typ::<ListChildDirectoriesOutput>()
		.typ::<ListWorkspacesOutput>()
		.typ::<WorkspaceGitStatusInput>()
		.typ::<WorkspaceGitStatusOutput>()
		.typ::<WorkspaceGitChangeEntry>()
		.typ::<WorkspaceGitPathInput>()
		.typ::<WorkspaceGitCommitPushInput>()
		.typ::<WorkspaceGitPushInput>()
		.typ::<ListProvidersOutput>()
		.typ::<StartThreadInput>()
		.typ::<StartThreadOutput>()
		.typ::<SendTurnInput>()
		.typ::<SendTurnOutput>()
		.typ::<AbortRunInput>()
		.typ::<AbortRunOutput>()
		.typ::<ResumeSessionInput>()
		.typ::<ResumeSessionOutput>()
		.typ::<CoreEvent>()
		.constant(
			"WORKSPACE_METHODS",
			WorkspaceMethods {
				create_workspace_for_repo: "create_workspace_for_repo".to_string(),
				create_workspace_from_url: "create_workspace_from_url".to_string(),
				list_local_branches: "list_local_branches".to_string(),
				list_git_tracked_files: "list_git_tracked_files".to_string(),
				list_child_directories: "list_child_directories".to_string(),
				list_workspaces: "list_workspaces".to_string(),
				workspace_gh_pr_create_fill: "workspace_gh_pr_create_fill".to_string(),
				workspace_gh_pr_view_web: "workspace_gh_pr_view_web".to_string(),
				workspace_git_commit_push: "workspace_git_commit_push".to_string(),
				workspace_git_discard_file: "workspace_git_discard_file".to_string(),
				workspace_git_push: "workspace_git_push".to_string(),
				workspace_git_stage_file: "workspace_git_stage_file".to_string(),
				workspace_git_status: "workspace_git_status".to_string(),
				workspace_git_unstage_file: "workspace_git_unstage_file".to_string(),
			},
		);

	let builder = builder.constant(
		"SESSION_METHODS",
		SessionMethods {
			start_thread: "start_thread".to_string(),
			send_turn: "send_turn".to_string(),
			abort_run: "abort_run".to_string(),
			resume_session: "resume_session".to_string(),
			list_thread_events: "list_thread_events".to_string(),
		},
	);

	let builder = builder.constant(
		"PROVIDER_METHODS",
		ProviderMethods {
			list_providers: "list_providers".to_string(),
		},
	);

	builder
		.export(Typescript::default(), &output_path)
		.expect("failed to export DCC contracts");
}

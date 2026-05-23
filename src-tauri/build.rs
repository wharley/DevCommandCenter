use std::{env, fs, path::PathBuf};

use dcc_core::{
    application::{
        AbortRunInput, AbortRunOutput, CloseSessionInput, CloseSessionOutput,
        CreateWorkspaceForRepoInput, CreateWorkspaceFromUrlInput, RestoreSessionInput,
        RestoreSessionOutput, ResumeSessionInput, ResumeSessionOutput, SendTurnInput,
        SendTurnOutput, StartThreadInput, StartThreadOutput,
    },
    domain::{
        project::ProjectId,
        provider::{ProviderCatalog, ProviderDescriptor},
        repository::{Repository, RepositoryId},
        session::{
            Checkpoint, CheckpointId, Session, SessionEventKind, SessionEventRecord, SessionId,
            SessionProjection, SessionSearchResult, SessionState, Turn, TurnId, TurnState,
            WorkspaceSessionSummary,
        },
        workspace::{
            Workspace, WorkspaceId, WorkspaceSetupReport, WorkspaceSetupStatus,
            WorkspaceSetupStepReport, WorkspaceState,
        },
    },
    ports::{events::CoreEvent, ProviderRuntimeConfig},
};
use dcc_tauri::commands::{
    forge_commands::{
        ForgeCliAccountEntry, ForgeCliAccountsInput, ForgeCliAccountsOutput, ForgeCliProvider,
        ForgeCliSelectLoginInput, ForgeCliStatusInput, ForgeCliStatusOutput, GithubCliStatusInput,
        GithubCliStatusOutput, WorkspaceForgeContextInput, WorkspaceForgeContextOutput,
        WorkspacePrStatusInput, WorkspacePrStatusOutput,
    },
    provider_commands::ListProvidersOutput,
    session_commands::{
        RespondToPermissionRequestInput, RespondToPermissionRequestOutput, RespondToUserInputInput,
        RespondToUserInputOutput, SearchSessionsInput,
    },
    workspace_commands::{
        CompileMissionSpecContextInput, CompileMissionSpecContextOutput,
        CompiledMissionSpecContextFile, CreateWorkspaceForRepoOutput, CreateWorkspaceFromUrlOutput,
        ListChildDirectoriesInput, ListChildDirectoriesOutput, ListGitTrackedFilesInput,
        ListGitTrackedFilesOutput, ListLocalBranchesInput, ListLocalBranchesOutput,
        ListMissionSpecsInput, ListMissionSpecsOutput, ListRepositoriesOutput,
        ListWorkspacesOutput, MissionSpecContextFileState, MissionSpecContextFileStatus,
        MissionSpecContextStatusInput, MissionSpecContextStatusOutput, MissionSpecEntry,
        MissionValidationEntry, RepositoryIdInput, SaveMissionValidationInput,
        SaveMissionValidationOutput, WorkspaceContinueFromBaseBranchInput,
        WorkspaceContinueFromBaseBranchOutput, WorkspaceGitBranchDiffInput,
        WorkspaceGitBranchDiffOutput, WorkspaceGitChangeEntry, WorkspaceGitCommitPushInput,
        WorkspaceGitPathInput, WorkspaceGitPushInput, WorkspaceGitStatusInput,
        WorkspaceGitStatusOutput, WorkspaceRunSetupInput, WorkspaceRunSetupOutput,
        WorkspaceSetupHint,
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
    archive_workspace: String,
    restore_workspace: String,
    delete_workspace: String,
    delete_repository: String,
    workspace_github_cli_status: String,
    workspace_forge_cli_status: String,
    workspace_forge_cli_accounts: String,
    workspace_forge_cli_hosts: String,
    workspace_forge_cli_select_login: String,
    workspace_backfill_forge_repo_bindings: String,
    workspace_retry_repository_forge_binding: String,
    workspace_forge_context: String,
    list_local_branches: String,
    list_git_tracked_files: String,
    list_mission_specs: String,
    compile_mission_spec_context: String,
    mission_spec_context_status: String,
    save_mission_validation: String,
    list_child_directories: String,
    list_workspaces: String,
    list_repositories: String,
    workspace_continue_from_base_branch: String,
    workspace_change_request_create: String,
    workspace_change_request_merge: String,
    workspace_change_request_view_web: String,
    workspace_gh_pr_create_fill: String,
    workspace_gh_pr_merge: String,
    workspace_gh_pr_view_web: String,
    workspace_pr_status: String,
    workspace_git_branch_diff: String,
    workspace_git_commit_push: String,
    workspace_git_discard_file: String,
    workspace_git_file_preview: String,
    workspace_git_file_preview_content: String,
    workspace_git_push: String,
    workspace_git_stage_all: String,
    workspace_git_stage_file: String,
    workspace_git_status: String,
    workspace_git_unstage_file: String,
    workspace_run_setup: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
struct SessionMethods {
    start_thread: String,
    send_turn: String,
    abort_run: String,
    resume_session: String,
    close_session: String,
    restore_session: String,
    list_thread_events: String,
    list_workspace_sessions: String,
    search_sessions: String,
    respond_to_user_input: String,
    respond_to_permission_request: String,
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
        .typ::<RepositoryId>()
        .typ::<WorkspaceState>()
        .typ::<Workspace>()
        .typ::<Repository>()
        .typ::<SessionId>()
        .typ::<TurnId>()
        .typ::<CheckpointId>()
        .typ::<ProviderCatalog>()
        .typ::<ProviderDescriptor>()
        .typ::<ProviderRuntimeConfig>()
        .typ::<dcc_core::domain::provider::HealthStatus>()
        .typ::<SessionState>()
        .typ::<TurnState>()
        .typ::<Turn>()
        .typ::<Checkpoint>()
        .typ::<Session>()
        .typ::<SessionEventKind>()
        .typ::<SessionEventRecord>()
        .typ::<SessionProjection>()
        .typ::<SessionSearchResult>()
        .typ::<WorkspaceSessionSummary>()
        .typ::<CreateWorkspaceForRepoInput>()
        .typ::<CreateWorkspaceForRepoOutput>()
        .typ::<CreateWorkspaceFromUrlInput>()
        .typ::<CreateWorkspaceFromUrlOutput>()
        .typ::<WorkspaceRunSetupInput>()
        .typ::<WorkspaceRunSetupOutput>()
        .typ::<WorkspaceSetupHint>()
        .typ::<WorkspaceSetupStatus>()
        .typ::<WorkspaceSetupStepReport>()
        .typ::<WorkspaceSetupReport>()
        .typ::<ListLocalBranchesInput>()
        .typ::<ListLocalBranchesOutput>()
        .typ::<ListGitTrackedFilesInput>()
        .typ::<ListGitTrackedFilesOutput>()
        .typ::<ListMissionSpecsInput>()
        .typ::<MissionSpecEntry>()
        .typ::<MissionValidationEntry>()
        .typ::<ListMissionSpecsOutput>()
        .typ::<CompileMissionSpecContextInput>()
        .typ::<CompiledMissionSpecContextFile>()
        .typ::<CompileMissionSpecContextOutput>()
        .typ::<MissionSpecContextStatusInput>()
        .typ::<MissionSpecContextFileState>()
        .typ::<MissionSpecContextFileStatus>()
        .typ::<MissionSpecContextStatusOutput>()
        .typ::<SaveMissionValidationInput>()
        .typ::<SaveMissionValidationOutput>()
        .typ::<ListChildDirectoriesInput>()
        .typ::<ListChildDirectoriesOutput>()
        .typ::<ListWorkspacesOutput>()
        .typ::<ListRepositoriesOutput>()
        .typ::<RepositoryIdInput>()
        .typ::<GithubCliStatusInput>()
        .typ::<GithubCliStatusOutput>()
        .typ::<ForgeCliProvider>()
        .typ::<ForgeCliAccountsInput>()
        .typ::<ForgeCliAccountEntry>()
        .typ::<ForgeCliAccountsOutput>()
        .typ::<dcc_tauri::commands::forge_commands::ForgeCliHostsInput>()
        .typ::<dcc_tauri::commands::forge_commands::ForgeCliHostsOutput>()
        .typ::<ForgeCliSelectLoginInput>()
        .typ::<ForgeCliStatusInput>()
        .typ::<ForgeCliStatusOutput>()
        .typ::<WorkspaceForgeContextInput>()
        .typ::<WorkspaceForgeContextOutput>()
        .typ::<WorkspaceGitStatusInput>()
        .typ::<WorkspaceGitStatusOutput>()
        .typ::<WorkspaceGitChangeEntry>()
        .typ::<WorkspaceContinueFromBaseBranchInput>()
        .typ::<WorkspaceContinueFromBaseBranchOutput>()
        .typ::<WorkspacePrStatusInput>()
        .typ::<WorkspacePrStatusOutput>()
        .typ::<WorkspaceGitBranchDiffInput>()
        .typ::<WorkspaceGitBranchDiffOutput>()
        .typ::<dcc_tauri::commands::workspace_commands::WorkspaceGitPreviewScope>()
        .typ::<dcc_tauri::commands::workspace_commands::WorkspaceGitFilePreviewInput>()
        .typ::<dcc_tauri::commands::workspace_commands::WorkspaceGitFilePreviewContentOutput>()
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
        .typ::<CloseSessionInput>()
        .typ::<CloseSessionOutput>()
        .typ::<RestoreSessionInput>()
        .typ::<RestoreSessionOutput>()
        .typ::<RespondToUserInputInput>()
        .typ::<RespondToUserInputOutput>()
        .typ::<RespondToPermissionRequestInput>()
        .typ::<RespondToPermissionRequestOutput>()
        .typ::<SearchSessionsInput>()
        .typ::<CoreEvent>()
        .constant(
            "WORKSPACE_METHODS",
            WorkspaceMethods {
                create_workspace_for_repo: "create_workspace_for_repo".to_string(),
                create_workspace_from_url: "create_workspace_from_url".to_string(),
                archive_workspace: "archive_workspace".to_string(),
                restore_workspace: "restore_workspace".to_string(),
                delete_workspace: "delete_workspace".to_string(),
                delete_repository: "delete_repository".to_string(),
                workspace_github_cli_status: "workspace_github_cli_status".to_string(),
                workspace_forge_cli_status: "workspace_forge_cli_status".to_string(),
                workspace_forge_cli_accounts: "workspace_forge_cli_accounts".to_string(),
                workspace_forge_cli_hosts: "workspace_forge_cli_hosts".to_string(),
                workspace_forge_cli_select_login: "workspace_forge_cli_select_login".to_string(),
                workspace_backfill_forge_repo_bindings: "workspace_backfill_forge_repo_bindings"
                    .to_string(),
                workspace_retry_repository_forge_binding:
                    "workspace_retry_repository_forge_binding".to_string(),
                workspace_forge_context: "workspace_forge_context".to_string(),
                list_local_branches: "list_local_branches".to_string(),
                list_git_tracked_files: "list_git_tracked_files".to_string(),
                list_mission_specs: "list_mission_specs".to_string(),
                compile_mission_spec_context: "compile_mission_spec_context".to_string(),
                mission_spec_context_status: "mission_spec_context_status".to_string(),
                save_mission_validation: "save_mission_validation".to_string(),
                list_child_directories: "list_child_directories".to_string(),
                list_workspaces: "list_workspaces".to_string(),
                list_repositories: "list_repositories".to_string(),
                workspace_continue_from_base_branch: "workspace_continue_from_base_branch"
                    .to_string(),
                workspace_change_request_create: "workspace_change_request_create".to_string(),
                workspace_change_request_merge: "workspace_change_request_merge".to_string(),
                workspace_change_request_view_web: "workspace_change_request_view_web".to_string(),
                workspace_gh_pr_create_fill: "workspace_gh_pr_create_fill".to_string(),
                workspace_gh_pr_merge: "workspace_gh_pr_merge".to_string(),
                workspace_gh_pr_view_web: "workspace_gh_pr_view_web".to_string(),
                workspace_pr_status: "workspace_pr_status".to_string(),
                workspace_git_branch_diff: "workspace_git_branch_diff".to_string(),
                workspace_git_file_preview: "workspace_git_file_preview".to_string(),
                workspace_git_file_preview_content: "workspace_git_file_preview_content"
                    .to_string(),
                workspace_git_commit_push: "workspace_git_commit_push".to_string(),
                workspace_git_discard_file: "workspace_git_discard_file".to_string(),
                workspace_git_push: "workspace_git_push".to_string(),
                workspace_git_stage_all: "workspace_git_stage_all".to_string(),
                workspace_git_stage_file: "workspace_git_stage_file".to_string(),
                workspace_git_status: "workspace_git_status".to_string(),
                workspace_git_unstage_file: "workspace_git_unstage_file".to_string(),
                workspace_run_setup: "workspace_run_setup".to_string(),
            },
        );

    let builder = builder.constant(
        "SESSION_METHODS",
        SessionMethods {
            start_thread: "start_thread".to_string(),
            send_turn: "send_turn".to_string(),
            abort_run: "abort_run".to_string(),
            resume_session: "resume_session".to_string(),
            close_session: "close_session".to_string(),
            restore_session: "restore_session".to_string(),
            list_thread_events: "list_thread_events".to_string(),
            list_workspace_sessions: "list_workspace_sessions".to_string(),
            search_sessions: "search_sessions".to_string(),
            respond_to_user_input: "respond_to_user_input".to_string(),
            respond_to_permission_request: "respond_to_permission_request".to_string(),
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

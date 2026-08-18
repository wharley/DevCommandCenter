use std::{env, fs, path::PathBuf};

use dcc_core::{
    application::{
        AbortRunInput, AbortRunOutput, ActivateMcpDefinitionInput, ApprovePlanInput,
        ApprovePlanOutput, CloseSessionInput, CloseSessionOutput, CreateWorkspaceForRepoInput,
        CreateWorkspaceFromUrlInput, QueueTurnInput, RecordPlanHandoffInput,
        RecordPlanHandoffOutput, RemoveQueuedTurnInput, ReorderTurnQueueInput, RestoreSessionInput,
        RestoreSessionOutput, ResumeSessionInput, ResumeSessionOutput, SendTurnInput,
        SendTurnOutput, StartThreadInput, StartThreadOutput, SteerTurnInput, SteerTurnOutput,
    },
    domain::{
        delegation::{
            Delegation, DelegationBudget, DelegationContextPolicy, DelegationId, DelegationMode,
            DelegationStatus,
        },
        mcp::{
            McpBinding, McpBindingId, McpBindingScope, McpDefinition, McpDefinitionId,
            McpDefinitionOwnership, McpErrorCategory, McpImportSource, McpImportSourceKind,
            McpProbeReport, McpRuntimeError, McpRuntimeState, McpRuntimeStatus, McpSecretBinding,
            McpSecretReferenceId, McpSecretTarget, McpToolAnnotations, McpToolPolicy,
            McpToolPolicyDecision, McpToolSummary, McpTransport, McpTransportKind, McpTrust,
            McpTrustDecision, McpTrustFingerprint,
        },
        project::ProjectId,
        provider::{
            McpOauthSupport, McpSupportLevel, NativeSubagentStatus, ProviderCatalog,
            ProviderDescriptor,
        },
        repository::{Repository, RepositoryId},
        session::{
            Checkpoint, CheckpointId, QueuedTurn, Session, SessionEventKind, SessionEventRecord,
            SessionId, SessionProjection, SessionSearchResult, SessionState, Turn, TurnId,
            TurnState, WorkspaceSessionSummary,
        },
        workspace::{
            Workspace, WorkspaceId, WorkspacePushTarget, WorkspaceSetupReport,
            WorkspaceSetupStatus, WorkspaceSetupStepReport, WorkspaceSource, WorkspaceSourceKind,
            WorkspaceState,
        },
        workspace_bundle::{
            WorkspaceBundle, WorkspaceBundleId, WorkspaceBundleMember, WorkspaceBundleState,
            WorkspaceBundleSummary,
        },
    },
    ports::{events::CoreEvent, ProviderRuntimeConfig},
};
use dcc_tauri::commands::{
    coderabbit::{
        CodeRabbitAuthStatusOutput, CodeRabbitCliStatusState, CodeRabbitDiffFingerprint,
        CodeRabbitFinding, CodeRabbitFindingSeverity, CodeRabbitReviewComplete,
        CodeRabbitReviewJobStatus, CodeRabbitReviewStatusEvent, CodeRabbitReviewStreamEvent,
        CodeRabbitReviewType, WorkspaceCodeRabbitCliStatusInput,
        WorkspaceCodeRabbitCliStatusOutput, WorkspaceCodeRabbitDoctorInput,
        WorkspaceCodeRabbitDoctorOutput, WorkspaceCodeRabbitFingerprintInput,
        WorkspaceCodeRabbitLogoutInput, WorkspaceCodeRabbitLogoutOutput,
        WorkspaceCodeRabbitReviewHistoryEntry, WorkspaceCodeRabbitReviewHistoryInput,
        WorkspaceCodeRabbitReviewHistoryOutput, WorkspaceCodeRabbitReviewInput,
        WorkspaceCodeRabbitReviewJobInput, WorkspaceCodeRabbitReviewJobSnapshot,
        WorkspaceCodeRabbitReviewOutput, WorkspaceCodeRabbitReviewStartOutput,
        WorkspaceCodeRabbitSaveReviewInput, WorkspaceCodeRabbitStoredReviewInput,
        WorkspaceCodeRabbitStoredReviewOutput,
    },
    delegation_commands::{
        ApproveDelegationInput, ApproveDelegationOutput, CancelDelegationInput,
        CancelDelegationOutput, CompleteDelegationInput, CompleteDelegationOutput,
        CreateDelegationInput, CreateDelegationOutput, FailDelegationInput, FailDelegationOutput,
        GetDelegationInput, GetDelegationOutput, ListDelegationsInput, ListDelegationsOutput,
        StartDelegationInput, StartDelegationOutput,
    },
    forge_commands::{
        ForgeCliAccountEntry, ForgeCliAccountsInput, ForgeCliAccountsOutput, ForgeCliProvider,
        ForgeCliSelectLoginInput, ForgeCliStatusInput, ForgeCliStatusOutput, GithubCliStatusInput,
        GithubCliStatusOutput, PullRequestHubActor, PullRequestHubCheck, PullRequestHubComment,
        PullRequestHubCommentInput, PullRequestHubCommentOutput, PullRequestHubDetailInput,
        PullRequestHubDetailOutput, PullRequestHubDraftComment, PullRequestHubFile,
        PullRequestHubInlineComment, PullRequestHubItem, PullRequestHubListInput,
        PullRequestHubListOutput, PullRequestHubReviewCapabilities, PullRequestHubReviewEvent,
        PullRequestHubSubmitReviewInput, PullRequestHubSubmitReviewOutput,
        PullRequestHubThreadReplyInput, PullRequestHubThreadReplyOutput,
        PullRequestHubThreadResolveInput, PullRequestHubThreadResolveOutput, PullRequestHubWarning,
        WorkspaceForgeContextInput, WorkspaceForgeContextOutput, WorkspacePipeline,
        WorkspacePipelineJob, WorkspacePipelineJobInput, WorkspacePipelineJobLogOutput,
        WorkspacePipelineStatusInput, WorkspacePipelineStatusOutput, WorkspacePrReviewComment,
        WorkspacePrReviewCommentAuthor, WorkspacePrReviewCommentsInput,
        WorkspacePrReviewCommentsOutput, WorkspacePrStatusInput, WorkspacePrStatusOutput,
        WorkspaceReviewStateInput, WorkspaceReviewStateOutput, WorkspaceReviewer,
    },
    mcp_commands::{
        ActivateMcpIntegrationOutput, CreateMcpIntegrationInput, CreateMcpIntegrationOutput,
        DisableMcpIntegrationInput, DisableMcpIntegrationOutput, DisconnectMcpOauthInput,
        DisconnectMcpOauthOutput, ListMcpIntegrationsOutput, McpCredentialInput,
        McpIntegrationRecord, RemoveMcpIntegrationInput, RemoveMcpIntegrationOutput,
        SetMcpToolPolicyInput, SetMcpToolPolicyOutput,
    },
    provider_commands::{
        ListProvidersOutput, ProviderAccountUsageInput, ProviderAccountUsageOutput,
    },
    session_commands::{
        ApplyTaskTitleInput, ApplyTaskTitleOutput, InterruptNativeSubagentInput,
        ListMcpRuntimeStatusesInput, ListMcpRuntimeStatusesOutput, McpTurnPreflightState,
        NativeSubagentControlOutput, PrepareTurnOutput, RespondToPermissionRequestInput,
        RespondToPermissionRequestOutput, RespondToUserInputInput, RespondToUserInputOutput,
        RunPullRequestReviewAgentInput, RunPullRequestReviewAgentOutput, SearchSessionsInput,
        StartMcpOauthInput, StartMcpOauthOutput, SteerNativeSubagentInput, WaitMcpOauthInput,
        WaitMcpOauthOutput,
    },
    workspace_commands::{
        CompileMissionSpecContextInput, CompileMissionSpecContextOutput,
        CompiledMissionSpecContextFile, CreateWorkspaceBundleForReposInput,
        CreateWorkspaceBundleForReposOutput, CreateWorkspaceForRepoOutput,
        CreateWorkspaceFromSourceUrlInput, CreateWorkspaceFromUrlOutput,
        DeleteWorkspaceBundleInput, DeleteWorkspaceInput, ListChildDirectoriesInput,
        ListChildDirectoriesOutput, ListGitTrackedFilesInput, ListGitTrackedFilesOutput,
        ListLocalBranchesInput, ListLocalBranchesOutput, ListMissionSpecsInput,
        ListMissionSpecsOutput, ListRepositoriesOutput, ListWorkspaceBundlesOutput,
        ListWorkspacesOutput, MissionSpecContextFileState, MissionSpecContextFileStatus,
        MissionSpecContextStatusInput, MissionSpecContextStatusOutput, MissionSpecEntry,
        MissionValidationEntry, ReadWorkspaceFileInput, ReadWorkspaceFileOutput,
        RenameWorkspaceInput, RepositoryIdInput, ResolveWorkspaceSourceUrlInput,
        SaveMissionValidationInput, SaveMissionValidationOutput, SearchWorkspaceInput,
        SearchWorkspaceMatch, SearchWorkspaceOutput, SetRepositoryPinnedInput,
        SetWorkspacePinnedInput, UpdateRepositoryIdentityInput,
        WorkspaceApplyDelegationWorktreeInput, WorkspaceApplyDelegationWorktreeOutput,
        WorkspaceBundleIdInput, WorkspaceBundleStateOutput, WorkspaceContinueFromBaseBranchInput,
        WorkspaceContinueFromBaseBranchOutput, WorkspaceDeliveryPolicy,
        WorkspaceGitAcceptConflictInput, WorkspaceGitBranchDiffInput, WorkspaceGitBranchDiffOutput,
        WorkspaceGitChangeEntry, WorkspaceGitCommitPushInput, WorkspaceGitCommitSuggestionInput,
        WorkspaceGitCommitSuggestionOutput, WorkspaceGitCompleteMergeInput,
        WorkspaceGitCompleteMergeOutput, WorkspaceGitConflictContent, WorkspaceGitConflictEntry,
        WorkspaceGitConflictKind, WorkspaceGitConflictOperation, WorkspaceGitConflictSide,
        WorkspaceGitConflictStateInput, WorkspaceGitConflictStateOutput,
        WorkspaceGitMarkConflictResolvedInput, WorkspaceGitPathInput, WorkspaceGitPushInput,
        WorkspaceGitStatusInput, WorkspaceGitStatusOutput, WorkspaceGitSyncBaseInput,
        WorkspaceGitSyncBaseOutput, WorkspaceGitValidationConfigOutput,
        WorkspaceGitValidationReport, WorkspaceGitValidationStatus, WorkspaceGitValidationStep,
        WorkspacePrepareDelegationWorktreeInput, WorkspacePrepareDelegationWorktreeOutput,
        WorkspaceProjectAutomationConfigOutput, WorkspaceProjectTask, WorkspaceProjectTaskKind,
        WorkspaceRecordSetupOutcomeInput, WorkspaceRemoveDelegationWorktreeInput,
        WorkspaceRunProjectTasksInput, WorkspaceRunProjectTasksOutput, WorkspaceRunSetupInput,
        WorkspaceRunSetupOutput, WorkspaceSaveProjectAutomationInput, WorkspaceSetupHint,
        WorkspaceSourceUrlResolution, WriteWorkspaceFileInput, WriteWorkspaceFileOutput,
    },
};
use dcc_tauri::delivery_failure::{
    WorkspaceDeliveryFailureClassification, WorkspaceDeliveryFailureInput,
    WorkspaceDeliveryFailureOperation, WorkspaceDeliveryFailureOutput,
    WorkspaceDeliveryFailureSnapshot, WorkspaceDeliveryPushTarget, WorkspaceDeliveryRecoveryAction,
    WorkspaceDeliveryRecoveryInput, WorkspaceDeliveryRecoveryOutput,
};
use serde::{Deserialize, Serialize};
use specta::Type;
use specta_typescript::Typescript;
use tauri_specta::Builder;

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
struct WorkspaceMethods {
    archive_workspace_bundle: String,
    complete_workspace_bundle: String,
    create_workspace_bundle_for_repos: String,
    create_workspace_for_repo: String,
    create_workspace_from_source_url: String,
    create_workspace_from_url: String,
    resolve_workspace_source_url: String,
    archive_workspace: String,
    complete_workspace: String,
    restore_workspace: String,
    rename_workspace: String,
    restore_workspace_bundle: String,
    delete_workspace_bundle: String,
    delete_workspace: String,
    delete_repository: String,
    update_repository_identity: String,
    set_repository_pinned: String,
    set_workspace_pinned: String,
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
    read_workspace_file: String,
    write_workspace_file: String,
    search_workspace: String,
    list_mission_specs: String,
    compile_mission_spec_context: String,
    mission_spec_context_status: String,
    save_mission_validation: String,
    list_child_directories: String,
    list_workspaces: String,
    list_repositories: String,
    list_workspace_bundles: String,
    workspace_continue_from_base_branch: String,
    workspace_change_request_context: String,
    workspace_change_request_create: String,
    workspace_change_request_merge: String,
    workspace_change_request_view_web: String,
    workspace_gh_pr_create_fill: String,
    workspace_gh_pr_merge: String,
    workspace_gh_pr_view_web: String,
    workspace_pr_review_comments: String,
    workspace_pr_status: String,
    pull_request_hub_list: String,
    pull_request_hub_detail: String,
    pull_request_hub_comment: String,
    pull_request_hub_submit_review: String,
    pull_request_hub_reply_thread: String,
    pull_request_hub_resolve_thread: String,
    workspace_pipeline_status: String,
    workspace_pipeline_job_log: String,
    workspace_pipeline_job_retry: String,
    workspace_review_state: String,
    workspace_delivery_failure_snapshot: String,
    workspace_delivery_recovery_execute: String,
    workspace_git_branch_diff: String,
    workspace_apply_delegation_worktree: String,
    workspace_git_commit_push: String,
    workspace_git_commit: String,
    workspace_git_commit_suggestion: String,
    workspace_git_accept_conflict: String,
    workspace_git_mark_conflict_resolved: String,
    workspace_git_abort_merge: String,
    workspace_git_complete_merge: String,
    workspace_git_validation_config: String,
    workspace_project_automation_config: String,
    workspace_save_project_automation: String,
    workspace_run_project_tasks: String,
    workspace_git_conflict_state: String,
    workspace_git_discard_file: String,
    workspace_git_file_preview: String,
    workspace_git_file_preview_content: String,
    workspace_git_push: String,
    workspace_git_stage_all: String,
    workspace_git_stage_file: String,
    workspace_git_status: String,
    workspace_git_sync_base: String,
    workspace_git_unstage_file: String,
    workspace_prepare_delegation_worktree: String,
    workspace_remove_delegation_worktree: String,
    workspace_record_setup_outcome: String,
    workspace_run_setup: String,
    workspace_skip_setup: String,
    workspace_coderabbit_cli_status: String,
    workspace_coderabbit_logout: String,
    workspace_coderabbit_doctor: String,
    workspace_coderabbit_diff_fingerprint: String,
    workspace_coderabbit_review: String,
    workspace_coderabbit_review_start: String,
    workspace_coderabbit_review_job: String,
    workspace_coderabbit_review_cancel: String,
    workspace_coderabbit_review_load: String,
    workspace_coderabbit_review_save: String,
    workspace_coderabbit_review_history: String,
    workspace_coderabbit_review_clear: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
struct SessionMethods {
    start_thread: String,
    run_pull_request_review_agent: String,
    apply_task_title: String,
    prepare_turn: String,
    send_turn: String,
    steer_turn: String,
    steer_native_subagent: String,
    interrupt_native_subagent: String,
    queue_turn: String,
    list_turn_queue: String,
    remove_queued_turn: String,
    reorder_turn_queue: String,
    dispatch_next_queued_turn: String,
    approve_plan: String,
    record_plan_handoff: String,
    abort_run: String,
    resume_session: String,
    close_session: String,
    restore_session: String,
    list_thread_events: String,
    list_mcp_runtime_statuses: String,
    start_mcp_oauth: String,
    wait_mcp_oauth: String,
    list_workspace_sessions: String,
    search_sessions: String,
    respond_to_user_input: String,
    respond_to_permission_request: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
struct ProviderMethods {
    list_providers: String,
    provider_account_usage: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
struct McpMethods {
    list_mcp_integrations: String,
    create_mcp_integration: String,
    activate_mcp_integration: String,
    disable_mcp_integration: String,
    remove_mcp_integration: String,
    disconnect_mcp_oauth: String,
    set_mcp_tool_policy: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
struct DelegationMethods {
    create_delegation: String,
    list_delegations: String,
    get_delegation: String,
    cancel_delegation: String,
    start_delegation: String,
    complete_delegation: String,
    approve_delegation: String,
    fail_delegation: String,
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
        .typ::<WorkspaceSourceKind>()
        .typ::<WorkspacePushTarget>()
        .typ::<WorkspaceSource>()
        .typ::<Workspace>()
        .typ::<WorkspaceBundleId>()
        .typ::<WorkspaceBundleState>()
        .typ::<WorkspaceBundle>()
        .typ::<WorkspaceBundleMember>()
        .typ::<WorkspaceBundleSummary>()
        .typ::<Repository>()
        .typ::<SessionId>()
        .typ::<TurnId>()
        .typ::<CheckpointId>()
        .typ::<ProviderCatalog>()
        .typ::<ProviderDescriptor>()
        .typ::<McpSupportLevel>()
        .typ::<McpOauthSupport>()
        .typ::<NativeSubagentStatus>()
        .typ::<dcc_core::domain::provider::ProviderAccountUsage>()
        .typ::<dcc_core::domain::provider::ProviderAccountUsageState>()
        .typ::<dcc_core::domain::provider::ProviderUsageWindow>()
        .typ::<ProviderRuntimeConfig>()
        .typ::<dcc_core::domain::provider::HealthStatus>()
        .typ::<DelegationId>()
        .typ::<DelegationMode>()
        .typ::<DelegationStatus>()
        .typ::<DelegationContextPolicy>()
        .typ::<DelegationBudget>()
        .typ::<Delegation>()
        .typ::<McpDefinitionId>()
        .typ::<McpBindingId>()
        .typ::<McpSecretReferenceId>()
        .typ::<McpTrustFingerprint>()
        .typ::<McpTransportKind>()
        .typ::<McpTransport>()
        .typ::<McpImportSourceKind>()
        .typ::<McpImportSource>()
        .typ::<McpDefinitionOwnership>()
        .typ::<McpSecretTarget>()
        .typ::<McpSecretBinding>()
        .typ::<McpTrustDecision>()
        .typ::<McpTrust>()
        .typ::<McpDefinition>()
        .typ::<McpBindingScope>()
        .typ::<McpBinding>()
        .typ::<McpRuntimeState>()
        .typ::<McpErrorCategory>()
        .typ::<McpRuntimeError>()
        .typ::<McpToolAnnotations>()
        .typ::<McpToolSummary>()
        .typ::<McpProbeReport>()
        .typ::<McpRuntimeStatus>()
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
        .typ::<ResolveWorkspaceSourceUrlInput>()
        .typ::<WorkspaceSourceUrlResolution>()
        .typ::<CreateWorkspaceFromSourceUrlInput>()
        .typ::<CreateWorkspaceBundleForReposInput>()
        .typ::<CreateWorkspaceBundleForReposOutput>()
        .typ::<WorkspaceBundleIdInput>()
        .typ::<WorkspaceBundleStateOutput>()
        .typ::<DeleteWorkspaceBundleInput>()
        .typ::<DeleteWorkspaceInput>()
        .typ::<RenameWorkspaceInput>()
        .typ::<CreateWorkspaceFromUrlInput>()
        .typ::<CreateWorkspaceFromUrlOutput>()
        .typ::<WorkspaceRunSetupInput>()
        .typ::<WorkspaceRecordSetupOutcomeInput>()
        .typ::<WorkspaceRunSetupOutput>()
        .typ::<WorkspaceSetupHint>()
        .typ::<WorkspaceSetupStatus>()
        .typ::<WorkspaceSetupStepReport>()
        .typ::<WorkspaceSetupReport>()
        .typ::<ListLocalBranchesInput>()
        .typ::<ListLocalBranchesOutput>()
        .typ::<ListGitTrackedFilesInput>()
        .typ::<ListGitTrackedFilesOutput>()
        .typ::<ReadWorkspaceFileInput>()
        .typ::<ReadWorkspaceFileOutput>()
        .typ::<WriteWorkspaceFileInput>()
        .typ::<WriteWorkspaceFileOutput>()
        .typ::<SearchWorkspaceInput>()
        .typ::<SearchWorkspaceMatch>()
        .typ::<SearchWorkspaceOutput>()
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
        .typ::<ListWorkspaceBundlesOutput>()
        .typ::<RepositoryIdInput>()
        .typ::<UpdateRepositoryIdentityInput>()
        .typ::<SetRepositoryPinnedInput>()
        .typ::<SetWorkspacePinnedInput>()
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
        .typ::<WorkspacePrReviewCommentAuthor>()
        .typ::<WorkspacePrReviewComment>()
        .typ::<WorkspacePrReviewCommentsInput>()
        .typ::<WorkspacePrReviewCommentsOutput>()
        .typ::<WorkspacePipelineStatusInput>()
        .typ::<WorkspacePipelineJobInput>()
        .typ::<WorkspacePipelineJob>()
        .typ::<WorkspacePipeline>()
        .typ::<WorkspacePipelineStatusOutput>()
        .typ::<WorkspacePipelineJobLogOutput>()
        .typ::<WorkspaceReviewStateInput>()
        .typ::<WorkspaceReviewer>()
        .typ::<WorkspaceReviewStateOutput>()
        .typ::<WorkspaceDeliveryFailureInput>()
        .typ::<WorkspaceDeliveryFailureOperation>()
        .typ::<WorkspaceDeliveryFailureClassification>()
        .typ::<WorkspaceDeliveryPushTarget>()
        .typ::<WorkspaceDeliveryFailureSnapshot>()
        .typ::<WorkspaceDeliveryFailureOutput>()
        .typ::<WorkspaceDeliveryRecoveryAction>()
        .typ::<WorkspaceDeliveryRecoveryInput>()
        .typ::<WorkspaceDeliveryRecoveryOutput>()
        .typ::<WorkspaceGitStatusInput>()
        .typ::<WorkspaceGitStatusOutput>()
        .typ::<WorkspaceGitChangeEntry>()
        .typ::<WorkspaceGitConflictStateInput>()
        .typ::<WorkspaceGitConflictStateOutput>()
        .typ::<WorkspaceGitConflictOperation>()
        .typ::<WorkspaceGitConflictKind>()
        .typ::<WorkspaceGitConflictContent>()
        .typ::<WorkspaceGitConflictEntry>()
        .typ::<WorkspaceGitConflictSide>()
        .typ::<WorkspaceGitAcceptConflictInput>()
        .typ::<WorkspaceGitMarkConflictResolvedInput>()
        .typ::<WorkspaceContinueFromBaseBranchInput>()
        .typ::<WorkspaceContinueFromBaseBranchOutput>()
        .typ::<WorkspacePrStatusInput>()
        .typ::<WorkspacePrStatusOutput>()
        .typ::<PullRequestHubListInput>()
        .typ::<PullRequestHubActor>()
        .typ::<PullRequestHubCheck>()
        .typ::<PullRequestHubComment>()
        .typ::<PullRequestHubItem>()
        .typ::<PullRequestHubWarning>()
        .typ::<PullRequestHubListOutput>()
        .typ::<PullRequestHubDetailInput>()
        .typ::<PullRequestHubDetailOutput>()
        .typ::<PullRequestHubCommentInput>()
        .typ::<PullRequestHubCommentOutput>()
        .typ::<PullRequestHubFile>()
        .typ::<PullRequestHubInlineComment>()
        .typ::<PullRequestHubReviewCapabilities>()
        .typ::<PullRequestHubReviewEvent>()
        .typ::<PullRequestHubDraftComment>()
        .typ::<PullRequestHubSubmitReviewInput>()
        .typ::<PullRequestHubSubmitReviewOutput>()
        .typ::<PullRequestHubThreadReplyInput>()
        .typ::<PullRequestHubThreadReplyOutput>()
        .typ::<PullRequestHubThreadResolveInput>()
        .typ::<PullRequestHubThreadResolveOutput>()
        .typ::<RunPullRequestReviewAgentInput>()
        .typ::<RunPullRequestReviewAgentOutput>()
        .typ::<WorkspaceGitBranchDiffInput>()
        .typ::<WorkspaceGitBranchDiffOutput>()
        .typ::<dcc_tauri::commands::workspace_commands::WorkspaceGitPreviewScope>()
        .typ::<dcc_tauri::commands::workspace_commands::WorkspaceGitFilePreviewInput>()
        .typ::<dcc_tauri::commands::workspace_commands::WorkspaceGitFilePreviewContentOutput>()
        .typ::<WorkspaceGitPathInput>()
        .typ::<WorkspaceGitCommitPushInput>()
        .typ::<dcc_tauri::commands::workspace_commands::WorkspaceGitCommitInput>()
        .typ::<WorkspaceGitCommitSuggestionInput>()
        .typ::<WorkspaceGitCommitSuggestionOutput>()
        .typ::<dcc_tauri::commands::workspace_commands::WorkspaceChangeRequestCreateInput>()
        .typ::<dcc_tauri::commands::workspace_commands::WorkspaceChangeRequestContextInput>()
        .typ::<dcc_tauri::commands::workspace_commands::WorkspaceChangeRequestContextOutput>()
        .typ::<WorkspaceGitPushInput>()
        .typ::<WorkspaceGitCompleteMergeInput>()
        .typ::<WorkspaceGitCompleteMergeOutput>()
        .typ::<WorkspaceGitValidationStatus>()
        .typ::<WorkspaceGitValidationStep>()
        .typ::<WorkspaceGitValidationReport>()
        .typ::<WorkspaceGitValidationConfigOutput>()
        .typ::<WorkspaceProjectTaskKind>()
        .typ::<WorkspaceProjectTask>()
        .typ::<WorkspaceDeliveryPolicy>()
        .typ::<WorkspaceProjectAutomationConfigOutput>()
        .typ::<WorkspaceSaveProjectAutomationInput>()
        .typ::<WorkspaceRunProjectTasksInput>()
        .typ::<WorkspaceRunProjectTasksOutput>()
        .typ::<WorkspaceGitSyncBaseInput>()
        .typ::<WorkspaceGitSyncBaseOutput>()
        .typ::<WorkspaceApplyDelegationWorktreeInput>()
        .typ::<WorkspaceApplyDelegationWorktreeOutput>()
        .typ::<WorkspacePrepareDelegationWorktreeInput>()
        .typ::<WorkspacePrepareDelegationWorktreeOutput>()
        .typ::<WorkspaceRemoveDelegationWorktreeInput>()
        .typ::<CodeRabbitCliStatusState>()
        .typ::<CodeRabbitReviewType>()
        .typ::<CodeRabbitFindingSeverity>()
        .typ::<WorkspaceCodeRabbitCliStatusInput>()
        .typ::<CodeRabbitAuthStatusOutput>()
        .typ::<WorkspaceCodeRabbitCliStatusOutput>()
        .typ::<WorkspaceCodeRabbitLogoutInput>()
        .typ::<WorkspaceCodeRabbitLogoutOutput>()
        .typ::<WorkspaceCodeRabbitDoctorInput>()
        .typ::<WorkspaceCodeRabbitDoctorOutput>()
        .typ::<WorkspaceCodeRabbitFingerprintInput>()
        .typ::<CodeRabbitDiffFingerprint>()
        .typ::<WorkspaceCodeRabbitReviewInput>()
        .typ::<CodeRabbitFinding>()
        .typ::<CodeRabbitReviewStatusEvent>()
        .typ::<CodeRabbitReviewComplete>()
        .typ::<WorkspaceCodeRabbitReviewOutput>()
        .typ::<CodeRabbitReviewStreamEvent>()
        .typ::<CodeRabbitReviewJobStatus>()
        .typ::<WorkspaceCodeRabbitReviewStartOutput>()
        .typ::<WorkspaceCodeRabbitReviewJobInput>()
        .typ::<WorkspaceCodeRabbitReviewJobSnapshot>()
        .typ::<WorkspaceCodeRabbitStoredReviewInput>()
        .typ::<WorkspaceCodeRabbitSaveReviewInput>()
        .typ::<WorkspaceCodeRabbitStoredReviewOutput>()
        .typ::<WorkspaceCodeRabbitReviewHistoryInput>()
        .typ::<WorkspaceCodeRabbitReviewHistoryEntry>()
        .typ::<WorkspaceCodeRabbitReviewHistoryOutput>()
        .typ::<ListProvidersOutput>()
        .typ::<ProviderAccountUsageInput>()
        .typ::<ProviderAccountUsageOutput>()
        .typ::<McpIntegrationRecord>()
        .typ::<McpToolPolicy>()
        .typ::<McpToolPolicyDecision>()
        .typ::<ListMcpIntegrationsOutput>()
        .typ::<McpCredentialInput>()
        .typ::<CreateMcpIntegrationInput>()
        .typ::<CreateMcpIntegrationOutput>()
        .typ::<ActivateMcpDefinitionInput>()
        .typ::<ActivateMcpIntegrationOutput>()
        .typ::<DisableMcpIntegrationInput>()
        .typ::<DisableMcpIntegrationOutput>()
        .typ::<RemoveMcpIntegrationInput>()
        .typ::<RemoveMcpIntegrationOutput>()
        .typ::<DisconnectMcpOauthInput>()
        .typ::<DisconnectMcpOauthOutput>()
        .typ::<SetMcpToolPolicyInput>()
        .typ::<SetMcpToolPolicyOutput>()
        .typ::<StartThreadInput>()
        .typ::<StartThreadOutput>()
        .typ::<ApplyTaskTitleInput>()
        .typ::<ApplyTaskTitleOutput>()
        .typ::<McpTurnPreflightState>()
        .typ::<PrepareTurnOutput>()
        .typ::<SendTurnInput>()
        .typ::<SendTurnOutput>()
        .typ::<SteerTurnInput>()
        .typ::<SteerTurnOutput>()
        .typ::<SteerNativeSubagentInput>()
        .typ::<InterruptNativeSubagentInput>()
        .typ::<NativeSubagentControlOutput>()
        .typ::<QueuedTurn>()
        .typ::<QueueTurnInput>()
        .typ::<RemoveQueuedTurnInput>()
        .typ::<ReorderTurnQueueInput>()
        .typ::<ApprovePlanInput>()
        .typ::<ApprovePlanOutput>()
        .typ::<RecordPlanHandoffInput>()
        .typ::<RecordPlanHandoffOutput>()
        .typ::<AbortRunInput>()
        .typ::<AbortRunOutput>()
        .typ::<ResumeSessionInput>()
        .typ::<ResumeSessionOutput>()
        .typ::<CloseSessionInput>()
        .typ::<CloseSessionOutput>()
        .typ::<RestoreSessionInput>()
        .typ::<RestoreSessionOutput>()
        .typ::<ListMcpRuntimeStatusesInput>()
        .typ::<ListMcpRuntimeStatusesOutput>()
        .typ::<StartMcpOauthInput>()
        .typ::<StartMcpOauthOutput>()
        .typ::<WaitMcpOauthInput>()
        .typ::<WaitMcpOauthOutput>()
        .typ::<RespondToUserInputInput>()
        .typ::<RespondToUserInputOutput>()
        .typ::<RespondToPermissionRequestInput>()
        .typ::<RespondToPermissionRequestOutput>()
        .typ::<SearchSessionsInput>()
        .typ::<CreateDelegationInput>()
        .typ::<CreateDelegationOutput>()
        .typ::<ListDelegationsInput>()
        .typ::<ListDelegationsOutput>()
        .typ::<GetDelegationInput>()
        .typ::<GetDelegationOutput>()
        .typ::<CancelDelegationInput>()
        .typ::<CancelDelegationOutput>()
        .typ::<StartDelegationInput>()
        .typ::<StartDelegationOutput>()
        .typ::<CompleteDelegationInput>()
        .typ::<CompleteDelegationOutput>()
        .typ::<ApproveDelegationInput>()
        .typ::<ApproveDelegationOutput>()
        .typ::<FailDelegationInput>()
        .typ::<FailDelegationOutput>()
        .typ::<CoreEvent>()
        .constant(
            "WORKSPACE_METHODS",
            WorkspaceMethods {
                archive_workspace_bundle: "archive_workspace_bundle".to_string(),
                complete_workspace_bundle: "complete_workspace_bundle".to_string(),
                create_workspace_bundle_for_repos: "create_workspace_bundle_for_repos".to_string(),
                create_workspace_for_repo: "create_workspace_for_repo".to_string(),
                create_workspace_from_source_url: "create_workspace_from_source_url".to_string(),
                create_workspace_from_url: "create_workspace_from_url".to_string(),
                resolve_workspace_source_url: "resolve_workspace_source_url".to_string(),
                archive_workspace: "archive_workspace".to_string(),
                complete_workspace: "complete_workspace".to_string(),
                restore_workspace: "restore_workspace".to_string(),
                rename_workspace: "rename_workspace".to_string(),
                restore_workspace_bundle: "restore_workspace_bundle".to_string(),
                delete_workspace_bundle: "delete_workspace_bundle".to_string(),
                delete_workspace: "delete_workspace".to_string(),
                delete_repository: "delete_repository".to_string(),
                update_repository_identity: "update_repository_identity".to_string(),
                set_repository_pinned: "set_repository_pinned".to_string(),
                set_workspace_pinned: "set_workspace_pinned".to_string(),
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
                read_workspace_file: "read_workspace_file".to_string(),
                write_workspace_file: "write_workspace_file".to_string(),
                search_workspace: "search_workspace".to_string(),
                list_mission_specs: "list_mission_specs".to_string(),
                compile_mission_spec_context: "compile_mission_spec_context".to_string(),
                mission_spec_context_status: "mission_spec_context_status".to_string(),
                save_mission_validation: "save_mission_validation".to_string(),
                list_child_directories: "list_child_directories".to_string(),
                list_workspaces: "list_workspaces".to_string(),
                list_repositories: "list_repositories".to_string(),
                list_workspace_bundles: "list_workspace_bundles".to_string(),
                workspace_continue_from_base_branch: "workspace_continue_from_base_branch"
                    .to_string(),
                workspace_change_request_context: "workspace_change_request_context".to_string(),
                workspace_change_request_create: "workspace_change_request_create".to_string(),
                workspace_change_request_merge: "workspace_change_request_merge".to_string(),
                workspace_change_request_view_web: "workspace_change_request_view_web".to_string(),
                workspace_gh_pr_create_fill: "workspace_gh_pr_create_fill".to_string(),
                workspace_gh_pr_merge: "workspace_gh_pr_merge".to_string(),
                workspace_gh_pr_view_web: "workspace_gh_pr_view_web".to_string(),
                workspace_pr_review_comments: "workspace_pr_review_comments".to_string(),
                workspace_pr_status: "workspace_pr_status".to_string(),
                pull_request_hub_list: "pull_request_hub_list".to_string(),
                pull_request_hub_detail: "pull_request_hub_detail".to_string(),
                pull_request_hub_comment: "pull_request_hub_comment".to_string(),
                pull_request_hub_submit_review: "pull_request_hub_submit_review".to_string(),
                pull_request_hub_reply_thread: "pull_request_hub_reply_thread".to_string(),
                pull_request_hub_resolve_thread: "pull_request_hub_resolve_thread".to_string(),
                workspace_pipeline_status: "workspace_pipeline_status".to_string(),
                workspace_pipeline_job_log: "workspace_pipeline_job_log".to_string(),
                workspace_pipeline_job_retry: "workspace_pipeline_job_retry".to_string(),
                workspace_review_state: "workspace_review_state".to_string(),
                workspace_delivery_failure_snapshot: "workspace_delivery_failure_snapshot"
                    .to_string(),
                workspace_delivery_recovery_execute: "workspace_delivery_recovery_execute"
                    .to_string(),
                workspace_git_branch_diff: "workspace_git_branch_diff".to_string(),
                workspace_apply_delegation_worktree: "workspace_apply_delegation_worktree"
                    .to_string(),
                workspace_git_file_preview: "workspace_git_file_preview".to_string(),
                workspace_git_file_preview_content: "workspace_git_file_preview_content"
                    .to_string(),
                workspace_git_commit_push: "workspace_git_commit_push".to_string(),
                workspace_git_commit: "workspace_git_commit".to_string(),
                workspace_git_commit_suggestion: "workspace_git_commit_suggestion".to_string(),
                workspace_git_accept_conflict: "workspace_git_accept_conflict".to_string(),
                workspace_git_mark_conflict_resolved: "workspace_git_mark_conflict_resolved"
                    .to_string(),
                workspace_git_abort_merge: "workspace_git_abort_merge".to_string(),
                workspace_git_complete_merge: "workspace_git_complete_merge".to_string(),
                workspace_git_validation_config: "workspace_git_validation_config".to_string(),
                workspace_project_automation_config: "workspace_project_automation_config"
                    .to_string(),
                workspace_save_project_automation: "workspace_save_project_automation".to_string(),
                workspace_run_project_tasks: "workspace_run_project_tasks".to_string(),
                workspace_git_conflict_state: "workspace_git_conflict_state".to_string(),
                workspace_git_discard_file: "workspace_git_discard_file".to_string(),
                workspace_git_push: "workspace_git_push".to_string(),
                workspace_git_stage_all: "workspace_git_stage_all".to_string(),
                workspace_git_stage_file: "workspace_git_stage_file".to_string(),
                workspace_git_status: "workspace_git_status".to_string(),
                workspace_git_sync_base: "workspace_git_sync_base".to_string(),
                workspace_git_unstage_file: "workspace_git_unstage_file".to_string(),
                workspace_prepare_delegation_worktree: "workspace_prepare_delegation_worktree"
                    .to_string(),
                workspace_remove_delegation_worktree: "workspace_remove_delegation_worktree"
                    .to_string(),
                workspace_record_setup_outcome: "workspace_record_setup_outcome".to_string(),
                workspace_run_setup: "workspace_run_setup".to_string(),
                workspace_skip_setup: "workspace_skip_setup".to_string(),
                workspace_coderabbit_cli_status: "workspace_coderabbit_cli_status".to_string(),
                workspace_coderabbit_logout: "workspace_coderabbit_logout".to_string(),
                workspace_coderabbit_doctor: "workspace_coderabbit_doctor".to_string(),
                workspace_coderabbit_diff_fingerprint: "workspace_coderabbit_diff_fingerprint"
                    .to_string(),
                workspace_coderabbit_review: "workspace_coderabbit_review".to_string(),
                workspace_coderabbit_review_start: "workspace_coderabbit_review_start".to_string(),
                workspace_coderabbit_review_job: "workspace_coderabbit_review_job".to_string(),
                workspace_coderabbit_review_cancel: "workspace_coderabbit_review_cancel"
                    .to_string(),
                workspace_coderabbit_review_load: "workspace_coderabbit_review_load".to_string(),
                workspace_coderabbit_review_save: "workspace_coderabbit_review_save".to_string(),
                workspace_coderabbit_review_history: "workspace_coderabbit_review_history"
                    .to_string(),
                workspace_coderabbit_review_clear: "workspace_coderabbit_review_clear".to_string(),
            },
        );

    let builder = builder.constant(
        "SESSION_METHODS",
        SessionMethods {
            start_thread: "start_thread".to_string(),
            run_pull_request_review_agent: "run_pull_request_review_agent".to_string(),
            apply_task_title: "apply_task_title".to_string(),
            prepare_turn: "prepare_turn".to_string(),
            send_turn: "send_turn".to_string(),
            steer_turn: "steer_turn".to_string(),
            steer_native_subagent: "steer_native_subagent".to_string(),
            interrupt_native_subagent: "interrupt_native_subagent".to_string(),
            queue_turn: "queue_turn".to_string(),
            list_turn_queue: "list_turn_queue".to_string(),
            remove_queued_turn: "remove_queued_turn".to_string(),
            reorder_turn_queue: "reorder_turn_queue".to_string(),
            dispatch_next_queued_turn: "dispatch_next_queued_turn".to_string(),
            approve_plan: "approve_plan".to_string(),
            record_plan_handoff: "record_plan_handoff".to_string(),
            abort_run: "abort_run".to_string(),
            resume_session: "resume_session".to_string(),
            close_session: "close_session".to_string(),
            restore_session: "restore_session".to_string(),
            list_thread_events: "list_thread_events".to_string(),
            list_mcp_runtime_statuses: "list_mcp_runtime_statuses".to_string(),
            start_mcp_oauth: "start_mcp_oauth".to_string(),
            wait_mcp_oauth: "wait_mcp_oauth".to_string(),
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
            provider_account_usage: "provider_account_usage".to_string(),
        },
    );

    let builder = builder.constant(
        "MCP_METHODS",
        McpMethods {
            list_mcp_integrations: "list_mcp_integrations".to_string(),
            create_mcp_integration: "create_mcp_integration".to_string(),
            activate_mcp_integration: "activate_mcp_integration".to_string(),
            disable_mcp_integration: "disable_mcp_integration".to_string(),
            remove_mcp_integration: "remove_mcp_integration".to_string(),
            disconnect_mcp_oauth: "disconnect_mcp_oauth".to_string(),
            set_mcp_tool_policy: "set_mcp_tool_policy".to_string(),
        },
    );

    let builder = builder.constant(
        "DELEGATION_METHODS",
        DelegationMethods {
            create_delegation: "create_delegation".to_string(),
            list_delegations: "list_delegations".to_string(),
            get_delegation: "get_delegation".to_string(),
            cancel_delegation: "cancel_delegation".to_string(),
            start_delegation: "start_delegation".to_string(),
            complete_delegation: "complete_delegation".to_string(),
            approve_delegation: "approve_delegation".to_string(),
            fail_delegation: "fail_delegation".to_string(),
        },
    );

    builder
        .export(Typescript::default(), &output_path)
        .expect("failed to export DCC contracts");

    let generated =
        fs::read_to_string(&output_path).expect("failed to read generated DCC contracts");
    let normalized = generated
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(
        &output_path,
        format!("{}\n", normalized.trim_end_matches('\n')),
    )
    .expect("failed to normalize generated DCC contracts");
}

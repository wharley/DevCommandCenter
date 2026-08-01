use tauri::State;

use dcc_tauri::{
    commands::forge_commands::{
        ForgeCliAccountsInput, ForgeCliAccountsOutput, ForgeCliHostsInput, ForgeCliHostsOutput,
        ForgeCliSelectLoginInput, ForgeCliStatusInput, ForgeCliStatusOutput, GithubCliStatusInput,
        GithubCliStatusOutput, PullRequestHubCommentInput, PullRequestHubCommentOutput,
        PullRequestHubDetailInput, PullRequestHubDetailOutput, PullRequestHubListInput,
        PullRequestHubListOutput, PullRequestHubSubmitReviewInput,
        PullRequestHubSubmitReviewOutput, WorkspaceForgeContextInput, WorkspaceForgeContextOutput,
        WorkspacePipelineJobInput, WorkspacePipelineJobLogOutput, WorkspacePipelineStatusInput,
        WorkspacePipelineStatusOutput, WorkspacePrReviewCommentsInput,
        WorkspacePrReviewCommentsOutput, WorkspacePrStatusInput, WorkspacePrStatusOutput,
        WorkspaceReviewStateInput, WorkspaceReviewStateOutput,
    },
    commands::workspace_commands::{RepositoryIdInput, WorkspaceGitPushInput},
    state::WorkspaceCommandState,
};

#[tauri::command]
pub async fn pull_request_hub_list(
    state: State<'_, WorkspaceCommandState>,
    input: PullRequestHubListInput,
) -> Result<PullRequestHubListOutput, String> {
    dcc_tauri::commands::forge_commands::pull_request_hub_list(state, input).await
}

#[tauri::command]
pub async fn pull_request_hub_detail(
    state: State<'_, WorkspaceCommandState>,
    input: PullRequestHubDetailInput,
) -> Result<PullRequestHubDetailOutput, String> {
    dcc_tauri::commands::forge_commands::pull_request_hub_detail(state, input).await
}

#[tauri::command]
pub async fn pull_request_hub_comment(
    state: State<'_, WorkspaceCommandState>,
    input: PullRequestHubCommentInput,
) -> Result<PullRequestHubCommentOutput, String> {
    dcc_tauri::commands::forge_commands::pull_request_hub_comment(state, input).await
}

#[tauri::command]
pub async fn pull_request_hub_submit_review(
    state: State<'_, WorkspaceCommandState>,
    input: PullRequestHubSubmitReviewInput,
) -> Result<PullRequestHubSubmitReviewOutput, String> {
    dcc_tauri::commands::forge_commands::pull_request_hub_submit_review(state, input).await
}

#[tauri::command]
pub async fn pull_request_hub_reply_thread(
    state: State<'_, WorkspaceCommandState>,
    input: dcc_tauri::commands::forge_commands::PullRequestHubThreadReplyInput,
) -> Result<dcc_tauri::commands::forge_commands::PullRequestHubThreadReplyOutput, String> {
    dcc_tauri::commands::forge_commands::pull_request_hub_reply_thread(state, input).await
}

#[tauri::command]
pub async fn pull_request_hub_resolve_thread(
    state: State<'_, WorkspaceCommandState>,
    input: dcc_tauri::commands::forge_commands::PullRequestHubThreadResolveInput,
) -> Result<dcc_tauri::commands::forge_commands::PullRequestHubThreadResolveOutput, String> {
    dcc_tauri::commands::forge_commands::pull_request_hub_resolve_thread(state, input).await
}

#[tauri::command]
pub async fn workspace_github_cli_status(
    state: State<'_, WorkspaceCommandState>,
    input: GithubCliStatusInput,
) -> Result<GithubCliStatusOutput, String> {
    dcc_tauri::commands::forge_commands::workspace_github_cli_status(state, input).await
}

#[tauri::command]
pub async fn workspace_forge_cli_status(
    state: State<'_, WorkspaceCommandState>,
    input: ForgeCliStatusInput,
) -> Result<ForgeCliStatusOutput, String> {
    dcc_tauri::commands::forge_commands::workspace_forge_cli_status(state, input).await
}

#[tauri::command]
pub async fn workspace_forge_cli_accounts(
    state: State<'_, WorkspaceCommandState>,
    input: ForgeCliAccountsInput,
) -> Result<ForgeCliAccountsOutput, String> {
    dcc_tauri::commands::forge_commands::workspace_forge_cli_accounts(state, input).await
}

#[tauri::command]
pub async fn workspace_forge_cli_hosts(
    state: State<'_, WorkspaceCommandState>,
    input: ForgeCliHostsInput,
) -> Result<ForgeCliHostsOutput, String> {
    dcc_tauri::commands::forge_commands::workspace_forge_cli_hosts(state, input).await
}

#[tauri::command]
pub async fn workspace_forge_cli_select_login(
    state: State<'_, WorkspaceCommandState>,
    input: ForgeCliSelectLoginInput,
) -> Result<(), String> {
    dcc_tauri::commands::forge_commands::workspace_forge_cli_select_login(state, input).await
}

#[tauri::command]
pub async fn workspace_backfill_forge_repo_bindings(
    state: State<'_, WorkspaceCommandState>,
) -> Result<usize, String> {
    dcc_tauri::commands::forge_commands::workspace_backfill_forge_repo_bindings(state).await
}

#[tauri::command]
pub async fn workspace_retry_repository_forge_binding(
    state: State<'_, WorkspaceCommandState>,
    input: RepositoryIdInput,
) -> Result<Option<String>, String> {
    dcc_tauri::commands::forge_commands::workspace_retry_repository_forge_binding(state, input)
        .await
}

#[tauri::command]
pub async fn workspace_forge_context(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceForgeContextInput,
) -> Result<WorkspaceForgeContextOutput, String> {
    dcc_tauri::commands::forge_commands::workspace_forge_context(state, input).await
}

#[tauri::command]
pub async fn workspace_change_request_view_web(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceGitPushInput,
) -> Result<(), String> {
    dcc_tauri::commands::forge_commands::workspace_change_request_view_web(state, input).await
}

#[tauri::command]
pub async fn workspace_change_request_create(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceGitPushInput,
) -> Result<(), String> {
    dcc_tauri::commands::forge_commands::workspace_change_request_create(state, input).await
}

#[tauri::command]
pub async fn workspace_change_request_merge(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceGitPushInput,
) -> Result<(), String> {
    dcc_tauri::commands::forge_commands::workspace_change_request_merge(state, input).await
}

#[tauri::command]
pub async fn workspace_gh_pr_view_web(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceGitPushInput,
) -> Result<(), String> {
    dcc_tauri::commands::forge_commands::workspace_gh_pr_view_web(state, input).await
}

#[tauri::command]
pub async fn workspace_gh_pr_create_fill(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceGitPushInput,
) -> Result<(), String> {
    dcc_tauri::commands::forge_commands::workspace_gh_pr_create_fill(state, input).await
}

#[tauri::command]
pub async fn workspace_gh_pr_merge(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceGitPushInput,
) -> Result<(), String> {
    dcc_tauri::commands::forge_commands::workspace_gh_pr_merge(state, input).await
}

#[tauri::command]
pub async fn workspace_pr_status(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspacePrStatusInput,
) -> Result<WorkspacePrStatusOutput, String> {
    dcc_tauri::commands::forge_commands::workspace_pr_status(state, input).await
}

#[tauri::command]
pub async fn workspace_pr_review_comments(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspacePrReviewCommentsInput,
) -> Result<WorkspacePrReviewCommentsOutput, String> {
    dcc_tauri::commands::forge_commands::workspace_pr_review_comments(state, input).await
}

#[tauri::command]
pub async fn workspace_pipeline_status(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspacePipelineStatusInput,
) -> Result<WorkspacePipelineStatusOutput, String> {
    dcc_tauri::commands::forge_commands::workspace_pipeline_status(state, input).await
}

#[tauri::command]
pub async fn workspace_pipeline_job_log(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspacePipelineJobInput,
) -> Result<WorkspacePipelineJobLogOutput, String> {
    dcc_tauri::commands::forge_commands::workspace_pipeline_job_log(state, input).await
}

#[tauri::command]
pub async fn workspace_pipeline_job_retry(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspacePipelineJobInput,
) -> Result<(), String> {
    dcc_tauri::commands::forge_commands::workspace_pipeline_job_retry(state, input).await
}

#[tauri::command]
pub async fn workspace_review_state(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceReviewStateInput,
) -> Result<WorkspaceReviewStateOutput, String> {
    dcc_tauri::commands::forge_commands::workspace_review_state(state, input).await
}

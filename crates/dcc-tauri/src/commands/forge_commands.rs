use serde::{Deserialize, Serialize};
use serde_json::Value;
use specta::Type;
use tauri::State;

use dcc_core::domain::workspace::{WorkspaceSource, WorkspaceSourceKind};
use dcc_core::ports::{RepositoryRepo, WorkspaceRepo};
use dcc_infra::db::{SqliteSessionRepo, SqliteWorkspaceRepo};

use crate::{
    commands::forge::context as forge_context,
    commands::forge::provider as forge_provider,
    commands::workspace_commands::{
        complete_repository_forge_binding_retry, push_current_branch, RepositoryIdInput,
        WorkspaceChangeRequestContextInput, WorkspaceChangeRequestContextOutput,
        WorkspaceChangeRequestCreateInput, WorkspaceGitPushInput,
    },
    commands::workspace_support::{
        ensure_pushable_branch, find_workspace_by_root, preferred_workspace_branch_name,
        preflight_workspace_root, resolve_branch_diff_base, resolve_current_branch_name,
        resolve_current_commit_sha, resolve_workspace_target_branch, workspace_branch_hints,
    },
    delivery_failure::{
        capture_workspace_delivery_failure, clear_workspace_delivery_failure,
        CaptureDeliveryFailureOptions, WorkspaceDeliveryFailureOperation,
    },
    state::WorkspaceCommandState,
};

async fn imported_workspace_source(
    state: &WorkspaceCommandState,
    root: &str,
) -> Result<Option<WorkspaceSource>, String> {
    let repo = SqliteWorkspaceRepo::open(&state.db_path).map_err(|error| error.to_string())?;
    Ok(find_workspace_by_root(&repo, root)
        .await?
        .and_then(|workspace| workspace.source))
}

fn is_provisional_task_title(title: &str) -> bool {
    let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
    title.is_empty()
        || title.eq_ignore_ascii_case("new task")
        || title.eq_ignore_ascii_case("nova tarefa")
}

fn preferred_change_request_title(
    workspace_name: Option<&str>,
    thread_titles: impl IntoIterator<Item = String>,
) -> Option<String> {
    if let Some(title) = workspace_name
        .map(str::trim)
        .filter(|title| !is_provisional_task_title(title))
    {
        return Some(title.to_string());
    }
    thread_titles.into_iter().find_map(|title| {
        let title = title.trim();
        (!is_provisional_task_title(title) && !title.eq_ignore_ascii_case("new session"))
            .then(|| title.to_string())
    })
}

async fn workspace_change_request_title(
    state: &WorkspaceCommandState,
    root: &str,
) -> Result<Option<String>, String> {
    let workspace_repo =
        SqliteWorkspaceRepo::open(&state.db_path).map_err(|error| error.to_string())?;
    let Some(workspace) = find_workspace_by_root(&workspace_repo, root).await? else {
        return Ok(None);
    };
    let session_repo =
        SqliteSessionRepo::open(&state.db_path).map_err(|error| error.to_string())?;
    let sessions = session_repo
        .list_workspace_sessions(&workspace.id)
        .map_err(|error| error.to_string())?;
    Ok(preferred_change_request_title(
        workspace.name.as_deref(),
        sessions.into_iter().map(|summary| summary.thread.title),
    ))
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GithubCliStatusInput {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum ForgeCliProvider {
    Github,
    Gitlab,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ForgeCliStatusInput {
    pub provider: ForgeCliProvider,
    pub host: Option<String>,
    pub force_refresh: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ForgeCliAccountsInput {
    pub provider: ForgeCliProvider,
    pub host: Option<String>,
    pub force_refresh: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ForgeCliHostsInput {
    pub provider: ForgeCliProvider,
    pub force_refresh: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ForgeCliSelectLoginInput {
    pub provider: ForgeCliProvider,
    pub host: Option<String>,
    pub login: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum ForgeCliStatusState {
    Ready,
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum WorkspaceForgeRemoteState {
    Ok,
    Unauthenticated,
    Unavailable,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ForgeCliStatusOutput {
    pub provider: ForgeCliProvider,
    pub cli_name: String,
    pub hostname: String,
    pub status: ForgeCliStatusState,
    pub login: Option<String>,
    pub selected_login: Option<String>,
    pub logins: Vec<String>,
    pub message: String,
    pub login_command: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ForgeCliAccountEntry {
    pub login: String,
    pub name: Option<String>,
    pub avatar_url: Option<String>,
    pub email: Option<String>,
    pub active: bool,
    pub selected: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ForgeCliAccountsOutput {
    pub provider: ForgeCliProvider,
    pub cli_name: String,
    pub hostname: String,
    pub status: ForgeCliStatusState,
    pub login: Option<String>,
    pub selected_login: Option<String>,
    pub accounts: Vec<ForgeCliAccountEntry>,
    pub message: String,
    pub login_command: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ForgeCliHostsOutput {
    pub provider: ForgeCliProvider,
    pub cli_name: String,
    pub hosts: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum GithubCliStatusState {
    Ready,
    Error,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GithubCliStatusOutput {
    pub cli_name: String,
    pub hostname: String,
    pub status: GithubCliStatusState,
    pub login: Option<String>,
    pub message: String,
    pub login_command: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspacePrStatusInput {
    pub workspace_root: String,
    pub branch: Option<String>,
    pub forge_login: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspacePrReviewCommentsInput {
    pub workspace_root: String,
    pub branch: Option<String>,
    pub forge_login: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestHubListInput {}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestHubActor {
    pub login: String,
    pub name: Option<String>,
    pub avatar_url: Option<String>,
    pub html_url: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestHubCheck {
    pub name: String,
    pub state: String,
    pub details_url: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestHubFile {
    pub path: String,
    pub previous_path: Option<String>,
    pub status: String,
    pub additions: u64,
    pub deletions: u64,
    pub patch: Option<String>,
    pub blob_url: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestHubInlineComment {
    pub id: String,
    pub thread_id: Option<String>,
    pub path: String,
    pub line: Option<u32>,
    pub side: Option<String>,
    pub body: String,
    pub author: Option<PullRequestHubActor>,
    pub created_at: Option<String>,
    pub url: Option<String>,
    pub resolved: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestHubReviewCapabilities {
    pub inline_comments: bool,
    pub approve: bool,
    pub request_changes: bool,
    pub reply_to_threads: bool,
    pub resolve_threads: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestHubComment {
    pub id: String,
    pub body: String,
    pub author: Option<PullRequestHubActor>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub url: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestHubItem {
    pub id: String,
    pub provider: String,
    pub host: String,
    pub repository_id: String,
    pub project_id: String,
    pub repository_name: String,
    pub repository_root: String,
    pub forge_login: Option<String>,
    pub number: u32,
    pub title: String,
    pub body: Option<String>,
    pub url: String,
    pub author: Option<PullRequestHubActor>,
    pub head_branch: String,
    pub base_branch: String,
    pub state: String,
    pub is_draft: bool,
    pub review_decision: Option<String>,
    pub review_requested_for_viewer: bool,
    pub created_by_viewer: bool,
    pub reviewers: Vec<PullRequestHubActor>,
    pub additions: Option<u64>,
    pub deletions: Option<u64>,
    pub changed_files: Option<u64>,
    pub comment_count: u64,
    pub checks_state: String,
    pub updated_at: Option<String>,
    pub linked_workspace_id: Option<String>,
    pub linked_workspace_name: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestHubWarning {
    pub repository_name: String,
    pub repository_root: String,
    pub message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestHubListOutput {
    pub items: Vec<PullRequestHubItem>,
    pub warnings: Vec<PullRequestHubWarning>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestHubDetailInput {
    pub repository_root: String,
    pub number: u32,
    pub forge_login: Option<String>,
    pub include_code: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestHubDetailOutput {
    pub body: Option<String>,
    pub comments: Vec<PullRequestHubComment>,
    pub checks: Vec<PullRequestHubCheck>,
    pub files: Vec<PullRequestHubFile>,
    pub inline_comments: Vec<PullRequestHubInlineComment>,
    pub review_capabilities: PullRequestHubReviewCapabilities,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestHubCommentInput {
    pub repository_root: String,
    pub number: u32,
    pub body: String,
    pub forge_login: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestHubCommentOutput {
    pub comment: PullRequestHubComment,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestHubThreadReplyInput {
    pub repository_root: String,
    pub number: u32,
    pub comment_id: String,
    pub thread_id: String,
    pub body: String,
    pub forge_login: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestHubThreadReplyOutput {
    pub submitted: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestHubThreadResolveInput {
    pub repository_root: String,
    pub number: u32,
    pub thread_id: String,
    pub resolved: bool,
    pub forge_login: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestHubThreadResolveOutput {
    pub resolved: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum PullRequestHubReviewEvent {
    Comment,
    Approve,
    RequestChanges,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestHubDraftComment {
    pub path: String,
    pub body: String,
    pub line: u32,
    pub side: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestHubSubmitReviewInput {
    pub repository_root: String,
    pub number: u32,
    pub body: Option<String>,
    pub event: PullRequestHubReviewEvent,
    pub comments: Vec<PullRequestHubDraftComment>,
    pub forge_login: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestHubSubmitReviewOutput {
    pub submitted: bool,
    pub url: Option<String>,
    pub submitted_comment_count: u32,
    pub body_submitted: bool,
    pub decision_submitted: bool,
    pub warning: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspacePrStatusOutput {
    pub provider: Option<String>,
    pub host: Option<String>,
    pub number: Option<u32>,
    pub title: Option<String>,
    pub url: Option<String>,
    pub head_branch: Option<String>,
    pub base_branch: Option<String>,
    pub state: Option<String>,
    pub is_draft: bool,
    pub mergeable: Option<String>,
    pub merge_state_status: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceReviewStateInput {
    pub workspace_root: String,
    pub branch: Option<String>,
    pub forge_login: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceReviewer {
    pub login: Option<String>,
    pub name: Option<String>,
    pub avatar_url: Option<String>,
    pub html_url: Option<String>,
    pub state: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceReviewStateOutput {
    pub provider: Option<String>,
    pub host: Option<String>,
    pub number: Option<u32>,
    pub url: Option<String>,
    pub review_state: Option<String>,
    pub reviewers: Vec<WorkspaceReviewer>,
    pub reviewers_available: bool,
    pub approvals_available: bool,
    pub approvals_required: Option<u32>,
    pub approvals_received: u32,
    pub approvals_left: Option<u32>,
    pub approved: Option<bool>,
    pub mergeable: Option<String>,
    pub merge_state_status: Option<String>,
    pub has_conflicts: Option<bool>,
    pub behind_by: Option<u32>,
    pub discussions_resolved: Option<bool>,
    pub draft: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspacePrReviewCommentAuthor {
    pub login: Option<String>,
    pub avatar_url: Option<String>,
    pub html_url: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspacePrReviewComment {
    pub id: String,
    pub parent_id: Option<String>,
    pub thread_id: String,
    pub path: String,
    pub body: String,
    pub diff_hunk: Option<String>,
    pub html_url: Option<String>,
    pub side: Option<String>,
    pub line: Option<i64>,
    pub start_line: Option<i64>,
    pub original_line: Option<i64>,
    pub original_start_line: Option<i64>,
    pub author: Option<WorkspacePrReviewCommentAuthor>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub resolvable: Option<bool>,
    pub resolved: Option<bool>,
    pub outdated: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspacePrReviewCommentsOutput {
    pub provider: Option<String>,
    pub host: Option<String>,
    pub number: Option<u32>,
    pub url: Option<String>,
    pub comments: Vec<WorkspacePrReviewComment>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspacePipelineStatusInput {
    pub workspace_root: String,
    pub forge_login: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspacePipelineJobInput {
    pub workspace_root: String,
    pub pipeline_id: u64,
    pub job_id: u64,
    pub forge_login: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspacePipelineJob {
    pub id: u64,
    pub name: String,
    pub stage: String,
    pub status: String,
    pub duration: Option<f64>,
    pub queued_duration: Option<f64>,
    pub web_url: Option<String>,
    pub allow_failure: bool,
    pub archived: bool,
    pub retryable: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspacePipeline {
    pub id: u64,
    pub sha: String,
    pub ref_name: Option<String>,
    pub status: String,
    pub source: Option<String>,
    pub web_url: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub duration: Option<f64>,
    pub queued_duration: Option<f64>,
    pub jobs: Vec<WorkspacePipelineJob>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspacePipelineStatusOutput {
    pub provider: Option<String>,
    pub host: Option<String>,
    pub change_request_number: Option<u32>,
    pub head_sha: Option<String>,
    pub pipeline: Option<WorkspacePipeline>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspacePipelineJobLogOutput {
    pub job_id: u64,
    pub content: String,
    pub truncated: bool,
    pub loaded_bytes: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceForgeContextInput {
    pub workspace_root: String,
    pub forge_login: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceForgeContextOutput {
    pub provider: Option<ForgeCliProvider>,
    pub host: Option<String>,
    pub remote_name: Option<String>,
    pub namespace: Option<String>,
    pub repo: Option<String>,
    pub cli_name: Option<String>,
    pub status: Option<ForgeCliStatusState>,
    pub remote_state: Option<WorkspaceForgeRemoteState>,
    pub bound_login: Option<String>,
    pub login: Option<String>,
    pub selected_login: Option<String>,
    pub effective_login: Option<String>,
    pub known_hosts: Vec<String>,
    pub message: Option<String>,
    pub login_command: Option<String>,
}

fn empty_workspace_forge_context() -> WorkspaceForgeContextOutput {
    WorkspaceForgeContextOutput {
        provider: None,
        host: None,
        remote_name: None,
        namespace: None,
        repo: None,
        cli_name: None,
        status: None,
        remote_state: None,
        bound_login: None,
        login: None,
        selected_login: None,
        effective_login: None,
        known_hosts: Vec::new(),
        message: None,
        login_command: None,
    }
}

fn legacy_github_cli_status(output: ForgeCliStatusOutput) -> GithubCliStatusOutput {
    GithubCliStatusOutput {
        cli_name: output.cli_name,
        hostname: output.hostname,
        status: match output.status {
            ForgeCliStatusState::Ready => GithubCliStatusState::Ready,
            ForgeCliStatusState::Error => GithubCliStatusState::Error,
        },
        login: output.login,
        message: output.message,
        login_command: output.login_command,
    }
}

fn resolve_forge_cli_snapshot(
    state: &WorkspaceCommandState,
    provider: ForgeCliProvider,
    host: &str,
    force_refresh: bool,
) -> Result<ForgeCliStatusOutput, String> {
    let status =
        forge_provider::resolve_forge_cli_status_with_options(provider, host, force_refresh)?;
    let selected_login =
        forge_context::resolve_selected_forge_login(&state.db_path, provider, host, &status)?;

    Ok(ForgeCliStatusOutput {
        provider: status.provider,
        cli_name: status.cli_name,
        hostname: status.hostname,
        status: if status.ready {
            ForgeCliStatusState::Ready
        } else {
            ForgeCliStatusState::Error
        },
        login: status.login,
        selected_login,
        logins: status.logins,
        message: status.message,
        login_command: status.login_command,
    })
}

#[tauri::command]
pub async fn workspace_forge_cli_status(
    state: State<'_, WorkspaceCommandState>,
    input: ForgeCliStatusInput,
) -> Result<ForgeCliStatusOutput, String> {
    let host = forge_context::normalize_forge_host(input.provider, input.host)?;
    resolve_forge_cli_snapshot(
        &state,
        input.provider,
        &host,
        input.force_refresh.unwrap_or(false),
    )
}

#[tauri::command]
pub async fn workspace_forge_cli_accounts(
    state: State<'_, WorkspaceCommandState>,
    input: ForgeCliAccountsInput,
) -> Result<ForgeCliAccountsOutput, String> {
    let host = forge_context::normalize_forge_host(input.provider, input.host)?;
    let snapshot = resolve_forge_cli_snapshot(
        &state,
        input.provider,
        &host,
        input.force_refresh.unwrap_or(false),
    )?;
    let selected_login = snapshot.selected_login.clone();
    let active_login = snapshot.login.clone();
    let backend = crate::commands::forge::accounts::backend_for(input.provider);
    let profiles = backend.list_accounts(&host, input.force_refresh.unwrap_or(false))?;
    let accounts = profiles
        .iter()
        .map(|account| ForgeCliAccountEntry {
            login: account.login.clone(),
            name: account.name.clone(),
            avatar_url: account.avatar_url.clone(),
            email: account.email.clone(),
            active: account.active || active_login.as_deref() == Some(account.login.as_str()),
            selected: selected_login.as_deref() == Some(account.login.as_str()),
        })
        .collect();

    Ok(ForgeCliAccountsOutput {
        provider: snapshot.provider,
        cli_name: snapshot.cli_name,
        hostname: snapshot.hostname,
        status: snapshot.status,
        login: active_login,
        selected_login,
        accounts,
        message: snapshot.message,
        login_command: snapshot.login_command,
    })
}

#[tauri::command]
pub async fn workspace_forge_cli_hosts(
    _state: State<'_, WorkspaceCommandState>,
    input: ForgeCliHostsInput,
) -> Result<ForgeCliHostsOutput, String> {
    let backend = crate::commands::forge::accounts::backend_for(input.provider);
    let hosts = backend.list_hosts(input.force_refresh.unwrap_or(false))?;
    Ok(ForgeCliHostsOutput {
        provider: input.provider,
        cli_name: backend.cli_name().to_string(),
        hosts,
    })
}

#[tauri::command]
pub async fn workspace_forge_cli_select_login(
    state: State<'_, WorkspaceCommandState>,
    input: ForgeCliSelectLoginInput,
) -> Result<(), String> {
    let host = forge_context::normalize_forge_host(input.provider, input.host)?;
    let repo = SqliteWorkspaceRepo::open(&state.db_path).map_err(|error| error.to_string())?;
    repo.set_forge_login_preference(
        forge_context::forge_provider_key(input.provider),
        &host,
        input.login.as_deref(),
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn workspace_backfill_forge_repo_bindings(
    state: State<'_, WorkspaceCommandState>,
) -> Result<usize, String> {
    let repo = SqliteWorkspaceRepo::open(&state.db_path).map_err(|error| error.to_string())?;
    let summary = crate::commands::forge::accounts::backfill_repository_bindings(&repo)?;
    Ok(summary.bound)
}

#[tauri::command]
pub async fn workspace_retry_repository_forge_binding(
    state: State<'_, WorkspaceCommandState>,
    input: RepositoryIdInput,
) -> Result<Option<String>, String> {
    let repo = SqliteWorkspaceRepo::open(&state.db_path).map_err(|error| error.to_string())?;
    let repository_id = dcc_core::domain::repository::RepositoryId(input.repository_id);
    let login = crate::commands::forge::accounts::auto_bind_repository(&repo, &repository_id)?;
    if login.is_some() {
        complete_repository_forge_binding_retry(&repo, &repository_id).await?;
    }
    Ok(login)
}

#[tauri::command]
pub async fn workspace_github_cli_status(
    state: State<'_, WorkspaceCommandState>,
    _input: GithubCliStatusInput,
) -> Result<GithubCliStatusOutput, String> {
    let output = workspace_forge_cli_status(
        state,
        ForgeCliStatusInput {
            provider: ForgeCliProvider::Github,
            host: Some("github.com".to_string()),
            force_refresh: None,
        },
    )
    .await?;
    Ok(legacy_github_cli_status(output))
}

#[tauri::command]
pub async fn workspace_forge_context(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceForgeContextInput,
) -> Result<WorkspaceForgeContextOutput, String> {
    preflight_workspace_root(&state, &input.workspace_root).await?;

    let root = input.workspace_root.trim();
    if root.is_empty() {
        return Ok(empty_workspace_forge_context());
    }

    let Some(context) = forge_context::resolve_workspace_forge_context(
        &state.db_path,
        root,
        input.forge_login.as_deref(),
    )?
    else {
        return Ok(empty_workspace_forge_context());
    };

    Ok(WorkspaceForgeContextOutput {
        provider: Some(context.provider),
        host: Some(context.host),
        remote_name: Some(context.remote_name),
        namespace: Some(context.namespace),
        repo: Some(context.repo),
        cli_name: Some(context.cli_name),
        status: Some(if context.ready {
            ForgeCliStatusState::Ready
        } else {
            ForgeCliStatusState::Error
        }),
        remote_state: Some(match context.remote_state {
            forge_context::WorkspaceForgeRemoteState::Ok => WorkspaceForgeRemoteState::Ok,
            forge_context::WorkspaceForgeRemoteState::Unauthenticated => {
                WorkspaceForgeRemoteState::Unauthenticated
            }
            forge_context::WorkspaceForgeRemoteState::Unavailable => {
                WorkspaceForgeRemoteState::Unavailable
            }
        }),
        bound_login: context.bound_login,
        login: context.login,
        selected_login: context.selected_login,
        effective_login: context.effective_login,
        known_hosts: context.known_hosts,
        message: Some(context.message),
        login_command: Some(context.login_command),
    })
}

#[tauri::command]
pub async fn workspace_change_request_view_web(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceGitPushInput,
) -> Result<(), String> {
    preflight_workspace_root(&state, &input.workspace_root).await?;
    let root = input.workspace_root.trim();
    if root.is_empty() {
        return Err("workspace_root is empty".to_string());
    }
    let login = forge_context::resolve_workspace_forge_context(
        &state.db_path,
        root,
        input.forge_login.as_deref(),
    )?
    .and_then(|context| context.effective_login);
    if let Some(source) = imported_workspace_source(&state, root).await? {
        if source.kind == WorkspaceSourceKind::PullRequest {
            return forge_provider::view_workspace_change_request_source(
                root,
                &source.url,
                source.change_request_number,
                login.as_deref(),
            );
        }
    }
    forge_provider::view_workspace_change_request(root, login.as_deref())
}

#[tauri::command]
pub async fn workspace_change_request_merge(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceGitPushInput,
) -> Result<(), String> {
    preflight_workspace_root(&state, &input.workspace_root).await?;
    let root = input.workspace_root.trim();
    if root.is_empty() {
        return Err("workspace_root is empty".to_string());
    }
    let source = imported_workspace_source(&state, root).await?;
    let branch = match source.as_ref() {
        Some(source) => source.head_branch.clone(),
        None => resolve_current_branch_name(root)?,
    };
    let login = forge_context::resolve_workspace_forge_context(
        &state.db_path,
        root,
        input.forge_login.as_deref(),
    )?
    .and_then(|context| context.effective_login);
    if let Some(source) = source {
        if source.kind == WorkspaceSourceKind::PullRequest {
            return forge_provider::merge_workspace_change_request_source(
                root,
                &source.url,
                &branch,
                login.as_deref(),
            );
        }
    }
    forge_provider::merge_workspace_change_request(root, &branch, login.as_deref())
}

#[tauri::command]
pub async fn workspace_change_request_create(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceChangeRequestCreateInput,
) -> Result<(), String> {
    preflight_workspace_root(&state, &input.workspace_root).await?;
    let root = input.workspace_root.trim();
    if root.is_empty() {
        return Err("workspace_root is empty".to_string());
    }

    let source = imported_workspace_source(&state, root).await?;
    let protected_branch = resolve_workspace_target_branch(&state, root).await;
    let title = input
        .title
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or(workspace_change_request_title(&state, root).await?);
    let preferred_branch = preferred_workspace_branch_name(title.as_deref());
    let head_branch = if let Some(source) = source.as_ref() {
        source.head_branch.clone()
    } else {
        ensure_pushable_branch(
            root,
            protected_branch.as_deref(),
            preferred_branch.as_deref(),
        )?
    };
    let base_branch = if let Some(source) = source.as_ref() {
        source.base_branch.clone()
    } else {
        let base_ref = resolve_branch_diff_base(root, protected_branch.as_deref())
            .unwrap_or_else(|| "main".to_string());
        let base_stripped = base_ref
            .split_once('/')
            .map(|(_, branch)| branch)
            .unwrap_or(&base_ref);
        if base_stripped == "HEAD" {
            "main".to_string()
        } else {
            base_stripped.to_string()
        }
    };
    let login = forge_context::resolve_workspace_forge_context(
        &state.db_path,
        root,
        input.forge_login.as_deref(),
    )?
    .and_then(|context| context.effective_login);

    push_current_branch(&state, root, protected_branch.as_deref(), login.as_deref())
        .await
        .map_err(|error| format!("git push failed: {error}"))?;

    forge_provider::create_workspace_change_request(
        root,
        &base_branch,
        &head_branch,
        title.as_deref(),
        input.body.as_deref(),
        input.draft,
        login.as_deref(),
    )
}

#[tauri::command]
pub async fn workspace_change_request_context(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceChangeRequestContextInput,
) -> Result<WorkspaceChangeRequestContextOutput, String> {
    preflight_workspace_root(&state, &input.workspace_root).await?;
    let root = input.workspace_root.trim();
    if root.is_empty() {
        return Err("workspace_root is empty".to_string());
    }

    let source = imported_workspace_source(&state, root).await?;
    let protected_branch = resolve_workspace_target_branch(&state, root).await;
    let title = workspace_change_request_title(&state, root).await?;
    let preferred_branch = preferred_workspace_branch_name(title.as_deref());
    let head_branch = if let Some(source) = source.as_ref() {
        source.head_branch.clone()
    } else {
        ensure_pushable_branch(
            root,
            protected_branch.as_deref(),
            preferred_branch.as_deref(),
        )?
    };
    let base_branch = if let Some(source) = source.as_ref() {
        source.base_branch.clone()
    } else {
        let base_ref = resolve_branch_diff_base(root, protected_branch.as_deref())
            .unwrap_or_else(|| "main".to_string());
        let base_stripped = base_ref
            .split_once('/')
            .map(|(_, branch)| branch)
            .unwrap_or(&base_ref);
        if base_stripped == "HEAD" {
            "main".to_string()
        } else {
            base_stripped.to_string()
        }
    };
    let forge = forge_context::resolve_workspace_forge_context(
        &state.db_path,
        root,
        input.forge_login.as_deref(),
    )?;
    let provider = forge.as_ref().map(|context| match context.provider {
        ForgeCliProvider::Github => "github".to_string(),
        ForgeCliProvider::Gitlab => "gitlab".to_string(),
    });
    let request_label = if provider.as_deref() == Some("gitlab") {
        "MR"
    } else {
        "PR"
    };

    Ok(WorkspaceChangeRequestContextOutput {
        head_branch,
        base_branch,
        title,
        provider,
        request_label: request_label.to_string(),
    })
}

#[tauri::command]
pub async fn workspace_gh_pr_view_web(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceGitPushInput,
) -> Result<(), String> {
    workspace_change_request_view_web(state, input).await
}

#[tauri::command]
pub async fn workspace_gh_pr_merge(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceGitPushInput,
) -> Result<(), String> {
    workspace_change_request_merge(state, input).await
}

#[tauri::command]
pub async fn workspace_gh_pr_create_fill(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceGitPushInput,
) -> Result<(), String> {
    workspace_change_request_create(
        state,
        WorkspaceChangeRequestCreateInput {
            workspace_root: input.workspace_root,
            forge_login: input.forge_login,
            title: None,
            body: None,
            draft: false,
        },
    )
    .await
}

fn hub_actor(value: &Value, provider: ForgeCliProvider) -> Option<PullRequestHubActor> {
    let login_key = match provider {
        ForgeCliProvider::Github => "login",
        ForgeCliProvider::Gitlab => "username",
    };
    let login = value.get(login_key)?.as_str()?.trim();
    if login.is_empty() {
        return None;
    }
    Some(PullRequestHubActor {
        login: login.to_string(),
        name: value
            .get("name")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        avatar_url: value
            .get("avatarUrl")
            .or_else(|| value.get("avatar_url"))
            .and_then(Value::as_str)
            .map(ToString::to_string),
        html_url: value
            .get("url")
            .or_else(|| value.get("html_url"))
            .or_else(|| value.get("web_url"))
            .and_then(Value::as_str)
            .map(ToString::to_string),
    })
}

fn github_check_state(value: &Value) -> String {
    let conclusion = value
        .get("conclusion")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    let status = value
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if matches!(conclusion.as_str(), "success" | "neutral") {
        "success".to_string()
    } else if matches!(
        conclusion.as_str(),
        "failure" | "timed_out" | "cancelled" | "action_required"
    ) {
        "failure".to_string()
    } else if conclusion == "skipped" {
        "skipped".to_string()
    } else if matches!(
        status.as_str(),
        "queued" | "in_progress" | "pending" | "waiting"
    ) {
        "pending".to_string()
    } else {
        "unknown".to_string()
    }
}

fn aggregate_checks_state(states: impl Iterator<Item = String>) -> String {
    let states = states.collect::<Vec<_>>();
    if states.iter().any(|state| state == "failure") {
        "failure".to_string()
    } else if states.iter().any(|state| state == "pending") {
        "pending".to_string()
    } else if !states.is_empty()
        && states
            .iter()
            .all(|state| matches!(state.as_str(), "success" | "skipped"))
    {
        "success".to_string()
    } else {
        "unknown".to_string()
    }
}

fn github_checks(value: &Value) -> Vec<PullRequestHubCheck> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .map(|check| PullRequestHubCheck {
            name: check
                .get("name")
                .or_else(|| check.get("context"))
                .and_then(Value::as_str)
                .unwrap_or("Check")
                .to_string(),
            state: github_check_state(check),
            details_url: check
                .get("detailsUrl")
                .or_else(|| check.get("targetUrl"))
                .and_then(Value::as_str)
                .map(ToString::to_string),
        })
        .collect()
}

fn gitlab_pipeline_state(status: &str) -> String {
    match status.to_ascii_lowercase().as_str() {
        "success" => "success".to_string(),
        "failed" | "canceled" => "failure".to_string(),
        "pending" | "running" | "created" | "preparing" | "waiting_for_resource" | "scheduled" => {
            "pending".to_string()
        }
        "skipped" | "manual" => "skipped".to_string(),
        _ => "unknown".to_string(),
    }
}

fn linked_workspace_for_pr(
    workspaces: &[dcc_core::domain::workspace::Workspace],
    repository_root: &str,
    provider: &str,
    number: u32,
    head_branch: &str,
) -> (Option<String>, Option<String>) {
    workspaces
        .iter()
        .find(|workspace| {
            if workspace.root_path != repository_root {
                return false;
            }
            workspace.source.as_ref().is_some_and(|source| {
                (source.kind == WorkspaceSourceKind::PullRequest
                    && source.provider.eq_ignore_ascii_case(provider)
                    && source.change_request_number == Some(number))
                    || source.head_branch == head_branch
            })
        })
        .map(|workspace| (Some(workspace.id.0.clone()), workspace.name.clone()))
        .unwrap_or((None, None))
}

fn github_hub_item(
    raw: &Value,
    context: &forge_context::ResolvedWorkspaceForgeContext,
    repository: &dcc_core::domain::repository::Repository,
    workspaces: &[dcc_core::domain::workspace::Workspace],
) -> Option<PullRequestHubItem> {
    let number = raw.get("number")?.as_u64()? as u32;
    let head_branch = raw.get("headRefName")?.as_str()?.to_string();
    let login = context.effective_login.as_deref().unwrap_or_default();
    let author = raw
        .get("author")
        .and_then(|value| hub_actor(value, ForgeCliProvider::Github));
    let review_requests = raw
        .get("reviewRequests")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut reviewers = review_requests
        .iter()
        .filter_map(|value| hub_actor(value, ForgeCliProvider::Github))
        .collect::<Vec<_>>();
    for review in raw
        .get("latestReviews")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(actor) = review
            .get("author")
            .and_then(|value| hub_actor(value, ForgeCliProvider::Github))
        {
            if !reviewers
                .iter()
                .any(|candidate| candidate.login == actor.login)
            {
                reviewers.push(actor);
            }
        }
    }
    let checks = github_checks(raw.get("statusCheckRollup").unwrap_or(&Value::Null));
    let (linked_workspace_id, linked_workspace_name) = linked_workspace_for_pr(
        workspaces,
        &repository.root_path,
        "github",
        number,
        &head_branch,
    );
    Some(PullRequestHubItem {
        id: format!("github:{}:{}:{}", context.host, repository.id.0, number),
        provider: "github".to_string(),
        host: context.host.clone(),
        repository_id: repository.id.0.clone(),
        project_id: repository.project_id.0.clone(),
        repository_name: repository.name.clone(),
        repository_root: repository.root_path.clone(),
        forge_login: context.effective_login.clone(),
        number,
        title: raw
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("Pull request")
            .to_string(),
        body: raw
            .get("body")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        url: raw
            .get("url")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        created_by_viewer: author
            .as_ref()
            .is_some_and(|actor| actor.login.eq_ignore_ascii_case(login)),
        review_requested_for_viewer: review_requests.iter().any(|value| {
            value
                .get("login")
                .and_then(Value::as_str)
                .is_some_and(|candidate| candidate.eq_ignore_ascii_case(login))
        }),
        author,
        head_branch,
        base_branch: raw
            .get("baseRefName")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        state: raw
            .get("state")
            .and_then(Value::as_str)
            .unwrap_or("OPEN")
            .to_ascii_lowercase(),
        is_draft: raw.get("isDraft").and_then(Value::as_bool).unwrap_or(false),
        review_decision: raw
            .get("reviewDecision")
            .and_then(Value::as_str)
            .map(|value| value.to_ascii_lowercase()),
        reviewers,
        additions: raw.get("additions").and_then(Value::as_u64),
        deletions: raw.get("deletions").and_then(Value::as_u64),
        changed_files: raw.get("changedFiles").and_then(Value::as_u64),
        comment_count: raw
            .get("comments")
            .and_then(Value::as_array)
            .map(|comments| comments.len() as u64)
            .unwrap_or(0),
        checks_state: aggregate_checks_state(checks.into_iter().map(|check| check.state)),
        updated_at: raw
            .get("updatedAt")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        linked_workspace_id,
        linked_workspace_name,
    })
}

fn gitlab_hub_item(
    raw: &Value,
    context: &forge_context::ResolvedWorkspaceForgeContext,
    repository: &dcc_core::domain::repository::Repository,
    workspaces: &[dcc_core::domain::workspace::Workspace],
) -> Option<PullRequestHubItem> {
    let number = raw.get("iid")?.as_u64()? as u32;
    let head_branch = raw.get("source_branch")?.as_str()?.to_string();
    let login = context.effective_login.as_deref().unwrap_or_default();
    let author = raw
        .get("author")
        .and_then(|value| hub_actor(value, ForgeCliProvider::Gitlab));
    let reviewers = raw
        .get("reviewers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|value| hub_actor(value, ForgeCliProvider::Gitlab))
        .collect::<Vec<_>>();
    let checks_state = raw
        .get("head_pipeline")
        .and_then(|value| value.get("status"))
        .and_then(Value::as_str)
        .map(gitlab_pipeline_state)
        .unwrap_or_else(|| "unknown".to_string());
    let (linked_workspace_id, linked_workspace_name) = linked_workspace_for_pr(
        workspaces,
        &repository.root_path,
        "gitlab",
        number,
        &head_branch,
    );
    Some(PullRequestHubItem {
        id: format!("gitlab:{}:{}:{}", context.host, repository.id.0, number),
        provider: "gitlab".to_string(),
        host: context.host.clone(),
        repository_id: repository.id.0.clone(),
        project_id: repository.project_id.0.clone(),
        repository_name: repository.name.clone(),
        repository_root: repository.root_path.clone(),
        forge_login: context.effective_login.clone(),
        number,
        title: raw
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("Merge request")
            .to_string(),
        body: raw
            .get("description")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        url: raw
            .get("web_url")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        created_by_viewer: author
            .as_ref()
            .is_some_and(|actor| actor.login.eq_ignore_ascii_case(login)),
        review_requested_for_viewer: reviewers
            .iter()
            .any(|actor| actor.login.eq_ignore_ascii_case(login)),
        author,
        head_branch,
        base_branch: raw
            .get("target_branch")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        state: raw
            .get("state")
            .and_then(Value::as_str)
            .unwrap_or("opened")
            .to_string(),
        is_draft: raw
            .get("draft")
            .or_else(|| raw.get("work_in_progress"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
        review_decision: raw
            .get("detailed_merge_status")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        reviewers,
        additions: None,
        deletions: None,
        changed_files: raw
            .get("changes_count")
            .and_then(Value::as_str)
            .and_then(|value| value.parse().ok()),
        comment_count: raw
            .get("user_notes_count")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        checks_state,
        updated_at: raw
            .get("updated_at")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        linked_workspace_id,
        linked_workspace_name,
    })
}

#[tauri::command]
pub async fn pull_request_hub_list(
    state: State<'_, WorkspaceCommandState>,
    _input: PullRequestHubListInput,
) -> Result<PullRequestHubListOutput, String> {
    let repo = SqliteWorkspaceRepo::open(&state.db_path).map_err(|error| error.to_string())?;
    let repositories = repo
        .list_repositories()
        .await
        .map_err(|error| error.to_string())?;
    let workspaces = repo
        .list_workspaces()
        .await
        .map_err(|error| error.to_string())?;
    let mut items = Vec::new();
    let mut warnings = Vec::new();
    for repository in repositories {
        let root = repository.root_path.trim();
        if root.is_empty() {
            continue;
        }
        let result = (|| -> Result<Vec<PullRequestHubItem>, String> {
            let context = forge_context::resolve_workspace_forge_context(
                &state.db_path,
                root,
                repository.forge_login.as_deref(),
            )?
            .ok_or_else(|| "No GitHub or GitLab remote was found.".to_string())?;
            let raw = match context.provider {
                ForgeCliProvider::Github => {
                    crate::commands::forge::github::list_pull_requests_json(
                        root,
                        &context.host,
                        context.effective_login.as_deref(),
                    )?
                }
                ForgeCliProvider::Gitlab => {
                    crate::commands::forge::gitlab::list_merge_requests_json(
                        root,
                        &context.host,
                        &context.namespace,
                        &context.repo,
                        context.effective_login.as_deref(),
                    )?
                }
            };
            let values = raw
                .as_array()
                .ok_or_else(|| "Forge returned an unexpected pull request list.".to_string())?;
            Ok(values
                .iter()
                .filter_map(|value| match context.provider {
                    ForgeCliProvider::Github => {
                        github_hub_item(value, &context, &repository, &workspaces)
                    }
                    ForgeCliProvider::Gitlab => {
                        gitlab_hub_item(value, &context, &repository, &workspaces)
                    }
                })
                .collect())
        })();
        match result {
            Ok(mut repository_items) => items.append(&mut repository_items),
            Err(message) => warnings.push(PullRequestHubWarning {
                repository_name: repository.name.clone(),
                repository_root: repository.root_path.clone(),
                message,
            }),
        }
    }
    items.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    Ok(PullRequestHubListOutput { items, warnings })
}

fn github_comment(value: &Value) -> Option<PullRequestHubComment> {
    Some(PullRequestHubComment {
        id: value
            .get("id")?
            .as_str()
            .map(ToString::to_string)
            .or_else(|| {
                value
                    .get("id")
                    .and_then(Value::as_u64)
                    .map(|id| id.to_string())
            })
            .or_else(|| {
                value
                    .get("databaseId")
                    .and_then(Value::as_u64)
                    .map(|id| id.to_string())
            })?,
        body: value
            .get("body")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        author: value
            .get("author")
            .and_then(|actor| hub_actor(actor, ForgeCliProvider::Github))
            .or_else(|| {
                value
                    .get("user")
                    .and_then(|actor| hub_actor(actor, ForgeCliProvider::Github))
            }),
        created_at: value
            .get("createdAt")
            .or_else(|| value.get("created_at"))
            .and_then(Value::as_str)
            .map(ToString::to_string),
        updated_at: value
            .get("updatedAt")
            .or_else(|| value.get("updated_at"))
            .and_then(Value::as_str)
            .map(ToString::to_string),
        url: value
            .get("url")
            .or_else(|| value.get("html_url"))
            .and_then(Value::as_str)
            .map(ToString::to_string),
    })
}

fn gitlab_comment(value: &Value) -> Option<PullRequestHubComment> {
    if value
        .get("system")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return None;
    }
    Some(PullRequestHubComment {
        id: value.get("id")?.as_u64()?.to_string(),
        body: value
            .get("body")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        author: value
            .get("author")
            .and_then(|actor| hub_actor(actor, ForgeCliProvider::Gitlab)),
        created_at: value
            .get("created_at")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        updated_at: value
            .get("updated_at")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        url: None,
    })
}

fn github_hub_file(value: &Value) -> Option<PullRequestHubFile> {
    Some(PullRequestHubFile {
        path: value.get("filename")?.as_str()?.to_string(),
        previous_path: value
            .get("previous_filename")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        status: value
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("modified")
            .to_string(),
        additions: value.get("additions").and_then(Value::as_u64).unwrap_or(0),
        deletions: value.get("deletions").and_then(Value::as_u64).unwrap_or(0),
        patch: value
            .get("patch")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        blob_url: value
            .get("blob_url")
            .and_then(Value::as_str)
            .map(ToString::to_string),
    })
}

fn gitlab_hub_file(value: &Value) -> Option<PullRequestHubFile> {
    let old_path = value.get("old_path").and_then(Value::as_str);
    let new_path = value.get("new_path").and_then(Value::as_str);
    let path = new_path.or(old_path)?.to_string();
    let patch = value
        .get("diff")
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let mut additions = 0;
    let mut deletions = 0;
    if let Some(patch) = patch.as_deref() {
        for line in patch.lines() {
            if line.starts_with('+') && !line.starts_with("+++") {
                additions += 1;
            } else if line.starts_with('-') && !line.starts_with("---") {
                deletions += 1;
            }
        }
    }
    let status = if value
        .get("new_file")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        "added"
    } else if value
        .get("deleted_file")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        "removed"
    } else if value
        .get("renamed_file")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        "renamed"
    } else {
        "modified"
    };
    Some(PullRequestHubFile {
        path,
        previous_path: old_path
            .filter(|old_path| Some(*old_path) != new_path)
            .map(ToString::to_string),
        status: status.to_string(),
        additions,
        deletions,
        patch,
        blob_url: None,
    })
}

fn github_inline_comment(
    comment: crate::commands::forge::github::GithubReviewComment,
) -> PullRequestHubInlineComment {
    let line = comment
        .line
        .or(comment.original_line)
        .and_then(|line| u32::try_from(line).ok());
    PullRequestHubInlineComment {
        id: comment.id.to_string(),
        thread_id: None,
        path: comment.path,
        line,
        side: comment.side.map(|side| side.to_ascii_lowercase()),
        body: comment.body.unwrap_or_default(),
        author: comment.user.and_then(|user| {
            let login = user.login?.trim().to_string();
            (!login.is_empty()).then_some(PullRequestHubActor {
                login,
                name: None,
                avatar_url: user.avatar_url,
                html_url: user.html_url,
            })
        }),
        created_at: comment.created_at,
        url: comment.html_url,
        resolved: None,
    }
}

fn github_thread_inline_comments(raw: &Value) -> Vec<PullRequestHubInlineComment> {
    let pages = raw
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_else(|| std::slice::from_ref(raw));
    pages
        .iter()
        .flat_map(|page| {
            page.pointer("/data/repository/pullRequest/reviewThreads/nodes")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
        .flat_map(|thread| {
            let thread_id = thread
                .get("id")
                .and_then(Value::as_str)
                .map(ToString::to_string);
            let resolved = thread.get("isResolved").and_then(Value::as_bool);
            let thread_path = thread
                .get("path")
                .and_then(Value::as_str)
                .map(ToString::to_string);
            let thread_line = thread
                .get("line")
                .or_else(|| thread.get("originalLine"))
                .and_then(Value::as_u64)
                .and_then(|line| u32::try_from(line).ok());
            thread
                .pointer("/comments/nodes")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(move |comment| {
                    let id = comment.get("databaseId")?.as_i64()?.to_string();
                    let path = comment
                        .get("path")
                        .and_then(Value::as_str)
                        .map(ToString::to_string)
                        .or_else(|| thread_path.clone())?;
                    let line = comment
                        .get("line")
                        .or_else(|| comment.get("originalLine"))
                        .and_then(Value::as_u64)
                        .and_then(|line| u32::try_from(line).ok())
                        .or(thread_line);
                    let author = comment.get("author").and_then(|author| {
                        let login = author.get("login")?.as_str()?.trim().to_string();
                        (!login.is_empty()).then_some(PullRequestHubActor {
                            login,
                            name: None,
                            avatar_url: author
                                .get("avatarUrl")
                                .and_then(Value::as_str)
                                .map(ToString::to_string),
                            html_url: author
                                .get("url")
                                .and_then(Value::as_str)
                                .map(ToString::to_string),
                        })
                    });
                    Some(PullRequestHubInlineComment {
                        id,
                        thread_id: thread_id.clone(),
                        path,
                        line,
                        side: comment
                            .get("side")
                            .and_then(Value::as_str)
                            .map(|side| side.to_ascii_lowercase()),
                        body: comment
                            .get("body")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        author,
                        created_at: comment
                            .get("createdAt")
                            .and_then(Value::as_str)
                            .map(ToString::to_string),
                        url: comment
                            .get("url")
                            .and_then(Value::as_str)
                            .map(ToString::to_string),
                        resolved,
                    })
                })
        })
        .collect()
}

fn gitlab_inline_comments(
    discussions: Vec<crate::commands::forge::gitlab::GitlabDiscussion>,
) -> Vec<PullRequestHubInlineComment> {
    discussions
        .into_iter()
        .flat_map(|discussion| {
            let thread_id = discussion.id;
            let anchor = discussion
                .notes
                .iter()
                .find_map(|note| note.position.clone());
            discussion.notes.into_iter().filter_map(move |note| {
                if note.system.unwrap_or(false) {
                    return None;
                }
                let position = note.position.or_else(|| anchor.clone())?;
                let path = position.new_path.or(position.old_path)?;
                let (line, side) = match (position.new_line, position.old_line) {
                    (Some(line), _) => (u32::try_from(line).ok(), Some("right".to_string())),
                    (None, Some(line)) => (u32::try_from(line).ok(), Some("left".to_string())),
                    _ => (None, None),
                };
                Some(PullRequestHubInlineComment {
                    id: note.id.to_string(),
                    thread_id: Some(thread_id.clone()),
                    path,
                    line,
                    side,
                    body: note.body.unwrap_or_default(),
                    author: note.author.and_then(|author| {
                        let login = author.username?.trim().to_string();
                        (!login.is_empty()).then_some(PullRequestHubActor {
                            login,
                            name: None,
                            avatar_url: author.avatar_url,
                            html_url: author.web_url,
                        })
                    }),
                    created_at: note.created_at,
                    url: None,
                    resolved: note.resolved,
                })
            })
        })
        .collect()
}

#[tauri::command]
pub async fn pull_request_hub_detail(
    state: State<'_, WorkspaceCommandState>,
    input: PullRequestHubDetailInput,
) -> Result<PullRequestHubDetailOutput, String> {
    let root = input.repository_root.trim();
    let include_code = input.include_code.unwrap_or(false);
    let context = forge_context::resolve_workspace_forge_context(
        &state.db_path,
        root,
        input.forge_login.as_deref(),
    )?
    .ok_or_else(|| "No GitHub or GitLab remote was found.".to_string())?;
    match context.provider {
        ForgeCliProvider::Github => {
            let raw = crate::commands::forge::github::pull_request_detail_json(
                root,
                &context.host,
                input.number,
                context.effective_login.as_deref(),
            )?;
            let checks = github_checks(raw.get("statusCheckRollup").unwrap_or(&Value::Null));
            let comments = raw
                .get("comments")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(github_comment)
                .collect();
            let (files, inline_comments) = if include_code {
                let files_raw = crate::commands::forge::github::pull_request_files_json(
                    root,
                    &context.host,
                    &context.namespace,
                    &context.repo,
                    input.number,
                    context.effective_login.as_deref(),
                )?;
                let files = files_raw
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(github_hub_file)
                    .collect();
                let inline_comments =
                    crate::commands::forge::github::pull_request_review_threads_json(
                        root,
                        &context.host,
                        &context.namespace,
                        &context.repo,
                        input.number,
                        context.effective_login.as_deref(),
                    )
                    .map(|raw| github_thread_inline_comments(&raw))
                    .unwrap_or_else(|_| {
                        crate::commands::forge::github::list_pull_review_comments(
                            &context.host,
                            &context.namespace,
                            &context.repo,
                            input.number,
                            context.effective_login.as_deref(),
                        )
                        .unwrap_or_default()
                        .into_iter()
                        .map(github_inline_comment)
                        .collect()
                    });
                (files, inline_comments)
            } else {
                (Vec::new(), Vec::new())
            };
            Ok(PullRequestHubDetailOutput {
                body: raw
                    .get("body")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
                comments,
                checks,
                files,
                inline_comments,
                review_capabilities: PullRequestHubReviewCapabilities {
                    inline_comments: true,
                    approve: true,
                    request_changes: true,
                    reply_to_threads: true,
                    resolve_threads: true,
                },
            })
        }
        ForgeCliProvider::Gitlab => {
            let raw = crate::commands::forge::gitlab::merge_request_detail_json(
                root,
                &context.host,
                &context.namespace,
                &context.repo,
                input.number,
                context.effective_login.as_deref(),
            )?;
            let detail = raw.get("detail").unwrap_or(&Value::Null);
            let comments = raw
                .get("comments")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(gitlab_comment)
                .collect();
            let checks = raw
                .get("pipelines")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(|pipeline| PullRequestHubCheck {
                    name: pipeline
                        .get("name")
                        .and_then(Value::as_str)
                        .map(ToString::to_string)
                        .unwrap_or_else(|| "Pipeline".to_string()),
                    state: pipeline
                        .get("status")
                        .and_then(Value::as_str)
                        .map(gitlab_pipeline_state)
                        .unwrap_or_else(|| "unknown".to_string()),
                    details_url: pipeline
                        .get("web_url")
                        .and_then(Value::as_str)
                        .map(ToString::to_string),
                })
                .collect();
            let (files, inline_comments) = if include_code {
                let changes = crate::commands::forge::gitlab::merge_request_changes_json(
                    root,
                    &context.host,
                    &context.namespace,
                    &context.repo,
                    input.number,
                    context.effective_login.as_deref(),
                )?;
                let files = changes
                    .get("changes")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(gitlab_hub_file)
                    .collect();
                let inline_comments =
                    crate::commands::forge::gitlab::list_merge_request_discussions(
                        root,
                        &context.host,
                        &context.namespace,
                        &context.repo,
                        input.number,
                        context.effective_login.as_deref(),
                    )
                    .map(gitlab_inline_comments)
                    .unwrap_or_default();
                (files, inline_comments)
            } else {
                (Vec::new(), Vec::new())
            };
            Ok(PullRequestHubDetailOutput {
                body: detail
                    .get("description")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
                comments,
                checks,
                files,
                inline_comments,
                review_capabilities: PullRequestHubReviewCapabilities {
                    inline_comments: true,
                    approve: true,
                    request_changes: false,
                    reply_to_threads: true,
                    resolve_threads: true,
                },
            })
        }
    }
}

#[tauri::command]
pub async fn pull_request_hub_reply_thread(
    state: State<'_, WorkspaceCommandState>,
    input: PullRequestHubThreadReplyInput,
) -> Result<PullRequestHubThreadReplyOutput, String> {
    let body = input.body.trim();
    if body.is_empty() {
        return Err("Reply cannot be empty.".to_string());
    }
    let root = input.repository_root.trim();
    let thread_id = input.thread_id.trim();
    if thread_id.is_empty() {
        return Err("Review thread is missing.".to_string());
    }
    let context = forge_context::resolve_workspace_forge_context(
        &state.db_path,
        root,
        input.forge_login.as_deref(),
    )?
    .ok_or_else(|| "No GitHub or GitLab remote was found.".to_string())?;
    match context.provider {
        ForgeCliProvider::Github => {
            let comment_id = input
                .comment_id
                .trim()
                .parse::<i64>()
                .map_err(|_| "GitHub review comment is invalid.".to_string())?;
            crate::commands::forge::github::reply_to_pull_review_comment_json(
                root,
                &context.host,
                &context.namespace,
                &context.repo,
                input.number,
                comment_id,
                body,
                context.effective_login.as_deref(),
            )?;
        }
        ForgeCliProvider::Gitlab => {
            crate::commands::forge::gitlab::reply_to_merge_request_discussion_json(
                root,
                &context.host,
                &context.namespace,
                &context.repo,
                input.number,
                thread_id,
                body,
                context.effective_login.as_deref(),
            )?;
        }
    }
    Ok(PullRequestHubThreadReplyOutput { submitted: true })
}

#[tauri::command]
pub async fn pull_request_hub_resolve_thread(
    state: State<'_, WorkspaceCommandState>,
    input: PullRequestHubThreadResolveInput,
) -> Result<PullRequestHubThreadResolveOutput, String> {
    let root = input.repository_root.trim();
    let thread_id = input.thread_id.trim();
    if thread_id.is_empty() {
        return Err("Review thread is missing.".to_string());
    }
    let context = forge_context::resolve_workspace_forge_context(
        &state.db_path,
        root,
        input.forge_login.as_deref(),
    )?
    .ok_or_else(|| "No GitHub or GitLab remote was found.".to_string())?;
    match context.provider {
        ForgeCliProvider::Github => {
            crate::commands::forge::github::set_pull_review_thread_resolved_json(
                root,
                &context.host,
                thread_id,
                input.resolved,
                context.effective_login.as_deref(),
            )?;
        }
        ForgeCliProvider::Gitlab => {
            crate::commands::forge::gitlab::set_merge_request_discussion_resolved_json(
                root,
                &context.host,
                &context.namespace,
                &context.repo,
                input.number,
                thread_id,
                input.resolved,
                context.effective_login.as_deref(),
            )?;
        }
    }
    Ok(PullRequestHubThreadResolveOutput {
        resolved: input.resolved,
    })
}

#[tauri::command]
pub async fn pull_request_hub_comment(
    state: State<'_, WorkspaceCommandState>,
    input: PullRequestHubCommentInput,
) -> Result<PullRequestHubCommentOutput, String> {
    let body = input.body.trim();
    if body.is_empty() {
        return Err("Comment cannot be empty.".to_string());
    }
    let root = input.repository_root.trim();
    let context = forge_context::resolve_workspace_forge_context(
        &state.db_path,
        root,
        input.forge_login.as_deref(),
    )?
    .ok_or_else(|| "No GitHub or GitLab remote was found.".to_string())?;
    let comment = match context.provider {
        ForgeCliProvider::Github => {
            let raw = crate::commands::forge::github::create_pull_request_comment_json(
                root,
                &context.host,
                &context.namespace,
                &context.repo,
                input.number,
                body,
                context.effective_login.as_deref(),
            )?;
            github_comment(&raw).ok_or_else(|| "GitHub returned an invalid comment.".to_string())?
        }
        ForgeCliProvider::Gitlab => {
            let raw = crate::commands::forge::gitlab::create_merge_request_comment_json(
                root,
                &context.host,
                &context.namespace,
                &context.repo,
                input.number,
                body,
                context.effective_login.as_deref(),
            )?;
            gitlab_comment(&raw).ok_or_else(|| "GitLab returned an invalid comment.".to_string())?
        }
    };
    Ok(PullRequestHubCommentOutput { comment })
}

#[tauri::command]
pub async fn pull_request_hub_submit_review(
    state: State<'_, WorkspaceCommandState>,
    input: PullRequestHubSubmitReviewInput,
) -> Result<PullRequestHubSubmitReviewOutput, String> {
    let body = input
        .body
        .as_deref()
        .map(str::trim)
        .filter(|body| !body.is_empty());
    if input.comments.is_empty()
        && body.is_none()
        && input.event == PullRequestHubReviewEvent::Comment
    {
        return Err("Add a review comment or an inline comment before submitting.".to_string());
    }
    if input.event == PullRequestHubReviewEvent::RequestChanges && body.is_none() {
        return Err("A summary is required when requesting changes.".to_string());
    }
    if input
        .comments
        .iter()
        .any(|comment| comment.body.trim().is_empty() || comment.path.trim().is_empty())
    {
        return Err("Inline review comments must include a file and a message.".to_string());
    }

    let root = input.repository_root.trim();
    let context = forge_context::resolve_workspace_forge_context(
        &state.db_path,
        root,
        input.forge_login.as_deref(),
    )?
    .ok_or_else(|| "No GitHub or GitLab remote was found.".to_string())?;

    match context.provider {
        ForgeCliProvider::Github => {
            let event = match input.event {
                PullRequestHubReviewEvent::Comment => "COMMENT",
                PullRequestHubReviewEvent::Approve => "APPROVE",
                PullRequestHubReviewEvent::RequestChanges => "REQUEST_CHANGES",
            };
            let comments = input
                .comments
                .iter()
                .map(|comment| {
                    serde_json::json!({
                        "path": comment.path,
                        "body": comment.body.trim(),
                        "line": comment.line,
                        "side": comment.side.to_ascii_uppercase(),
                    })
                })
                .collect::<Vec<_>>();
            let mut payload = serde_json::json!({
                "event": event,
                "comments": comments,
            });
            if let Some(body) = body {
                payload["body"] = Value::String(body.to_string());
            }
            let response = crate::commands::forge::github::submit_pull_request_review_json(
                root,
                &context.host,
                &context.namespace,
                &context.repo,
                input.number,
                &payload,
                context.effective_login.as_deref(),
            )?;
            Ok(PullRequestHubSubmitReviewOutput {
                submitted: true,
                url: response
                    .get("html_url")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
                submitted_comment_count: input.comments.len() as u32,
                body_submitted: body.is_some(),
                decision_submitted: true,
                warning: None,
            })
        }
        ForgeCliProvider::Gitlab => {
            if input.event == PullRequestHubReviewEvent::RequestChanges {
                return Err(
                    "GitLab does not expose a formal request-changes review action.".to_string(),
                );
            }
            let mut submitted_comment_count = 0_u32;
            if !input.comments.is_empty() {
                let detail = crate::commands::forge::gitlab::merge_request_detail_json(
                    root,
                    &context.host,
                    &context.namespace,
                    &context.repo,
                    input.number,
                    context.effective_login.as_deref(),
                )?;
                let diff_refs = detail
                    .get("detail")
                    .and_then(|detail| detail.get("diff_refs"))
                    .ok_or_else(|| {
                        "GitLab did not return diff references for this MR.".to_string()
                    })?;
                let base_sha = diff_refs
                    .get("base_sha")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "GitLab did not return the base diff SHA.".to_string())?;
                let start_sha = diff_refs
                    .get("start_sha")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "GitLab did not return the start diff SHA.".to_string())?;
                let head_sha = diff_refs
                    .get("head_sha")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "GitLab did not return the head diff SHA.".to_string())?;
                let changes_response = crate::commands::forge::gitlab::merge_request_changes_json(
                    root,
                    &context.host,
                    &context.namespace,
                    &context.repo,
                    input.number,
                    context.effective_login.as_deref(),
                )?;
                let changes = changes_response
                    .get("changes")
                    .and_then(Value::as_array)
                    .ok_or_else(|| {
                        "GitLab did not return changed files for this MR.".to_string()
                    })?;
                for comment in &input.comments {
                    let change = changes
                        .iter()
                        .find(|change| {
                            change.get("new_path").and_then(Value::as_str)
                                == Some(comment.path.as_str())
                                || change.get("old_path").and_then(Value::as_str)
                                    == Some(comment.path.as_str())
                        })
                        .ok_or_else(|| {
                            format!("GitLab no longer lists `{}` in this MR diff.", comment.path)
                        })?;
                    let old_path = change
                        .get("old_path")
                        .and_then(Value::as_str)
                        .unwrap_or(&comment.path);
                    let new_path = change
                        .get("new_path")
                        .and_then(Value::as_str)
                        .unwrap_or(&comment.path);
                    let result =
                        crate::commands::forge::gitlab::create_merge_request_discussion_json(
                            root,
                            &context.host,
                            &context.namespace,
                            &context.repo,
                            input.number,
                            comment.body.trim(),
                            old_path,
                            new_path,
                            comment.line,
                            &comment.side,
                            base_sha,
                            start_sha,
                            head_sha,
                            context.effective_login.as_deref(),
                        );
                    if let Err(error) = result {
                        return Ok(PullRequestHubSubmitReviewOutput {
                            submitted: false,
                            url: None,
                            submitted_comment_count,
                            body_submitted: false,
                            decision_submitted: false,
                            warning: Some(format!(
                                "GitLab accepted {submitted_comment_count} inline comment(s), then stopped: {error}"
                            )),
                        });
                    }
                    submitted_comment_count += 1;
                }
            }
            let mut url = None;
            let mut body_submitted = false;
            if let Some(body) = body {
                let response =
                    match crate::commands::forge::gitlab::create_merge_request_comment_json(
                        root,
                        &context.host,
                        &context.namespace,
                        &context.repo,
                        input.number,
                        body,
                        context.effective_login.as_deref(),
                    ) {
                        Ok(response) => response,
                        Err(error) => {
                            return Ok(PullRequestHubSubmitReviewOutput {
                                submitted: false,
                                url: None,
                                submitted_comment_count,
                                body_submitted: false,
                                decision_submitted: false,
                                warning: Some(format!(
                                    "Inline comments were accepted, but the review summary failed: {error}"
                                )),
                            });
                        }
                    };
                url = response
                    .get("web_url")
                    .and_then(Value::as_str)
                    .map(ToString::to_string);
                body_submitted = true;
            }
            let mut decision_submitted = input.event == PullRequestHubReviewEvent::Comment;
            if input.event == PullRequestHubReviewEvent::Approve {
                if let Err(error) = crate::commands::forge::gitlab::approve_merge_request_json(
                    root,
                    &context.host,
                    &context.namespace,
                    &context.repo,
                    input.number,
                    context.effective_login.as_deref(),
                ) {
                    return Ok(PullRequestHubSubmitReviewOutput {
                        submitted: false,
                        url,
                        submitted_comment_count,
                        body_submitted,
                        decision_submitted: false,
                        warning: Some(format!(
                            "Review comments were accepted, but GitLab did not approve the MR: {error}"
                        )),
                    });
                }
                decision_submitted = true;
            }
            Ok(PullRequestHubSubmitReviewOutput {
                submitted: true,
                url,
                submitted_comment_count,
                body_submitted,
                decision_submitted,
                warning: None,
            })
        }
    }
}

#[tauri::command]
pub async fn workspace_pr_status(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspacePrStatusInput,
) -> Result<WorkspacePrStatusOutput, String> {
    preflight_workspace_root(&state, &input.workspace_root).await?;

    let root = input.workspace_root.trim();
    if root.is_empty() {
        return Ok(WorkspacePrStatusOutput {
            provider: None,
            host: None,
            number: None,
            title: None,
            url: None,
            head_branch: None,
            base_branch: None,
            state: None,
            is_draft: false,
            mergeable: None,
            merge_state_status: None,
        });
    }

    let forge_context = forge_context::resolve_workspace_forge_context(
        &state.db_path,
        root,
        input.forge_login.as_deref(),
    )?;
    let effective_login = forge_context
        .as_ref()
        .and_then(|context| context.effective_login.as_deref());
    let head_sha = resolve_current_commit_sha(root).ok().flatten();
    let source = imported_workspace_source(&state, root).await?;
    let branch = match input
        .branch
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        Some(branch) => branch,
        None if source.is_some() => source
            .as_ref()
            .map(|source| source.head_branch.clone())
            .unwrap_or_default(),
        None => match resolve_current_branch_name(root) {
            Ok(branch) => branch,
            Err(_) => {
                return Ok(WorkspacePrStatusOutput {
                    provider: None,
                    host: None,
                    number: None,
                    title: None,
                    url: None,
                    head_branch: None,
                    base_branch: None,
                    state: None,
                    is_draft: false,
                    mergeable: None,
                    merge_state_status: None,
                });
            }
        },
    };
    let branch_hints = workspace_branch_hints(root, Some(&branch));

    let resolved = forge_provider::resolve_workspace_change_request_status(
        root,
        &branch,
        &branch_hints,
        head_sha.as_deref(),
        effective_login,
    )?;
    let Some(resolved) = resolved else {
        return Ok(WorkspacePrStatusOutput {
            provider: forge_context
                .as_ref()
                .map(|context| match context.provider {
                    ForgeCliProvider::Github => "github".to_string(),
                    ForgeCliProvider::Gitlab => "gitlab".to_string(),
                }),
            host: forge_context.as_ref().map(|context| context.host.clone()),
            number: None,
            title: None,
            url: None,
            head_branch: Some(branch),
            base_branch: None,
            state: None,
            is_draft: false,
            mergeable: None,
            merge_state_status: None,
        });
    };

    Ok(WorkspacePrStatusOutput {
        provider: Some(resolved.provider),
        host: resolved.host,
        number: resolved.number,
        title: resolved.title,
        url: resolved.url,
        head_branch: resolved.head_branch,
        base_branch: resolved.base_branch,
        state: resolved.state,
        is_draft: resolved.is_draft,
        mergeable: resolved.mergeable,
        merge_state_status: resolved.merge_state_status,
    })
}

pub async fn workspace_review_state(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceReviewStateInput,
) -> Result<WorkspaceReviewStateOutput, String> {
    preflight_workspace_root(&state, &input.workspace_root).await?;
    let root = input.workspace_root.trim();
    if root.is_empty() {
        return Ok(empty_workspace_review_state(None, None));
    }
    let forge_context = forge_context::resolve_workspace_forge_context(
        &state.db_path,
        root,
        input.forge_login.as_deref(),
    )?;
    let Some(forge_context) = forge_context else {
        return Ok(empty_workspace_review_state(None, None));
    };
    let provider_key = forge_context::forge_provider_key(forge_context.provider).to_string();
    let source = imported_workspace_source(&state, root).await?;
    let branch = input
        .branch
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(|| source.as_ref().map(|source| source.head_branch.clone()))
        .or_else(|| resolve_current_branch_name(root).ok());
    let head_sha = resolve_current_commit_sha(root).ok().flatten();
    let imported_request = source.as_ref().filter(|source| {
        source.kind == WorkspaceSourceKind::PullRequest
            && source.provider.eq_ignore_ascii_case(&provider_key)
            && source.change_request_number.is_some()
    });
    let (number, fallback_url) = if let Some(source) = imported_request {
        (
            source.change_request_number,
            Some(source.url.clone()).filter(|url| !url.trim().is_empty()),
        )
    } else if let Some(branch) = branch.as_deref() {
        let branch_hints = workspace_branch_hints(root, Some(branch));
        let resolved = forge_provider::resolve_workspace_change_request_status(
            root,
            branch,
            &branch_hints,
            head_sha.as_deref(),
            forge_context.effective_login.as_deref(),
        )?;
        (
            resolved.as_ref().and_then(|status| status.number),
            resolved.and_then(|status| status.url),
        )
    } else {
        (None, None)
    };
    let Some(number) = number else {
        return Ok(empty_workspace_review_state(
            Some(provider_key),
            Some(forge_context.host),
        ));
    };

    match forge_context.provider {
        ForgeCliProvider::Github => {
            let raw = crate::commands::forge::github::resolve_pull_review_state_json(
                root,
                &forge_context.host,
                &forge_context.namespace,
                &forge_context.repo,
                number,
                forge_context.effective_login.as_deref(),
            )?;
            let behind_by = raw
                .get("baseRefOid")
                .and_then(|value| value.as_str())
                .zip(raw.get("headRefOid").and_then(|value| value.as_str()))
                .and_then(|(base_sha, head_sha)| {
                    crate::commands::forge::github::compare_commits_json(
                        &forge_context.host,
                        &forge_context.namespace,
                        &forge_context.repo,
                        base_sha,
                        head_sha,
                        forge_context.effective_login.as_deref(),
                    )
                    .ok()
                })
                .and_then(|comparison| {
                    comparison
                        .get("behind_by")
                        .and_then(|value| value.as_u64())
                        .map(|value| value as u32)
                });
            Ok(map_github_review_state(
                raw,
                Some(forge_context.host),
                number,
                fallback_url,
                behind_by,
            ))
        }
        ForgeCliProvider::Gitlab => {
            let raw = crate::commands::forge::gitlab::load_merge_request_review(
                root,
                &forge_context.host,
                &forge_context.namespace,
                &forge_context.repo,
                number,
                forge_context.effective_login.as_deref(),
            )?;
            Ok(map_gitlab_review_state(
                raw,
                Some(forge_context.host),
                number,
                fallback_url,
            ))
        }
    }
}

fn empty_workspace_review_state(
    provider: Option<String>,
    host: Option<String>,
) -> WorkspaceReviewStateOutput {
    WorkspaceReviewStateOutput {
        provider,
        host,
        number: None,
        url: None,
        review_state: None,
        reviewers: Vec::new(),
        reviewers_available: false,
        approvals_available: false,
        approvals_required: None,
        approvals_received: 0,
        approvals_left: None,
        approved: None,
        mergeable: None,
        merge_state_status: None,
        has_conflicts: None,
        behind_by: None,
        discussions_resolved: None,
        draft: None,
    }
}

fn reviewer_from_json(value: &serde_json::Value, state: &str) -> WorkspaceReviewer {
    let author = value.get("author").unwrap_or(value);
    WorkspaceReviewer {
        login: author
            .get("login")
            .or_else(|| author.get("username"))
            .or_else(|| author.get("slug"))
            .and_then(|value| value.as_str())
            .map(ToString::to_string),
        name: author
            .get("name")
            .or_else(|| author.get("slug"))
            .and_then(|value| value.as_str())
            .map(ToString::to_string),
        avatar_url: author
            .get("avatarUrl")
            .or_else(|| author.get("avatar_url"))
            .and_then(|value| value.as_str())
            .map(ToString::to_string),
        html_url: author
            .get("url")
            .or_else(|| author.get("web_url"))
            .and_then(|value| value.as_str())
            .map(ToString::to_string),
        state: state.to_string(),
    }
}

fn reviewer_key(reviewer: &WorkspaceReviewer, fallback: usize) -> String {
    reviewer
        .login
        .as_ref()
        .or(reviewer.name.as_ref())
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_else(|| format!("reviewer-{fallback}"))
}

fn map_github_review_state(
    raw: serde_json::Value,
    host: Option<String>,
    number: u32,
    fallback_url: Option<String>,
    behind_by: Option<u32>,
) -> WorkspaceReviewStateOutput {
    let mut reviewers = std::collections::BTreeMap::new();
    let latest_reviews = raw
        .get("latestReviews")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    for (index, review) in latest_reviews.iter().enumerate() {
        let state = match review
            .get("state")
            .and_then(|value| value.as_str())
            .unwrap_or("")
        {
            "APPROVED" => "approved",
            "CHANGES_REQUESTED" => "changes_requested",
            "DISMISSED" => "dismissed",
            _ => "reviewed",
        };
        let reviewer = reviewer_from_json(review, state);
        reviewers.insert(reviewer_key(&reviewer, index), reviewer);
    }
    let review_requests = raw
        .get("reviewRequests")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    for (index, request) in review_requests.iter().enumerate() {
        let reviewer = reviewer_from_json(request, "pending");
        reviewers.insert(
            reviewer_key(&reviewer, latest_reviews.len() + index),
            reviewer,
        );
    }
    let reviewers = reviewers.into_values().collect::<Vec<_>>();
    let approvals_received = reviewers
        .iter()
        .filter(|reviewer| reviewer.state == "approved")
        .count() as u32;
    let decision = raw.get("reviewDecision").and_then(|value| value.as_str());
    let review_state = match decision {
        Some("APPROVED") => "approved",
        Some("CHANGES_REQUESTED") => "changes_requested",
        Some("REVIEW_REQUIRED") => "pending",
        _ if reviewers
            .iter()
            .any(|reviewer| reviewer.state == "changes_requested") =>
        {
            "changes_requested"
        }
        _ if reviewers.iter().any(|reviewer| reviewer.state == "pending") => "pending",
        _ if approvals_received > 0 => "approved",
        _ => "not_required",
    };
    let mergeable =
        raw.get("mergeable")
            .and_then(|value| value.as_str())
            .map(|value| match value {
                "MERGEABLE" => "mergeable".to_string(),
                "CONFLICTING" => "conflicting".to_string(),
                _ => "unknown".to_string(),
            });
    let has_conflicts = mergeable
        .as_deref()
        .map(|value| value == "conflicting")
        .filter(|_| mergeable.as_deref() != Some("unknown"));
    let approved = decision.map(|value| value == "APPROVED");

    WorkspaceReviewStateOutput {
        provider: Some("github".to_string()),
        host,
        number: Some(number),
        url: raw
            .get("url")
            .and_then(|value| value.as_str())
            .map(ToString::to_string)
            .or(fallback_url),
        review_state: Some(review_state.to_string()),
        reviewers,
        reviewers_available: true,
        approvals_available: true,
        approvals_required: None,
        approvals_received,
        approvals_left: (decision == Some("APPROVED")).then_some(0),
        approved,
        mergeable,
        merge_state_status: raw
            .get("mergeStateStatus")
            .and_then(|value| value.as_str())
            .map(ToString::to_string),
        has_conflicts,
        behind_by,
        discussions_resolved: None,
        draft: raw.get("isDraft").and_then(|value| value.as_bool()),
    }
}

fn map_gitlab_review_state(
    raw: crate::commands::forge::gitlab::GitlabMergeRequestReview,
    host: Option<String>,
    number: u32,
    fallback_url: Option<String>,
) -> WorkspaceReviewStateOutput {
    let approvals_available = raw.approvals.is_some();
    let reviewers_available = raw.reviewers.is_some();
    let approved_logins = raw
        .approvals
        .as_ref()
        .map(|approvals| {
            approvals
                .approved_by
                .iter()
                .filter_map(|approval| approval.user.username.as_deref())
                .map(|login| login.to_ascii_lowercase())
                .collect::<std::collections::HashSet<_>>()
        })
        .unwrap_or_default();
    let mut reviewers = std::collections::BTreeMap::new();
    for (index, reviewer) in raw.reviewers.as_ref().into_iter().flatten().enumerate() {
        let approved = reviewer
            .user
            .username
            .as_ref()
            .map(|login| approved_logins.contains(&login.to_ascii_lowercase()))
            .unwrap_or(false);
        let state = if approved {
            "approved"
        } else {
            match reviewer.state.as_deref() {
                Some("reviewed") => "reviewed",
                _ => "pending",
            }
        };
        let normalized = WorkspaceReviewer {
            login: reviewer.user.username.clone(),
            name: reviewer.user.name.clone(),
            avatar_url: reviewer.user.avatar_url.clone(),
            html_url: reviewer.user.web_url.clone(),
            state: state.to_string(),
        };
        reviewers.insert(reviewer_key(&normalized, index), normalized);
    }
    if let Some(approvals) = raw.approvals.as_ref() {
        for (index, approval) in approvals.approved_by.iter().enumerate() {
            let normalized = WorkspaceReviewer {
                login: approval.user.username.clone(),
                name: approval.user.name.clone(),
                avatar_url: approval.user.avatar_url.clone(),
                html_url: approval.user.web_url.clone(),
                state: "approved".to_string(),
            };
            reviewers.insert(
                reviewer_key(&normalized, reviewers.len() + index),
                normalized,
            );
        }
    }
    let reviewers = reviewers.into_values().collect::<Vec<_>>();
    let approvals_received = raw
        .approvals
        .as_ref()
        .map(|approvals| approvals.approved_by.len() as u32)
        .unwrap_or(0);
    let detailed_status = raw
        .detail
        .get("detailed_merge_status")
        .and_then(|value| value.as_str())
        .or_else(|| {
            raw.detail
                .get("merge_status")
                .and_then(|value| value.as_str())
        });
    let approvals_required = raw
        .approvals
        .as_ref()
        .and_then(|approvals| approvals.approvals_required);
    let approvals_left = raw
        .approvals
        .as_ref()
        .and_then(|approvals| approvals.approvals_left);
    let approved = raw
        .approvals
        .as_ref()
        .and_then(|approvals| approvals.approved);
    let review_state = if detailed_status == Some("requested_changes") {
        "changes_requested"
    } else if approved == Some(true) {
        "approved"
    } else if approvals_left.unwrap_or(0) > 0
        || detailed_status == Some("not_approved")
        || reviewers.iter().any(|reviewer| reviewer.state == "pending")
    {
        "pending"
    } else if approvals_required == Some(0) {
        "not_required"
    } else {
        "unknown"
    };
    let mergeable = crate::commands::forge::gitlab::map_mergeable(&raw.detail)
        .map(|value| value.to_ascii_lowercase());
    let has_conflicts = raw
        .detail
        .get("has_conflicts")
        .and_then(|value| value.as_bool())
        .or_else(|| {
            mergeable
                .as_deref()
                .filter(|value| *value != "unknown")
                .map(|value| value == "conflicting")
        });

    WorkspaceReviewStateOutput {
        provider: Some("gitlab".to_string()),
        host,
        number: Some(number),
        url: raw
            .detail
            .get("web_url")
            .and_then(|value| value.as_str())
            .map(ToString::to_string)
            .or(fallback_url),
        review_state: Some(review_state.to_string()),
        reviewers,
        reviewers_available,
        approvals_available,
        approvals_required,
        approvals_received,
        approvals_left,
        approved,
        mergeable,
        merge_state_status: detailed_status.map(ToString::to_string),
        has_conflicts,
        behind_by: raw
            .detail
            .get("diverged_commits_count")
            .and_then(|value| value.as_u64())
            .map(|value| value as u32),
        discussions_resolved: raw
            .detail
            .get("blocking_discussions_resolved")
            .and_then(|value| value.as_bool()),
        draft: raw
            .detail
            .get("draft")
            .and_then(|value| value.as_bool())
            .or_else(|| {
                raw.detail
                    .get("work_in_progress")
                    .and_then(|value| value.as_bool())
            }),
    }
}

pub async fn workspace_pr_review_comments(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspacePrReviewCommentsInput,
) -> Result<WorkspacePrReviewCommentsOutput, String> {
    preflight_workspace_root(&state, &input.workspace_root).await?;

    let root = input.workspace_root.trim();
    if root.is_empty() {
        return Ok(WorkspacePrReviewCommentsOutput {
            provider: None,
            host: None,
            number: None,
            url: None,
            comments: Vec::new(),
        });
    }

    let forge_context = forge_context::resolve_workspace_forge_context(
        &state.db_path,
        root,
        input.forge_login.as_deref(),
    )?;
    let Some(forge_context) = forge_context else {
        return Ok(WorkspacePrReviewCommentsOutput {
            provider: None,
            host: None,
            number: None,
            url: None,
            comments: Vec::new(),
        });
    };
    let provider_key = forge_context::forge_provider_key(forge_context.provider).to_string();
    let imported_source = imported_workspace_source(&state, root).await?;

    let branch = match input
        .branch
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(branch) => branch.to_string(),
        None => match imported_source.as_ref() {
            Some(source) => source.head_branch.clone(),
            None => match resolve_current_branch_name(root) {
                Ok(branch) => branch,
                Err(_) => {
                    return Ok(WorkspacePrReviewCommentsOutput {
                        provider: Some(provider_key),
                        host: Some(forge_context.host),
                        number: None,
                        url: None,
                        comments: Vec::new(),
                    });
                }
            },
        },
    };
    let current_head_sha = resolve_current_commit_sha(root).ok().flatten();
    let imported_request = imported_source.as_ref().filter(|source| {
        source.kind == WorkspaceSourceKind::PullRequest
            && source.provider.eq_ignore_ascii_case(&provider_key)
            && source.change_request_number.is_some()
    });
    let (number, request_url) = if let Some(source) = imported_request {
        (
            source.change_request_number,
            Some(source.url.trim().to_string()).filter(|url| !url.is_empty()),
        )
    } else {
        let branch_hints = workspace_branch_hints(root, Some(&branch));
        let resolved = forge_provider::resolve_workspace_change_request_status(
            root,
            &branch,
            &branch_hints,
            current_head_sha.as_deref(),
            forge_context.effective_login.as_deref(),
        )?;
        let Some(resolved) = resolved else {
            return Ok(WorkspacePrReviewCommentsOutput {
                provider: Some(provider_key),
                host: Some(forge_context.host),
                number: None,
                url: None,
                comments: Vec::new(),
            });
        };
        (resolved.number, resolved.url)
    };
    let Some(number) = number else {
        return Ok(WorkspacePrReviewCommentsOutput {
            provider: Some(provider_key),
            host: Some(forge_context.host),
            number: None,
            url: request_url,
            comments: Vec::new(),
        });
    };

    let comments = match forge_context.provider {
        ForgeCliProvider::Github => crate::commands::forge::github::list_pull_review_comments(
            &forge_context.host,
            &forge_context.namespace,
            &forge_context.repo,
            number,
            forge_context.effective_login.as_deref(),
        )?
        .into_iter()
        .map(|comment| {
            let thread_id = comment.in_reply_to_id.unwrap_or(comment.id);
            WorkspacePrReviewComment {
                id: comment.id.to_string(),
                parent_id: comment.in_reply_to_id.map(|id| id.to_string()),
                thread_id: thread_id.to_string(),
                path: comment.path,
                body: comment.body.unwrap_or_default(),
                diff_hunk: comment.diff_hunk,
                html_url: comment.html_url,
                side: comment.side,
                line: comment.line,
                start_line: comment.start_line,
                original_line: comment.original_line,
                original_start_line: comment.original_start_line,
                author: comment.user.map(|user| WorkspacePrReviewCommentAuthor {
                    login: user.login,
                    avatar_url: user.avatar_url,
                    html_url: user.html_url,
                }),
                created_at: comment.created_at,
                updated_at: comment.updated_at,
                resolvable: None,
                resolved: None,
                outdated: None,
            }
        })
        .collect(),
        ForgeCliProvider::Gitlab => {
            let discussions = crate::commands::forge::gitlab::list_merge_request_discussions(
                root,
                &forge_context.host,
                &forge_context.namespace,
                &forge_context.repo,
                number,
                forge_context.effective_login.as_deref(),
            )?;
            map_gitlab_review_comments(
                discussions,
                request_url.as_deref(),
                current_head_sha.as_deref(),
            )
        }
    };

    Ok(WorkspacePrReviewCommentsOutput {
        provider: Some(provider_key),
        host: Some(forge_context.host),
        number: Some(number),
        url: request_url,
        comments,
    })
}

#[tauri::command]
pub async fn workspace_pipeline_status(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspacePipelineStatusInput,
) -> Result<WorkspacePipelineStatusOutput, String> {
    preflight_workspace_root(&state, &input.workspace_root).await?;
    let root = input.workspace_root.trim();
    if root.is_empty() {
        return Ok(WorkspacePipelineStatusOutput {
            provider: None,
            host: None,
            change_request_number: None,
            head_sha: None,
            pipeline: None,
        });
    }

    let forge_context = forge_context::resolve_workspace_forge_context(
        &state.db_path,
        root,
        input.forge_login.as_deref(),
    )?;
    let Some(forge_context) = forge_context else {
        return Ok(WorkspacePipelineStatusOutput {
            provider: None,
            host: None,
            change_request_number: None,
            head_sha: None,
            pipeline: None,
        });
    };
    let provider_key = forge_context::forge_provider_key(forge_context.provider).to_string();
    if forge_context.provider != ForgeCliProvider::Gitlab {
        return Ok(WorkspacePipelineStatusOutput {
            provider: Some(provider_key),
            host: Some(forge_context.host),
            change_request_number: None,
            head_sha: resolve_current_commit_sha(root).ok().flatten(),
            pipeline: None,
        });
    }

    let head_sha = resolve_current_commit_sha(root).ok().flatten();
    let Some(head_sha) = head_sha else {
        return Ok(WorkspacePipelineStatusOutput {
            provider: Some(provider_key),
            host: Some(forge_context.host),
            change_request_number: None,
            head_sha: None,
            pipeline: None,
        });
    };
    let imported_source = imported_workspace_source(&state, root).await?;
    let branch = imported_source
        .as_ref()
        .map(|source| source.head_branch.clone())
        .or_else(|| resolve_current_branch_name(root).ok());
    let imported_number = imported_source.as_ref().and_then(|source| {
        (source.kind == WorkspaceSourceKind::PullRequest
            && source.provider.eq_ignore_ascii_case("gitlab"))
        .then_some(source.change_request_number)
        .flatten()
    });
    let change_request_number = if imported_number.is_some() {
        imported_number
    } else if let Some(branch) = branch.as_deref() {
        let branch_hints = workspace_branch_hints(root, Some(branch));
        forge_provider::resolve_workspace_change_request_status(
            root,
            branch,
            &branch_hints,
            Some(&head_sha),
            forge_context.effective_login.as_deref(),
        )?
        .and_then(|status| status.number)
    } else {
        None
    };

    let pipeline = match crate::commands::forge::gitlab::find_pipeline_for_sha(
        root,
        &forge_context.host,
        &forge_context.namespace,
        &forge_context.repo,
        change_request_number,
        &head_sha,
        forge_context.effective_login.as_deref(),
    ) {
        Ok(pipeline) => pipeline,
        Err(error) => {
            capture_workspace_delivery_failure(
                &state,
                root,
                WorkspaceDeliveryFailureOperation::Pipeline,
                &error,
                CaptureDeliveryFailureOptions {
                    remote: Some(forge_context.remote_name.clone()),
                    external_url: None,
                    ..CaptureDeliveryFailureOptions::default()
                },
            )
            .await;
            return Err(error);
        }
    };
    let pipeline = match pipeline {
        Some(pipeline) => {
            let jobs = match crate::commands::forge::gitlab::list_pipeline_jobs(
                root,
                &forge_context.host,
                &forge_context.namespace,
                &forge_context.repo,
                pipeline.id,
                forge_context.effective_login.as_deref(),
            ) {
                Ok(jobs) => jobs,
                Err(error) => {
                    capture_workspace_delivery_failure(
                        &state,
                        root,
                        WorkspaceDeliveryFailureOperation::Pipeline,
                        &error,
                        CaptureDeliveryFailureOptions {
                            remote: Some(forge_context.remote_name.clone()),
                            external_url: pipeline.web_url.clone(),
                            ..CaptureDeliveryFailureOptions::default()
                        },
                    )
                    .await;
                    return Err(error);
                }
            };
            let pipeline = map_gitlab_pipeline(pipeline, jobs);
            if pipeline.status == "failed" {
                let failed_jobs = pipeline
                    .jobs
                    .iter()
                    .filter(|job| job.status == "failed")
                    .take(20)
                    .map(|job| format!("- {} / {}", job.stage, job.name))
                    .collect::<Vec<_>>();
                let mut detail = format!(
                    "GitLab pipeline #{} failed for commit {}.",
                    pipeline.id, pipeline.sha
                );
                if !failed_jobs.is_empty() {
                    detail.push_str("\nFailed jobs:\n");
                    detail.push_str(&failed_jobs.join("\n"));
                }
                capture_workspace_delivery_failure(
                    &state,
                    root,
                    WorkspaceDeliveryFailureOperation::Pipeline,
                    &detail,
                    CaptureDeliveryFailureOptions {
                        remote: Some(forge_context.remote_name.clone()),
                        external_url: pipeline.web_url.clone(),
                        ..CaptureDeliveryFailureOptions::default()
                    },
                )
                .await;
            } else {
                clear_workspace_delivery_failure(
                    &state,
                    root,
                    WorkspaceDeliveryFailureOperation::Pipeline,
                );
            }
            Some(pipeline)
        }
        None => {
            clear_workspace_delivery_failure(
                &state,
                root,
                WorkspaceDeliveryFailureOperation::Pipeline,
            );
            None
        }
    };

    Ok(WorkspacePipelineStatusOutput {
        provider: Some(provider_key),
        host: Some(forge_context.host),
        change_request_number,
        head_sha: Some(head_sha),
        pipeline,
    })
}

#[tauri::command]
pub async fn workspace_pipeline_job_log(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspacePipelineJobInput,
) -> Result<WorkspacePipelineJobLogOutput, String> {
    preflight_workspace_root(&state, &input.workspace_root).await?;
    let root = input.workspace_root.trim();
    let (forge_context, pipeline, jobs) = resolve_pipeline_action_context(
        &state,
        root,
        input.pipeline_id,
        input.forge_login.as_deref(),
    )?;
    let job = jobs
        .iter()
        .find(|job| job.id == input.job_id)
        .ok_or_else(|| "The selected job does not belong to the current pipeline.".to_string())?;
    let trace = crate::commands::forge::gitlab::read_job_trace(
        root,
        &forge_context.host,
        &forge_context.namespace,
        &forge_context.repo,
        job.id,
        job.trace_size(),
        forge_context.effective_login.as_deref(),
    )?;

    debug_assert_eq!(pipeline.id, input.pipeline_id);
    Ok(WorkspacePipelineJobLogOutput {
        job_id: job.id,
        content: trace.content,
        truncated: trace.truncated,
        loaded_bytes: trace.loaded_bytes,
    })
}

#[tauri::command]
pub async fn workspace_pipeline_job_retry(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspacePipelineJobInput,
) -> Result<(), String> {
    preflight_workspace_root(&state, &input.workspace_root).await?;
    let root = input.workspace_root.trim();
    let (forge_context, _, jobs) = resolve_pipeline_action_context(
        &state,
        root,
        input.pipeline_id,
        input.forge_login.as_deref(),
    )?;
    let job = jobs
        .iter()
        .find(|job| job.id == input.job_id)
        .ok_or_else(|| "The selected job does not belong to the current pipeline.".to_string())?;
    if !job.is_retryable() {
        return Err("GitLab does not expose retry for this job state.".to_string());
    }

    crate::commands::forge::gitlab::retry_job(
        root,
        &forge_context.host,
        &forge_context.namespace,
        &forge_context.repo,
        job.id,
        forge_context.effective_login.as_deref(),
    )?;
    Ok(())
}

fn resolve_pipeline_action_context(
    state: &WorkspaceCommandState,
    root: &str,
    pipeline_id: u64,
    forge_login: Option<&str>,
) -> Result<
    (
        crate::commands::forge::context::ResolvedWorkspaceForgeContext,
        crate::commands::forge::gitlab::GitlabPipeline,
        Vec<crate::commands::forge::gitlab::GitlabPipelineJob>,
    ),
    String,
> {
    if root.is_empty() {
        return Err("workspace_root is empty".to_string());
    }
    let forge_context =
        forge_context::resolve_workspace_forge_context(&state.db_path, root, forge_login)?
            .ok_or_else(|| "No forge context is available for this workspace.".to_string())?;
    if forge_context.provider != ForgeCliProvider::Gitlab {
        return Err("Pipeline jobs are currently available for GitLab workspaces.".to_string());
    }
    let head_sha = resolve_current_commit_sha(root)?
        .ok_or_else(|| "The current workspace commit could not be resolved.".to_string())?;
    let pipeline = crate::commands::forge::gitlab::pipeline_by_id(
        root,
        &forge_context.host,
        &forge_context.namespace,
        &forge_context.repo,
        pipeline_id,
        forge_context.effective_login.as_deref(),
    )?;
    if pipeline.sha != head_sha {
        return Err(
            "The selected pipeline no longer matches the current workspace commit.".to_string(),
        );
    }
    let jobs = crate::commands::forge::gitlab::list_pipeline_jobs(
        root,
        &forge_context.host,
        &forge_context.namespace,
        &forge_context.repo,
        pipeline_id,
        forge_context.effective_login.as_deref(),
    )?;
    Ok((forge_context, pipeline, jobs))
}

fn map_gitlab_pipeline(
    pipeline: crate::commands::forge::gitlab::GitlabPipeline,
    jobs: Vec<crate::commands::forge::gitlab::GitlabPipelineJob>,
) -> WorkspacePipeline {
    WorkspacePipeline {
        id: pipeline.id,
        sha: pipeline.sha,
        ref_name: pipeline.ref_name,
        status: pipeline.status,
        source: pipeline.source,
        web_url: pipeline.web_url,
        created_at: pipeline.created_at,
        updated_at: pipeline.updated_at,
        started_at: pipeline.started_at,
        finished_at: pipeline.finished_at,
        duration: pipeline.duration,
        queued_duration: pipeline.queued_duration,
        jobs: jobs
            .into_iter()
            .map(|job| {
                let retryable = job.is_retryable();
                WorkspacePipelineJob {
                    id: job.id,
                    name: job.name,
                    stage: job.stage,
                    status: job.status,
                    duration: job.duration,
                    queued_duration: job.queued_duration,
                    web_url: job.web_url,
                    allow_failure: job.allow_failure.unwrap_or(false),
                    archived: job.archived.unwrap_or(false),
                    retryable,
                }
            })
            .collect(),
    }
}

fn map_gitlab_review_comments(
    discussions: Vec<crate::commands::forge::gitlab::GitlabDiscussion>,
    request_url: Option<&str>,
    current_head_sha: Option<&str>,
) -> Vec<WorkspacePrReviewComment> {
    let mut comments = Vec::new();

    for discussion in discussions {
        let Some(anchor_position) = discussion
            .notes
            .iter()
            .find_map(|note| note.position.clone())
        else {
            continue;
        };
        let parent_id = discussion
            .notes
            .iter()
            .find(|note| !note.system.unwrap_or(false))
            .map(|note| note.id.to_string());

        for note in discussion.notes {
            if note.system.unwrap_or(false) {
                continue;
            }
            let position = note.position.unwrap_or_else(|| anchor_position.clone());
            let Some(path) = position
                .new_path
                .clone()
                .or_else(|| position.old_path.clone())
            else {
                continue;
            };
            let uses_new_side = position.new_line.is_some()
                || position
                    .line_range
                    .as_ref()
                    .and_then(|range| range.end.new_line)
                    .is_some();
            let (line, start_line, original_line, original_start_line, side) = if uses_new_side {
                (
                    position
                        .line_range
                        .as_ref()
                        .and_then(|range| range.end.new_line)
                        .or(position.new_line),
                    position
                        .line_range
                        .as_ref()
                        .and_then(|range| range.start.new_line)
                        .or(position.new_line),
                    position.old_line,
                    position
                        .line_range
                        .as_ref()
                        .and_then(|range| range.start.old_line),
                    Some("RIGHT".to_string()),
                )
            } else {
                (
                    None,
                    None,
                    position
                        .line_range
                        .as_ref()
                        .and_then(|range| range.end.old_line)
                        .or(position.old_line),
                    position
                        .line_range
                        .as_ref()
                        .and_then(|range| range.start.old_line)
                        .or(position.old_line),
                    Some("LEFT".to_string()),
                )
            };
            let note_id = note.id.to_string();
            let outdated = position.head_sha.as_deref().and_then(|position_sha| {
                current_head_sha.map(|current_sha| position_sha != current_sha)
            });
            comments.push(WorkspacePrReviewComment {
                id: note_id.clone(),
                parent_id: parent_id
                    .as_ref()
                    .filter(|parent| parent.as_str() != note_id)
                    .cloned(),
                thread_id: discussion.id.clone(),
                path,
                body: note.body.unwrap_or_default(),
                diff_hunk: None,
                html_url: request_url
                    .map(|url| format!("{}#note_{}", url.trim_end_matches('/'), note.id)),
                side,
                line,
                start_line,
                original_line,
                original_start_line,
                author: note.author.map(|author| WorkspacePrReviewCommentAuthor {
                    login: author.username,
                    avatar_url: author.avatar_url,
                    html_url: author.web_url,
                }),
                created_at: note.created_at,
                updated_at: note.updated_at,
                resolvable: note.resolvable,
                resolved: note.resolved,
                outdated,
            });
        }
    }

    comments
}

#[cfg(test)]
mod tests {
    use super::{
        github_checks, github_comment, github_hub_file, github_thread_inline_comments,
        gitlab_hub_file, is_provisional_task_title, map_github_review_state,
        map_gitlab_review_comments, map_gitlab_review_state, preferred_change_request_title,
    };
    use crate::commands::forge::gitlab::{
        GitlabApproval, GitlabApprovals, GitlabDiscussion, GitlabDiscussionAuthor,
        GitlabDiscussionLinePosition, GitlabDiscussionLineRange, GitlabDiscussionNote,
        GitlabDiscussionPosition, GitlabMergeRequestReview, GitlabPipelineJob, GitlabReviewUser,
        GitlabReviewer,
    };
    use serde_json::json;

    fn note(id: i64, position: Option<GitlabDiscussionPosition>) -> GitlabDiscussionNote {
        GitlabDiscussionNote {
            id,
            body: Some(format!("note {id}")),
            author: Some(GitlabDiscussionAuthor {
                username: Some("reviewer".to_string()),
                avatar_url: None,
                web_url: Some("https://gitlab.example/reviewer".to_string()),
            }),
            created_at: None,
            updated_at: None,
            system: Some(false),
            resolvable: Some(true),
            resolved: Some(false),
            position,
        }
    }

    #[test]
    fn maps_github_issue_comment_with_numeric_rest_id() {
        let comment = github_comment(&json!({
            "id": 987,
            "body": "Looks good",
            "user": { "login": "alice", "avatar_url": "https://example.test/alice.png" },
            "created_at": "2026-08-01T10:00:00Z",
            "html_url": "https://github.com/acme/app/pull/12#issuecomment-987"
        }))
        .expect("map REST issue comment");

        assert_eq!(comment.id, "987");
        assert_eq!(comment.body, "Looks good");
        assert_eq!(comment.author.expect("comment author").login, "alice");
    }

    #[test]
    fn recognizes_only_provisional_task_titles() {
        assert!(is_provisional_task_title("Nova tarefa"));
        assert!(is_provisional_task_title("  NEW   TASK "));
        assert!(!is_provisional_task_title("Corrigir autenticação"));
    }

    #[test]
    fn pull_request_title_falls_back_from_placeholder_to_thread() {
        assert_eq!(
            preferred_change_request_title(
                Some("Nova tarefa"),
                vec!["Corrigir autenticação".to_string()],
            ),
            Some("Corrigir autenticação".to_string()),
        );
        assert_eq!(
            preferred_change_request_title(
                Some("Atualizar checkout"),
                vec!["Outro título".to_string()],
            ),
            Some("Atualizar checkout".to_string()),
        );
    }

    #[test]
    fn maps_paginated_github_review_threads_with_resolution_state() {
        let comments = github_thread_inline_comments(&json!([{
            "data": { "repository": { "pullRequest": { "reviewThreads": { "nodes": [{
                "id": "PRRT_thread",
                "isResolved": true,
                "path": "src/review.ts",
                "line": 12,
                "originalLine": 10,
                "comments": { "nodes": [{
                    "databaseId": 101,
                    "body": "Please keep this a draft.",
                    "createdAt": "2026-08-01T10:00:00Z",
                    "url": "https://github.com/acme/app/pull/12#discussion_r101",
                    "author": { "login": "alice", "avatarUrl": null, "url": null },
                    "path": "src/review.ts",
                    "line": 12,
                    "side": "RIGHT"
                }] }
            }] } } } }
        }]));

        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].id, "101");
        assert_eq!(comments[0].thread_id.as_deref(), Some("PRRT_thread"));
        assert_eq!(comments[0].side.as_deref(), Some("right"));
        assert_eq!(comments[0].resolved, Some(true));
    }

    #[test]
    fn normalizes_github_check_rollup_for_hub() {
        let checks = github_checks(&json!([
            { "name": "tests", "status": "COMPLETED", "conclusion": "SUCCESS" },
            { "context": "deploy", "status": "IN_PROGRESS", "conclusion": "" }
        ]));

        assert_eq!(checks.len(), 2);
        assert_eq!(checks[0].state, "success");
        assert_eq!(checks[1].name, "deploy");
        assert_eq!(checks[1].state, "pending");
    }

    #[test]
    fn maps_github_changed_file_patch() {
        let file = github_hub_file(&json!({
            "filename": "src/app.ts",
            "status": "modified",
            "additions": 3,
            "deletions": 1,
            "patch": "@@ -1 +1 @@\n-old\n+new"
        }))
        .expect("map GitHub file");

        assert_eq!(file.path, "src/app.ts");
        assert_eq!(file.additions, 3);
        assert_eq!(file.deletions, 1);
        assert!(file.patch.is_some());
    }

    #[test]
    fn counts_gitlab_patch_lines_and_preserves_rename() {
        let file = gitlab_hub_file(&json!({
            "old_path": "src/old.ts",
            "new_path": "src/new.ts",
            "renamed_file": true,
            "diff": "@@ -1,2 +1,2 @@\n-old\n+new\n context"
        }))
        .expect("map GitLab file");

        assert_eq!(file.path, "src/new.ts");
        assert_eq!(file.previous_path.as_deref(), Some("src/old.ts"));
        assert_eq!(file.status, "renamed");
        assert_eq!(file.additions, 1);
        assert_eq!(file.deletions, 1);
    }

    #[test]
    fn maps_gitlab_diff_thread_and_inherits_position_for_replies() {
        let position = GitlabDiscussionPosition {
            old_path: Some("src/lib.rs".to_string()),
            new_path: Some("src/lib.rs".to_string()),
            old_line: Some(8),
            new_line: Some(10),
            head_sha: Some("old-head".to_string()),
            line_range: Some(GitlabDiscussionLineRange {
                start: GitlabDiscussionLinePosition {
                    old_line: Some(8),
                    new_line: Some(10),
                },
                end: GitlabDiscussionLinePosition {
                    old_line: Some(9),
                    new_line: Some(11),
                },
            }),
        };
        let comments = map_gitlab_review_comments(
            vec![GitlabDiscussion {
                id: "discussion-abc".to_string(),
                notes: vec![note(101, Some(position)), note(102, None)],
            }],
            Some("https://gitlab.example/group/repo/-/merge_requests/7"),
            Some("current-head"),
        );

        assert_eq!(comments.len(), 2);
        assert_eq!(comments[0].thread_id, "discussion-abc");
        assert_eq!(comments[0].parent_id, None);
        assert_eq!(comments[1].parent_id.as_deref(), Some("101"));
        assert_eq!(comments[1].path, "src/lib.rs");
        assert_eq!(comments[1].side.as_deref(), Some("RIGHT"));
        assert_eq!(comments[1].start_line, Some(10));
        assert_eq!(comments[1].line, Some(11));
        assert_eq!(comments[1].outdated, Some(true));
        assert_eq!(
            comments[1].html_url.as_deref(),
            Some("https://gitlab.example/group/repo/-/merge_requests/7#note_102")
        );
    }

    #[test]
    fn ignores_general_gitlab_discussions_without_a_diff_position() {
        let comments = map_gitlab_review_comments(
            vec![GitlabDiscussion {
                id: "general-discussion".to_string(),
                notes: vec![note(201, None)],
            }],
            None,
            None,
        );

        assert!(comments.is_empty());
    }

    fn pipeline_job(status: &str, archived: bool) -> GitlabPipelineJob {
        GitlabPipelineJob {
            id: 1,
            name: "verify".to_string(),
            stage: "test".to_string(),
            status: status.to_string(),
            duration: None,
            queued_duration: None,
            web_url: None,
            allow_failure: Some(false),
            archived: Some(archived),
            artifacts: Vec::new(),
        }
    }

    #[test]
    fn only_completed_non_archived_jobs_are_retryable() {
        assert!(pipeline_job("failed", false).is_retryable());
        assert!(pipeline_job("success", false).is_retryable());
        assert!(!pipeline_job("running", false).is_retryable());
        assert!(!pipeline_job("failed", true).is_retryable());
    }

    #[test]
    fn normalizes_github_reviewers_decision_and_merge_state() {
        let output = map_github_review_state(
            json!({
                "url": "https://github.com/dcc/app/pull/12",
                "reviewDecision": "CHANGES_REQUESTED",
                "latestReviews": [
                    {
                        "state": "APPROVED",
                        "author": {
                            "login": "alice",
                            "name": "Alice",
                            "avatarUrl": "https://example.test/alice.png",
                            "url": "https://github.com/alice"
                        }
                    },
                    {
                        "state": "CHANGES_REQUESTED",
                        "author": { "login": "bob" }
                    }
                ],
                "reviewRequests": [
                    { "login": "carol" },
                    { "slug": "platform-team" }
                ],
                "mergeable": "CONFLICTING",
                "mergeStateStatus": "DIRTY",
                "isDraft": false
            }),
            Some("github.com".to_string()),
            12,
            None,
            Some(3),
        );

        assert_eq!(output.review_state.as_deref(), Some("changes_requested"));
        assert_eq!(output.approvals_received, 1);
        assert!(output.approvals_available);
        assert_eq!(output.reviewers.len(), 4);
        assert!(output
            .reviewers
            .iter()
            .any(|reviewer| reviewer.login.as_deref() == Some("platform-team")));
        assert_eq!(output.has_conflicts, Some(true));
        assert_eq!(output.behind_by, Some(3));
    }

    fn review_user(login: &str, name: &str) -> GitlabReviewUser {
        GitlabReviewUser {
            username: Some(login.to_string()),
            name: Some(name.to_string()),
            avatar_url: None,
            web_url: Some(format!("https://gitlab.example/{login}")),
        }
    }

    #[test]
    fn normalizes_gitlab_pending_approvals_and_reviewers() {
        let output = map_gitlab_review_state(
            GitlabMergeRequestReview {
                detail: json!({
                    "web_url": "https://gitlab.example/dcc/app/-/merge_requests/7",
                    "detailed_merge_status": "not_approved",
                    "merge_status": "can_be_merged",
                    "has_conflicts": false,
                    "diverged_commits_count": 2,
                    "blocking_discussions_resolved": false,
                    "draft": false
                }),
                reviewers: Some(vec![
                    GitlabReviewer {
                        user: review_user("alice", "Alice"),
                        state: Some("unreviewed".to_string()),
                    },
                    GitlabReviewer {
                        user: review_user("bob", "Bob"),
                        state: Some("reviewed".to_string()),
                    },
                ]),
                approvals: Some(GitlabApprovals {
                    approvals_required: Some(2),
                    approvals_left: Some(1),
                    approved: Some(false),
                    approved_by: vec![GitlabApproval {
                        user: review_user("bob", "Bob"),
                    }],
                }),
            },
            Some("gitlab.example".to_string()),
            7,
            None,
        );

        assert_eq!(output.review_state.as_deref(), Some("pending"));
        assert_eq!(output.approvals_required, Some(2));
        assert_eq!(output.approvals_received, 1);
        assert_eq!(output.approvals_left, Some(1));
        assert_eq!(output.behind_by, Some(2));
        assert_eq!(output.discussions_resolved, Some(false));
        assert_eq!(output.has_conflicts, Some(false));
        assert!(output
            .reviewers
            .iter()
            .any(|reviewer| reviewer.login.as_deref() == Some("alice")
                && reviewer.state == "pending"));
        assert!(output.reviewers.iter().any(
            |reviewer| reviewer.login.as_deref() == Some("bob") && reviewer.state == "approved"
        ));
    }
}

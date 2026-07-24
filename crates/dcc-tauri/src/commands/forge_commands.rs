use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;

use dcc_core::domain::workspace::{WorkspaceSource, WorkspaceSourceKind};
use dcc_infra::db::SqliteWorkspaceRepo;

use crate::{
    commands::forge::context as forge_context,
    commands::forge::provider as forge_provider,
    commands::workspace_commands::{push_current_branch, RepositoryIdInput, WorkspaceGitPushInput},
    commands::workspace_support::{
        ensure_pushable_branch, find_workspace_by_root, preflight_workspace_root,
        resolve_branch_diff_base, resolve_current_branch_name, resolve_current_commit_sha,
        resolve_workspace_target_branch, workspace_branch_hints,
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
pub struct WorkspacePrStatusOutput {
    pub provider: Option<String>,
    pub host: Option<String>,
    pub number: Option<u32>,
    pub title: Option<String>,
    pub url: Option<String>,
    pub head_branch: Option<String>,
    pub base_branch: Option<String>,
    pub state: Option<String>,
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
    crate::commands::forge::accounts::auto_bind_repository(
        &repo,
        &dcc_core::domain::repository::RepositoryId(input.repository_id),
    )
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
    input: WorkspaceGitPushInput,
) -> Result<(), String> {
    preflight_workspace_root(&state, &input.workspace_root).await?;
    let root = input.workspace_root.trim();
    if root.is_empty() {
        return Err("workspace_root is empty".to_string());
    }

    let source = imported_workspace_source(&state, root).await?;
    let protected_branch = resolve_workspace_target_branch(&state, root).await;
    let head_branch = if let Some(source) = source.as_ref() {
        source.head_branch.clone()
    } else {
        ensure_pushable_branch(root, protected_branch.as_deref())?
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
        login.as_deref(),
    )
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
    workspace_change_request_create(state, input).await
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
    if !gitlab_job_is_retryable(job) {
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

fn gitlab_job_is_retryable(job: &crate::commands::forge::gitlab::GitlabPipelineJob) -> bool {
    !job.archived.unwrap_or(false)
        && matches!(job.status.as_str(), "failed" | "canceled" | "success")
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
                let retryable = gitlab_job_is_retryable(&job);
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
        gitlab_job_is_retryable, map_github_review_state, map_gitlab_review_comments,
        map_gitlab_review_state,
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
        assert!(gitlab_job_is_retryable(&pipeline_job("failed", false)));
        assert!(gitlab_job_is_retryable(&pipeline_job("success", false)));
        assert!(!gitlab_job_is_retryable(&pipeline_job("running", false)));
        assert!(!gitlab_job_is_retryable(&pipeline_job("failed", true)));
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

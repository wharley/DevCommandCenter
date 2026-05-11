use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;

use dcc_infra::db::SqliteWorkspaceRepo;

use crate::{
    commands::forge::context as forge_context,
    commands::forge::provider as forge_provider,
    commands::workspace_commands::WorkspaceGitPushInput,
    commands::workspace_support::{
        ensure_pushable_branch, preflight_workspace_root, push_branch_refspec,
        resolve_branch_diff_base, resolve_current_branch_name, resolve_current_commit_sha,
        resolve_workspace_target_branch, workspace_branch_hints,
    },
    state::WorkspaceCommandState,
};

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
    let branch = resolve_current_branch_name(root)?;
    let login = forge_context::resolve_workspace_forge_context(
        &state.db_path,
        root,
        input.forge_login.as_deref(),
    )?
    .and_then(|context| context.effective_login);
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

    let protected_branch = resolve_workspace_target_branch(&state, root).await;
    let head_branch = ensure_pushable_branch(root, protected_branch.as_deref())?;
    let base_ref = resolve_branch_diff_base(root, protected_branch.as_deref())
        .unwrap_or_else(|| "main".to_string());
    let base_stripped = base_ref
        .split_once('/')
        .map(|(_, branch)| branch)
        .unwrap_or(&base_ref);
    let base_branch = if base_stripped == "HEAD" {
        "main"
    } else {
        base_stripped
    };
    let login = forge_context::resolve_workspace_forge_context(
        &state.db_path,
        root,
        input.forge_login.as_deref(),
    )?
    .and_then(|context| context.effective_login);

    push_branch_refspec(&state.db_path, root, &head_branch, login.as_deref())
        .map_err(|error| format!("git push failed: {error}"))?;

    forge_provider::create_workspace_change_request(
        root,
        base_branch,
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
    let branch = match input
        .branch
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        Some(branch) => branch,
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

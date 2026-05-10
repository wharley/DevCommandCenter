use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;

use dcc_infra::db::SqliteWorkspaceRepo;

use crate::{
    commands::forge::provider as forge_provider,
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
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ForgeCliAccountsInput {
    pub provider: ForgeCliProvider,
    pub host: Option<String>,
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

fn default_forge_host(provider: ForgeCliProvider) -> &'static str {
    match provider {
        ForgeCliProvider::Github => "github.com",
        ForgeCliProvider::Gitlab => "gitlab.com",
    }
}

fn forge_provider_key(provider: ForgeCliProvider) -> &'static str {
    match provider {
        ForgeCliProvider::Github => "github",
        ForgeCliProvider::Gitlab => "gitlab",
    }
}

fn normalize_forge_host(provider: ForgeCliProvider, host: Option<String>) -> Result<String, String> {
    let host = host
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(default_forge_host(provider));

    if host.contains(['\n', '\r']) || host.chars().any(char::is_whitespace) {
        return Err(format!("Invalid host `{host}`."));
    }

    Ok(host.to_string())
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
) -> Result<ForgeCliStatusOutput, String> {
    let status = forge_provider::resolve_forge_cli_status(provider, host)?;
    let repo = SqliteWorkspaceRepo::open(&state.db_path).map_err(|error| error.to_string())?;
    let stored_selected_login = repo
        .get_forge_login_preference(forge_provider_key(provider), host)
        .map_err(|error| error.to_string())?;
    let selected_login = stored_selected_login
        .filter(|login| status.logins.iter().any(|candidate| candidate == login))
        .or_else(|| status.login.clone());

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
    let host = normalize_forge_host(input.provider, input.host)?;
    resolve_forge_cli_snapshot(&state, input.provider, &host)
}

#[tauri::command]
pub async fn workspace_forge_cli_accounts(
    state: State<'_, WorkspaceCommandState>,
    input: ForgeCliAccountsInput,
) -> Result<ForgeCliAccountsOutput, String> {
    let host = normalize_forge_host(input.provider, input.host)?;
    let snapshot = resolve_forge_cli_snapshot(&state, input.provider, &host)?;
    let selected_login = snapshot.selected_login.clone();
    let active_login = snapshot.login.clone();
    let accounts = snapshot
        .logins
        .iter()
        .map(|login| ForgeCliAccountEntry {
            login: login.clone(),
            active: active_login.as_deref() == Some(login.as_str()),
            selected: selected_login.as_deref() == Some(login.as_str()),
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
pub async fn workspace_forge_cli_select_login(
    state: State<'_, WorkspaceCommandState>,
    input: ForgeCliSelectLoginInput,
) -> Result<(), String> {
    let host = normalize_forge_host(input.provider, input.host)?;
    let repo = SqliteWorkspaceRepo::open(&state.db_path).map_err(|error| error.to_string())?;
    repo.set_forge_login_preference(
        forge_provider_key(input.provider),
        &host,
        input.login.as_deref(),
    )
    .map_err(|error| error.to_string())
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
        },
    )
    .await?;
    Ok(legacy_github_cli_status(output))
}

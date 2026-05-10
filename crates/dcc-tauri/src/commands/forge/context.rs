use std::path::Path;

use dcc_infra::db::SqliteWorkspaceRepo;

use crate::commands::forge::{
    provider::{self as forge_provider, ResolvedCliStatus, ResolvedGitAuth},
    remote::resolve_workspace_forge_target,
};
use crate::commands::forge_commands::ForgeCliProvider;

#[derive(Debug, Clone)]
pub(crate) struct ResolvedWorkspaceForgeContext {
    pub(crate) provider: ForgeCliProvider,
    pub(crate) host: String,
    pub(crate) remote_name: String,
    pub(crate) namespace: String,
    pub(crate) repo: String,
    pub(crate) cli_name: String,
    pub(crate) ready: bool,
    pub(crate) login: Option<String>,
    pub(crate) selected_login: Option<String>,
    pub(crate) effective_login: Option<String>,
    pub(crate) message: String,
    pub(crate) login_command: String,
}

pub(crate) fn forge_provider_key(provider: ForgeCliProvider) -> &'static str {
    match provider {
        ForgeCliProvider::Github => "github",
        ForgeCliProvider::Gitlab => "gitlab",
    }
}

pub(crate) fn default_forge_host(provider: ForgeCliProvider) -> &'static str {
    match provider {
        ForgeCliProvider::Github => "github.com",
        ForgeCliProvider::Gitlab => "gitlab.com",
    }
}

pub(crate) fn normalize_forge_host(
    provider: ForgeCliProvider,
    host: Option<String>,
) -> Result<String, String> {
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

pub(crate) fn resolve_selected_forge_login(
    db_path: &Path,
    provider: ForgeCliProvider,
    host: &str,
    status: &ResolvedCliStatus,
) -> Result<Option<String>, String> {
    let repo = SqliteWorkspaceRepo::open(db_path).map_err(|error| error.to_string())?;
    let stored_selected_login = repo
        .get_forge_login_preference(forge_provider_key(provider), host)
        .map_err(|error| error.to_string())?;
    Ok(stored_selected_login
        .filter(|login| status.logins.iter().any(|candidate| candidate == login))
        .or_else(|| status.login.clone()))
}

pub(crate) fn resolve_workspace_forge_context(
    db_path: &Path,
    root: &str,
    requested_login: Option<&str>,
) -> Result<Option<ResolvedWorkspaceForgeContext>, String> {
    let Some(target) = resolve_workspace_forge_target(root)? else {
        return Ok(None);
    };

    let status = forge_provider::resolve_forge_cli_status(target.provider, &target.remote.host)?;
    let selected_login =
        resolve_selected_forge_login(db_path, target.provider, &target.remote.host, &status)?;
    let requested_login = requested_login
        .map(str::trim)
        .filter(|login| !login.is_empty())
        .filter(|login| status.logins.iter().any(|candidate| candidate == login))
        .map(ToString::to_string);
    let effective_login = requested_login.or_else(|| selected_login.clone());

    Ok(Some(ResolvedWorkspaceForgeContext {
        provider: target.provider,
        host: target.remote.host.clone(),
        remote_name: target.remote_name,
        namespace: target.remote.namespace,
        repo: target.remote.repo,
        cli_name: status.cli_name,
        ready: status.ready,
        login: status.login,
        selected_login,
        effective_login,
        message: status.message,
        login_command: status.login_command,
    }))
}

pub(crate) fn resolve_workspace_git_auth(
    db_path: &Path,
    root: &str,
    requested_login: Option<&str>,
) -> Result<Option<ResolvedGitAuth>, String> {
    let Some(context) = resolve_workspace_forge_context(db_path, root, requested_login)? else {
        return Ok(None);
    };

    let Some(login) = context.effective_login.as_deref() else {
        return Ok(None);
    };

    forge_provider::resolve_forge_git_auth(context.provider, &context.host, Some(login))
}

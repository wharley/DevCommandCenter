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
    let resolved_login = stored_selected_login
        .as_ref()
        .filter(|login| status.logins.iter().any(|candidate| candidate == *login))
        .cloned()
        .or_else(|| status.login.clone())
        .or_else(|| status.logins.first().cloned());

    if stored_selected_login != resolved_login {
        repo.set_forge_login_preference(
            forge_provider_key(provider),
            host,
            resolved_login.as_deref(),
        )
        .map_err(|error| error.to_string())?;
    }

    Ok(resolved_login)
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

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::commands::forge::provider::ResolvedCliStatus;

    use super::*;

    fn temp_db_path(name: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("dcc-{name}-{unique}.sqlite"))
    }

    fn sample_status(logins: &[&str], active_login: Option<&str>) -> ResolvedCliStatus {
        ResolvedCliStatus {
            provider: ForgeCliProvider::Github,
            cli_name: "gh".to_string(),
            hostname: "github.com".to_string(),
            ready: !logins.is_empty(),
            login: active_login.map(ToString::to_string),
            logins: logins.iter().map(|login| (*login).to_string()).collect(),
            message: String::new(),
            login_command: "gh auth login".to_string(),
        }
    }

    #[test]
    fn rewrites_stale_selected_login_to_active_login() {
        let db_path = temp_db_path("stale-login");
        let repo = SqliteWorkspaceRepo::open(&db_path).unwrap();
        repo.set_forge_login_preference("github", "github.com", Some("stale-user"))
            .unwrap();

        let resolved = resolve_selected_forge_login(
            &db_path,
            ForgeCliProvider::Github,
            "github.com",
            &sample_status(&["fresh-user"], Some("fresh-user")),
        )
        .unwrap();

        assert_eq!(resolved, Some("fresh-user".to_string()));
        assert_eq!(
            repo.get_forge_login_preference("github", "github.com").unwrap(),
            Some("fresh-user".to_string())
        );

        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn clears_selected_login_when_host_has_no_accounts() {
        let db_path = temp_db_path("clear-login");
        let repo = SqliteWorkspaceRepo::open(&db_path).unwrap();
        repo.set_forge_login_preference("github", "github.com", Some("stale-user"))
            .unwrap();

        let resolved = resolve_selected_forge_login(
            &db_path,
            ForgeCliProvider::Github,
            "github.com",
            &sample_status(&[], None),
        )
        .unwrap();

        assert_eq!(resolved, None);
        assert_eq!(
            repo.get_forge_login_preference("github", "github.com").unwrap(),
            None
        );

        let _ = std::fs::remove_file(db_path);
    }
}

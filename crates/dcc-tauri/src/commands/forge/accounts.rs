use dcc_core::domain::repository::{Repository, RepositoryId};
use dcc_core::ports::RepositoryRepo;
use dcc_infra::db::SqliteWorkspaceRepo;

use crate::commands::forge::{github, gitlab, remote::parse_remote};
use crate::commands::forge_commands::ForgeCliProvider;

#[derive(Debug, Clone)]
pub(crate) struct ForgeCliAccountProfile {
    pub(crate) login: String,
    pub(crate) name: Option<String>,
    pub(crate) avatar_url: Option<String>,
    pub(crate) email: Option<String>,
    pub(crate) active: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct ForgeCliAuthStatus {
    pub(crate) active_login: Option<String>,
    pub(crate) logins: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct ForgeCliAuthContext {
    pub(crate) envs: Vec<(String, String)>,
    pub(crate) git_http_authorization: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AuthCheck {
    LoggedIn,
    LoggedOut,
    Indeterminate,
}

impl AuthCheck {
    pub(crate) fn is_definitely_logged_out(self) -> bool {
        matches!(self, Self::LoggedOut)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RepoAccess {
    Push,
    Probable,
    None,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct BackfillSummary {
    #[allow(dead_code)]
    pub(crate) examined: usize,
    pub(crate) bound: usize,
}

#[derive(Debug, Clone)]
struct RepoForgeTarget {
    provider: ForgeCliProvider,
    host: String,
    owner: String,
    name: String,
}

pub(crate) trait ForgeCliBackend: Sync {
    fn cli_name(&self) -> &'static str;
    fn provider_label(&self) -> &'static str;
    fn login_command(&self, host: &str) -> String;
    fn auth_status(&self, host: &str, force_refresh: bool) -> Result<ForgeCliAuthStatus, String>;
    fn list_logins(&self, host: &str, force_refresh: bool) -> Result<Vec<String>, String>;
    fn list_hosts(&self, force_refresh: bool) -> Result<Vec<String>, String>;
    fn list_accounts(
        &self,
        host: &str,
        force_refresh: bool,
    ) -> Result<Vec<ForgeCliAccountProfile>, String>;
    fn check_auth(&self, host: &str, login: &str) -> AuthCheck;
    fn repo_access(
        &self,
        host: &str,
        login: &str,
        owner: &str,
        name: &str,
    ) -> Result<RepoAccess, String>;
    fn resolve_auth_context(
        &self,
        host: &str,
        login: Option<&str>,
    ) -> Result<Option<ForgeCliAuthContext>, String>;
}

pub(crate) fn backend_for(provider: ForgeCliProvider) -> &'static dyn ForgeCliBackend {
    match provider {
        ForgeCliProvider::Github => &GithubCliBackend,
        ForgeCliProvider::Gitlab => &GitlabCliBackend,
    }
}

struct GithubCliBackend;

impl ForgeCliBackend for GithubCliBackend {
    fn cli_name(&self) -> &'static str {
        "gh"
    }

    fn provider_label(&self) -> &'static str {
        "GitHub"
    }

    fn login_command(&self, host: &str) -> String {
        if host == "github.com" {
            "gh auth login".to_string()
        } else {
            format!("gh auth login --hostname {host}")
        }
    }

    fn auth_status(&self, host: &str, force_refresh: bool) -> Result<ForgeCliAuthStatus, String> {
        let status = github::auth_status_with_options(host, force_refresh)?;
        Ok(ForgeCliAuthStatus {
            active_login: status.active_login,
            logins: status.logins,
        })
    }

    fn list_logins(&self, host: &str, force_refresh: bool) -> Result<Vec<String>, String> {
        github::list_logins(host, force_refresh)
    }

    fn list_hosts(&self, force_refresh: bool) -> Result<Vec<String>, String> {
        github::list_authenticated_hosts(force_refresh)
    }

    fn list_accounts(
        &self,
        host: &str,
        force_refresh: bool,
    ) -> Result<Vec<ForgeCliAccountProfile>, String> {
        github::list_accounts_for_host(host, force_refresh)
    }

    fn check_auth(&self, host: &str, login: &str) -> AuthCheck {
        github::check_auth(host, login)
    }

    fn repo_access(
        &self,
        host: &str,
        login: &str,
        owner: &str,
        name: &str,
    ) -> Result<RepoAccess, String> {
        github::repo_access(host, login, owner, name)
    }

    fn resolve_auth_context(
        &self,
        host: &str,
        login: Option<&str>,
    ) -> Result<Option<ForgeCliAuthContext>, String> {
        let Some(auth) = github::resolve_auth_context(host, login)? else {
            return Ok(None);
        };
        Ok(Some(ForgeCliAuthContext {
            envs: auth.envs,
            git_http_authorization: auth.git_http_authorization,
        }))
    }
}

struct GitlabCliBackend;

impl ForgeCliBackend for GitlabCliBackend {
    fn cli_name(&self) -> &'static str {
        "glab"
    }

    fn provider_label(&self) -> &'static str {
        "GitLab"
    }

    fn login_command(&self, host: &str) -> String {
        format!("glab auth login --hostname {host}")
    }

    fn auth_status(&self, host: &str, force_refresh: bool) -> Result<ForgeCliAuthStatus, String> {
        let status = gitlab::auth_status_with_options(host, force_refresh)?;
        Ok(ForgeCliAuthStatus {
            active_login: status.active_login,
            logins: status.logins,
        })
    }

    fn list_logins(&self, host: &str, force_refresh: bool) -> Result<Vec<String>, String> {
        gitlab::list_logins(host, force_refresh)
    }

    fn list_hosts(&self, force_refresh: bool) -> Result<Vec<String>, String> {
        gitlab::list_authenticated_hosts(force_refresh)
    }

    fn list_accounts(
        &self,
        host: &str,
        force_refresh: bool,
    ) -> Result<Vec<ForgeCliAccountProfile>, String> {
        gitlab::list_accounts_for_host(host, force_refresh)
    }

    fn check_auth(&self, host: &str, login: &str) -> AuthCheck {
        gitlab::check_auth(host, login)
    }

    fn repo_access(
        &self,
        host: &str,
        login: &str,
        owner: &str,
        name: &str,
    ) -> Result<RepoAccess, String> {
        gitlab::repo_access(host, login, owner, name)
    }

    fn resolve_auth_context(
        &self,
        host: &str,
        login: Option<&str>,
    ) -> Result<Option<ForgeCliAuthContext>, String> {
        let Some(auth) = gitlab::resolve_auth_context(host, login)? else {
            return Ok(None);
        };
        Ok(Some(ForgeCliAuthContext {
            envs: auth.envs,
            git_http_authorization: auth.git_http_authorization,
        }))
    }
}

fn repo_forge_target(repository: &Repository) -> Option<RepoForgeTarget> {
    let provider = match repository.forge_provider.as_deref()? {
        "github" => ForgeCliProvider::Github,
        "gitlab" => ForgeCliProvider::Gitlab,
        _ => return None,
    };
    let parsed = parse_remote(repository.remote_url.as_deref()?)?;
    Some(RepoForgeTarget {
        provider,
        host: parsed.host,
        owner: parsed.namespace,
        name: parsed.repo,
    })
}

pub(crate) fn auto_bind_repository(
    repo: &SqliteWorkspaceRepo,
    repository_id: &RepositoryId,
) -> Result<Option<String>, String> {
    let Some(repository) = futures::executor::block_on(repo.get_repository(repository_id))
        .map_err(|error| error.to_string())?
    else {
        return Ok(None);
    };
    let Some(target) = repo_forge_target(&repository) else {
        return Ok(None);
    };
    let backend = backend_for(target.provider);
    let candidates = backend.list_logins(&target.host, false)?;
    if candidates.is_empty() {
        return Ok(None);
    }

    let mut confirmed = Vec::new();
    let mut probable = Vec::new();
    for login in &candidates {
        match backend.repo_access(&target.host, login, &target.owner, &target.name) {
            Ok(RepoAccess::Push) => confirmed.push(login.clone()),
            Ok(RepoAccess::Probable) => probable.push(login.clone()),
            Ok(RepoAccess::None) => {}
            Err(_error) => {}
        }
    }

    let chosen = confirmed
        .first()
        .cloned()
        .or_else(|| probable.first().cloned());
    let Some(chosen) = chosen else {
        return Ok(None);
    };

    repo.update_repository_forge_login(repository_id, Some(chosen.as_str()))
        .map_err(|error| error.to_string())?;
    Ok(Some(chosen))
}

pub(crate) fn backfill_repository_bindings(
    repo: &SqliteWorkspaceRepo,
) -> Result<BackfillSummary, String> {
    let unbound = repo
        .list_repositories_needing_forge_binding()
        .map_err(|error| error.to_string())?;
    let stale = repo
        .list_forge_bound_repositories()
        .map_err(|error| error.to_string())?;
    let mut summary = BackfillSummary {
        examined: unbound.len() + stale.len(),
        ..BackfillSummary::default()
    };

    for repository_id in &unbound {
        match auto_bind_repository(repo, repository_id) {
            Ok(Some(_)) => summary.bound += 1,
            Ok(None) => {}
            Err(_error) => {}
        }
    }

    for entry in &stale {
        let Some(repository) = futures::executor::block_on(repo.get_repository(&entry.id))
            .map_err(|error| error.to_string())?
        else {
            continue;
        };
        let Some(target) = repo_forge_target(&repository) else {
            continue;
        };
        let backend = backend_for(target.provider);
        if !backend
            .check_auth(&target.host, &entry.login)
            .is_definitely_logged_out()
        {
            continue;
        }

        repo.update_repository_forge_login(&entry.id, None)
            .map_err(|error| error.to_string())?;
        match auto_bind_repository(repo, &entry.id) {
            Ok(Some(_)) => summary.bound += 1,
            Ok(None) => {}
            Err(_error) => {}
        }
    }

    Ok(summary)
}

#[cfg(test)]
mod tests {
    use dcc_core::domain::{project::ProjectId, repository::RepositoryId};

    use super::*;

    fn sample_repository(provider: Option<&str>, remote_url: Option<&str>) -> Repository {
        Repository {
            id: RepositoryId("/tmp/repo".to_string()),
            project_id: ProjectId("project-1".to_string()),
            name: "repo".to_string(),
            root_path: "/tmp/repo".to_string(),
            base_branch: "main".to_string(),
            remote: Some("origin".to_string()),
            remote_url: remote_url.map(ToString::to_string),
            forge_provider: provider.map(ToString::to_string),
            forge_login: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn repo_target_from_repository_parses_github_remote() {
        let repository =
            sample_repository(Some("github"), Some("git@github.com:acme/platform-api.git"));
        let target = repo_forge_target(&repository).expect("target");
        assert_eq!(target.provider, ForgeCliProvider::Github);
        assert_eq!(target.host, "github.com");
        assert_eq!(target.owner, "acme");
        assert_eq!(target.name, "platform-api");
    }

    #[test]
    fn repo_target_from_repository_rejects_unknown_provider() {
        let repository = sample_repository(
            Some("unknown"),
            Some("git@github.com:acme/platform-api.git"),
        );
        assert!(repo_forge_target(&repository).is_none());
    }

    #[test]
    fn auth_check_is_definitely_logged_out_only_for_logged_out() {
        assert!(AuthCheck::LoggedOut.is_definitely_logged_out());
        assert!(!AuthCheck::LoggedIn.is_definitely_logged_out());
        assert!(!AuthCheck::Indeterminate.is_definitely_logged_out());
    }
}

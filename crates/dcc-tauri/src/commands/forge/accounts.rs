use crate::commands::forge::{github, gitlab};
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

pub(crate) trait ForgeCliBackend: Sync {
    fn cli_name(&self) -> &'static str;
    fn provider_label(&self) -> &'static str;
    fn login_command(&self, host: &str) -> String;
    fn auth_status(&self, host: &str, force_refresh: bool) -> Result<ForgeCliAuthStatus, String>;
    fn list_accounts(
        &self,
        host: &str,
        force_refresh: bool,
    ) -> Result<Vec<ForgeCliAccountProfile>, String>;
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

    fn list_accounts(
        &self,
        host: &str,
        force_refresh: bool,
    ) -> Result<Vec<ForgeCliAccountProfile>, String> {
        github::list_accounts_for_host(host, force_refresh)
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

    fn list_accounts(
        &self,
        host: &str,
        force_refresh: bool,
    ) -> Result<Vec<ForgeCliAccountProfile>, String> {
        gitlab::list_accounts_for_host(host, force_refresh)
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

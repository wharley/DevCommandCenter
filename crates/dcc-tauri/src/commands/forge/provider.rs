use crate::commands::forge::{github, gitlab};
use crate::commands::forge::remote::resolve_workspace_forge_target;
use crate::commands::forge_commands::ForgeCliProvider;

#[derive(Debug, Clone)]
pub(crate) struct ResolvedCliStatus {
    pub(crate) provider: ForgeCliProvider,
    pub(crate) cli_name: String,
    pub(crate) hostname: String,
    pub(crate) ready: bool,
    pub(crate) login: Option<String>,
    pub(crate) logins: Vec<String>,
    pub(crate) message: String,
    pub(crate) login_command: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ResolvedChangeRequestStatus {
    pub(crate) provider: String,
    pub(crate) host: Option<String>,
    pub(crate) number: Option<u32>,
    pub(crate) title: Option<String>,
    pub(crate) url: Option<String>,
    pub(crate) head_branch: Option<String>,
    pub(crate) base_branch: Option<String>,
    pub(crate) state: Option<String>,
    pub(crate) mergeable: Option<String>,
    pub(crate) merge_state_status: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct ResolvedGitAuth {
    pub(crate) host: String,
    pub(crate) git_http_authorization: String,
    pub(crate) envs: Vec<(String, String)>,
}

fn cli_name_for_provider(provider: ForgeCliProvider) -> &'static str {
    match provider {
        ForgeCliProvider::Github => "gh",
        ForgeCliProvider::Gitlab => "glab",
    }
}

fn provider_label(provider: ForgeCliProvider) -> &'static str {
    match provider {
        ForgeCliProvider::Github => "GitHub",
        ForgeCliProvider::Gitlab => "GitLab",
    }
}

fn login_command_for(provider: ForgeCliProvider, host: &str) -> String {
    match provider {
        ForgeCliProvider::Github if host == "github.com" => "gh auth login".to_string(),
        ForgeCliProvider::Github => format!("gh auth login --hostname {host}"),
        ForgeCliProvider::Gitlab => format!("glab auth login --hostname {host}"),
    }
}

pub(crate) fn resolve_forge_cli_status(
    provider: ForgeCliProvider,
    host: &str,
) -> Result<ResolvedCliStatus, String> {
    let result = match provider {
        ForgeCliProvider::Github => github::auth_status(host).map(|status| {
            (status.logins, status.active_login)
        }),
        ForgeCliProvider::Gitlab => gitlab::auth_status(host).map(|status| {
            (status.logins, status.active_login)
        }),
    };

    match result {
        Ok((logins, login)) if !logins.is_empty() => {
            let message = if logins.len() == 1 {
                format!("Logged in as {}", login.as_deref().unwrap_or(logins[0].as_str()))
            } else {
                format!("{} accounts available: {}", logins.len(), logins.join(", "))
            };
            Ok(ResolvedCliStatus {
                provider,
                cli_name: cli_name_for_provider(provider).to_string(),
                hostname: host.to_string(),
                ready: true,
                login,
                logins,
                message,
                login_command: login_command_for(provider, host),
            })
        }
        Ok(_) => Ok(ResolvedCliStatus {
            provider,
            cli_name: cli_name_for_provider(provider).to_string(),
            hostname: host.to_string(),
            ready: false,
            login: None,
            logins: Vec::new(),
            message: format!(
                "Run `{}` to connect {} locally.",
                login_command_for(provider, host),
                provider_label(provider)
            ),
            login_command: login_command_for(provider, host),
        }),
        Err(message) => Ok(ResolvedCliStatus {
            provider,
            cli_name: cli_name_for_provider(provider).to_string(),
            hostname: host.to_string(),
            ready: false,
            login: None,
            logins: Vec::new(),
            message,
            login_command: login_command_for(provider, host),
        }),
    }
}

pub(crate) fn resolve_workspace_change_request_status(
    root: &str,
    branch: &str,
    branch_hints: &[String],
    head_sha: Option<&str>,
    login: Option<&str>,
) -> Result<Option<ResolvedChangeRequestStatus>, String> {
    let target = resolve_workspace_forge_target(root)?;
    match target {
        Some(target) if target.provider == ForgeCliProvider::Gitlab => {
            let raw_mr = gitlab::resolve_change_request_json(root, branch, &target, login)?;
            let Some(raw_mr) = raw_mr else {
                return Ok(None);
            };
            Ok(Some(ResolvedChangeRequestStatus {
                provider: "gitlab".to_string(),
                host: Some(target.remote.host.clone()),
                number: raw_mr.get("iid").and_then(|value| value.as_u64()).map(|value| value as u32),
                title: raw_mr.get("title").and_then(|value| value.as_str()).map(ToString::to_string),
                url: raw_mr.get("web_url").and_then(|value| value.as_str()).map(ToString::to_string),
                head_branch: raw_mr.get("source_branch").and_then(|value| value.as_str()).map(ToString::to_string),
                base_branch: raw_mr.get("target_branch").and_then(|value| value.as_str()).map(ToString::to_string),
                state: gitlab::map_state(raw_mr.get("state").and_then(|value| value.as_str())),
                mergeable: gitlab::map_mergeable(&raw_mr),
                merge_state_status: raw_mr
                    .get("detailed_merge_status")
                    .and_then(|value| value.as_str())
                    .or_else(|| raw_mr.get("merge_status").and_then(|value| value.as_str()))
                    .map(ToString::to_string),
            }))
        }
        _ => {
            let github_host = target
                .as_ref()
                .map(|target| target.remote.host.as_str())
                .unwrap_or("github.com");
            let raw_pr = github::resolve_change_request_json(
                root,
                github_host,
                branch_hints,
                head_sha,
                login,
            )?;
            let Some(raw_pr) = raw_pr else {
                return Ok(None);
            };
            let state = raw_pr
                .get("state")
                .and_then(|value| value.as_str())
                .map(|value| value.to_lowercase());
            let state = match state.as_deref() {
                Some("open") | Some("opened") => Some("open".to_string()),
                Some("closed") => Some("closed".to_string()),
                Some("merged") => Some("merged".to_string()),
                _ => None,
            };

            Ok(Some(ResolvedChangeRequestStatus {
                provider: "github".to_string(),
                host: target.as_ref().map(|target| target.remote.host.clone()),
                number: raw_pr.get("number").and_then(|value| value.as_u64()).map(|value| value as u32),
                title: raw_pr.get("title").and_then(|value| value.as_str()).map(ToString::to_string),
                url: raw_pr.get("url").and_then(|value| value.as_str()).map(ToString::to_string),
                head_branch: raw_pr.get("headRefName").and_then(|value| value.as_str()).map(ToString::to_string),
                base_branch: raw_pr.get("baseRefName").and_then(|value| value.as_str()).map(ToString::to_string),
                state,
                mergeable: raw_pr.get("mergeable").and_then(|value| value.as_str()).map(ToString::to_string),
                merge_state_status: raw_pr
                    .get("mergeStateStatus")
                    .and_then(|value| value.as_str())
                    .map(ToString::to_string),
            }))
        }
    }
}

pub(crate) fn view_workspace_change_request(root: &str, login: Option<&str>) -> Result<(), String> {
    match resolve_workspace_forge_target(root)? {
        Some(target) if target.provider == ForgeCliProvider::Gitlab => {
            gitlab::view_change_request_web(root, &target.remote.host, login)
        }
        Some(target) => github::view_change_request_web(root, &target.remote.host, login),
        None => github::view_change_request_web(root, "github.com", login),
    }
}

pub(crate) fn merge_workspace_change_request(
    root: &str,
    branch: &str,
    login: Option<&str>,
) -> Result<(), String> {
    match resolve_workspace_forge_target(root)? {
        Some(target) if target.provider == ForgeCliProvider::Gitlab => {
            gitlab::merge_change_request(root, branch, &target, login)
        }
        Some(target) => github::merge_change_request(root, &target.remote.host, login),
        None => github::merge_change_request(root, "github.com", login),
    }
}

pub(crate) fn create_workspace_change_request(
    root: &str,
    base_branch: &str,
    head_branch: &str,
    login: Option<&str>,
) -> Result<(), String> {
    match resolve_workspace_forge_target(root)? {
        Some(target) if target.provider == ForgeCliProvider::Gitlab => {
            gitlab::create_change_request(root, base_branch, head_branch, &target.remote.host, login)
        }
        Some(target) => {
            github::create_change_request(root, base_branch, head_branch, &target.remote.host, login)
        }
        None => github::create_change_request(root, base_branch, head_branch, "github.com", login),
    }
}

pub(crate) fn resolve_workspace_git_auth(
    root: &str,
    login: Option<&str>,
) -> Result<Option<ResolvedGitAuth>, String> {
    let requested_login = login.map(str::trim).filter(|login| !login.is_empty());
    if requested_login.is_none() {
        return Ok(None);
    }

    let Some(target) = resolve_workspace_forge_target(root)? else {
        return Ok(None);
    };

    match target.provider {
        ForgeCliProvider::Github => {
            let Some(auth) = github::resolve_auth_context(&target.remote.host, requested_login)? else {
                return Ok(None);
            };
            Ok(Some(ResolvedGitAuth {
                host: target.remote.host,
                git_http_authorization: auth.git_http_authorization,
                envs: auth.envs,
            }))
        }
        ForgeCliProvider::Gitlab => {
            let Some(auth) = gitlab::resolve_auth_context(&target.remote.host, requested_login)? else {
                return Ok(None);
            };
            Ok(Some(ResolvedGitAuth {
                host: target.remote.host,
                git_http_authorization: auth.git_http_authorization,
                envs: auth.envs,
            }))
        }
    }
}

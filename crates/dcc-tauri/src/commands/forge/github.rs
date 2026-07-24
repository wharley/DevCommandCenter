use std::process::Command;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde_json::Value;

use crate::commands::forge::accounts::{AuthCheck, ForgeCliAccountProfile, RepoAccess};
use crate::commands::forge::resolve_cli_binary;
use dcc_infra::git::git_output_err;

#[derive(Debug, Clone)]
pub(crate) struct GithubCliAuthStatus {
    pub(crate) logins: Vec<String>,
    pub(crate) active_login: Option<String>,
}

#[derive(Clone)]
struct CachedGithubAuthStatus {
    status: GithubCliAuthStatus,
    cached_at: Instant,
}

const GITHUB_AUTH_STATUS_CACHE_TTL: Duration = Duration::from_secs(2);

static GITHUB_AUTH_STATUS_CACHE: LazyLock<
    Mutex<std::collections::HashMap<String, CachedGithubAuthStatus>>,
> = LazyLock::new(|| Mutex::new(std::collections::HashMap::new()));

#[derive(Clone)]
struct CachedGithubProfile {
    profile: GithubUserProfile,
    cached_at: Instant,
}

static GITHUB_PROFILE_CACHE: LazyLock<
    Mutex<std::collections::HashMap<(String, String), CachedGithubProfile>>,
> = LazyLock::new(|| Mutex::new(std::collections::HashMap::new()));

#[derive(Debug, Clone)]
pub(crate) struct GithubAuthContext {
    pub(crate) envs: Vec<(String, String)>,
    pub(crate) git_http_authorization: String,
}

#[derive(Debug, Clone, Deserialize)]
struct GithubUserProfile {
    login: Option<String>,
    name: Option<String>,
    avatar_url: Option<String>,
    email: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct GithubReviewCommentUser {
    pub(crate) login: Option<String>,
    pub(crate) avatar_url: Option<String>,
    pub(crate) html_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct GithubReviewComment {
    pub(crate) id: i64,
    pub(crate) in_reply_to_id: Option<i64>,
    pub(crate) path: String,
    pub(crate) body: Option<String>,
    pub(crate) diff_hunk: Option<String>,
    pub(crate) html_url: Option<String>,
    pub(crate) side: Option<String>,
    pub(crate) line: Option<i64>,
    pub(crate) start_line: Option<i64>,
    pub(crate) original_line: Option<i64>,
    pub(crate) original_start_line: Option<i64>,
    pub(crate) user: Option<GithubReviewCommentUser>,
    pub(crate) created_at: Option<String>,
    pub(crate) updated_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct GhAuthStatusResponse {
    hosts: std::collections::HashMap<String, Vec<GhHostStatusEntry>>,
}

#[derive(Debug, Clone, Deserialize)]
struct GhHostStatusEntry {
    state: Option<String>,
    login: Option<String>,
    active: Option<bool>,
}

fn parse_github_logins_for_host(stdout: &str, host: &str) -> Result<Vec<String>, String> {
    let parsed: GhAuthStatusResponse = serde_json::from_str(stdout)
        .map_err(|error| format!("Failed to decode `gh auth status --json hosts`: {error}"))?;
    Ok(parsed
        .hosts
        .get(host)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| {
                    let login = entry.login.as_deref()?.trim();
                    if login.is_empty() || entry.state.as_deref() == Some("failure") {
                        return None;
                    }
                    Some(login.to_string())
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default())
}

fn parse_github_authenticated_hosts(stdout: &str) -> Result<Vec<String>, String> {
    let parsed: GhAuthStatusResponse = serde_json::from_str(stdout)
        .map_err(|error| format!("Failed to decode `gh auth status --json hosts`: {error}"))?;
    let mut hosts = parsed
        .hosts
        .into_iter()
        .filter_map(|(host, entries)| {
            let authenticated = entries.iter().any(|entry| {
                entry.state.as_deref() != Some("failure")
                    && entry
                        .login
                        .as_deref()
                        .map(str::trim)
                        .is_some_and(|login| !login.is_empty())
            });
            authenticated.then_some(host)
        })
        .collect::<Vec<_>>();
    hosts.sort();
    hosts.dedup();
    Ok(hosts)
}

fn parse_github_active_login_for_host(stdout: &str, host: &str) -> Result<Option<String>, String> {
    let parsed: GhAuthStatusResponse = serde_json::from_str(stdout)
        .map_err(|error| format!("Failed to decode `gh auth status --json hosts`: {error}"))?;
    let Some(entries) = parsed.hosts.get(host) else {
        return Ok(None);
    };

    let preferred = entries
        .iter()
        .find(|entry| entry.active.unwrap_or(false) && entry.state.as_deref() != Some("failure"))
        .and_then(|entry| entry.login.as_deref())
        .map(str::trim)
        .filter(|login| !login.is_empty())
        .map(ToString::to_string);

    if preferred.is_some() {
        return Ok(preferred);
    }

    Ok(entries
        .iter()
        .filter(|entry| entry.state.as_deref() != Some("failure"))
        .filter_map(|entry| entry.login.as_deref())
        .map(str::trim)
        .find(|login| !login.is_empty())
        .map(ToString::to_string))
}

fn looks_like_github_unauthenticated(message: &str) -> bool {
    let normalized = message.to_ascii_lowercase();
    normalized.contains("not logged in")
        || normalized.contains("not logged into")
        || normalized.contains("no active account")
        || normalized.contains("authenticate")
        || normalized.contains("gh auth login")
}

fn looks_like_github_missing_repo_access(message: &str) -> bool {
    let normalized = message.to_ascii_lowercase();
    normalized.contains("not found")
        || normalized.contains("http 404")
        || normalized.contains("http 403")
        || normalized.contains("forbidden")
        || normalized.contains("resource not accessible")
        || normalized.contains("sso")
}

fn trim_token(stdout: &[u8]) -> Option<String> {
    let token = String::from_utf8_lossy(stdout).trim().to_string();
    (!token.is_empty()).then_some(token)
}

fn github_http_username(login: Option<&str>) -> String {
    login
        .map(str::trim)
        .filter(|login| !login.is_empty())
        .unwrap_or("x-access-token")
        .to_string()
}

pub(crate) fn resolve_auth_context(
    host: &str,
    login: Option<&str>,
) -> Result<Option<GithubAuthContext>, String> {
    let requested_login = login.map(str::trim).filter(|login| !login.is_empty());
    if requested_login.is_none() {
        return Ok(None);
    }

    let status = auth_status(host)?;
    if let Some(login) = requested_login {
        if !status.logins.iter().any(|candidate| candidate == login) {
            return Err(format!(
                "GitHub CLI does not currently list `{login}` as authenticated for `{host}`."
            ));
        }
    }

    let gh = resolve_cli_binary("gh")?;
    let mut command = Command::new(gh);
    command.args(["auth", "token", "--hostname", host]);
    if let Some(login) = requested_login {
        command.args(["--user", login]);
    }
    let output = command.output().map_err(|error| error.to_string())?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            "Failed to resolve GitHub CLI token for the selected account.".to_string()
        } else {
            stderr
        });
    }

    let Some(token) = trim_token(&output.stdout) else {
        return Err("GitHub CLI returned an empty token for the selected account.".to_string());
    };
    let auth = format!("{}:{}", github_http_username(requested_login), token);
    Ok(Some(GithubAuthContext {
        envs: vec![
            ("GH_HOST".to_string(), host.to_string()),
            ("GH_TOKEN".to_string(), token.clone()),
            ("GITHUB_TOKEN".to_string(), token.clone()),
            ("GH_ENTERPRISE_TOKEN".to_string(), token.clone()),
            ("GITHUB_ENTERPRISE_TOKEN".to_string(), token.clone()),
        ],
        git_http_authorization: auth,
    }))
}

pub(crate) fn auth_status(host: &str) -> Result<GithubCliAuthStatus, String> {
    auth_status_with_options(host, false)
}

pub(crate) fn list_logins(host: &str, force_refresh: bool) -> Result<Vec<String>, String> {
    Ok(auth_status_with_options(host, force_refresh)?.logins)
}

pub(crate) fn check_auth(host: &str, login: &str) -> AuthCheck {
    match auth_status(host) {
        Ok(status) if status.logins.iter().any(|candidate| candidate == login) => {
            AuthCheck::LoggedIn
        }
        Ok(_) => AuthCheck::LoggedOut,
        Err(_) => AuthCheck::Indeterminate,
    }
}

pub(crate) fn list_authenticated_hosts(force_refresh: bool) -> Result<Vec<String>, String> {
    let gh = resolve_cli_binary("gh")?;
    let output = Command::new(gh)
        .args(["auth", "status", "--json", "hosts"])
        .output()
        .map_err(|error| error.to_string())?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let combined = format!("{stdout}\n{stderr}");

    if !output.status.success() {
        if looks_like_github_unauthenticated(&combined) {
            return Ok(Vec::new());
        }

        let trimmed = combined.trim();
        return Err(if trimmed.is_empty() {
            "GitHub CLI authentication failed.".to_string()
        } else {
            trimmed.to_string()
        });
    }

    let hosts = parse_github_authenticated_hosts(&stdout)?;
    if force_refresh {
        for host in &hosts {
            auth_status_cache::invalidate(host);
            profile_cache::invalidate_host(host);
        }
    }
    Ok(hosts)
}

pub(crate) fn auth_status_with_options(
    host: &str,
    force_refresh: bool,
) -> Result<GithubCliAuthStatus, String> {
    if !force_refresh {
        if let Some(cached) = auth_status_cache::get(host) {
            return Ok(cached);
        }
    } else {
        auth_status_cache::invalidate(host);
    }

    let gh = resolve_cli_binary("gh")?;
    let output = Command::new(gh)
        .args(["auth", "status", "--hostname", host, "--json", "hosts"])
        .output()
        .map_err(|error| error.to_string())?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let combined = format!("{stdout}\n{stderr}");

    if !output.status.success() {
        if looks_like_github_unauthenticated(&combined) {
            let status = GithubCliAuthStatus {
                logins: Vec::new(),
                active_login: None,
            };
            auth_status_cache::put(host, status.clone());
            return Ok(status);
        }

        let trimmed = combined.trim();
        return Err(if trimmed.is_empty() {
            "GitHub CLI authentication failed.".to_string()
        } else {
            trimmed.to_string()
        });
    }

    let status = GithubCliAuthStatus {
        logins: parse_github_logins_for_host(&stdout, host)?,
        active_login: parse_github_active_login_for_host(&stdout, host)?,
    };
    auth_status_cache::put(host, status.clone());
    Ok(status)
}

pub(crate) fn list_accounts_for_host(
    host: &str,
    force_refresh: bool,
) -> Result<Vec<ForgeCliAccountProfile>, String> {
    let status = auth_status_with_options(host, force_refresh)?;
    if force_refresh {
        profile_cache::invalidate_host(host);
    }

    let accounts = std::thread::scope(|scope| {
        let active_login = status.active_login.clone();
        let handles: Vec<_> = status
            .logins
            .iter()
            .map(|login| {
                let host = host.to_string();
                let login = login.clone();
                let active_login = active_login.clone();
                scope.spawn(move || {
                    let profile = fetch_github_profile(&host, &login).ok();
                    let resolved_login = profile
                        .as_ref()
                        .and_then(|profile| profile.login.clone())
                        .unwrap_or_else(|| login.clone());
                    ForgeCliAccountProfile {
                        login: resolved_login,
                        name: profile.as_ref().and_then(|profile| profile.name.clone()),
                        avatar_url: profile
                            .as_ref()
                            .and_then(|profile| profile.avatar_url.clone()),
                        email: profile.and_then(|profile| profile.email),
                        active: active_login.as_deref() == Some(login.as_str()),
                    }
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().expect("github profile worker panicked"))
            .collect::<Vec<_>>()
    });

    Ok(accounts)
}

pub(crate) fn repo_access(
    host: &str,
    login: &str,
    owner: &str,
    name: &str,
) -> Result<RepoAccess, String> {
    let auth = match resolve_auth_context(host, Some(login)) {
        Ok(Some(auth)) => auth,
        Ok(None) => return Ok(RepoAccess::None),
        Err(error)
            if looks_like_github_unauthenticated(&error)
                || error.contains("does not currently list") =>
        {
            return Ok(RepoAccess::None);
        }
        Err(error) => return Err(error),
    };

    let path = format!("/repos/{owner}/{name}");
    let gh = resolve_cli_binary("gh")?;
    let mut command = Command::new(gh);
    command
        .args([
            "api",
            "--hostname",
            host,
            "-H",
            "Accept: application/vnd.github+json",
            path.as_str(),
        ])
        .envs(auth.envs.iter().map(|(key, value)| (key, value)));
    let output = command.output().map_err(|error| error.to_string())?;
    if !output.status.success() {
        let detail = format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        if looks_like_github_unauthenticated(&detail)
            || looks_like_github_missing_repo_access(&detail)
        {
            return Ok(RepoAccess::None);
        }
        return Err(detail.trim().to_string());
    }

    parse_repo_push_permission(&output.stdout)
}

pub(crate) fn list_pull_review_comments(
    host: &str,
    owner: &str,
    repo: &str,
    pull_number: u32,
    login: Option<&str>,
) -> Result<Vec<GithubReviewComment>, String> {
    let Some(auth) = resolve_auth_context(host, login)? else {
        return Err("GitHub CLI is not authenticated for this repository host.".to_string());
    };

    let path = format!("/repos/{owner}/{repo}/pulls/{pull_number}/comments");
    let gh = resolve_cli_binary("gh")?;
    let mut command = Command::new(gh);
    command
        .args([
            "api",
            "--hostname",
            host,
            "--paginate",
            "--slurp",
            "-H",
            "Accept: application/vnd.github+json",
            path.as_str(),
        ])
        .envs(auth.envs.iter().map(|(key, value)| (key, value)));
    let output = command.output().map_err(|error| error.to_string())?;
    if !output.status.success() {
        let detail = format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let trimmed = detail.trim();
        return Err(if trimmed.is_empty() {
            "Failed to load GitHub pull request review comments.".to_string()
        } else {
            trimmed.to_string()
        });
    }

    let pages: Vec<Vec<GithubReviewComment>> = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("Failed to decode GitHub review comments: {error}"))?;
    Ok(pages.into_iter().flatten().collect())
}

fn fetch_github_profile(host: &str, login: &str) -> Result<GithubUserProfile, String> {
    if let Some(cached) = profile_cache::get(host, login) {
        return Ok(cached);
    }

    let Some(auth) = resolve_auth_context(host, Some(login))? else {
        return Err(format!(
            "GitHub CLI could not resolve auth context for `{login}` on `{host}`."
        ));
    };

    let gh = resolve_cli_binary("gh")?;
    let mut command = Command::new(gh);
    command
        .args(["api", "--hostname", host, "/user"])
        .envs(auth.envs.iter().map(|(key, value)| (key, value)));
    let output = command.output().map_err(|error| error.to_string())?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            format!("`gh api /user` failed for `{login}`.")
        } else {
            stderr
        });
    }

    let profile: GithubUserProfile =
        serde_json::from_slice(&output.stdout).map_err(|error| error.to_string())?;
    profile_cache::put(host, login, profile.clone());
    Ok(profile)
}

fn parse_repo_push_permission(stdout: &[u8]) -> Result<RepoAccess, String> {
    let parsed: GithubRepoPermissionsResponse =
        serde_json::from_slice(stdout).map_err(|error| error.to_string())?;
    Ok(match parsed.permissions {
        Some(permissions) if permissions.push => RepoAccess::Push,
        Some(_) => RepoAccess::None,
        None => RepoAccess::Probable,
    })
}

#[derive(Debug, Deserialize)]
struct GithubRepoPermissionsResponse {
    permissions: Option<GithubRepoPermissions>,
}

#[derive(Debug, Deserialize)]
struct GithubRepoPermissions {
    push: bool,
}

mod auth_status_cache {
    use super::*;

    pub(super) fn get(host: &str) -> Option<GithubCliAuthStatus> {
        let mut cache = GITHUB_AUTH_STATUS_CACHE.lock().ok()?;
        let fresh = cache
            .get(host)
            .filter(|entry| entry.cached_at.elapsed() < GITHUB_AUTH_STATUS_CACHE_TTL)
            .map(|entry| entry.status.clone());
        if fresh.is_some() {
            return fresh;
        }
        cache.remove(host);
        None
    }

    pub(super) fn put(host: &str, status: GithubCliAuthStatus) {
        let Ok(mut cache) = GITHUB_AUTH_STATUS_CACHE.lock() else {
            return;
        };
        cache.insert(
            host.to_string(),
            CachedGithubAuthStatus {
                status,
                cached_at: Instant::now(),
            },
        );
    }

    pub(super) fn invalidate(host: &str) {
        let Ok(mut cache) = GITHUB_AUTH_STATUS_CACHE.lock() else {
            return;
        };
        cache.remove(host);
    }
}

mod profile_cache {
    use super::*;

    const TTL: Duration = Duration::from_secs(30);

    fn key(host: &str, login: &str) -> (String, String) {
        (host.to_string(), login.to_string())
    }

    pub(super) fn get(host: &str, login: &str) -> Option<GithubUserProfile> {
        let mut cache = GITHUB_PROFILE_CACHE.lock().ok()?;
        let cache_key = key(host, login);
        let fresh = cache
            .get(&cache_key)
            .filter(|entry| entry.cached_at.elapsed() < TTL)
            .map(|entry| entry.profile.clone());
        if fresh.is_some() {
            return fresh;
        }
        cache.remove(&cache_key);
        None
    }

    pub(super) fn put(host: &str, login: &str, profile: GithubUserProfile) {
        let Ok(mut cache) = GITHUB_PROFILE_CACHE.lock() else {
            return;
        };
        cache.insert(
            key(host, login),
            CachedGithubProfile {
                profile,
                cached_at: Instant::now(),
            },
        );
    }

    pub(super) fn invalidate_host(host: &str) {
        let Ok(mut cache) = GITHUB_PROFILE_CACHE.lock() else {
            return;
        };
        cache.retain(|(entry_host, _), _| entry_host != host);
    }
}

pub(crate) fn resolve_change_request_json(
    root: &str,
    host: &str,
    branch_hints: &[String],
    head_sha: Option<&str>,
    login: Option<&str>,
) -> Result<Option<Value>, String> {
    let gh = match resolve_cli_binary("gh") {
        Ok(path) => path,
        Err(_) => return Ok(None),
    };
    let auth = resolve_auth_context(host, login)?;

    let json_fields = "number,title,url,state,mergeable,mergeStateStatus,isDraft,headRefName,headRefOid,baseRefName";

    let mut view_output = Command::new(&gh);
    view_output.current_dir(root);
    if let Some(auth) = auth.as_ref() {
        view_output.envs(auth.envs.iter().map(|(key, value)| (key, value)));
    }
    let view_output = view_output
        .args(["pr", "view", "--json", json_fields])
        .output();
    if let Ok(output) = view_output {
        if output.status.success() {
            if let Ok(pr) = serde_json::from_slice::<Value>(&output.stdout) {
                if pr.is_object() && !pr.as_object().map_or(true, |o| o.is_empty()) {
                    return Ok(Some(pr));
                }
            }
        }
    }

    for hint in branch_hints {
        let mut output = Command::new(&gh);
        output.current_dir(root);
        if let Some(auth) = auth.as_ref() {
            output.envs(auth.envs.iter().map(|(key, value)| (key, value)));
        }
        let output = output
            .args(["pr", "view", hint, "--json", json_fields])
            .output();
        if let Ok(output) = output {
            if output.status.success() {
                if let Ok(pr) = serde_json::from_slice::<Value>(&output.stdout) {
                    if pr.is_object() && !pr.as_object().map_or(true, |o| o.is_empty()) {
                        return Ok(Some(pr));
                    }
                }
            }
        }
    }

    let mut output = Command::new(&gh);
    output.current_dir(root);
    if let Some(auth) = auth.as_ref() {
        output.envs(auth.envs.iter().map(|(key, value)| (key, value)));
    }
    let output = output
        .args(["pr", "list", "--state", "all", "--json", json_fields])
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Ok(None);
    }

    let parsed: Value = serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())?;
    let Some(items) = parsed.as_array() else {
        return Ok(None);
    };
    let pr = items.iter().find(|item| {
        let matches_branch = item
            .get("headRefName")
            .and_then(|value| value.as_str())
            .map(|value| branch_hints.iter().any(|hint| hint == value))
            .unwrap_or(false);
        let matches_sha = head_sha
            .and_then(|sha| {
                item.get("headRefOid")
                    .and_then(|value| value.as_str())
                    .map(|value| value == sha)
            })
            .unwrap_or(false);
        matches_branch || matches_sha
    });
    Ok(pr.cloned())
}

pub(crate) fn resolve_change_request_url_json(
    root: &str,
    host: &str,
    url: &str,
    login: Option<&str>,
) -> Result<Value, String> {
    let gh = resolve_cli_binary("gh")?;
    let auth = resolve_auth_context(host, login)?;
    let json_fields = "number,title,url,state,isDraft,headRefName,headRefOid,headRepositoryOwner,headRepository,baseRefName,author";
    let mut command = Command::new(gh);
    command.current_dir(root);
    if let Some(auth) = auth.as_ref() {
        command.envs(auth.envs.iter().map(|(key, value)| (key, value)));
    }
    let output = command
        .args(["pr", "view", url, "--json", json_fields])
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(git_output_err("gh pr view", &output.stderr));
    }
    serde_json::from_slice::<Value>(&output.stdout)
        .map_err(|error| format!("Failed to decode GitHub pull request: {error}"))
}

pub(crate) fn view_change_request_web(
    root: &str,
    host: &str,
    login: Option<&str>,
) -> Result<(), String> {
    let gh = resolve_cli_binary("gh")?;
    let auth = resolve_auth_context(host, login)?;
    let mut output = Command::new(gh);
    output.current_dir(root);
    if let Some(auth) = auth.as_ref() {
        output.envs(auth.envs.iter().map(|(key, value)| (key, value)));
    }
    let output = output
        .args(["pr", "view", "--web"])
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "gh pr view failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    ))
}

pub(crate) fn view_change_request_url_web(
    root: &str,
    host: &str,
    url: &str,
    login: Option<&str>,
) -> Result<(), String> {
    let gh = resolve_cli_binary("gh")?;
    let auth = resolve_auth_context(host, login)?;
    let mut command = Command::new(gh);
    command.current_dir(root);
    if let Some(auth) = auth.as_ref() {
        command.envs(auth.envs.iter().map(|(key, value)| (key, value)));
    }
    let output = command
        .args(["pr", "view", url, "--web"])
        .output()
        .map_err(|error| error.to_string())?;
    if output.status.success() {
        return Ok(());
    }
    Err(git_output_err("gh pr view --web", &output.stderr))
}

pub(crate) fn merge_change_request(
    root: &str,
    host: &str,
    login: Option<&str>,
) -> Result<(), String> {
    let gh = resolve_cli_binary("gh")?;
    let auth = resolve_auth_context(host, login)?;
    let mut output = Command::new(gh);
    output.current_dir(root);
    if let Some(auth) = auth.as_ref() {
        output.envs(auth.envs.iter().map(|(key, value)| (key, value)));
    }
    let output = output
        .args(["pr", "merge", "--merge"])
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "gh pr merge failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    ))
}

pub(crate) fn merge_change_request_url(
    root: &str,
    host: &str,
    url: &str,
    login: Option<&str>,
) -> Result<(), String> {
    let gh = resolve_cli_binary("gh")?;
    let auth = resolve_auth_context(host, login)?;
    let mut command = Command::new(gh);
    command.current_dir(root);
    if let Some(auth) = auth.as_ref() {
        command.envs(auth.envs.iter().map(|(key, value)| (key, value)));
    }
    let output = command
        .args(["pr", "merge", url, "--merge"])
        .output()
        .map_err(|error| error.to_string())?;
    if output.status.success() {
        return Ok(());
    }
    Err(git_output_err("gh pr merge", &output.stderr))
}

pub(crate) fn create_change_request(
    root: &str,
    base_branch: &str,
    head_branch: &str,
    host: &str,
    login: Option<&str>,
) -> Result<(), String> {
    let gh = resolve_cli_binary("gh")?;
    let auth = resolve_auth_context(host, login)?;
    let mut output = Command::new(gh);
    output.current_dir(root);
    if let Some(auth) = auth.as_ref() {
        output.envs(auth.envs.iter().map(|(key, value)| (key, value)));
    }
    let output = output
        .args([
            "pr",
            "create",
            "--fill",
            "--base",
            base_branch,
            "--head",
            head_branch,
            "--assignee",
            "@me",
        ])
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "gh pr create failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    ))
}

#[cfg(test)]
mod tests {
    use super::{
        parse_github_active_login_for_host, parse_github_authenticated_hosts,
        parse_github_logins_for_host, parse_repo_push_permission,
    };

    use crate::commands::forge::accounts::RepoAccess;

    #[test]
    fn parses_active_github_login_from_json() {
        let output = r#"{
            "hosts": {
                "github.com": [
                    {"state":"success","login":"demo-user","active":true},
                    {"state":"success","login":"backup-user","active":false}
                ]
            }
        }"#;
        assert_eq!(
            parse_github_active_login_for_host(output, "github.com").unwrap(),
            Some("demo-user".to_string())
        );
    }

    #[test]
    fn parses_github_logins_for_requested_host_only() {
        let output = r#"{
            "hosts": {
                "github.com": [
                    {"state":"success","login":"demo-user","active":true},
                    {"state":"failure","login":"stale-user","active":false}
                ],
                "ghe.example.com": [
                    {"state":"success","login":"enterprise-user","active":true}
                ]
            }
        }"#;
        assert_eq!(
            parse_github_logins_for_host(output, "github.com").unwrap(),
            vec!["demo-user".to_string()]
        );
    }

    #[test]
    fn parses_github_authenticated_hosts_only() {
        let output = r#"{
            "hosts": {
                "github.com": [
                    {"state":"success","login":"demo-user","active":true}
                ],
                "ghe.example.com": [
                    {"state":"success","login":"enterprise-user","active":true}
                ],
                "stale.example.com": [
                    {"state":"failure","login":"old-user","active":false}
                ]
            }
        }"#;
        assert_eq!(
            parse_github_authenticated_hosts(output).unwrap(),
            vec!["ghe.example.com".to_string(), "github.com".to_string()]
        );
    }

    #[test]
    fn parses_repo_push_permission_push_when_explicitly_allowed() {
        let payload = br#"{"permissions":{"push":true}}"#;
        assert_eq!(
            parse_repo_push_permission(payload).unwrap(),
            RepoAccess::Push
        );
    }

    #[test]
    fn parses_repo_push_permission_probable_when_permissions_missing() {
        let payload = br#"{"id":1,"name":"repo"}"#;
        assert_eq!(
            parse_repo_push_permission(payload).unwrap(),
            RepoAccess::Probable
        );
    }
}

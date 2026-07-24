use std::process::{Command, Stdio};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::commands::forge::accounts::{AuthCheck, ForgeCliAccountProfile, RepoAccess};
use crate::commands::forge::remote::WorkspaceForgeTarget;
use crate::commands::forge::resolve_cli_binary;
use crate::git::{git_output_err, run_git_output};

#[derive(Debug, Clone)]
pub(crate) struct GitlabCliAuthStatus {
    pub(crate) logins: Vec<String>,
    pub(crate) active_login: Option<String>,
}

#[derive(Clone)]
struct CachedGitlabAuthStatus {
    status: GitlabCliAuthStatus,
    cached_at: Instant,
}

const GITLAB_AUTH_STATUS_CACHE_TTL: Duration = Duration::from_secs(2);

static GITLAB_AUTH_STATUS_CACHE: LazyLock<
    Mutex<std::collections::HashMap<String, CachedGitlabAuthStatus>>,
> = LazyLock::new(|| Mutex::new(std::collections::HashMap::new()));

#[derive(Clone)]
struct CachedGitlabProfile {
    profile: GitlabUserProfile,
    cached_at: Instant,
}

static GITLAB_PROFILE_CACHE: LazyLock<
    Mutex<std::collections::HashMap<String, CachedGitlabProfile>>,
> = LazyLock::new(|| Mutex::new(std::collections::HashMap::new()));

#[derive(Debug, Clone)]
pub(crate) struct GitlabAuthContext {
    pub(crate) envs: Vec<(String, String)>,
    pub(crate) git_http_authorization: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct GitlabUserProfile {
    username: Option<String>,
    name: Option<String>,
    avatar_url: Option<String>,
    public_email: Option<String>,
    email: Option<String>,
}

fn trim_glab_status_prefix(raw: &str) -> &str {
    raw.trim_start_matches(|c: char| {
        c.is_whitespace() || c == '✓' || c == '✗' || c == '*' || c == '-' || c == '•'
    })
}

fn parse_glab_logged_in_pairs(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for raw in text.lines() {
        let body = trim_glab_status_prefix(raw);
        let Some(after_to) = body.strip_prefix("Logged in to ") else {
            continue;
        };
        let Some((host, after_as)) = after_to.split_once(" as ") else {
            continue;
        };
        let host = host.trim().trim_end_matches(['.', ',', ';', ':']);
        let login = after_as
            .split(|c: char| c.is_whitespace() || c == '(' || c == ')')
            .next()
            .unwrap_or("")
            .trim_end_matches(['.', ',', ';', ':']);
        if !host.is_empty() && !login.is_empty() {
            out.push((host.to_string(), login.to_string()));
        }
    }
    out
}

fn parse_glab_authenticated_hosts(text: &str) -> Vec<String> {
    let mut hosts = parse_glab_logged_in_pairs(text)
        .into_iter()
        .map(|(host, _)| host)
        .collect::<Vec<_>>();
    hosts.sort();
    hosts.dedup();
    hosts
}

fn looks_like_gitlab_unauthenticated(message: &str) -> bool {
    let normalized = message.to_ascii_lowercase();
    normalized.contains("no token found")
        || normalized.contains("not logged in")
        || normalized.contains("not logged into")
        || normalized.contains("unauthenticated")
}

fn looks_like_gitlab_missing_repo_access(message: &str) -> bool {
    let normalized = message.to_ascii_lowercase();
    normalized.contains("not found")
        || normalized.contains("404")
        || normalized.contains("403")
        || normalized.contains("forbidden")
        || normalized.contains("401")
        || normalized.contains("unauthorized")
}

fn extract_glab_token(text: &str) -> Option<String> {
    for raw in text.lines() {
        let line = trim_glab_status_prefix(raw);
        if line.is_empty() {
            continue;
        }
        let lower = line.to_ascii_lowercase();
        if let Some((_, value)) = line.split_once(':') {
            let label = lower.split_once(':').map(|(label, _)| label.trim());
            if matches!(
                label,
                Some("token" | "access token" | "token found" | "access token found")
            ) {
                let token = value.trim();
                if !token.is_empty() && !token.chars().all(|c| c == '*') {
                    return Some(token.to_string());
                }
            }
        }
    }
    None
}

fn canonical_gitlab_username(host: &str) -> Option<String> {
    let profile = fetch_gitlab_profile(host).ok()?;
    profile
        .username
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn encode_percent(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        let is_unreserved =
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~');
        if is_unreserved {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{:02X}", byte));
        }
    }
    out
}

fn last_commit_title(root: &str) -> Result<String, String> {
    let output = run_git_output(root, &["log", "-1", "--pretty=%s"])?;
    if !output.status.success() {
        return Err(git_output_err("git log -1 --pretty=%s", &output.stderr));
    }
    let title = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if title.is_empty() {
        return Ok("Update branch".to_string());
    }
    Ok(title)
}

pub(crate) fn auth_status(host: &str) -> Result<GitlabCliAuthStatus, String> {
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
    let glab = resolve_cli_binary("glab")?;
    let output = Command::new(glab)
        .args(["auth", "status"])
        .output()
        .map_err(|error| error.to_string())?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let combined = format!("{stdout}\n{stderr}");

    if !output.status.success() {
        if looks_like_gitlab_unauthenticated(&combined) {
            return Ok(Vec::new());
        }

        let trimmed = combined.trim();
        return Err(if trimmed.is_empty() {
            "GitLab CLI authentication failed.".to_string()
        } else {
            trimmed.to_string()
        });
    }

    let hosts = parse_glab_authenticated_hosts(&combined);
    if force_refresh {
        for host in &hosts {
            auth_status_cache::invalidate(host);
            profile_cache::invalidate(host);
        }
    }
    Ok(hosts)
}

pub(crate) fn auth_status_with_options(
    host: &str,
    force_refresh: bool,
) -> Result<GitlabCliAuthStatus, String> {
    if !force_refresh {
        if let Some(cached) = auth_status_cache::get(host) {
            return Ok(cached);
        }
    } else {
        auth_status_cache::invalidate(host);
    }

    let glab = resolve_cli_binary("glab")?;
    let output = Command::new(glab)
        .args(["auth", "status", "--hostname", host])
        .output()
        .map_err(|error| error.to_string())?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let combined = format!("{stdout}\n{stderr}");

    if !output.status.success() {
        if looks_like_gitlab_unauthenticated(&combined) {
            let status = GitlabCliAuthStatus {
                logins: Vec::new(),
                active_login: None,
            };
            auth_status_cache::put(host, status.clone());
            return Ok(status);
        }

        let trimmed = combined.trim();
        return Err(if trimmed.is_empty() {
            "GitLab CLI authentication failed.".to_string()
        } else {
            trimmed.to_string()
        });
    }

    let mut logins = parse_glab_logged_in_pairs(&combined)
        .into_iter()
        .filter_map(|(entry_host, login)| (entry_host == host).then_some(login))
        .collect::<Vec<_>>();
    if let Some(canonical) = canonical_gitlab_username(host) {
        logins = vec![canonical];
    }
    logins.dedup();
    let active_login = logins.first().cloned();
    let status = GitlabCliAuthStatus {
        logins,
        active_login,
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
        profile_cache::invalidate(host);
    }

    let profile = fetch_gitlab_profile(host).ok();
    Ok(status
        .logins
        .into_iter()
        .map(|login| ForgeCliAccountProfile {
            login,
            name: profile.as_ref().and_then(|profile| profile.name.clone()),
            avatar_url: profile
                .as_ref()
                .and_then(|profile| profile.avatar_url.clone()),
            email: profile
                .as_ref()
                .and_then(|profile| profile.public_email.clone().or(profile.email.clone())),
            active: true,
        })
        .collect())
}

const GITLAB_DEVELOPER_ACCESS_LEVEL: i32 = 30;

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
            if looks_like_gitlab_unauthenticated(&error)
                || error.contains("currently exposes the active account") =>
        {
            return Ok(RepoAccess::None);
        }
        Err(error) => return Err(error),
    };

    let path = format!("projects/{}", encode_percent(&format!("{owner}/{name}")));
    let glab = resolve_cli_binary("glab")?;
    let mut command = Command::new(glab);
    command
        .args(["api", "--hostname", host, path.as_str()])
        .envs(auth.envs.iter().map(|(key, value)| (key, value)));
    let output = command.output().map_err(|error| error.to_string())?;
    if !output.status.success() {
        let detail = format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        if looks_like_gitlab_unauthenticated(&detail)
            || looks_like_gitlab_missing_repo_access(&detail)
        {
            return Ok(RepoAccess::None);
        }
        return Err(detail.trim().to_string());
    }

    parse_project_push_permission(&output.stdout)
}

fn fetch_gitlab_profile(host: &str) -> Result<GitlabUserProfile, String> {
    if let Some(cached) = profile_cache::get(host) {
        return Ok(cached);
    }

    let glab = resolve_cli_binary("glab")?;
    let output = Command::new(glab)
        .args(["api", "--hostname", host, "user"])
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            format!("`glab api user` failed for `{host}`.")
        } else {
            stderr
        });
    }

    let profile: GitlabUserProfile =
        serde_json::from_slice(&output.stdout).map_err(|error| error.to_string())?;
    profile_cache::put(host, profile.clone());
    Ok(profile)
}

fn parse_project_push_permission(stdout: &[u8]) -> Result<RepoAccess, String> {
    let parsed: GitlabProjectPermissionsResponse =
        serde_json::from_slice(stdout).map_err(|error| error.to_string())?;
    let project_level = parsed
        .permissions
        .as_ref()
        .and_then(|permissions| permissions.project_access.as_ref())
        .and_then(|access| access.access_level);
    let group_level = parsed
        .permissions
        .as_ref()
        .and_then(|permissions| permissions.group_access.as_ref())
        .and_then(|access| access.access_level);

    if project_level.is_none() && group_level.is_none() {
        return Ok(RepoAccess::Probable);
    }

    let highest = project_level.unwrap_or(0).max(group_level.unwrap_or(0));
    Ok(if highest >= GITLAB_DEVELOPER_ACCESS_LEVEL {
        RepoAccess::Push
    } else {
        RepoAccess::None
    })
}

#[derive(Debug, serde::Deserialize)]
struct GitlabProjectPermissionsResponse {
    permissions: Option<GitlabProjectPermissions>,
}

#[derive(Debug, serde::Deserialize)]
struct GitlabProjectPermissions {
    project_access: Option<GitlabAccessLevel>,
    group_access: Option<GitlabAccessLevel>,
}

#[derive(Debug, serde::Deserialize)]
struct GitlabAccessLevel {
    access_level: Option<i32>,
}

mod auth_status_cache {
    use super::*;

    pub(super) fn get(host: &str) -> Option<GitlabCliAuthStatus> {
        let mut cache = GITLAB_AUTH_STATUS_CACHE.lock().ok()?;
        let fresh = cache
            .get(host)
            .filter(|entry| entry.cached_at.elapsed() < GITLAB_AUTH_STATUS_CACHE_TTL)
            .map(|entry| entry.status.clone());
        if fresh.is_some() {
            return fresh;
        }
        cache.remove(host);
        None
    }

    pub(super) fn put(host: &str, status: GitlabCliAuthStatus) {
        let Ok(mut cache) = GITLAB_AUTH_STATUS_CACHE.lock() else {
            return;
        };
        cache.insert(
            host.to_string(),
            CachedGitlabAuthStatus {
                status,
                cached_at: Instant::now(),
            },
        );
    }

    pub(super) fn invalidate(host: &str) {
        let Ok(mut cache) = GITLAB_AUTH_STATUS_CACHE.lock() else {
            return;
        };
        cache.remove(host);
    }
}

mod profile_cache {
    use super::*;

    const TTL: Duration = Duration::from_secs(30);

    pub(super) fn get(host: &str) -> Option<GitlabUserProfile> {
        let mut cache = GITLAB_PROFILE_CACHE.lock().ok()?;
        let fresh = cache
            .get(host)
            .filter(|entry| entry.cached_at.elapsed() < TTL)
            .map(|entry| entry.profile.clone());
        if fresh.is_some() {
            return fresh;
        }
        cache.remove(host);
        None
    }

    pub(super) fn put(host: &str, profile: GitlabUserProfile) {
        let Ok(mut cache) = GITLAB_PROFILE_CACHE.lock() else {
            return;
        };
        cache.insert(
            host.to_string(),
            CachedGitlabProfile {
                profile,
                cached_at: Instant::now(),
            },
        );
    }

    pub(super) fn invalidate(host: &str) {
        let Ok(mut cache) = GITLAB_PROFILE_CACHE.lock() else {
            return;
        };
        cache.remove(host);
    }
}

pub(crate) fn resolve_auth_context(
    host: &str,
    login: Option<&str>,
) -> Result<Option<GitlabAuthContext>, String> {
    let requested_login = login.map(str::trim).filter(|login| !login.is_empty());
    if requested_login.is_none() {
        return Ok(None);
    }

    let status = auth_status(host)?;
    if let Some(login) = requested_login {
        if status.active_login.as_deref() != Some(login) {
            return Err(format!(
                "GitLab CLI currently exposes the active account for `{host}` as `{}`. Switch the active `glab` account before using `{login}` in DCC.",
                status.active_login.as_deref().unwrap_or("unknown")
            ));
        }
    }

    let glab = resolve_cli_binary("glab")?;
    let output = Command::new(glab)
        .args(["auth", "status", "--hostname", host, "--show-token"])
        .output()
        .map_err(|error| error.to_string())?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let combined = format!("{stdout}\n{stderr}");
    if !output.status.success() {
        let trimmed = combined.trim();
        return Err(if trimmed.is_empty() {
            "Failed to resolve GitLab CLI token for the selected account.".to_string()
        } else {
            trimmed.to_string()
        });
    }

    let Some(token) = extract_glab_token(&combined) else {
        return Err("GitLab CLI did not expose a token for the selected account.".to_string());
    };
    let host_url = if host.starts_with("http://") || host.starts_with("https://") {
        host.to_string()
    } else {
        format!("https://{host}")
    };
    Ok(Some(GitlabAuthContext {
        envs: vec![
            ("GITLAB_HOST".to_string(), host_url.clone()),
            ("GL_HOST".to_string(), host_url),
            ("GITLAB_TOKEN".to_string(), token.clone()),
            ("GITLAB_ACCESS_TOKEN".to_string(), token.clone()),
            ("OAUTH_TOKEN".to_string(), token.clone()),
        ],
        git_http_authorization: format!("oauth2:{token}"),
    }))
}

pub(crate) fn resolve_change_request_json(
    root: &str,
    branch: &str,
    target: &WorkspaceForgeTarget,
    login: Option<&str>,
) -> Result<Option<Value>, String> {
    let auth = resolve_auth_context(&target.remote.host, login)?;
    let project_path = format!("{}/{}", target.remote.namespace, target.remote.repo);
    let endpoint = format!(
        "projects/{}/merge_requests?source_branch={}&state=all&order_by=updated_at&sort=desc&per_page=1",
        encode_percent(&project_path),
        encode_percent(branch),
    );
    let list_output = {
        let glab = resolve_cli_binary("glab")?;
        let mut command = Command::new(glab);
        command.current_dir(root).args([
            "api",
            "--hostname",
            &target.remote.host,
            endpoint.as_str(),
        ]);
        if let Some(auth) = auth.as_ref() {
            command.envs(auth.envs.iter().map(|(key, value)| (key, value)));
        }
        command.output().map_err(|error| error.to_string())?
    };
    if !list_output.status.success() {
        return Ok(None);
    }

    let items: Value =
        serde_json::from_slice(&list_output.stdout).map_err(|error| error.to_string())?;
    let Some(first) = items.as_array().and_then(|entries| entries.first()) else {
        return Ok(None);
    };
    let Some(iid) = first.get("iid").and_then(|value| value.as_i64()) else {
        return Ok(Some(first.clone()));
    };

    let detail_endpoint = format!(
        "projects/{}/merge_requests/{iid}",
        encode_percent(&project_path),
    );
    let detail_output = {
        let glab = resolve_cli_binary("glab")?;
        let mut command = Command::new(glab);
        command.current_dir(root).args([
            "api",
            "--hostname",
            &target.remote.host,
            detail_endpoint.as_str(),
        ]);
        if let Some(auth) = auth.as_ref() {
            command.envs(auth.envs.iter().map(|(key, value)| (key, value)));
        }
        command.output().map_err(|error| error.to_string())?
    };
    if !detail_output.status.success() {
        return Ok(Some(first.clone()));
    }

    let detail: Value =
        serde_json::from_slice(&detail_output.stdout).map_err(|error| error.to_string())?;
    Ok(Some(detail))
}

pub(crate) fn resolve_change_request_url_json(
    root: &str,
    target: &WorkspaceForgeTarget,
    iid: u32,
    login: Option<&str>,
) -> Result<Value, String> {
    let auth = resolve_auth_context(&target.remote.host, login)?;
    let project_path = format!("{}/{}", target.remote.namespace, target.remote.repo);
    let endpoint = format!(
        "projects/{}/merge_requests/{iid}",
        encode_percent(&project_path),
    );
    let glab = resolve_cli_binary("glab")?;
    let mut command = Command::new(glab);
    command
        .current_dir(root)
        .args(["api", "--hostname", &target.remote.host, endpoint.as_str()]);
    if let Some(auth) = auth.as_ref() {
        command.envs(auth.envs.iter().map(|(key, value)| (key, value)));
    }
    let output = command.output().map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(git_output_err("glab api merge request", &output.stderr));
    }
    serde_json::from_slice::<Value>(&output.stdout)
        .map_err(|error| format!("Failed to decode GitLab merge request: {error}"))
}

pub(crate) fn map_state(state: Option<&str>) -> Option<String> {
    match state {
        Some("opened") | Some("open") => Some("open".to_string()),
        Some("closed") => Some("closed".to_string()),
        Some("merged") => Some("merged".to_string()),
        _ => None,
    }
}

pub(crate) fn map_mergeable(raw_mr: &Value) -> Option<String> {
    if raw_mr
        .get("has_conflicts")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
    {
        return Some("CONFLICTING".to_string());
    }

    let status = raw_mr
        .get("detailed_merge_status")
        .and_then(|value| value.as_str())
        .or_else(|| raw_mr.get("merge_status").and_then(|value| value.as_str()))?;
    match status {
        "can_be_merged" | "mergeable" => Some("MERGEABLE".to_string()),
        "checking" | "unchecked" | "ci_must_pass" | "not_open" => Some("UNKNOWN".to_string()),
        value if value.contains("conflict") => Some("CONFLICTING".to_string()),
        _ => None,
    }
}

pub(crate) fn view_change_request_web(
    root: &str,
    host: &str,
    login: Option<&str>,
) -> Result<(), String> {
    let glab = resolve_cli_binary("glab")?;
    let auth = resolve_auth_context(host, login)?;
    let mut output = Command::new(glab);
    output.current_dir(root);
    if let Some(auth) = auth.as_ref() {
        output.envs(auth.envs.iter().map(|(key, value)| (key, value)));
    }
    let output = output
        .args(["mr", "view", "--web"])
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "glab mr view failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    ))
}

pub(crate) fn view_change_request_number_web(
    root: &str,
    host: &str,
    number: u32,
    login: Option<&str>,
) -> Result<(), String> {
    let glab = resolve_cli_binary("glab")?;
    let auth = resolve_auth_context(host, login)?;
    let mut command = Command::new(glab);
    command.current_dir(root);
    if let Some(auth) = auth.as_ref() {
        command.envs(auth.envs.iter().map(|(key, value)| (key, value)));
    }
    let number = number.to_string();
    let output = command
        .args(["mr", "view", &number, "--web"])
        .output()
        .map_err(|error| error.to_string())?;
    if output.status.success() {
        return Ok(());
    }
    Err(git_output_err("glab mr view --web", &output.stderr))
}

pub(crate) fn merge_change_request(
    root: &str,
    branch: &str,
    target: &WorkspaceForgeTarget,
    login: Option<&str>,
) -> Result<(), String> {
    let auth = resolve_auth_context(&target.remote.host, login)?;
    let Some(raw_mr) = resolve_change_request_json(root, branch, target, login)? else {
        return Err("No open merge request found for the current branch.".to_string());
    };
    let iid = raw_mr
        .get("iid")
        .and_then(|value| value.as_i64())
        .ok_or_else(|| "Unable to resolve GitLab merge request id.".to_string())?;
    let project_path = format!("{}/{}", target.remote.namespace, target.remote.repo);
    let endpoint = format!(
        "projects/{}/merge_requests/{iid}/merge",
        encode_percent(&project_path),
    );
    let output = {
        let glab = resolve_cli_binary("glab")?;
        let mut command = Command::new(glab);
        command.current_dir(root).args([
            "api",
            "--hostname",
            &target.remote.host,
            "--method",
            "PUT",
            endpoint.as_str(),
        ]);
        if let Some(auth) = auth.as_ref() {
            command.envs(auth.envs.iter().map(|(key, value)| (key, value)));
        }
        command.output().map_err(|error| error.to_string())?
    };
    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "glab mr merge failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    ))
}

pub(crate) fn create_change_request(
    root: &str,
    base_branch: &str,
    head_branch: &str,
    host: &str,
    login: Option<&str>,
) -> Result<(), String> {
    let glab = resolve_cli_binary("glab")?;
    let auth = resolve_auth_context(host, login)?;
    let title = last_commit_title(root)?;
    let mut output = Command::new(glab);
    output.current_dir(root);
    output.stdin(Stdio::null());
    if let Some(auth) = auth.as_ref() {
        output.envs(auth.envs.iter().map(|(key, value)| (key, value)));
    }
    let output = output
        .args([
            "mr",
            "create",
            "--fill",
            "--source-branch",
            head_branch,
            "--target-branch",
            base_branch,
            "--title",
            &title,
            "--yes",
        ])
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "glab mr create failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    ))
}

#[cfg(test)]
mod tests {
    use super::{
        extract_glab_token, parse_glab_authenticated_hosts, parse_glab_logged_in_pairs,
        parse_project_push_permission,
    };

    use crate::commands::forge::accounts::RepoAccess;

    #[test]
    fn parses_gitlab_logged_in_pairs() {
        let output = "gitlab.com\n  ✓ Logged in to gitlab.com as octo (/path/to/config)\n";
        assert_eq!(
            parse_glab_logged_in_pairs(output),
            vec![("gitlab.com".to_string(), "octo".to_string())]
        );
    }

    #[test]
    fn parses_gitlab_authenticated_hosts() {
        let output = "\
gitlab.com\n  ✓ Logged in to gitlab.com as octo (/path/to/config)\n\
self.gitlab.example.com\n  ✓ Logged in to self.gitlab.example.com as team-user (/path/to/config)\n";
        assert_eq!(
            parse_glab_authenticated_hosts(output),
            vec![
                "gitlab.com".to_string(),
                "self.gitlab.example.com".to_string()
            ]
        );
    }

    #[test]
    fn extracts_token_from_glab_status_line_with_success_marker() {
        let output = "gitlab.com\n  ✓ Token: glpat-real-token\n";
        assert_eq!(
            extract_glab_token(output),
            Some("glpat-real-token".to_string())
        );
    }

    #[test]
    fn extracts_token_from_glab_token_found_line() {
        let output = "gitlab.com\n  ✓ Token found: glpat-real-token\n";
        assert_eq!(
            extract_glab_token(output),
            Some("glpat-real-token".to_string())
        );
    }

    #[test]
    fn ignores_masked_glab_token_status_line() {
        let output = "gitlab.com\n  ✓ Token: **************************\n";
        assert_eq!(extract_glab_token(output), None);
    }

    #[test]
    fn parses_project_push_permission_push_for_developer() {
        let payload =
            br#"{"permissions":{"project_access":{"access_level":30},"group_access":null}}"#;
        assert_eq!(
            parse_project_push_permission(payload).unwrap(),
            RepoAccess::Push
        );
    }

    #[test]
    fn parses_project_push_permission_probable_when_access_levels_missing() {
        let payload = br#"{"permissions":{"project_access":{"access_level":null},"group_access":{"access_level":null}}}"#;
        assert_eq!(
            parse_project_push_permission(payload).unwrap(),
            RepoAccess::Probable
        );
    }
}

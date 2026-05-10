use std::process::Command;

use serde::Deserialize;
use serde_json::Value;

use crate::commands::forge::resolve_cli_binary;

#[derive(Debug, Clone)]
pub(crate) struct GithubCliAuthStatus {
    pub(crate) logins: Vec<String>,
    pub(crate) active_login: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct GithubAuthContext {
    pub(crate) envs: Vec<(String, String)>,
    pub(crate) git_http_authorization: String,
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
    let auth = format!(
        "{}:{}",
        github_http_username(requested_login),
        token
    );
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
            return Ok(GithubCliAuthStatus {
                logins: Vec::new(),
                active_login: None,
            });
        }

        let trimmed = combined.trim();
        return Err(if trimmed.is_empty() {
            "GitHub CLI authentication failed.".to_string()
        } else {
            trimmed.to_string()
        });
    }

    Ok(GithubCliAuthStatus {
        logins: parse_github_logins_for_host(&stdout, host)?,
        active_login: parse_github_active_login_for_host(&stdout, host)?,
    })
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

pub(crate) fn view_change_request_web(root: &str, host: &str, login: Option<&str>) -> Result<(), String> {
    let gh = resolve_cli_binary("gh")?;
    let auth = resolve_auth_context(host, login)?;
    let mut output = Command::new(gh);
    output.current_dir(root);
    if let Some(auth) = auth.as_ref() {
        output.envs(auth.envs.iter().map(|(key, value)| (key, value)));
    }
    let output = output.args(["pr", "view", "--web"])
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

pub(crate) fn merge_change_request(root: &str, host: &str, login: Option<&str>) -> Result<(), String> {
    let gh = resolve_cli_binary("gh")?;
    let auth = resolve_auth_context(host, login)?;
    let mut output = Command::new(gh);
    output.current_dir(root);
    if let Some(auth) = auth.as_ref() {
        output.envs(auth.envs.iter().map(|(key, value)| (key, value)));
    }
    let output = output.args(["pr", "merge", "--merge"])
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
    let output = output.args([
            "pr",
            "create",
            "--fill",
            "--base",
            base_branch,
            "--head",
            head_branch,
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
    use super::{parse_github_active_login_for_host, parse_github_logins_for_host};

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
}

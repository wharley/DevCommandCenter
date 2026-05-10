use std::path::Path;

use crate::commands::forge_commands::ForgeCliProvider;
use crate::git::run_git_output;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParsedRemote {
    pub(crate) host: String,
    pub(crate) namespace: String,
    pub(crate) repo: String,
    pub(crate) path: String,
}

#[derive(Debug, Clone)]
pub(crate) struct WorkspaceForgeTarget {
    pub(crate) provider: ForgeCliProvider,
    pub(crate) remote_name: String,
    pub(crate) remote: ParsedRemote,
}

pub(crate) fn parse_remote(remote: &str) -> Option<ParsedRemote> {
    let remote = remote.trim();
    if remote.is_empty() {
        return None;
    }

    if let Some((user_host, path)) = remote.split_once(':') {
        if !user_host.contains("://") && user_host.contains('@') {
            let host = user_host.rsplit_once('@')?.1;
            return parsed_remote_from_host_path(host, path);
        }
    }

    for prefix in ["https://", "http://", "git://", "ssh://"] {
        if let Some(rest) = remote.strip_prefix(prefix) {
            let rest = rest.strip_prefix("git@").unwrap_or(rest);
            let (host, path) = rest.split_once('/')?;
            return parsed_remote_from_host_path(host, path);
        }
    }

    None
}

fn parsed_remote_from_host_path(host: &str, path: &str) -> Option<ParsedRemote> {
    let host = host.trim().trim_end_matches('/');
    let raw_path = path.trim().trim_matches('/');
    let trimmed_path = raw_path.trim_end_matches(".git");
    let mut parts = trimmed_path.rsplitn(2, '/');
    let repo = parts.next()?.trim();
    let namespace = parts.next()?.trim();
    if host.is_empty() || namespace.is_empty() || repo.is_empty() {
        return None;
    }
    Some(ParsedRemote {
        host: host.to_ascii_lowercase(),
        namespace: namespace.to_string(),
        repo: repo.to_string(),
        path: raw_path.to_string(),
    })
}

fn matches_wellknown_github(host: &str) -> bool {
    matches!(
        host.to_ascii_lowercase().as_str(),
        "github.com" | "www.github.com" | "gist.github.com" | "api.github.com"
    )
}

fn matches_wellknown_gitlab(host: &str) -> bool {
    matches!(
        host.to_ascii_lowercase().as_str(),
        "gitlab.com" | "www.gitlab.com" | "salsa.debian.org" | "framagit.org" | "invent.kde.org"
    )
}

fn host_looks_like_github(host: &str) -> bool {
    let host = host.to_ascii_lowercase();
    host.starts_with("github.")
        || host.ends_with(".github.com")
        || host.ends_with(".ghe.com")
        || host.ends_with(".ghe.io")
}

fn host_looks_like_gitlab(host: &str) -> bool {
    let host = host.to_ascii_lowercase();
    host.starts_with("gitlab.")
        || host.ends_with(".gitlab.com")
        || host.ends_with(".gitlab.io")
        || host.split('.').any(|segment| segment == "gitlab")
}

pub(crate) fn detect_provider_for_repo(
    remote_url: Option<&str>,
    repo_root: Option<&Path>,
) -> Option<ForgeCliProvider> {
    let parsed = remote_url.and_then(parse_remote);
    if let Some(remote) = parsed.as_ref() {
        let host = remote.host.as_str();
        if matches_wellknown_github(host) {
            return Some(ForgeCliProvider::Github);
        }
        if matches_wellknown_gitlab(host) {
            return Some(ForgeCliProvider::Gitlab);
        }
        if remote.path.contains("/-/") {
            return Some(ForgeCliProvider::Gitlab);
        }
        if host_looks_like_github(host) {
            return Some(ForgeCliProvider::Github);
        }
        if host_looks_like_gitlab(host) {
            return Some(ForgeCliProvider::Gitlab);
        }
    }

    if let Some(root) = repo_root {
        if root.join(".gitlab-ci.yml").is_file() {
            return Some(ForgeCliProvider::Gitlab);
        }
        if root.join(".github").join("workflows").is_dir() {
            return Some(ForgeCliProvider::Github);
        }
    }

    None
}

fn resolve_default_remote_name(root: &str) -> Result<String, String> {
    let output = run_git_output(root, &["remote"])?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }

    let remotes: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| line.to_string())
        .collect();
    if remotes.is_empty() {
        return Ok("origin".to_string());
    }
    if remotes.iter().any(|remote| remote == "origin") {
        return Ok("origin".to_string());
    }

    Ok(remotes[0].clone())
}

fn resolve_workspace_remote(root: &str) -> Result<Option<(String, String)>, String> {
    let remote = resolve_default_remote_name(root)?;
    let output = run_git_output(root, &["remote", "get-url", &remote])?;
    if !output.status.success() {
        return Ok(None);
    }

    let remote_url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if remote_url.is_empty() {
        return Ok(None);
    }

    Ok(Some((remote, remote_url)))
}

pub(crate) fn resolve_workspace_forge_target(root: &str) -> Result<Option<WorkspaceForgeTarget>, String> {
    let workspace_remote = resolve_workspace_remote(root)?;
    let remote_url = workspace_remote.as_ref().map(|(_, url)| url.as_str());
    let provider = detect_provider_for_repo(remote_url.as_deref(), Some(Path::new(root)));
    let Some(provider) = provider else {
        return Ok(None);
    };
    let Some(parsed_remote) = remote_url.as_deref().and_then(parse_remote) else {
        return Ok(None);
    };
    let remote_name = workspace_remote
        .as_ref()
        .map(|(name, _)| name.clone())
        .unwrap_or_else(|| "origin".to_string());
    Ok(Some(WorkspaceForgeTarget {
        provider,
        remote_name,
        remote: parsed_remote,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nested_gitlab_remote() {
        let parsed = parse_remote("git@gitlab.company.com:platform/tools/api.git").unwrap();
        assert_eq!(parsed.host, "gitlab.company.com");
        assert_eq!(parsed.namespace, "platform/tools");
        assert_eq!(parsed.repo, "api");
    }

    #[test]
    fn detects_github_from_well_known_host() {
        assert_eq!(
            detect_provider_for_repo(Some("https://github.com/acme/demo.git"), Some(Path::new("."))),
            Some(ForgeCliProvider::Github)
        );
    }

    #[test]
    fn detects_gitlab_from_host_pattern() {
        assert_eq!(
            detect_provider_for_repo(
                Some("git@gitlab.mycorp.com:team/service.git"),
                Some(Path::new(".")),
            ),
            Some(ForgeCliProvider::Gitlab)
        );
    }
}

use std::collections::HashMap;
use std::path::Path;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use reqwest::blocking::Client;
use reqwest::header::SERVER;

use crate::commands::forge::remote::{parse_remote, ParsedRemote};
use crate::commands::forge::resolve_cli_binary;
use crate::commands::forge_commands::ForgeCliProvider;
use dcc_infra::process::run_command_with_timeout;

const HTTP_PROBE_TIMEOUT: Duration = Duration::from_secs(2);
const CLI_PROBE_TIMEOUT: Duration = Duration::from_secs(3);
const DETECTION_CACHE_TTL: Duration = Duration::from_secs(30);

#[derive(Clone)]
struct CachedDetection {
    provider: Option<ForgeCliProvider>,
    cached_at: Instant,
}

static DETECTION_CACHE: LazyLock<Mutex<HashMap<String, CachedDetection>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SignalProvider {
    Github,
    Gitlab,
}

#[derive(Debug, Clone)]
struct DetectionSignal {
    provider: SignalProvider,
}

trait DetectorProbes {
    fn probe_gitlab_api(&self, host: &str) -> bool;
    fn probe_github_api(&self, host: &str) -> bool;
    fn glab_recognizes_remote(&self, remote: &ParsedRemote) -> bool;
    fn gh_recognizes_remote(&self, remote: &ParsedRemote) -> bool;
}

struct LiveDetectorProbes;

impl DetectorProbes for LiveDetectorProbes {
    fn probe_gitlab_api(&self, host: &str) -> bool {
        probe_gitlab_api(host)
    }

    fn probe_github_api(&self, host: &str) -> bool {
        probe_github_api(host)
    }

    fn glab_recognizes_remote(&self, remote: &ParsedRemote) -> bool {
        cli_recognizes_remote(
            "glab",
            &[
                "repo",
                "view",
                &format!("{}/{}", remote.namespace, remote.repo),
                "--hostname",
                &remote.host,
            ],
            CLI_PROBE_TIMEOUT,
        )
    }

    fn gh_recognizes_remote(&self, remote: &ParsedRemote) -> bool {
        cli_recognizes_remote(
            "gh",
            &[
                "repo",
                "view",
                &format!("{}/{}", remote.namespace, remote.repo),
                "--hostname",
                &remote.host,
            ],
            CLI_PROBE_TIMEOUT,
        )
    }
}

pub(crate) fn detect_provider_for_repo(
    remote_url: Option<&str>,
    repo_root: Option<&Path>,
) -> Option<ForgeCliProvider> {
    let key = remote_url
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    let probes = LiveDetectorProbes;
    detect_provider_for_repo_cached(remote_url, repo_root, &probes, key.as_deref())
}

#[allow(dead_code)]
pub(crate) fn detect_provider_for_repo_offline(
    remote_url: Option<&str>,
    repo_root: Option<&Path>,
) -> Option<ForgeCliProvider> {
    detect_provider_for_repo_impl(remote_url, repo_root, false, &LiveDetectorProbes)
}

fn detect_provider_for_repo_cached(
    remote_url: Option<&str>,
    repo_root: Option<&Path>,
    probes: &impl DetectorProbes,
    cache_key: Option<&str>,
) -> Option<ForgeCliProvider> {
    let provider = detect_provider_for_repo_impl(remote_url, repo_root, false, probes);
    if provider.is_some() {
        return provider;
    }

    if let Some(cache_key) = cache_key {
        if let Some(provider) = cached_detection(cache_key) {
            return provider;
        }
    }

    let provider = detect_provider_for_repo_impl(remote_url, repo_root, true, probes);
    if let Some(cache_key) = cache_key {
        cache_detection(cache_key, provider);
    }
    provider
}

fn detect_provider_for_repo_impl(
    remote_url: Option<&str>,
    repo_root: Option<&Path>,
    allow_expensive_probes: bool,
    probes: &impl DetectorProbes,
) -> Option<ForgeCliProvider> {
    let parsed = remote_url.and_then(parse_remote);
    let mut signals = Vec::new();

    if let Some(remote) = parsed.as_ref() {
        let host = remote.host.as_str();
        if matches_wellknown_github(host) {
            return Some(ForgeCliProvider::Github);
        }
        if matches_wellknown_gitlab(host) {
            return Some(ForgeCliProvider::Gitlab);
        }
        if host_looks_like_github(host) {
            signals.push(DetectionSignal {
                provider: SignalProvider::Github,
            });
        } else if host_looks_like_gitlab(host) {
            signals.push(DetectionSignal {
                provider: SignalProvider::Gitlab,
            });
        }
        if remote.path.contains("/-/") {
            return Some(ForgeCliProvider::Gitlab);
        }
    }

    if let Some(root) = repo_root {
        if root.join(".gitlab-ci.yml").is_file() {
            signals.push(DetectionSignal {
                provider: SignalProvider::Gitlab,
            });
        }
        if root.join(".github").join("workflows").is_dir() {
            signals.push(DetectionSignal {
                provider: SignalProvider::Github,
            });
        }
    }

    if let Some(provider) = resolve_from_signals(&signals) {
        return Some(provider);
    }

    if !allow_expensive_probes {
        return None;
    }

    if let Some(remote) = parsed.as_ref() {
        if probes.probe_gitlab_api(&remote.host) {
            return Some(ForgeCliProvider::Gitlab);
        }
        if probes.probe_github_api(&remote.host) {
            return Some(ForgeCliProvider::Github);
        }
        if probes.glab_recognizes_remote(remote) {
            return Some(ForgeCliProvider::Gitlab);
        }
        if probes.gh_recognizes_remote(remote) {
            return Some(ForgeCliProvider::Github);
        }
    }

    resolve_from_signals(&signals)
}

fn resolve_from_signals(signals: &[DetectionSignal]) -> Option<ForgeCliProvider> {
    let mentions_github = signals
        .iter()
        .any(|signal| signal.provider == SignalProvider::Github);
    let mentions_gitlab = signals
        .iter()
        .any(|signal| signal.provider == SignalProvider::Gitlab);
    match (mentions_github, mentions_gitlab) {
        (true, false) => Some(ForgeCliProvider::Github),
        (false, true) => Some(ForgeCliProvider::Gitlab),
        _ => None,
    }
}

fn cached_detection(cache_key: &str) -> Option<Option<ForgeCliProvider>> {
    let mut cache = DETECTION_CACHE.lock().ok()?;
    let fresh = cache
        .get(cache_key)
        .filter(|entry| entry.cached_at.elapsed() < DETECTION_CACHE_TTL)
        .map(|entry| entry.provider);
    if fresh.is_some() {
        return fresh;
    }
    cache.remove(cache_key);
    None
}

fn cache_detection(cache_key: &str, provider: Option<ForgeCliProvider>) {
    let Ok(mut cache) = DETECTION_CACHE.lock() else {
        return;
    };
    cache.insert(
        cache_key.to_string(),
        CachedDetection {
            provider,
            cached_at: Instant::now(),
        },
    );
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

fn build_probe_client() -> Option<Client> {
    Client::builder()
        .timeout(HTTP_PROBE_TIMEOUT)
        .user_agent("DevCommandCenter forge detector")
        .build()
        .ok()
}

fn probe_gitlab_api(host: &str) -> bool {
    let client = match build_probe_client() {
        Some(client) => client,
        None => return false,
    };
    let response = match client.get(format!("https://{host}/api/v4/version")).send() {
        Ok(response) => response,
        Err(_) => return false,
    };
    let server = response
        .headers()
        .get(SERVER)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    let has_gitlab_header = response
        .headers()
        .keys()
        .any(|key| key.as_str().to_ascii_lowercase().starts_with("x-gitlab"));
    has_gitlab_header || server.contains("gitlab")
}

fn probe_github_api(host: &str) -> bool {
    let client = match build_probe_client() {
        Some(client) => client,
        None => return false,
    };
    let response = match client.get(format!("https://{host}/api/v3/")).send() {
        Ok(response) => response,
        Err(_) => return false,
    };
    response
        .headers()
        .keys()
        .any(|key| key.as_str().eq_ignore_ascii_case("x-github-request-id"))
}

fn cli_recognizes_remote(program: &str, args: &[&str], timeout: Duration) -> bool {
    let cli = match resolve_cli_binary(program) {
        Ok(path) => path,
        Err(_) => return false,
    };
    run_command_with_timeout(
        &cli,
        |command| {
            command.args(args);
        },
        timeout,
    )
    .map(|output| output.status.success())
    .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[derive(Default)]
    struct StubProbes {
        gitlab_api: bool,
        github_api: bool,
        glab_repo_view: bool,
        gh_repo_view: bool,
    }

    impl DetectorProbes for StubProbes {
        fn probe_gitlab_api(&self, _host: &str) -> bool {
            self.gitlab_api
        }

        fn probe_github_api(&self, _host: &str) -> bool {
            self.github_api
        }

        fn glab_recognizes_remote(&self, _remote: &ParsedRemote) -> bool {
            self.glab_repo_view
        }

        fn gh_recognizes_remote(&self, _remote: &ParsedRemote) -> bool {
            self.gh_repo_view
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("dcc-detect-{name}-{unique}"));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn detects_github_from_well_known_host() {
        assert_eq!(
            detect_provider_for_repo_impl(
                Some("https://github.com/acme/demo.git"),
                Some(Path::new(".")),
                false,
                &StubProbes::default(),
            ),
            Some(ForgeCliProvider::Github)
        );
    }

    #[test]
    fn detects_gitlab_from_url_path() {
        assert_eq!(
            detect_provider_for_repo_impl(
                Some("https://code.example.com/group/proj/-/tree/main"),
                Some(Path::new(".")),
                false,
                &StubProbes::default(),
            ),
            Some(ForgeCliProvider::Gitlab)
        );
    }

    #[test]
    fn detects_gitlab_from_repo_file_signal() {
        let root = temp_dir("gitlab-file");
        std::fs::write(root.join(".gitlab-ci.yml"), b"stages: []").unwrap();
        assert_eq!(
            detect_provider_for_repo_impl(
                Some("https://code.example.com/acme/demo.git"),
                Some(root.as_path()),
                false,
                &StubProbes::default(),
            ),
            Some(ForgeCliProvider::Gitlab)
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn uses_http_probe_for_ambiguous_hosts() {
        assert_eq!(
            detect_provider_for_repo_impl(
                Some("https://code.example.com/acme/demo.git"),
                Some(Path::new(".")),
                true,
                &StubProbes {
                    gitlab_api: true,
                    ..StubProbes::default()
                },
            ),
            Some(ForgeCliProvider::Gitlab)
        );
    }

    #[test]
    fn uses_cli_probe_when_http_probe_is_inconclusive() {
        assert_eq!(
            detect_provider_for_repo_impl(
                Some("https://code.example.com/acme/demo.git"),
                Some(Path::new(".")),
                true,
                &StubProbes {
                    gh_repo_view: true,
                    ..StubProbes::default()
                },
            ),
            Some(ForgeCliProvider::Github)
        );
    }
}

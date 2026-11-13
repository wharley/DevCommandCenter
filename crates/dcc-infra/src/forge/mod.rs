pub mod accounts;
pub mod context;
pub mod detect;
pub mod github;
pub mod github_checks;
pub mod github_merge;
pub mod github_read;
pub mod github_read_run;
pub mod gitlab;
pub mod provider;
pub mod remote;

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use crate::process::run_command_with_timeout;

pub const FORGE_CLI_COMMAND_TIMEOUT: Duration = Duration::from_secs(10);

pub fn run_forge_cli_command<F>(
    program: &std::path::Path,
    configure: F,
) -> Result<std::process::Output, String>
where
    F: FnOnce(&mut Command),
{
    run_command_with_timeout(program, configure, FORGE_CLI_COMMAND_TIMEOUT)
}

/// A forge CLI's signed-in hosts, kept as long as its sign-in status: the
/// background polls (the attention watch, every minute, for every task) read
/// it once per host instead of running the CLI again for each task. A screen
/// that must be current asks with `force_refresh`.
pub(crate) struct HostsCache {
    hosts: Mutex<Option<(Instant, Vec<String>)>>,
    ttl: Duration,
}

impl HostsCache {
    pub(crate) const fn new(ttl: Duration) -> Self {
        Self {
            hosts: Mutex::new(None),
            ttl,
        }
    }

    pub(crate) fn get_or(
        &self,
        force_refresh: bool,
        read: impl FnOnce() -> Result<Vec<String>, String>,
    ) -> Result<Vec<String>, String> {
        if !force_refresh {
            if let Ok(cached) = self.hosts.lock() {
                if let Some((at, hosts)) = cached.as_ref() {
                    if at.elapsed() < self.ttl {
                        return Ok(hosts.clone());
                    }
                }
            }
        }
        let hosts = read()?;
        if let Ok(mut cached) = self.hosts.lock() {
            *cached = Some((Instant::now(), hosts.clone()));
        }
        Ok(hosts)
    }
}

/// Where each CLI was found: checking runs it (`--version`), so once is enough.
static RESOLVED: LazyLock<Mutex<HashMap<String, PathBuf>>> = LazyLock::new(Default::default);

pub fn resolve_cli_binary(program: &str) -> Result<PathBuf, String> {
    if let Some(found) = RESOLVED
        .lock()
        .ok()
        .and_then(|found| found.get(program).cloned())
    {
        return Ok(found);
    }
    let found = find_cli_binary(program)?;
    if let Ok(mut resolved) = RESOLVED.lock() {
        resolved.insert(program.to_string(), found.clone());
    }
    Ok(found)
}

fn find_cli_binary(program: &str) -> Result<PathBuf, String> {
    let candidates = [
        PathBuf::from(program),
        PathBuf::from(format!("/opt/homebrew/bin/{program}")),
        PathBuf::from(format!("/usr/local/bin/{program}")),
    ];

    for candidate in candidates {
        if candidate.as_os_str() == program {
            if run_forge_cli_command(&candidate, |command| {
                command.arg("--version");
            })
            .is_ok()
            {
                return Ok(candidate);
            }
            continue;
        }

        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    let provider_name = match program {
        "gh" => "GitHub CLI",
        "glab" => "GitLab CLI",
        _ => "CLI",
    };

    Err(format!(
        "{provider_name} (`{program}`) is not available to the app. Install it or launch DCC from a shell where `{program}` is on PATH."
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hosts_are_read_once_until_they_expire_or_a_refresh_is_asked() {
        let cache = HostsCache::new(Duration::from_secs(60));
        let reads = std::cell::Cell::new(0);
        let read = || {
            reads.set(reads.get() + 1);
            Ok(vec!["gitlab.com".to_string()])
        };
        assert_eq!(cache.get_or(false, read).unwrap(), ["gitlab.com"]);
        assert_eq!(cache.get_or(false, read).unwrap(), ["gitlab.com"]);
        assert_eq!(reads.get(), 1, "the second read is kept");
        cache.get_or(true, read).unwrap();
        assert_eq!(reads.get(), 2, "a refresh reads again");
        // A failed read is not kept.
        let fresh = HostsCache::new(Duration::from_secs(60));
        assert!(fresh.get_or(false, || Err("no glab".into())).is_err());
        assert_eq!(fresh.get_or(false, read).unwrap(), ["gitlab.com"]);
        let expired = HostsCache::new(Duration::ZERO);
        expired.get_or(false, read).unwrap();
        expired.get_or(false, read).unwrap();
        assert_eq!(reads.get(), 5, "expired: read each time");
    }
}

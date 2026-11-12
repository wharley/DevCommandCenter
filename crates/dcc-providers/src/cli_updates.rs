//! A newer release of an agent's CLI, and its update from DCC. Each CLI is
//! the one DCC runs (found on the same PATH), its installed version from its
//! own `--version`, and how it was installed decides both where the newest
//! version is read and what updates it (as Synara and T3 Code do):
//! its own updater for a native install, the npm of the very prefix it lives
//! in, or Homebrew (`/opt/homebrew` on the Mac, `/home/linuxbrew` on
//! Linux). A global npm only root can write (a distro's Node under `/usr`)
//! is updated in a terminal, with `sudo`. After updating, the version is read again — an exit code
//! alone proves nothing (as MonoCode does) — and its model list is re-read.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use dcc_core::{CoreError, Result};
use serde_json::Value;
use tokio::process::Command;

use crate::cli_install_places::{homebrew_bins, writable};
use crate::common::augmented_path;

const FEED_TIMEOUT: Duration = Duration::from_secs(8);
const VERSION_TIMEOUT: Duration = Duration::from_secs(15);
const UPDATE_TIMEOUT: Duration = Duration::from_secs(300);

/// Where a CLI's newest version is read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Feed {
    Npm(&'static str),
    /// `grok update --check --json`
    GrokCheck,
    /// The version the official install script pins.
    CursorInstaller,
}

/// A CLI DCC runs, and what it knows about how it ships.
struct Cli {
    provider_id: &'static str,
    binary: &'static str,
    npm: Option<&'static str>,
    /// Its Homebrew package (a cask or a formula, also as `name@channel`,
    /// like `claude-code@latest`); the installed one is read from its path.
    homebrew: Option<&'static str>,
    /// A path fragment of its own installer, and the feed it follows.
    native: Option<(&'static str, Feed)>,
    /// Its own updater's arguments, when it has one.
    self_update: Option<&'static [&'static str]>,
}

const CLIS: &[Cli] = &[
    Cli {
        provider_id: "codex",
        binary: "codex",
        npm: Some("@openai/codex"),
        homebrew: Some("codex"),
        native: None,
        self_update: Some(&["update"]),
    },
    Cli {
        provider_id: "claude_code",
        binary: "claude",
        npm: Some("@anthropic-ai/claude-code"),
        homebrew: Some("claude-code"),
        native: Some((
            "/.local/share/claude/",
            Feed::Npm("@anthropic-ai/claude-code"),
        )),
        self_update: Some(&["update"]),
    },
    Cli {
        provider_id: "cursor",
        binary: "cursor-agent",
        npm: None,
        homebrew: None,
        native: Some(("/.local/share/cursor-agent/", Feed::CursorInstaller)),
        self_update: Some(&["update"]),
    },
    Cli {
        provider_id: "grok",
        binary: "grok",
        npm: None,
        homebrew: None,
        native: Some(("/.grok/", Feed::GrokCheck)),
        self_update: Some(&["update"]),
    },
    Cli {
        provider_id: "gemini",
        binary: "gemini",
        npm: Some("@google/gemini-cli"),
        homebrew: Some("gemini-cli"),
        native: None,
        self_update: None,
    },
];

/// The command that updates a CLI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateAction {
    pub program: PathBuf,
    pub args: Vec<String>,
    /// Put first on PATH, so an npm runs with the node beside it.
    pub path_prepend: Option<PathBuf>,
}

/// A CLI with a newer release. Without an action DCC cannot tell how it
/// was installed, and only offers the terminal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CliUpdate {
    pub provider_id: String,
    pub installed: String,
    pub latest: String,
    pub action: Option<UpdateAction>,
    /// What runs in a terminal the person watches: the action, else the
    /// CLI's own updater, else npm.
    pub terminal_command: String,
}

/// A word as the shell reads it, quoted.
fn quoted(word: &str) -> String {
    if !word.is_empty()
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "/._-@=:+".contains(c))
    {
        return word.to_string();
    }
    format!("'{}'", word.replace('\'', "'\\''"))
}

fn terminal_command(
    cli: &Cli,
    source: &Source,
    action: Option<&UpdateAction>,
    found: &Path,
) -> String {
    let words = |program: &Path, args: &[String]| {
        std::iter::once(quoted(&program.to_string_lossy()))
            .chain(args.iter().map(|arg| quoted(arg)))
            .collect::<Vec<_>>()
            .join(" ")
    };
    if let Some(action) = action {
        let command = words(&action.program, &action.args);
        return match &action.path_prepend {
            Some(first) => format!(
                "PATH={}:\"$PATH\" {command}",
                quoted(&first.to_string_lossy())
            ),
            None => command,
        };
    }
    // A global npm only root can write: that npm, with sudo.
    if let (Source::Npm { prefix }, Some(package)) = (source, cli.npm) {
        let npm = prefix.join("bin").join("npm");
        if npm.is_file() {
            return format!(
                "sudo {} install -g {package}@latest",
                quoted(&npm.to_string_lossy())
            );
        }
    }
    match (cli.self_update, cli.npm) {
        (Some(args), _) => words(
            found,
            &args.iter().map(|arg| arg.to_string()).collect::<Vec<_>>(),
        ),
        (None, Some(package)) => format!("npm install -g {package}@latest"),
        (None, None) => format!("{} --version", quoted(&found.to_string_lossy())),
    }
}

/// How a CLI was installed, read from where its executable really lives.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Source {
    Native,
    Npm {
        prefix: PathBuf,
    },
    /// The package as installed (`claude-code@latest` is not `claude-code`).
    Homebrew {
        name: String,
        cask: bool,
    },
    Unknown,
}

/// The Homebrew package a path lives in: its name, and whether it is a cask.
fn homebrew_package(path: &str) -> Option<(String, bool)> {
    [("/Caskroom/", true), ("/Cellar/", false)]
        .into_iter()
        .find_map(|(marker, cask)| {
            let rest = &path[path.find(marker)? + marker.len()..];
            let name = rest.split('/').next().filter(|name| !name.is_empty())?;
            Some((name.to_string(), cask))
        })
}

fn source_of(cli: &Cli, found: &Path, real: &Path) -> Source {
    let paths = [real.to_string_lossy(), found.to_string_lossy()];
    if let Some((marker, _)) = cli.native {
        if paths.iter().any(|path| path.contains(marker)) {
            return Source::Native;
        }
    }
    let real_path = real.to_string_lossy();
    let in_node_modules = real_path.contains("/lib/node_modules/");
    // Before npm: a formula keeps its own node_modules inside the Cellar. A
    // package that is not this CLI's (Homebrew's `node`) only lends its npm.
    if let Some((name, cask)) = paths.iter().find_map(|path| homebrew_package(path)) {
        let ours = cli.homebrew.is_some_and(|base| {
            name == base
                || name
                    .strip_prefix(base)
                    .is_some_and(|rest| rest.starts_with('@'))
        });
        if ours || !in_node_modules {
            return Source::Homebrew { name, cask };
        }
    }
    match real_path.find("/lib/node_modules/") {
        Some(cut) => Source::Npm {
            prefix: PathBuf::from(&real_path[..cut]),
        },
        None => Source::Unknown,
    }
}

/// The numbers a version leads with: `2026.10.22-27dcf5f` is 2026, 10, 22.
fn numbers(version: &str) -> Vec<u64> {
    version
        .split(|c: char| !(c.is_ascii_digit() || c == '.'))
        .find(|part| part.chars().next().is_some_and(|c| c.is_ascii_digit()))
        .unwrap_or("")
        .split('.')
        .map_while(|number| number.parse().ok())
        .collect()
}

/// The first version in a CLI's `--version` (`codex-cli 0.160.0`,
/// `2.1.293 (Claude Code)`, `grok 1.0.62 (…)`, `2026.10.16-e3cb207`).
fn version_in(output: &str) -> Option<String> {
    output
        .split_whitespace()
        .map(|word| word.trim_matches(|c: char| c == '(' || c == ')' || c == ','))
        .find(|word| {
            word.chars().next().is_some_and(|c| c.is_ascii_digit()) && numbers(word).len() >= 2
        })
        .map(str::to_string)
}

/// A version as the person reads it: its numbers, without a build's hash
/// (`2026.10.22-27dcf5f` reads 2026.10.22).
pub fn short_version(version: &str) -> String {
    let numbers = numbers(version);
    if numbers.is_empty() {
        return version.to_string();
    }
    numbers
        .iter()
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join(".")
}

/// Whether `latest` is a newer release than `installed` (by its numbers:
/// two builds of the same day are the same release).
fn newer(latest: &str, installed: &str) -> bool {
    let (latest, installed) = (numbers(latest), numbers(installed));
    !latest.is_empty() && !installed.is_empty() && latest > installed
}

/// The PATH DCC runs this CLI with: Claude's own order (the app's PATH,
/// then its native install, then the rest), plain `augmented_path` for the
/// others. A stale copy earlier on another PATH is never the one checked.
fn search_path(provider_id: &str) -> String {
    if provider_id != "claude_code" {
        return augmented_path();
    }
    let home = std::env::var("HOME").ok();
    let path = crate::claude_sdk_sidecar::claude_command_path(
        &std::env::var("PATH").unwrap_or_default(),
        home.as_deref(),
        &augmented_path(),
    );
    // Then where the Claude helper also looks (Homebrew where this
    // platform keeps it).
    let mut system: Vec<String> = homebrew_bins(home.as_deref())
        .into_iter()
        .map(|dir| dir.to_string_lossy().into_owned())
        .collect();
    for dir in ["/usr/local/bin", "/usr/bin"] {
        if !system.iter().any(|known| known == dir) {
            system.push(dir.to_string());
        }
    }
    let extras = home
        .iter()
        .map(|home| format!("{home}/.claude/local"))
        .chain(system);
    std::iter::once(path)
        .chain(extras)
        .collect::<Vec<_>>()
        .join(":")
}

/// The CLI DCC runs: the first on its PATH (or Claude's chosen override).
fn locate(cli: &Cli) -> Option<PathBuf> {
    if cli.provider_id == "claude_code" {
        if let Some(path) = std::env::var_os("DCC_CLAUDE_CODE_BIN_PATH") {
            return Some(PathBuf::from(path)).filter(|path| path.is_file());
        }
    }
    std::env::split_paths(&search_path(cli.provider_id))
        .map(|dir| dir.join(cli.binary))
        .find(|path| path.is_file())
}

fn homebrew() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok();
    homebrew_bins(home.as_deref())
        .into_iter()
        .map(|bin| bin.join("brew"))
        .find(|path| path.is_file())
}

fn action(cli: &Cli, source: &Source, found: &Path) -> Option<UpdateAction> {
    match source {
        Source::Native => Some(UpdateAction {
            program: found.to_path_buf(),
            args: cli.self_update?.iter().map(|arg| arg.to_string()).collect(),
            path_prepend: None,
        }),
        Source::Npm { prefix } => {
            let bin = prefix.join("bin");
            let npm = bin.join("npm");
            // Root's prefix (`/usr`): only a terminal can ask for sudo.
            (npm.is_file() && writable(&prefix.join("lib").join("node_modules")))
                .then(|| UpdateAction {
                    program: npm,
                    args: vec![
                        "install".into(),
                        "-g".into(),
                        format!("{}@latest", cli.npm.unwrap_or_default()),
                    ],
                    path_prepend: Some(bin),
                })
                .filter(|_| cli.npm.is_some())
        }
        Source::Homebrew { name, cask } => {
            let mut args = vec!["upgrade".to_string()];
            if *cask {
                args.push("--cask".into());
            }
            args.push(name.clone());
            Some(UpdateAction {
                program: homebrew()?,
                args,
                path_prepend: None,
            })
        }
        Source::Unknown => None,
    }
}

async fn output(program: &Path, args: &[&str], limit: Duration) -> Option<String> {
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(std::env::temp_dir())
        .env("PATH", augmented_path())
        .stdin(Stdio::null())
        .kill_on_drop(true);
    let output = tokio::time::timeout(limit, command.output())
        .await
        .ok()?
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

async fn installed(program: &Path) -> Option<String> {
    version_in(&output(program, &["--version"], VERSION_TIMEOUT).await?)
}

async fn fetch(url: &str) -> Option<String> {
    let client = reqwest::Client::builder()
        .timeout(FEED_TIMEOUT)
        .user_agent("dcc")
        .build()
        .ok()?;
    let response = client.get(url).send().await.ok()?;
    response.status().is_success().then_some(())?;
    response.text().await.ok()
}

/// `latestVersion` of `grok update --check --json`.
fn grok_latest(output: &str) -> Option<String> {
    let value: Value = serde_json::from_str(output.trim()).ok()?;
    value.get("latestVersion")?.as_str().map(str::to_string)
}

/// The version Cursor's install script pins in its download address.
fn cursor_latest(script: &str) -> Option<String> {
    let (_, rest) = script.split_once("downloads.cursor.com/lab/")?;
    let version: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-'))
        .collect();
    (!version.is_empty()).then_some(version)
}

async fn latest_from(feed: Feed, program: &Path) -> Option<String> {
    match feed {
        Feed::Npm(package) => {
            let body = fetch(&format!("https://registry.npmjs.org/{package}/latest")).await?;
            let value: Value = serde_json::from_str(&body).ok()?;
            value.get("version")?.as_str().map(str::to_string)
        }
        Feed::GrokCheck => {
            grok_latest(&output(program, &["update", "--check", "--json"], VERSION_TIMEOUT).await?)
        }
        Feed::CursorInstaller => cursor_latest(&fetch("https://cursor.com/install").await?),
    }
}

async fn homebrew_latest(name: &str, cask: bool) -> Option<String> {
    let kind = if cask { "cask" } else { "formula" };
    let body = fetch(&format!("https://formulae.brew.sh/api/{kind}/{name}.json")).await?;
    let value: Value = serde_json::from_str(&body).ok()?;
    let version = if cask {
        value.get("version")
    } else {
        value.get("versions")?.get("stable")
    };
    version?.as_str().map(str::to_string)
}

async fn latest(cli: &Cli, source: &Source, program: &Path) -> Option<String> {
    match source {
        Source::Native => latest_from(cli.native?.1, program).await,
        Source::Homebrew { name, cask } => homebrew_latest(name, *cask).await,
        Source::Npm { .. } | Source::Unknown => latest_from(Feed::Npm(cli.npm?), program).await,
    }
}

async fn check(cli: &Cli) -> Option<CliUpdate> {
    let found = locate(cli)?;
    let real = std::fs::canonicalize(&found).unwrap_or_else(|_| found.clone());
    let source = source_of(cli, &found, &real);
    let installed = installed(&found).await?;
    let latest = latest(cli, &source, &found).await?;
    let action = action(cli, &source, &found);
    newer(&latest, &installed).then(|| CliUpdate {
        provider_id: cli.provider_id.to_string(),
        installed,
        latest,
        terminal_command: terminal_command(cli, &source, action.as_ref(), &found),
        action,
    })
}

/// The installed CLIs with a newer release. A CLI DCC cannot find, read or
/// compare is left out quietly.
pub async fn check_all() -> Vec<CliUpdate> {
    futures::future::join_all(CLIS.iter().map(check))
        .await
        .into_iter()
        .flatten()
        .collect()
}

/// The last line a failed command printed, as its reason.
fn reason(printed: &str, status: std::process::ExitStatus) -> String {
    printed
        .lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("exited with {status}"))
}

/// Runs the update, then `verify`. It waits for the updater itself, not
/// for its output to close: an updater can leave a helper running that
/// holds the output open (Grok does), so what it prints goes to a file.
/// A run that takes too long may still have updated: the version decides.
pub async fn run(update: &CliUpdate) -> Result<String> {
    let action = update
        .action
        .as_ref()
        .ok_or_else(|| CoreError::Provider("DCC cannot tell how this CLI was installed".into()))?;
    let mut path = augmented_path();
    if let Some(first) = &action.path_prepend {
        path = format!("{}:{path}", first.display());
    }
    let failed = |error: std::io::Error| CoreError::Provider(error.to_string());
    let log = std::env::temp_dir().join(format!("dcc-cli-update-{}.log", uuid::Uuid::new_v4()));
    let file = std::fs::File::create(&log).map_err(failed)?;
    let mut command = Command::new(&action.program);
    command
        .args(&action.args)
        .current_dir(std::env::temp_dir())
        .env("PATH", path)
        .stdin(Stdio::null())
        .stdout(Stdio::from(file.try_clone().map_err(failed)?))
        .stderr(Stdio::from(file))
        .kill_on_drop(true);
    let waited = match command.spawn() {
        Ok(mut child) => tokio::time::timeout(UPDATE_TIMEOUT, child.wait()).await,
        Err(error) => Ok(Err(error)),
    };
    let printed = std::fs::read_to_string(&log).unwrap_or_default();
    let _ = std::fs::remove_file(&log);
    match waited {
        Err(_) => verify(update)
            .await
            .map_err(|_| CoreError::Provider("the update took too long".into())),
        Ok(Err(error)) => Err(failed(error)),
        Ok(Ok(status)) if !status.success() => Err(CoreError::Provider(reason(&printed, status))),
        Ok(Ok(_)) => verify(update).await,
    }
}

/// After an update (here or in the person's terminal): the CLI really is on
/// the new version, and its model list is forgotten so the picker reads the
/// new one. The version it is on.
pub async fn verify(update: &CliUpdate) -> Result<String> {
    let cli = CLIS
        .iter()
        .find(|cli| cli.provider_id == update.provider_id)
        .ok_or_else(|| CoreError::Provider("unknown CLI".into()))?;
    let now = match locate(cli) {
        Some(found) => installed(&found).await,
        None => None,
    }
    .unwrap_or_else(|| update.installed.clone());
    if newer(&update.latest, &now) {
        return Err(CoreError::Provider(format!(
            "still on {now} after updating"
        )));
    }
    crate::model_discovery::forget(&update.provider_id);
    Ok(now)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cli(id: &str) -> &'static Cli {
        CLIS.iter().find(|cli| cli.provider_id == id).unwrap()
    }

    #[test]
    fn versions_are_read_and_compared_by_their_numbers() {
        assert_eq!(
            version_in("codex-cli 0.160.0\n").as_deref(),
            Some("0.160.0")
        );
        assert_eq!(
            version_in("2.1.293 (Claude Code)").as_deref(),
            Some("2.1.293")
        );
        assert_eq!(
            version_in("grok 1.0.62 (1474f9cbdd91) [stable]").as_deref(),
            Some("1.0.62")
        );
        assert_eq!(
            version_in("2026.10.16-e3cb207").as_deref(),
            Some("2026.10.16-e3cb207")
        );
        assert_eq!(version_in("no version here"), None);
        assert!(newer("0.161.0", "0.160.0"));
        assert!(newer("0.160.10", "0.160.9"));
        assert!(!newer("0.160.0", "0.160.0"));
        assert!(newer("2026.10.22-27dcf5f", "2026.10.16-e3cb207"));
        assert!(
            !newer("2026.10.16-aaaaaaa", "2026.10.16-e3cb207"),
            "same day's build"
        );
        assert!(!newer("", "1.0.0"));
        assert_eq!(short_version("2026.10.22-27dcf5f"), "2026.10.22");
        assert_eq!(short_version("0.166.0"), "0.166.0");
        assert_eq!(short_version("beta"), "beta");
    }

    #[test]
    fn the_install_decides_the_update() {
        let home = Path::new("/Users/w");
        // Codex from npm in nvm's Node 22: that Node's npm, its bin first.
        let found = home.join(".nvm/versions/node/v22.21.1/bin/codex");
        let real =
            home.join(".nvm/versions/node/v22.21.1/lib/node_modules/@openai/codex/bin/codex.js");
        let source = source_of(cli("codex"), &found, &real);
        assert_eq!(
            source,
            Source::Npm {
                prefix: home.join(".nvm/versions/node/v22.21.1")
            }
        );
        // Claude, Cursor and Grok from their own installers: their own updater.
        let claude = home.join(".local/bin/claude");
        let source = source_of(
            cli("claude_code"),
            &claude,
            &home.join(".local/share/claude/versions/2.1.293"),
        );
        assert_eq!(source, Source::Native);
        let update = action(cli("claude_code"), &source, &claude).unwrap();
        assert_eq!(
            (update.program, update.args),
            (claude, vec!["update".to_string()])
        );
        let grok = home.join(".local/bin/grok");
        assert_eq!(
            source_of(
                cli("grok"),
                &grok,
                &home.join(".grok/downloads/grok-1.0.62-macos-aarch64")
            ),
            Source::Native
        );
        // Gemini from Homebrew; an unknown place has no button.
        assert_eq!(
            source_of(
                cli("gemini"),
                Path::new("/opt/homebrew/bin/gemini"),
                Path::new("/opt/homebrew/Cellar/gemini-cli/0.32.1/libexec/lib/node_modules/@google/gemini-cli/dist/index.js"),
            ),
            Source::Homebrew {
                name: "gemini-cli".into(),
                cask: false
            },
            "a formula's own node_modules stay Homebrew's"
        );
        // Claude from the `@latest` cask: that cask, not `claude-code`.
        let source = source_of(
            cli("claude_code"),
            Path::new("/opt/homebrew/bin/claude"),
            Path::new("/opt/homebrew/Caskroom/claude-code@latest/2.1.282/claude"),
        );
        assert_eq!(
            source,
            Source::Homebrew {
                name: "claude-code@latest".into(),
                cask: true
            }
        );
        if let Some(update) = action(cli("claude_code"), &source, Path::new("/x")) {
            assert_eq!(update.args, ["upgrade", "--cask", "claude-code@latest"]);
        }
        // Claude from npm with Homebrew's Node: that Node's npm, not `brew upgrade node`.
        let node = "/opt/homebrew/Cellar/node/24.1.0";
        assert_eq!(
            source_of(
                cli("claude_code"),
                Path::new("/opt/homebrew/bin/claude"),
                &Path::new(node).join("lib/node_modules/@anthropic-ai/claude-code/cli.js"),
            ),
            Source::Npm {
                prefix: PathBuf::from(node)
            }
        );
        assert_eq!(
            source_of(
                cli("cursor"),
                Path::new("/somewhere/cursor-agent"),
                Path::new("/somewhere/cursor-agent")
            ),
            Source::Unknown
        );
        assert_eq!(
            action(cli("cursor"), &Source::Unknown, Path::new("/x")),
            None
        );
    }

    #[test]
    fn the_terminal_runs_the_action_or_the_clis_own_updater() {
        let npm = UpdateAction {
            program: PathBuf::from("/Users/w/.nvm/versions/node/v22.21.1/bin/npm"),
            args: vec!["install".into(), "-g".into(), "@openai/codex@latest".into()],
            path_prepend: Some(PathBuf::from("/Users/w/.nvm/versions/node/v22.21.1/bin")),
        };
        assert_eq!(
            terminal_command(
                cli("codex"),
                &Source::Unknown,
                Some(&npm),
                Path::new("/x/codex")
            ),
            "PATH=/Users/w/.nvm/versions/node/v22.21.1/bin:\"$PATH\" /Users/w/.nvm/versions/node/v22.21.1/bin/npm install -g @openai/codex@latest"
        );
        // Installed somewhere DCC does not know: the CLI's own updater, else npm.
        assert_eq!(
            terminal_command(
                cli("codex"),
                &Source::Unknown,
                None,
                Path::new("/My Apps/codex")
            ),
            "'/My Apps/codex' update"
        );
        assert_eq!(
            terminal_command(
                cli("gemini"),
                &Source::Unknown,
                None,
                Path::new("/x/gemini")
            ),
            "npm install -g @google/gemini-cli@latest"
        );
        assert_eq!(quoted("it's"), "'it'\\''s'");
    }

    #[test]
    fn linux_installs_are_read_from_their_places() {
        let home = Path::new("/home/w");
        // Claude's and Grok's own installers keep the Mac's layout.
        assert_eq!(
            source_of(
                cli("claude_code"),
                &home.join(".local/bin/claude"),
                &home.join(".local/share/claude/versions/2.1.293"),
            ),
            Source::Native
        );
        assert_eq!(
            source_of(
                cli("grok"),
                &home.join(".local/bin/grok"),
                &home.join(".grok/downloads/grok-1.0.62-linux-x86_64"),
            ),
            Source::Native
        );
        // Homebrew on Linux: a formula under /home/linuxbrew.
        assert_eq!(
            source_of(
                cli("gemini"),
                Path::new("/home/linuxbrew/.linuxbrew/bin/gemini"),
                Path::new("/home/linuxbrew/.linuxbrew/Cellar/gemini-cli/0.32.1/libexec/lib/node_modules/@google/gemini-cli/dist/index.js"),
            ),
            Source::Homebrew {
                name: "gemini-cli".into(),
                cask: false
            }
        );
        // The distro's Node: npm under /usr.
        assert_eq!(
            source_of(
                cli("codex"),
                Path::new("/usr/bin/codex"),
                Path::new("/usr/lib/node_modules/@openai/codex/bin/codex.js"),
            ),
            Source::Npm {
                prefix: PathBuf::from("/usr")
            }
        );
    }

    #[test]
    fn a_global_npm_only_root_can_write_is_updated_with_sudo_in_a_terminal() {
        use std::os::unix::fs::PermissionsExt;
        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        let prefix =
            std::env::temp_dir().join(format!("dcc-npm-root-test-{}", uuid::Uuid::new_v4()));
        let modules = prefix.join("lib/node_modules");
        std::fs::create_dir_all(&modules).unwrap();
        std::fs::create_dir_all(prefix.join("bin")).unwrap();
        let npm = prefix.join("bin/npm");
        std::fs::write(&npm, "#!/bin/sh\n").unwrap();
        let source = Source::Npm {
            prefix: prefix.clone(),
        };
        let found = Path::new("/usr/bin/codex");
        assert!(action(cli("codex"), &source, found).is_some(), "writable");
        std::fs::set_permissions(&modules, std::fs::Permissions::from_mode(0o555)).unwrap();
        let update = action(cli("codex"), &source, found);
        let command = terminal_command(cli("codex"), &source, update.as_ref(), found);
        std::fs::set_permissions(&modules, std::fs::Permissions::from_mode(0o755)).unwrap();
        let _ = std::fs::remove_dir_all(&prefix);
        assert_eq!(update, None);
        assert_eq!(
            command,
            format!(
                "sudo {} install -g @openai/codex@latest",
                quoted(&npm.to_string_lossy())
            )
        );
    }

    fn script(dir: &Path, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join("updater.sh");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    fn update_with(program: PathBuf) -> CliUpdate {
        CliUpdate {
            // Not one DCC knows: `verify` answers at once.
            provider_id: "test-cli".into(),
            installed: "1.0.0".into(),
            latest: "1.0.1".into(),
            action: Some(UpdateAction {
                program,
                args: Vec::new(),
                path_prepend: None,
            }),
            terminal_command: String::new(),
        }
    }

    #[tokio::test]
    async fn an_updater_that_leaves_a_helper_running_is_not_waited_on() {
        let dir =
            std::env::temp_dir().join(format!("dcc-cli-update-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        // Grok's case: the helper keeps the output open long after it exits.
        let program = script(&dir, "(sleep 30 &)\necho updated");
        let ran = tokio::time::timeout(Duration::from_secs(10), run(&update_with(program)))
            .await
            .expect("waited for the updater, not its helper");
        assert_eq!(ran.unwrap_err().to_string(), "provider error: unknown CLI");
        // A failure says the last thing it printed.
        let program = script(
            &dir,
            "echo checking\necho 'Update failed: [unauthenticated]' >&2\nexit 1",
        );
        let failed = run(&update_with(program)).await.unwrap_err().to_string();
        assert!(
            failed.ends_with("Update failed: [unauthenticated]"),
            "{failed}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn feeds_are_read_from_their_answers() {
        assert_eq!(
            grok_latest(
                r#"{"currentVersion":"1.0.62","latestVersion":"1.0.70","updateAvailable":true}"#
            )
            .as_deref(),
            Some("1.0.70")
        );
        assert_eq!(grok_latest("not json"), None);
        let script = "DOWNLOAD_URL=\"https://downloads.cursor.com/lab/2026.10.22-27dcf5f/${OS}/${ARCH}/agent-cli-package.tar.gz\"";
        assert_eq!(cursor_latest(script).as_deref(), Some("2026.10.22-27dcf5f"));
        assert_eq!(cursor_latest("echo hi"), None);
    }
}

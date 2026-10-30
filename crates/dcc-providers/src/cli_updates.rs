//! A newer release of an agent's CLI, and its update from DCC. Each CLI is
//! the one DCC runs (found on the same PATH), its installed version from its
//! own `--version`, and how it was installed decides both where the newest
//! version is read and what updates it (as Synara and T3 Code do):
//! its own updater for a native install, the npm of the very prefix it lives
//! in, or Homebrew. After updating, the version is read again — an exit code
//! alone proves nothing (as MonoCode does) — and its model list is re-read.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use dcc_core::{CoreError, Result};
use serde_json::Value;
use tokio::process::Command;

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
    /// Formula or cask name, and whether it is a cask.
    homebrew: Option<(&'static str, bool)>,
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
        homebrew: Some(("codex", true)),
        native: None,
        self_update: Some(&["update"]),
    },
    Cli {
        provider_id: "claude_code",
        binary: "claude",
        npm: Some("@anthropic-ai/claude-code"),
        homebrew: Some(("claude-code", true)),
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
        homebrew: Some(("gemini-cli", false)),
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

fn terminal_command(cli: &Cli, action: Option<&UpdateAction>, found: &Path) -> String {
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
    Npm { prefix: PathBuf },
    Homebrew,
    Unknown,
}

fn source_of(cli: &Cli, found: &Path, real: &Path) -> Source {
    let paths = [real.to_string_lossy(), found.to_string_lossy()];
    if let Some((marker, _)) = cli.native {
        if paths.iter().any(|path| path.contains(marker)) {
            return Source::Native;
        }
    }
    // Before npm: a formula keeps its own node_modules inside the Cellar.
    if paths
        .iter()
        .any(|path| path.contains("/Cellar/") || path.contains("/Caskroom/"))
    {
        return Source::Homebrew;
    }
    let real_path = real.to_string_lossy();
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
    // Then where the Claude helper also looks.
    let extras = home
        .iter()
        .map(|home| format!("{home}/.claude/local"))
        .chain(["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"].map(str::to_string));
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
    ["/opt/homebrew/bin/brew", "/usr/local/bin/brew"]
        .map(PathBuf::from)
        .into_iter()
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
            npm.is_file()
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
        Source::Homebrew => {
            let (name, cask) = cli.homebrew?;
            let mut args = vec!["upgrade".to_string()];
            if cask {
                args.push("--cask".into());
            }
            args.push(name.to_string());
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
        Source::Homebrew => {
            let (name, cask) = cli.homebrew?;
            homebrew_latest(name, cask).await
        }
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
        terminal_command: terminal_command(cli, action.as_ref(), &found),
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
fn reason(output: &std::process::Output) -> String {
    [&output.stderr, &output.stdout]
        .iter()
        .flat_map(|bytes| {
            String::from_utf8_lossy(bytes)
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .next_back()
        .unwrap_or_else(|| format!("exited with {}", output.status))
}

/// Runs the update, then `verify`.
pub async fn run(update: &CliUpdate) -> Result<String> {
    let action = update
        .action
        .as_ref()
        .ok_or_else(|| CoreError::Provider("DCC cannot tell how this CLI was installed".into()))?;
    let mut path = augmented_path();
    if let Some(first) = &action.path_prepend {
        path = format!("{}:{path}", first.display());
    }
    let mut command = Command::new(&action.program);
    command
        .args(&action.args)
        .current_dir(std::env::temp_dir())
        .env("PATH", path)
        .stdin(Stdio::null())
        .kill_on_drop(true);
    let output = tokio::time::timeout(UPDATE_TIMEOUT, command.output())
        .await
        .map_err(|_| CoreError::Provider("the update took too long".into()))?
        .map_err(|error| CoreError::Provider(error.to_string()))?;
    if !output.status.success() {
        return Err(CoreError::Provider(reason(&output)));
    }
    verify(update).await
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
            Source::Homebrew,
            "a formula's own node_modules stay Homebrew's"
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
            terminal_command(cli("codex"), Some(&npm), Path::new("/x/codex")),
            "PATH=/Users/w/.nvm/versions/node/v22.21.1/bin:\"$PATH\" /Users/w/.nvm/versions/node/v22.21.1/bin/npm install -g @openai/codex@latest"
        );
        // Installed somewhere DCC does not know: the CLI's own updater, else npm.
        assert_eq!(
            terminal_command(cli("codex"), None, Path::new("/My Apps/codex")),
            "'/My Apps/codex' update"
        );
        assert_eq!(
            terminal_command(cli("gemini"), None, Path::new("/x/gemini")),
            "npm install -g @google/gemini-cli@latest"
        );
        assert_eq!(quoted("it's"), "'it'\\''s'");
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

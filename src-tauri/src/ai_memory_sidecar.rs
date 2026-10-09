//! Optional local ai-memory process owned by the DCC application.
//!
//! Development builds opt into this manager with `DCC_AI_MEMORY_AUTO_START=1`.
//! Release builds enable it by default when a native binary is bundled.
//! A configured `DCC_AI_MEMORY_URL` always wins, which preserves the
//! remote-server mode.

use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Manager};

use dcc_core::{
    domain::mcp::McpSecretReferenceId,
    ports::{CredentialStore, SecretValue},
};
use dcc_infra::credential_store::SystemCredentialStore;

const DEFAULT_PORT: u16 = 49_374;
const STARTUP_TIMEOUT: Duration = Duration::from_secs(8);
const HEALTH_TIMEOUT: Duration = Duration::from_millis(350);
const SETTINGS_FILE_NAME: &str = "ai-memory-settings.json";
const TOKEN_REFERENCE: &str = "ai-memory:default";
const SHUTDOWN_GRACE: Duration = Duration::from_secs(3);
const VERSION_MARKER_FILE: &str = "dcc-sidecar-version";
const UPGRADE_BACKUP_PREFIX: &str = "before-";

/// Server settings the DCC pins on the sidecar it manages, over whatever the
/// data directory's `config.toml` says. ai-memory reads `AI_MEMORY_<SECTION>__<KEY>`
/// overrides; each value below was checked against the pinned release.
///
/// - The cross-project profile stays off: the DCC keeps one memory per
///   project, and the profile would merge habits across them.
/// - Session ends do not open handoffs: the DCC has its own re-anchor and
///   nothing would ever accept them.
/// - The background auto-improve job stays off, so the sidecar never starts
///   LLM work the user did not ask for.
/// - FTS stopwords cover Portuguese and English, because the DCC queries with
///   the user's whole prompt and function words otherwise match every page.
const PINNED_SERVER_ENV: &[(&str, &str)] = &[
    ("AI_MEMORY_PROFILE__ENABLED", "false"),
    ("AI_MEMORY_HANDOFF__CREATE_ON_SESSION_END", "false"),
    ("AI_MEMORY_AUTO_IMPROVE__SCHEDULER__ENABLED", "false"),
    (
        "AI_MEMORY_SEARCH_FTS_STOPWORDS",
        "a,o,e,é,as,os,um,uma,uns,umas,de,da,do,das,dos,em,no,na,nos,nas,ao,aos,à,às,\
         por,pra,para,com,sem,que,se,não,nao,mais,mas,ou,como,isso,isto,esse,essa,este,esta,\
         ele,ela,eu,me,meu,minha,seu,sua,the,an,and,or,of,to,in,on,at,for,with,from,by,\
         is,are,was,be,it,this,that,these,those,as,if,not,do,does,can,should,i,you,we",
    ),
];

#[derive(Clone, Debug, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiMemorySettingsInput {
    pub mode: String,
    pub base_url: Option<String>,
    pub workspace: String,
    pub project: String,
    pub data_dir: Option<String>,
    pub token: Option<String>,
}

#[derive(Clone, Debug, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiMemorySettingsOutput {
    pub mode: String,
    pub base_url: Option<String>,
    pub workspace: String,
    pub project: String,
    pub data_dir: Option<String>,
    pub token_configured: bool,
    pub restart_required: bool,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedAiMemorySettings {
    mode: String,
    base_url: Option<String>,
    workspace: String,
    project: String,
    data_dir: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiMemorySidecarStatus {
    pub mode: String,
    pub running: bool,
    pub url: Option<String>,
    pub data_dir: Option<String>,
    pub binary_path: Option<String>,
    pub version: Option<String>,
    pub log_path: Option<String>,
    pub message: Option<String>,
}

pub struct AiMemorySidecar {
    child: Mutex<Option<Child>>,
    pub base_url: Option<String>,
    pub data_dir: Option<PathBuf>,
    binary_path: Option<PathBuf>,
    version: Option<String>,
    startup_error: Option<String>,
    /// A server this instance did not start was already answering on the
    /// port, so the DCC uses it as is (its version and settings are unknown).
    shared: bool,
}

impl AiMemorySidecar {
    pub async fn load_persisted_settings(app_data_dir: &Path) -> Result<(), String> {
        let path = app_data_dir.join(SETTINGS_FILE_NAME);
        let contents = match fs::read_to_string(&path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(format!("could not read ai-memory settings: {error}")),
        };
        let settings: PersistedAiMemorySettings = serde_json::from_str(&contents)
            .map_err(|error| format!("could not parse ai-memory settings: {error}"))?;
        apply_settings_to_environment(&settings)?;
        let token = SystemCredentialStore::default()
            .resolve_secret(&McpSecretReferenceId(TOKEN_REFERENCE.to_string()))
            .await
            .map_err(|error| format!("could not read ai-memory token: {error}"))?;
        if let Some(token) = token {
            let token = String::from_utf8(token.expose_secret().to_vec())
                .map_err(|_| "stored ai-memory token is not valid UTF-8".to_string())?;
            std::env::set_var("DCC_AI_MEMORY_TOKEN", token);
        }
        Ok(())
    }

    pub async fn read_settings(app_data_dir: &Path) -> Result<AiMemorySettingsOutput, String> {
        let path = app_data_dir.join(SETTINGS_FILE_NAME);
        let persisted = match fs::read_to_string(&path) {
            Ok(contents) => serde_json::from_str::<PersistedAiMemorySettings>(&contents)
                .map_err(|error| format!("could not parse ai-memory settings: {error}"))?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(default_settings_output(false));
            }
            Err(error) => return Err(format!("could not read ai-memory settings: {error}")),
        };
        let token_configured = SystemCredentialStore::default()
            .resolve_secret(&McpSecretReferenceId(TOKEN_REFERENCE.to_string()))
            .await
            .map_err(|error| format!("could not read ai-memory token: {error}"))?
            .is_some();
        Ok(AiMemorySettingsOutput {
            mode: persisted.mode,
            base_url: persisted.base_url,
            workspace: persisted.workspace,
            project: persisted.project,
            data_dir: persisted.data_dir,
            token_configured,
            restart_required: false,
        })
    }

    pub async fn save_settings(
        app_data_dir: &Path,
        input: AiMemorySettingsInput,
    ) -> Result<AiMemorySettingsOutput, String> {
        validate_settings(&input)?;
        let persisted = PersistedAiMemorySettings {
            mode: input.mode.clone(),
            base_url: input.base_url.clone(),
            workspace: input.workspace.clone(),
            project: input.project.clone(),
            data_dir: input.data_dir.clone(),
        };
        let bytes = serde_json::to_vec_pretty(&persisted)
            .map_err(|error| format!("could not encode ai-memory settings: {error}"))?;
        let path = app_data_dir.join(SETTINGS_FILE_NAME);
        fs::write(&path, bytes)
            .map_err(|error| format!("could not save ai-memory settings: {error}"))?;
        let store = SystemCredentialStore::default();
        let reference = McpSecretReferenceId(TOKEN_REFERENCE.to_string());
        let token_configured = match input.token.as_deref().map(str::trim) {
            Some("") => {
                store
                    .delete_secret(&reference)
                    .await
                    .map_err(|error| format!("could not clear ai-memory token: {error}"))?;
                std::env::remove_var("DCC_AI_MEMORY_TOKEN");
                false
            }
            Some(token) => {
                store
                    .store_secret(
                        &reference,
                        SecretValue::new(token.as_bytes().to_vec())
                            .map_err(|error| error.to_string())?,
                    )
                    .await
                    .map_err(|error| format!("could not save ai-memory token: {error}"))?;
                std::env::set_var("DCC_AI_MEMORY_TOKEN", token);
                true
            }
            None => std::env::var("DCC_AI_MEMORY_TOKEN")
                .ok()
                .is_some_and(|value| !value.trim().is_empty()),
        };
        apply_settings_to_environment(&persisted)?;
        Ok(AiMemorySettingsOutput {
            mode: persisted.mode,
            base_url: persisted.base_url,
            workspace: persisted.workspace,
            project: persisted.project,
            data_dir: persisted.data_dir,
            token_configured,
            restart_required: true,
        })
    }

    pub fn start(app: &AppHandle, app_data_dir: &Path) -> Result<Self, String> {
        let release_default = !cfg!(debug_assertions) && !env_truthy("DCC_AI_MEMORY_DISABLE");
        if !env_truthy("DCC_AI_MEMORY_AUTO_START") && !release_default {
            return Ok(Self::disabled());
        }
        if let Ok(url) = std::env::var("DCC_AI_MEMORY_URL") {
            if !url.trim().is_empty() {
                eprintln!("[DCC][ai-memory] auto-start skipped; using configured URL {url}");
                return Ok(Self::disabled());
            }
        }

        let port = std::env::var("DCC_AI_MEMORY_PORT")
            .ok()
            .and_then(|value| value.parse::<u16>().ok())
            .filter(|port| *port > 0)
            .unwrap_or(DEFAULT_PORT);
        let base_url = format!("http://127.0.0.1:{port}");
        let data_dir = std::env::var_os("DCC_AI_MEMORY_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| app_data_dir.join("ai-memory"));
        if is_healthy_once(&base_url) {
            // A crash, a force quit or an update relaunch skips the exit hook
            // and leaves the previous sidecar running. Reusing it would keep
            // an old binary and none of the pinned settings, so a server on
            // our own data directory is replaced; anything else is shared.
            match orphaned_sidecar_pid(port, &data_dir) {
                Some(pid) if stop_orphan(pid, &base_url) => {
                    eprintln!("[DCC][ai-memory] replaced orphaned sidecar (pid {pid}) left by a previous run");
                }
                Some(pid) => {
                    return Err(format!(
                        "an orphaned ai-memory (pid {pid}) is holding {base_url} and did not stop"
                    ));
                }
                None => {
                    eprintln!(
                        "[DCC][ai-memory] server already reachable at {base_url}; reusing it without spawning a child"
                    );
                    std::env::set_var("DCC_AI_MEMORY_URL", &base_url);
                    return Ok(Self {
                        child: Mutex::new(None),
                        base_url: Some(base_url),
                        data_dir: None,
                        binary_path: None,
                        version: None,
                        startup_error: None,
                        shared: true,
                    });
                }
            }
        }
        let binary = locate_binary(app)?;
        fs::create_dir_all(&data_dir).map_err(|error| {
            format!(
                "could not create ai-memory data directory {}: {error}",
                data_dir.display()
            )
        })?;

        let version = sidecar_command(&binary)
            .arg("--version")
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
            .filter(|value| !value.is_empty());
        if let Err(error) = backup_before_upgrade(&data_dir, version.as_deref()) {
            // Starting is still safe; only the way back to an older DCC is
            // lost, so the failure is reported instead of disabling memory.
            eprintln!("[DCC][ai-memory] pre-upgrade backup failed: {error}");
        }

        let init_status = sidecar_command(&binary)
            .args(["--data-dir"])
            .arg(&data_dir)
            .arg("init")
            .status()
            .map_err(|error| format!("could not initialise ai-memory: {error}"))?;
        if !init_status.success() {
            return Err(format!("ai-memory init exited with status {init_status}"));
        }

        let log_dir = data_dir.join("logs");
        fs::create_dir_all(&log_dir).map_err(|error| error.to_string())?;
        let log_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_dir.join("dcc-sidecar.log"))
            .map_err(|error| error.to_string())?;
        let error_file = log_file.try_clone().map_err(|error| error.to_string())?;
        let child = sidecar_command(&binary)
            .args(["--data-dir"])
            .arg(&data_dir)
            .args(["serve", "--transport", "http", "--bind"])
            .arg(format!("127.0.0.1:{port}"))
            .stdin(Stdio::null())
            .stdout(Stdio::from(log_file))
            .stderr(Stdio::from(error_file))
            .spawn()
            .map_err(|error| format!("could not start ai-memory: {error}"))?;

        if !wait_until_healthy(&base_url) {
            let mut child = child;
            stop_child(&mut child);
            return Err(format!(
                "ai-memory did not become ready at {base_url} within {} seconds",
                STARTUP_TIMEOUT.as_secs()
            ));
        }

        // The DCC adapter reads this once per operation. Set it only after the
        // process is healthy so a failed start cannot create a misleading URL.
        std::env::set_var("DCC_AI_MEMORY_URL", &base_url);
        std::env::set_var("DCC_AI_MEMORY_DATA_DIR", &data_dir);
        if let Some(version) = version.as_deref() {
            if let Err(error) = fs::write(data_dir.join(VERSION_MARKER_FILE), version) {
                eprintln!("[DCC][ai-memory] could not record sidecar version: {error}");
            }
        }
        eprintln!(
            "[DCC][ai-memory] sidecar running at {base_url} (data: {})",
            data_dir.display()
        );
        Ok(Self {
            child: Mutex::new(Some(child)),
            base_url: Some(base_url),
            data_dir: Some(data_dir),
            binary_path: Some(binary),
            version,
            startup_error: None,
            shared: false,
        })
    }

    pub fn disabled() -> Self {
        Self {
            child: Mutex::new(None),
            base_url: None,
            data_dir: None,
            binary_path: None,
            version: None,
            startup_error: None,
            shared: false,
        }
    }

    pub fn unavailable(error: impl Into<String>) -> Self {
        Self {
            child: Mutex::new(None),
            base_url: None,
            data_dir: None,
            binary_path: None,
            version: None,
            startup_error: Some(error.into()),
            shared: false,
        }
    }

    pub fn status(&self) -> AiMemorySidecarStatus {
        let running = self
            .child
            .lock()
            .map(|child| child.is_some())
            .unwrap_or(false);
        let configured_url = std::env::var("DCC_AI_MEMORY_URL")
            .ok()
            .filter(|url| !url.trim().is_empty());
        let mode = if running {
            "managed"
        } else if self.shared {
            "shared"
        } else if configured_url.is_some() {
            "remote"
        } else if self.startup_error.is_some() {
            "unavailable"
        } else {
            "disabled"
        };
        let url = self.base_url.clone().or(configured_url);
        let data_dir = self
            .data_dir
            .as_ref()
            .map(|path| path.display().to_string())
            .or_else(|| {
                std::env::var_os("DCC_AI_MEMORY_DATA_DIR")
                    .map(PathBuf::from)
                    .map(|path| path.display().to_string())
            });
        let log_path = data_dir.as_ref().map(|path| {
            PathBuf::from(path)
                .join("logs/dcc-sidecar.log")
                .display()
                .to_string()
        });
        AiMemorySidecarStatus {
            mode: mode.to_string(),
            running,
            url,
            data_dir,
            binary_path: self
                .binary_path
                .as_ref()
                .map(|path| path.display().to_string()),
            version: self.version.clone(),
            log_path,
            message: self.startup_error.clone(),
        }
    }

    pub fn shutdown(&self) {
        let Ok(mut child) = self.child.lock() else {
            return;
        };
        let Some(mut child) = child.take() else {
            return;
        };
        stop_child(&mut child);
        eprintln!("[DCC][ai-memory] sidecar stopped");
    }
}

impl Drop for AiMemorySidecar {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn default_settings_output(token_configured: bool) -> AiMemorySettingsOutput {
    AiMemorySettingsOutput {
        mode: if env_truthy("DCC_AI_MEMORY_DISABLE") {
            "disabled".to_string()
        } else if std::env::var("DCC_AI_MEMORY_URL")
            .ok()
            .is_some_and(|value| !value.trim().is_empty())
        {
            "remote".to_string()
        } else {
            "managed".to_string()
        },
        base_url: std::env::var("DCC_AI_MEMORY_URL").ok(),
        workspace: std::env::var("DCC_AI_MEMORY_WORKSPACE")
            .unwrap_or_else(|_| "dcc-workspace".to_string()),
        project: std::env::var("DCC_AI_MEMORY_PROJECT")
            .unwrap_or_else(|_| "dcc-project".to_string()),
        data_dir: std::env::var("DCC_AI_MEMORY_DATA_DIR").ok(),
        token_configured,
        restart_required: false,
    }
}

fn validate_settings(input: &AiMemorySettingsInput) -> Result<(), String> {
    if !matches!(input.mode.as_str(), "managed" | "remote" | "disabled") {
        return Err("ai-memory mode must be managed, remote, or disabled".to_string());
    }
    for (label, value) in [("workspace", &input.workspace), ("project", &input.project)] {
        if value.trim().is_empty() || value.chars().count() > 200 || value.contains('\0') {
            return Err(format!("ai-memory {label} is invalid"));
        }
        // ai-memory reads `workspace/project` as a two-part label, so names
        // themselves cannot contain a slash.
        if value.contains('/') {
            return Err(format!("ai-memory {label} cannot contain '/'"));
        }
    }
    if input.mode == "remote" {
        let url = input
            .base_url
            .as_deref()
            .ok_or_else(|| "a URL is required for remote ai-memory mode".to_string())?;
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err("ai-memory URL must use http:// or https://".to_string());
        }
    }
    Ok(())
}

fn apply_settings_to_environment(settings: &PersistedAiMemorySettings) -> Result<(), String> {
    std::env::set_var("DCC_AI_MEMORY_WORKSPACE", &settings.workspace);
    std::env::set_var("DCC_AI_MEMORY_PROJECT", &settings.project);
    match settings.mode.as_str() {
        "managed" => {
            std::env::remove_var("DCC_AI_MEMORY_URL");
            std::env::remove_var("DCC_AI_MEMORY_DISABLE");
            std::env::set_var("DCC_AI_MEMORY_AUTO_START", "1");
        }
        "remote" => {
            let url = settings
                .base_url
                .as_deref()
                .ok_or_else(|| "a URL is required for remote ai-memory mode".to_string())?;
            std::env::set_var("DCC_AI_MEMORY_URL", url);
            std::env::remove_var("DCC_AI_MEMORY_DISABLE");
        }
        "disabled" => {
            std::env::remove_var("DCC_AI_MEMORY_URL");
            std::env::set_var("DCC_AI_MEMORY_DISABLE", "1");
        }
        _ => return Err("ai-memory mode is invalid".to_string()),
    }
    if let Some(data_dir) = settings
        .data_dir
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        std::env::set_var("DCC_AI_MEMORY_DATA_DIR", data_dir);
    } else {
        std::env::remove_var("DCC_AI_MEMORY_DATA_DIR");
    }
    Ok(())
}

/// A command for the managed binary that does not inherit the user's own
/// ai-memory configuration (`AI_MEMORY_*`, including LLM providers and
/// keys) from the shell that launched the DCC, plus the DCC's pinned settings.
fn sidecar_command(binary: &Path) -> Command {
    let mut command = Command::new(binary);
    for (name, _) in std::env::vars_os() {
        if name
            .to_str()
            .is_some_and(|name| name.starts_with("AI_MEMORY_"))
        {
            command.env_remove(name);
        }
    }
    for (name, value) in PINNED_SERVER_ENV {
        command.env(name, value);
    }
    command
}

/// Asks the server to exit so it can drain its writer, then forces it.
fn stop_child(child: &mut Child) {
    #[cfg(unix)]
    {
        if let Ok(pid) = libc::pid_t::try_from(child.id()) {
            // SAFETY: `pid` is our own live child; SIGTERM only asks it to exit.
            unsafe {
                libc::kill(pid, libc::SIGTERM);
            }
            let started = Instant::now();
            while started.elapsed() < SHUTDOWN_GRACE {
                if matches!(child.try_wait(), Ok(Some(_))) {
                    return;
                }
                thread::sleep(Duration::from_millis(50));
            }
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// ai-memory migrations are one-way: once a newer release opens the store, an
/// older DCC can no longer read it. Before the first start of a different
/// binary version, copy the database aside so going back stays possible. The
/// wiki is not copied; it is Markdown with its own git history. Only the
/// newest backup is kept.
fn backup_before_upgrade(data_dir: &Path, version: Option<&str>) -> Result<(), String> {
    let Some(version) = version else {
        return Ok(());
    };
    let db_dir = data_dir.join("db");
    if !db_dir.join("memory.sqlite").is_file() {
        return Ok(());
    }
    let previous = fs::read_to_string(data_dir.join(VERSION_MARKER_FILE)).ok();
    if previous.as_deref().map(str::trim) == Some(version) {
        return Ok(());
    }
    let backups = data_dir.join("backups");
    let target = backups.join(format!(
        "{UPGRADE_BACKUP_PREFIX}{}",
        version.replace(|c: char| !c.is_ascii_alphanumeric() && c != '.', "-")
    ));
    if target.exists() {
        return Ok(());
    }
    let staging = backups.join(".staging");
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(staging.join("db")).map_err(|error| error.to_string())?;
    for entry in fs::read_dir(&db_dir).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        if entry
            .file_type()
            .map_err(|error| error.to_string())?
            .is_file()
        {
            fs::copy(entry.path(), staging.join("db").join(entry.file_name()))
                .map_err(|error| error.to_string())?;
        }
    }
    fs::write(
        staging.join("README.txt"),
        format!(
            "ai-memory database copied by the DCC before {version} first opened it.\n\
             Previous version: {}\n\
             To go back, stop the DCC and replace <data dir>/db with this db folder.\n",
            previous
                .as_deref()
                .map(str::trim)
                .unwrap_or("unknown (before the DCC recorded it)")
        ),
    )
    .map_err(|error| error.to_string())?;
    fs::rename(&staging, &target).map_err(|error| error.to_string())?;
    for entry in fs::read_dir(&backups)
        .map_err(|error| error.to_string())?
        .flatten()
    {
        let path = entry.path();
        let is_older_backup = path != target
            && entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with(UPGRADE_BACKUP_PREFIX));
        if is_older_backup {
            let _ = fs::remove_dir_all(path);
        }
    }
    eprintln!(
        "[DCC][ai-memory] database backed up to {} before the first start of {version}",
        target.display()
    );
    Ok(())
}

fn env_truthy(name: &str) -> bool {
    std::env::var(name)
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes"
            )
        })
        .unwrap_or(false)
}

fn locate_binary(app: &AppHandle) -> Result<PathBuf, String> {
    let mut candidates = Vec::new();
    if let Some(path) = std::env::var_os("DCC_AI_MEMORY_BIN") {
        candidates.push(PathBuf::from(path));
    }
    if let Ok(resource_dir) = app.path().resource_dir() {
        candidates.push(resource_dir.join("ai-memory"));
        candidates.push(resource_dir.join("ai-memory-aarch64-apple-darwin"));
        candidates.push(resource_dir.join("ai-memory-x86_64-apple-darwin"));
    }
    if let Ok(executable) = std::env::current_exe() {
        if let Some(parent) = executable.parent() {
            candidates.push(parent.join("ai-memory"));
            candidates.push(parent.join("ai-memory-aarch64-apple-darwin"));
            candidates.push(parent.join("ai-memory-x86_64-apple-darwin"));
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        candidates.push(PathBuf::from(home).join("Applications/ai-memory/ai-memory"));
    }
    candidates
        .into_iter()
        .find(|candidate| candidate.is_file())
        .ok_or_else(|| {
            "ai-memory auto-start requested, but no binary was found; set DCC_AI_MEMORY_BIN or install the release under ~/Applications/ai-memory".to_string()
        })
}

/// The pid of an ai-memory server listening on `port` for `data_dir`, i.e. a
/// sidecar a previous DCC run started and never stopped. A server for any
/// other data directory (another install, the dev build, a remote tunnel)
/// is not ours and is left alone.
fn orphaned_sidecar_pid(port: u16, data_dir: &Path) -> Option<u32> {
    let listeners = Command::new("lsof")
        .args(["-nP", &format!("-iTCP:{port}"), "-sTCP:LISTEN", "-t"])
        .output()
        .ok()?;
    let data_dir = data_dir.to_str()?;
    String::from_utf8_lossy(&listeners.stdout)
        .lines()
        .filter_map(|line| line.trim().parse::<u32>().ok())
        .find(|pid| {
            Command::new("ps")
                .args(["-o", "command=", "-p", &pid.to_string()])
                .output()
                .is_ok_and(|output| {
                    is_sidecar_command(&String::from_utf8_lossy(&output.stdout), data_dir)
                })
        })
}

fn is_sidecar_command(command: &str, data_dir: &str) -> bool {
    let program = command.split(" --").next().unwrap_or_default();
    Path::new(program.trim())
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("ai-memory"))
        && command.contains(&format!("--data-dir {data_dir} "))
        && command.contains(" serve ")
}

/// Stops an orphaned sidecar and waits until its port is free.
fn stop_orphan(pid: u32, base_url: &str) -> bool {
    #[cfg(unix)]
    {
        let Ok(pid) = libc::pid_t::try_from(pid) else {
            return false;
        };
        // SAFETY: `pid` was matched to an ai-memory serving our data directory.
        unsafe {
            libc::kill(pid, libc::SIGTERM);
        }
        let started = Instant::now();
        while started.elapsed() < SHUTDOWN_GRACE {
            if !is_healthy_once(base_url) {
                return true;
            }
            thread::sleep(Duration::from_millis(100));
        }
        // SAFETY: as above; the server ignored SIGTERM.
        unsafe {
            libc::kill(pid, libc::SIGKILL);
        }
        thread::sleep(Duration::from_millis(200));
        !is_healthy_once(base_url)
    }
    #[cfg(not(unix))]
    {
        let _ = (pid, base_url);
        false
    }
}

fn wait_until_healthy(base_url: &str) -> bool {
    let client = match reqwest::blocking::Client::builder()
        .timeout(HEALTH_TIMEOUT)
        .build()
    {
        Ok(client) => client,
        Err(_) => return false,
    };
    let started = Instant::now();
    while started.elapsed() < STARTUP_TIMEOUT {
        if client
            .get(format!("{base_url}/mcp"))
            .send()
            .map(|response| response.status().as_u16() == 405 || response.status().is_success())
            .unwrap_or(false)
        {
            return true;
        }
        thread::sleep(Duration::from_millis(120));
    }
    false
}

fn is_healthy_once(base_url: &str) -> bool {
    reqwest::blocking::Client::builder()
        .timeout(HEALTH_TIMEOUT)
        .build()
        .ok()
        .and_then(|client| client.get(format!("{base_url}/mcp")).send().ok())
        .map(|response| response.status().as_u16() == 405 || response.status().is_success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(mode: &str, base_url: Option<&str>) -> AiMemorySettingsInput {
        AiMemorySettingsInput {
            mode: mode.to_string(),
            base_url: base_url.map(str::to_string),
            workspace: "workspace".to_string(),
            project: "project".to_string(),
            data_dir: None,
            token: None,
        }
    }

    #[test]
    fn remote_mode_requires_http_url() {
        assert!(validate_settings(&settings("remote", None)).is_err());
        assert!(validate_settings(&settings("remote", Some("file:///tmp/memory"))).is_err());
        assert!(validate_settings(&settings("remote", Some("http://127.0.0.1:49374"))).is_ok());
    }

    #[test]
    fn only_a_server_on_our_data_directory_counts_as_an_orphan() {
        let data_dir = "/Users/me/Library/Application Support/com.devcommandcenter.app/ai-memory";
        let ours = format!(
            "/Applications/Dev Command Center.app/Contents/MacOS/ai-memory --data-dir {data_dir} serve --transport http --bind 127.0.0.1:49374"
        );
        assert!(is_sidecar_command(&ours, data_dir));
        let dev_build = ours.replace("com.devcommandcenter.app/", "com.devcommandcenter.app.dev/");
        assert!(!is_sidecar_command(&dev_build, data_dir));
        let other_program = ours.replace("MacOS/ai-memory", "MacOS/opencode");
        assert!(!is_sidecar_command(&other_program, data_dir));
    }

    #[test]
    fn settings_reject_slashes_in_scope_names() {
        let mut input = settings("managed", None);
        input.project = "team/dcc".to_string();
        assert!(validate_settings(&input).is_err());
    }

    #[test]
    fn sidecar_command_drops_inherited_ai_memory_settings() {
        std::env::set_var("AI_MEMORY_LLM_PROVIDER", "openai");
        let command = sidecar_command(Path::new("/bin/true"));
        std::env::remove_var("AI_MEMORY_LLM_PROVIDER");
        let envs: Vec<_> = command.get_envs().collect();
        assert!(envs
            .iter()
            .any(|(name, value)| { *name == "AI_MEMORY_LLM_PROVIDER" && value.is_none() }));
        let (_, stopwords) = PINNED_SERVER_ENV
            .iter()
            .find(|(name, _)| *name == "AI_MEMORY_SEARCH_FTS_STOPWORDS")
            .unwrap();
        assert!(stopwords.split(',').all(|word| !word.is_empty() && word.trim() == word));
        assert!(stopwords.split(',').any(|word| word == "não"));
        assert!(envs.iter().any(|(name, value)| {
            *name == "AI_MEMORY_PROFILE__ENABLED"
                && value.and_then(|value| value.to_str()) == Some("false")
        }));
    }

    #[test]
    fn upgrade_backup_runs_once_per_version_and_keeps_only_the_newest() {
        let data_dir =
            std::env::temp_dir().join(format!("dcc-ai-memory-backup-{}", std::process::id()));
        let _ = fs::remove_dir_all(&data_dir);
        fs::create_dir_all(data_dir.join("db")).unwrap();
        fs::write(data_dir.join("db/memory.sqlite"), b"v63").unwrap();
        fs::write(data_dir.join(VERSION_MARKER_FILE), "ai-memory 2.2.2").unwrap();

        backup_before_upgrade(&data_dir, Some("ai-memory 2.6.1")).unwrap();
        let first = data_dir.join("backups/before-ai-memory-2.6.1");
        assert_eq!(fs::read(first.join("db/memory.sqlite")).unwrap(), b"v63");
        assert!(fs::read_to_string(first.join("README.txt"))
            .unwrap()
            .contains("ai-memory 2.2.2"));

        fs::write(data_dir.join(VERSION_MARKER_FILE), "ai-memory 2.6.1").unwrap();
        fs::write(data_dir.join("db/memory.sqlite"), b"v76").unwrap();
        backup_before_upgrade(&data_dir, Some("ai-memory 2.6.1")).unwrap();
        assert_eq!(fs::read(first.join("db/memory.sqlite")).unwrap(), b"v63");

        backup_before_upgrade(&data_dir, Some("ai-memory 2.7.0")).unwrap();
        assert!(!first.exists());
        assert!(data_dir.join("backups/before-ai-memory-2.7.0").is_dir());
        let _ = fs::remove_dir_all(&data_dir);
    }

    #[test]
    fn settings_reject_unknown_modes_and_empty_scopes() {
        assert!(validate_settings(&settings("other", None)).is_err());
        let mut input = settings("disabled", None);
        input.workspace.clear();
        assert!(validate_settings(&input).is_err());
    }
}

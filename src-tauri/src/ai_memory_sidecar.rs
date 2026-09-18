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
        if is_healthy_once(&base_url) {
            eprintln!(
                "[DCC][ai-memory] server already reachable at {base_url}; reusing it without spawning a child"
            );
            std::env::set_var("DCC_AI_MEMORY_URL", &base_url);
            return Ok(Self::disabled());
        }
        let binary = locate_binary(app)?;
        let data_dir = std::env::var_os("DCC_AI_MEMORY_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| app_data_dir.join("ai-memory"));
        fs::create_dir_all(&data_dir).map_err(|error| {
            format!(
                "could not create ai-memory data directory {}: {error}",
                data_dir.display()
            )
        })?;

        let init_status = Command::new(&binary)
            .args(["--data-dir"])
            .arg(&data_dir)
            .arg("init")
            .status()
            .map_err(|error| format!("could not initialise ai-memory: {error}"))?;
        if !init_status.success() {
            return Err(format!("ai-memory init exited with status {init_status}"));
        }
        let version = Command::new(&binary)
            .arg("--version")
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
            .filter(|value| !value.is_empty());

        let log_dir = data_dir.join("logs");
        fs::create_dir_all(&log_dir).map_err(|error| error.to_string())?;
        let log_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_dir.join("dcc-sidecar.log"))
            .map_err(|error| error.to_string())?;
        let error_file = log_file.try_clone().map_err(|error| error.to_string())?;
        let child = Command::new(&binary)
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
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "ai-memory did not become ready at {base_url} within {} seconds",
                STARTUP_TIMEOUT.as_secs()
            ));
        }

        // The DCC adapter reads this once per operation. Set it only after the
        // process is healthy so a failed start cannot create a misleading URL.
        std::env::set_var("DCC_AI_MEMORY_URL", &base_url);
        std::env::set_var("DCC_AI_MEMORY_DATA_DIR", &data_dir);
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
        let _ = child.kill();
        let _ = child.wait();
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
    fn settings_reject_unknown_modes_and_empty_scopes() {
        assert!(validate_settings(&settings("other", None)).is_err());
        let mut input = settings("disabled", None);
        input.workspace.clear();
        assert!(validate_settings(&input).is_err());
    }
}

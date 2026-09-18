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

use tauri::{AppHandle, Manager};

const DEFAULT_PORT: u16 = 49_374;
const STARTUP_TIMEOUT: Duration = Duration::from_secs(8);
const HEALTH_TIMEOUT: Duration = Duration::from_millis(350);

pub struct AiMemorySidecar {
    child: Mutex<Option<Child>>,
    pub base_url: Option<String>,
    pub data_dir: Option<PathBuf>,
}

impl AiMemorySidecar {
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
        })
    }

    pub fn disabled() -> Self {
        Self {
            child: Mutex::new(None),
            base_url: None,
            data_dir: None,
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

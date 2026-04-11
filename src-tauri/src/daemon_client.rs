use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DaemonRuntimeInfo {
    pub pid: u32,
    pub started_at: String,
    pub db_path: String,
}

fn binary_stem() -> &'static str {
    "dccd"
}

fn binary_extension() -> &'static str {
    if cfg!(windows) { ".exe" } else { "" }
}

fn target_triple() -> &'static str {
    if cfg!(target_os = "windows") {
        if cfg!(target_arch = "x86_64") {
            "x86_64-pc-windows-msvc"
        } else if cfg!(target_arch = "aarch64") {
            "aarch64-pc-windows-msvc"
        } else {
            "windows"
        }
    } else if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            "aarch64-apple-darwin"
        } else {
            "x86_64-apple-darwin"
        }
    } else if cfg!(target_os = "linux") {
        if cfg!(target_arch = "aarch64") {
            if cfg!(target_env = "musl") {
                "aarch64-unknown-linux-musl"
            } else {
                "aarch64-unknown-linux-gnu"
            }
        } else if cfg!(target_arch = "x86_64") {
            if cfg!(target_env = "musl") {
                "x86_64-unknown-linux-musl"
            } else {
                "x86_64-unknown-linux-gnu"
            }
        } else {
            "linux"
        }
    } else {
        std::env::consts::ARCH
    }
}

pub fn sidecar_binary_candidates() -> Vec<String> {
    vec![
        format!("{}{}", binary_stem(), binary_extension()),
        format!("{}-{}{}", binary_stem(), target_triple(), binary_extension()),
    ]
}

fn open_connection(db_path: &Path) -> Result<Connection, String> {
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let conn = Connection::open(db_path).map_err(|e| e.to_string())?;
    conn.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous = NORMAL;
        PRAGMA foreign_keys = ON;
        PRAGMA busy_timeout = 5000;
        ",
    )
    .map_err(|e| e.to_string())?;
    Ok(conn)
}

fn ensure_schema(conn: &Connection) -> Result<(), String> {
    let schema = include_str!("../../lib/database/schema.sql");
    conn.execute_batch(schema).map_err(|e| e.to_string())
}

pub fn runtime_file(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("daemon-runtime.json")
}

pub fn default_app_data_dir() -> PathBuf {
    if let Some(override_dir) = std::env::var_os("DCC_APP_DATA_DIR") {
        return PathBuf::from(override_dir);
    }
    if let Some(override_dir) = std::env::var_os("DCC_DAEMON_APP_DATA_DIR") {
        return PathBuf::from(override_dir);
    }

    let bundle_id = "com.devcommandcenter.app";
    if cfg!(target_os = "windows") {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            return PathBuf::from(appdata).join(bundle_id);
        }
    }

    if cfg!(target_os = "macos") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join(bundle_id);
        }
    }

    if let Some(xdg_data_home) = std::env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(xdg_data_home).join(bundle_id);
    }

    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home)
            .join(".local")
            .join("share")
            .join(bundle_id);
    }

    PathBuf::from(bundle_id)
}

pub fn read_runtime_info(app_data_dir: &Path) -> Option<DaemonRuntimeInfo> {
    let path = runtime_file(app_data_dir);
    let raw = fs::read_to_string(path).ok()?;
    serde_json::from_str::<DaemonRuntimeInfo>(&raw).ok()
}

pub fn write_runtime_info(app_data_dir: &Path, info: &DaemonRuntimeInfo) -> Result<(), String> {
    fs::create_dir_all(app_data_dir).map_err(|e| e.to_string())?;
    let path = runtime_file(app_data_dir);
    let raw = serde_json::to_string_pretty(info).map_err(|e| e.to_string())?;
    fs::write(path, raw).map_err(|e| e.to_string())
}

fn locate_sidecar_binary(app: &AppHandle) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    let binary_names = sidecar_binary_candidates();

    if let Ok(resource_dir) = app.path().resource_dir() {
        for name in &binary_names {
            candidates.push(resource_dir.join(name));
        }
    }

    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(parent) = current_exe.parent() {
            for name in &binary_names {
                candidates.push(parent.join(name));
            }
            if let Some(grand_parent) = parent.parent() {
                for name in &binary_names {
                    candidates.push(grand_parent.join(name));
                }
            }
        }
    }

    candidates.into_iter().find(|candidate| candidate.exists())
}

fn wait_for_runtime_info(app_data_dir: &Path, timeout: Duration) -> Option<DaemonRuntimeInfo> {
    let started = Instant::now();
    while started.elapsed() < timeout {
        if let Some(info) = read_runtime_info(app_data_dir) {
            return Some(info);
        }
        thread::sleep(Duration::from_millis(150));
    }
    None
}

fn wait_for_rpc_response(db_path: &Path, request_id: &str, timeout: Duration) -> Result<Value, String> {
    let started = Instant::now();
    loop {
        let conn = open_connection(db_path)?;
        ensure_schema(&conn)?;
        let row = conn
            .query_row(
                "SELECT status, response_json, error FROM daemon_rpc_requests WHERE id = ?1 LIMIT 1",
                params![request_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                },
            )
            .ok();

        if let Some((status, response_json, error)) = row {
            match status.as_str() {
                "done" => {
                    let raw = response_json.ok_or_else(|| "daemon response missing payload".to_string())?;
                    let response = serde_json::from_str::<Value>(&raw).map_err(|e| e.to_string())?;
                    if response.get("ok").and_then(|value| value.as_bool()) == Some(true) {
                        return Ok(response.get("result").cloned().unwrap_or(Value::Null));
                    }
                    return Err(
                        response
                            .get("error")
                            .and_then(|value| value.as_str())
                            .unwrap_or("daemon request failed")
                            .to_string(),
                    );
                }
                "error" => {
                    return Err(error.unwrap_or_else(|| "daemon request failed".to_string()));
                }
                _ => {}
            }
        }

        if started.elapsed() >= timeout {
            return Err("daemon request timed out".to_string());
        }
        thread::sleep(Duration::from_millis(100));
    }
}

pub fn rpc_with_info(info: &DaemonRuntimeInfo, method: &str, params: Value) -> Result<Value, String> {
    rpc_with_db_path_timeout(Path::new(&info.db_path), method, params, Duration::from_secs(30))
}

pub fn rpc_with_db_path(db_path: &Path, method: &str, params: Value) -> Result<Value, String> {
    rpc_with_db_path_timeout(db_path, method, params, Duration::from_secs(30))
}

pub fn rpc_with_db_path_timeout(
    db_path: &Path,
    method: &str,
    params: Value,
    timeout: Duration,
) -> Result<Value, String> {
    let conn = open_connection(db_path)?;
    ensure_schema(&conn)?;
    let request_id = Uuid::new_v4().to_string();
    let request_json = serde_json::to_string(&serde_json::json!({
        "method": method,
        "params": params,
    }))
    .map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO daemon_rpc_requests (id, method, params_json, status) VALUES (?1, ?2, ?3, 'pending')",
        params![request_id, method, request_json],
    )
    .map_err(|e| e.to_string())?;

    wait_for_rpc_response(db_path, &request_id, timeout)
}

pub fn ensure_sidecar_running(
    app: &AppHandle,
    db_path: &Path,
    app_data_dir: &Path,
) -> Result<DaemonRuntimeInfo, String> {
    if let Some(info) = read_runtime_info(app_data_dir) {
        if rpc_with_db_path_timeout(
            Path::new(&info.db_path),
            "daemon.getStatus",
            serde_json::json!({}),
            Duration::from_secs(2),
        )
        .is_ok()
        {
            return Ok(info);
        }
    }

    let mut command = if let Some(binary) = locate_sidecar_binary(app) {
        Command::new(binary)
    } else {
        let mut cargo = Command::new("cargo");
        cargo
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .env("CARGO_TARGET_DIR", std::env::temp_dir().join("dcc-sidecar-target"))
            .args(["run", "--quiet", "--bin", "dccd", "--"]);
        cargo
    };

    let child = command
        .env("DCC_DAEMON_DB_PATH", db_path)
        .env("DCC_DAEMON_APP_DATA_DIR", app_data_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;

    let _ = child.id();

    wait_for_runtime_info(app_data_dir, Duration::from_secs(30))
        .ok_or_else(|| "sidecar started but did not publish runtime info".to_string())
}

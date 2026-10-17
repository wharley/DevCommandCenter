use crate::daemon_runtime::{process_alive, terminate_process, DAEMON_VERSION, LAUNCH_ID_ENV};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use sysinfo::System;
use tauri::{AppHandle, Manager};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DaemonRuntimeInfo {
    pub pid: u32,
    pub started_at: String,
    pub db_path: String,
    /// dccd build version; absent in files written before it was recorded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Echo of `DCC_DAEMON_LAUNCH_ID` from the spawn that started this daemon.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launch_id: Option<String>,
}

fn binary_stem() -> &'static str {
    "dccd"
}

fn binary_extension() -> &'static str {
    if cfg!(windows) {
        ".exe"
    } else {
        ""
    }
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

pub fn sidecar_binary_candidates_for(binary_stem: &str) -> Vec<String> {
    vec![
        format!("{}{}", binary_stem, binary_extension()),
        format!("{}-{}{}", binary_stem, target_triple(), binary_extension()),
    ]
}

pub fn sidecar_binary_candidates() -> Vec<String> {
    sidecar_binary_candidates_for(binary_stem())
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
    dcc_infra::db::limit_wal_growth(&conn).map_err(|e| e.to_string())?;
    Ok(conn)
}

fn ensure_schema(conn: &Connection) -> Result<(), String> {
    let schema = include_str!("../sql/schema.sql");
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
    if cfg!(dev) {
        return None;
    }

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

/// Waits for the runtime file written by the daemon spawned with `launch_id`.
/// A file left by an earlier daemon (dead or replaced) never matches.
pub fn wait_for_runtime_info(
    app_data_dir: &Path,
    launch_id: &str,
    timeout: Duration,
) -> Option<DaemonRuntimeInfo> {
    let started = Instant::now();
    while started.elapsed() < timeout {
        if let Some(info) = read_runtime_info(app_data_dir) {
            if info.launch_id.as_deref() == Some(launch_id) {
                return Some(info);
            }
        }
        thread::sleep(Duration::from_millis(150));
    }
    None
}

fn wait_for_rpc_response(
    db_path: &Path,
    request_id: &str,
    timeout: Duration,
) -> Result<Value, String> {
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
                    let raw = response_json
                        .ok_or_else(|| "daemon response missing payload".to_string())?;
                    let response =
                        serde_json::from_str::<Value>(&raw).map_err(|e| e.to_string())?;
                    if response.get("ok").and_then(|value| value.as_bool()) == Some(true) {
                        return Ok(response.get("result").cloned().unwrap_or(Value::Null));
                    }
                    return Err(response
                        .get("error")
                        .and_then(|value| value.as_str())
                        .unwrap_or("daemon request failed")
                        .to_string());
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

pub fn rpc_with_info(
    info: &DaemonRuntimeInfo,
    method: &str,
    params: Value,
) -> Result<Value, String> {
    rpc_with_db_path_timeout(
        Path::new(&info.db_path),
        method,
        params,
        Duration::from_secs(30),
    )
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

/// Whether a `daemon.health` answer comes from a dccd of `expected_version`.
/// Daemons built before the version was reported never match.
pub fn health_matches_version(health: &Value, expected_version: &str) -> bool {
    health.get("version").and_then(|value| value.as_str()) == Some(expected_version)
}

/// Stops a daemon that runs another build: asks it to shut down (it then
/// kills its children and marks supervised processes for the next daemon to
/// resume), then falls back to SIGTERM and SIGKILL for daemons that predate
/// `daemon.shutdown`. Their orphaned children are cleaned up and resumed by
/// the next daemon's startup recovery.
fn stop_daemon(db_path: &Path, pid: u32) -> Result<(), String> {
    let mut system = System::new();
    let accepted = rpc_with_db_path_timeout(
        db_path,
        "daemon.shutdown",
        serde_json::json!({ "resumeProcesses": true, "pid": pid }),
        Duration::from_secs(3),
    )
    .is_ok();
    if accepted {
        let started = Instant::now();
        while started.elapsed() < Duration::from_secs(10) {
            if !process_alive(&mut system, pid) {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(100));
        }
    }
    if terminate_process(&mut system, pid, Duration::from_secs(3)) {
        Ok(())
    } else {
        Err(format!("daemon {pid} from another build did not exit"))
    }
}

/// Reuses the daemon that answers on `daemon.health` when it runs
/// `expected_version` (any healthy daemon when `None`); otherwise stops it and
/// starts `command`, waiting for the runtime file of that very launch.
pub fn ensure_daemon_version(
    expected_version: Option<&str>,
    db_path: &Path,
    app_data_dir: &Path,
    mut command: Command,
) -> Result<DaemonRuntimeInfo, String> {
    // More than one stale daemon may poll the same queue; each answer comes
    // from one of them, so keep stopping until none (or the right one) answers.
    for _ in 0..3 {
        let Some(info) = read_runtime_info(app_data_dir) else {
            break;
        };
        let Ok(health) = rpc_with_db_path_timeout(
            Path::new(&info.db_path),
            "daemon.health",
            serde_json::json!({}),
            Duration::from_secs(2),
        ) else {
            break;
        };
        let Some(expected_version) = expected_version else {
            return Ok(info);
        };
        if health_matches_version(&health, expected_version) {
            return Ok(info);
        }
        let pid = health
            .get("pid")
            .and_then(|value| value.as_u64())
            .map(|pid| pid as u32)
            .unwrap_or(info.pid);
        eprintln!(
            "[DCC][daemon] replacing dccd {pid} (version {}) with {expected_version}",
            health
                .get("version")
                .and_then(|value| value.as_str())
                .unwrap_or("unknown")
        );
        stop_daemon(Path::new(&info.db_path), pid)?;
    }

    let launch_id = Uuid::new_v4().to_string();
    command
        .env("DCC_DAEMON_DB_PATH", db_path)
        .env("DCC_DAEMON_APP_DATA_DIR", app_data_dir)
        .env(LAUNCH_ID_ENV, &launch_id)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;

    wait_for_runtime_info(app_data_dir, &launch_id, Duration::from_secs(30))
        .ok_or_else(|| "sidecar started but did not publish runtime info".to_string())
}

/// Command that starts dccd from `binary`, or through `cargo run` in a dev
/// checkout where no bundled sidecar exists.
pub fn sidecar_command(binary: Option<PathBuf>) -> Command {
    if let Some(binary) = binary {
        return Command::new(binary);
    }
    let mut cargo = Command::new("cargo");
    cargo
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env(
            "CARGO_TARGET_DIR",
            std::env::temp_dir().join("dcc-sidecar-target"),
        )
        .args(["run", "--quiet", "--bin", "dccd", "--"]);
    cargo
}

pub fn ensure_sidecar_running(
    app: &AppHandle,
    db_path: &Path,
    app_data_dir: &Path,
) -> Result<DaemonRuntimeInfo, String> {
    // dccd is built from this crate, so compare crate versions. The bundle
    // version can differ from it: `yarn app:local` stamps 0.1.99-local.g<sha>,
    // which would never match and restart the daemon on every launch.
    ensure_daemon_version(
        Some(DAEMON_VERSION),
        db_path,
        app_data_dir,
        sidecar_command(locate_sidecar_binary(app)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    fn runtime(pid: u32, db_path: &Path, launch_id: Option<&str>) -> DaemonRuntimeInfo {
        DaemonRuntimeInfo {
            pid,
            started_at: "2026-10-16T07:59:00-03:00".into(),
            db_path: db_path.to_string_lossy().into_owned(),
            version: None,
            launch_id: launch_id.map(str::to_string),
        }
    }

    /// Answers `daemon.health` on the queue like a dccd of `version` running
    /// as `pid`, until `pid` exits; every other method is unknown to it, like
    /// a daemon built before `daemon.shutdown` existed.
    fn fake_daemon(db_path: PathBuf, version: &'static str, pid: u32) -> Arc<AtomicBool> {
        let conn = open_connection(&db_path).unwrap();
        ensure_schema(&conn).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let stop_flag = stop.clone();
        thread::spawn(move || {
            let mut system = System::new();
            while !stop_flag.load(Ordering::Relaxed) && process_alive(&mut system, pid) {
                let pending: Vec<(String, String)> = conn
                    .prepare("SELECT id, method FROM daemon_rpc_requests WHERE status = 'pending'")
                    .unwrap()
                    .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
                    .unwrap()
                    .filter_map(Result::ok)
                    .collect();
                for (id, method) in pending {
                    let response = if method == "daemon.health" {
                        serde_json::json!({ "ok": true, "result": { "version": version, "pid": pid } })
                    } else {
                        serde_json::json!({ "ok": false, "error": format!("unknown method: {method}") })
                    };
                    let status = if response["ok"] == true {
                        "done"
                    } else {
                        "error"
                    };
                    conn.execute(
                        "UPDATE daemon_rpc_requests SET status = ?2, response_json = ?3 WHERE id = ?1",
                        params![id, status, response.to_string()],
                    )
                    .unwrap();
                }
                thread::sleep(Duration::from_millis(50));
            }
        });
        stop
    }

    #[test]
    fn runtime_file_from_an_older_daemon_still_parses() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            runtime_file(dir.path()),
            r#"{"pid": 42, "startedAt": "x", "dbPath": "/tmp/db.sqlite"}"#,
        )
        .unwrap();
        let info = read_runtime_info(dir.path()).unwrap();
        assert_eq!(info.pid, 42);
        assert_eq!(info.version, None);
        assert_eq!(info.launch_id, None);
    }

    #[test]
    fn health_must_report_the_expected_version() {
        assert!(health_matches_version(
            &serde_json::json!({ "version": "0.1.99" }),
            "0.1.99"
        ));
        assert!(!health_matches_version(
            &serde_json::json!({ "version": "0.1.98" }),
            "0.1.99"
        ));
        assert!(!health_matches_version(
            &serde_json::json!({ "ok": true }),
            "0.1.99"
        ));
    }

    #[test]
    fn waiting_ignores_a_runtime_file_left_by_an_earlier_daemon() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("database.sqlite");
        write_runtime_info(dir.path(), &runtime(1, &db_path, Some("old"))).unwrap();

        assert!(wait_for_runtime_info(dir.path(), "new", Duration::from_millis(400)).is_none());

        let app_data_dir = dir.path().to_path_buf();
        let writer_db = db_path.clone();
        let writer = thread::spawn(move || {
            thread::sleep(Duration::from_millis(300));
            write_runtime_info(&app_data_dir, &runtime(2, &writer_db, Some("new"))).unwrap();
        });
        let info = wait_for_runtime_info(dir.path(), "new", Duration::from_secs(5)).unwrap();
        writer.join().unwrap();
        assert_eq!(info.pid, 2);
    }

    #[test]
    fn a_daemon_of_the_expected_version_is_reused() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("database.sqlite");
        let me = std::process::id();
        write_runtime_info(dir.path(), &runtime(me, &db_path, None)).unwrap();
        let stop = fake_daemon(db_path.clone(), "1.2.3", me);

        let info =
            ensure_daemon_version(Some("1.2.3"), &db_path, dir.path(), Command::new("false"))
                .unwrap();
        stop.store(true, Ordering::Relaxed);
        assert_eq!(info.pid, me);
    }

    #[cfg(unix)]
    #[test]
    fn a_daemon_from_another_build_is_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("database.sqlite");
        // Reparented right away, like a dccd started by the previous app.
        let output = Command::new("sh")
            .args(["-c", "sleep 60 >/dev/null 2>&1 & echo $!"])
            .output()
            .unwrap();
        let old_pid: u32 = String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse()
            .unwrap();
        write_runtime_info(dir.path(), &runtime(old_pid, &db_path, None)).unwrap();
        let _fake = fake_daemon(db_path.clone(), "0.1.97", old_pid);

        let mut new_daemon = Command::new("sh");
        new_daemon.args([
            "-c",
            r#"printf '{"pid":%s,"startedAt":"now","dbPath":"%s","version":"0.1.98","launchId":"%s"}' $$ "$DCC_DAEMON_DB_PATH" "$DCC_DAEMON_LAUNCH_ID" > "$DCC_DAEMON_APP_DATA_DIR/daemon-runtime.json""#,
        ]);
        let info = ensure_daemon_version(Some("0.1.98"), &db_path, dir.path(), new_daemon).unwrap();

        assert!(
            !process_alive(&mut System::new(), old_pid),
            "old daemon still running"
        );
        assert_ne!(info.pid, old_pid);
        assert_eq!(info.version.as_deref(), Some("0.1.98"));
        assert!(info.launch_id.is_some());
    }
}

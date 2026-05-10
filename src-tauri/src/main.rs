#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod forge_commands;
mod git_support;
mod session_commands;
mod workspace_commands;

use chrono::{DateTime, Datelike, Duration as ChronoDuration, Local, Timelike};
use dcc_tauri::state::{SessionCommandState, WorkspaceCommandState};
use dev_command_center_tauri::daemon_client::{
    ensure_sidecar_running, rpc_with_info, DaemonRuntimeInfo,
};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use notify_rust::Notification as DesktopNotification;
#[cfg(all(unix, not(target_os = "macos")))]
use notify_rust::NotificationHandle as DesktopNotificationHandle;
use portable_pty::{native_pty_system, Child as PtyChild, CommandBuilder, MasterPty, PtySize};
use regex::Regex;
use rusqlite::{params, params_from_iter, types::ValueRef, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::{Read, Write};
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use sysinfo::{Pid, System};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::git_support::build_review_diffs_for_path;
use tauri_plugin_dialog::{
    DialogExt, FilePath, MessageDialogButtons, MessageDialogKind, MessageDialogResult,
};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_updater::UpdaterExt;
use uuid::Uuid;
use forge_commands::{
    workspace_forge_cli_accounts, workspace_forge_cli_select_login, workspace_forge_cli_status,
    workspace_github_cli_status,
};
use workspace_commands::{
    archive_workspace, create_workspace_for_repo, create_workspace_from_url, delete_repository,
    delete_workspace, list_child_directories, list_git_tracked_files, list_local_branches,
    list_repositories, list_workspaces, restore_workspace, workspace_continue_from_base_branch,
    workspace_change_request_create, workspace_change_request_merge,
    workspace_change_request_view_web, workspace_gh_pr_create_fill, workspace_gh_pr_merge,
    workspace_gh_pr_view_web, workspace_git_branch_diff,
    workspace_git_commit_push, workspace_git_discard_file, workspace_git_file_preview,
    workspace_git_file_preview_content, workspace_git_push, workspace_git_stage_all,
    workspace_git_stage_file, workspace_git_status, workspace_git_unstage_file,
    workspace_pr_status, workspace_run_setup,
};

/// Schema SQLite embutido (`src-tauri/sql/schema.sql`, CREATE IF NOT EXISTS).
const APP_SCHEMA_SQL: &str = include_str!("../sql/schema.sql");
const REPO_CONFIG_FILENAME: &str = ".dcc.toml";

// Regex para detectar quando um terminal está esperando input do usuário
lazy_static::lazy_static! {
    static ref WAIT_PATTERN: Regex = Regex::new(
        r"(?i)(trust|workspace\s+trust|permission|approve|confirm|\(y/n\)|\[y/n\]|\by/n\b|press\s+enter|waiting\s+for|allow\s+edit|must\s+be\s+trusted|denied|blocked|requires?\s+your|intervention|open\s+.*\s+to\s+continue|do\s+you\s+want)"
    ).unwrap();
}

#[derive(Clone)]
struct AppState {
    db_path: Arc<PathBuf>,
    app_data_dir: Arc<PathBuf>,
    conn: Arc<Mutex<Connection>>,
    terminals: Arc<Mutex<HashMap<String, ManagedTerminal>>>,
    daemon: Arc<DaemonState>,
    daemon_endpoint: Arc<Mutex<Option<DaemonRuntimeInfo>>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RepoConfigPayload {
    branch_prefix: Option<String>,
    default_agent_provider_id: Option<String>,
    setup_command: Option<String>,
    teardown_command: Option<String>,
    #[serde(default)]
    processes: Vec<RepoProcessPayload>,
    #[serde(default)]
    presets: Vec<RepoPresetPayload>,
    #[serde(default)]
    tasks: Vec<RepoTaskPayload>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RepoProcessPayload {
    id: String,
    name: String,
    command: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    cwd_mode: Option<String>,
    #[serde(default)]
    auto_restart: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RepoPresetPayload {
    id: String,
    name: String,
    command: String,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RepoTaskTriggerPayload {
    #[serde(default)]
    when: Option<String>,
    #[serde(default)]
    prompt: Option<String>,
    #[serde(default)]
    provider_id: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RepoTaskPayload {
    id: String,
    name: String,
    command: String,
    schedule: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    cwd_mode: Option<String>,
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    trigger: Option<RepoTaskTriggerPayload>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct RepoConfigToml {
    #[serde(default)]
    version: Option<u32>,
    #[serde(default)]
    branch: Option<RepoBranchToml>,
    #[serde(default)]
    agent: Option<RepoAgentToml>,
    #[serde(default)]
    scripts: Option<RepoScriptsToml>,
    #[serde(default)]
    branch_prefix: Option<String>,
    #[serde(default)]
    default_agent_provider_id: Option<String>,
    #[serde(default)]
    setup_command: Option<String>,
    #[serde(default)]
    teardown_command: Option<String>,
    #[serde(default)]
    processes: Vec<RepoProcessToml>,
    #[serde(default)]
    presets: Vec<RepoPresetToml>,
    #[serde(default)]
    tasks: Vec<RepoTaskToml>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct RepoBranchToml {
    #[serde(default)]
    prefix: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct RepoAgentToml {
    #[serde(default)]
    default_provider_id: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct RepoScriptsToml {
    #[serde(default)]
    setup: Option<String>,
    #[serde(default)]
    teardown: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct RepoProcessToml {
    id: String,
    name: String,
    command: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    cwd_mode: Option<String>,
    #[serde(default)]
    auto_restart: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct RepoPresetToml {
    id: String,
    name: String,
    command: String,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct RepoTaskTriggerToml {
    #[serde(default)]
    when: Option<String>,
    #[serde(default)]
    prompt: Option<String>,
    #[serde(default)]
    provider_id: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct RepoTaskToml {
    id: String,
    name: String,
    command: String,
    schedule: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    cwd_mode: Option<String>,
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    trigger: Option<RepoTaskTriggerToml>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DesktopNotificationActionToml {
    id: String,
    label: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum DesktopNotificationSoundToml {
    Enabled(bool),
    Named(String),
}

#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DesktopNotificationRequest {
    title: Option<String>,
    body: Option<String>,
    icon: Option<String>,
    sound: Option<DesktopNotificationSoundToml>,
    notification_id: Option<String>,
    source: Option<String>,
    pane_id: Option<String>,
    comb_id: Option<String>,
    project_id: Option<String>,
    #[serde(default)]
    actions: Vec<DesktopNotificationActionToml>,
}

#[cfg(all(unix, not(target_os = "macos")))]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DesktopNotificationActionEvent {
    notification_id: String,
    action_id: String,
    title: String,
    source: Option<String>,
    pane_id: Option<String>,
    comb_id: Option<String>,
    project_id: Option<String>,
    body: Option<String>,
}

/// Últimas N linhas de stdout/stderr por sessão (reidratação do xterm ao remontar).
const TERMINAL_OUTPUT_MAX_LINES: usize = 1000;

#[cfg(windows)]
mod windows_job {
    use std::ffi::c_void;
    use std::mem::size_of;
    use std::ptr::null_mut;

    type Handle = *mut c_void;

    const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: u32 = 0x0000_2000;
    const JOB_OBJECT_EXTENDED_LIMIT_INFORMATION_CLASS: u32 = 9;

    const PROCESS_TERMINATE: u32 = 0x0001;
    const PROCESS_SET_QUOTA: u32 = 0x0100;
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;

    #[repr(C)]
    struct IoCounters {
        read_operation_count: u64,
        write_operation_count: u64,
        other_operation_count: u64,
        read_transfer_count: u64,
        write_transfer_count: u64,
        other_transfer_count: u64,
    }

    #[repr(C)]
    struct JobObjectBasicLimitInformation {
        per_process_user_time_limit: i64,
        per_job_user_time_limit: i64,
        limit_flags: u32,
        minimum_working_set_size: usize,
        maximum_working_set_size: usize,
        active_process_limit: u32,
        affinity: usize,
        priority_class: u32,
        scheduling_class: u32,
    }

    #[repr(C)]
    struct JobObjectExtendedLimitInformation {
        basic_limit_information: JobObjectBasicLimitInformation,
        io_counters: IoCounters,
        process_memory_limit: usize,
        job_memory_limit: usize,
        peak_process_memory_used: usize,
        peak_job_memory_used: usize,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateJobObjectW(lp_job_attributes: *mut c_void, lp_name: *const u16) -> Handle;
        fn SetInformationJobObject(
            h_job: Handle,
            job_object_info_class: u32,
            lp_job_object_info: *mut c_void,
            cb_job_object_info_length: u32,
        ) -> i32;
        fn AssignProcessToJobObject(h_job: Handle, h_process: Handle) -> i32;
        fn TerminateJobObject(h_job: Handle, u_exit_code: u32) -> i32;
        fn OpenProcess(dw_desired_access: u32, b_inherit_handle: i32, dw_process_id: u32)
            -> Handle;
        fn CloseHandle(h_object: Handle) -> i32;
    }

    pub struct WindowsJobObject {
        handle: Handle,
    }

    impl WindowsJobObject {
        pub fn attach_process(pid: u32) -> Result<Self, String> {
            unsafe {
                let handle = CreateJobObjectW(null_mut(), null_mut());
                if handle.is_null() {
                    return Err(format!(
                        "CreateJobObjectW falhou: {}",
                        std::io::Error::last_os_error()
                    ));
                }

                let mut info = JobObjectExtendedLimitInformation {
                    basic_limit_information: JobObjectBasicLimitInformation {
                        per_process_user_time_limit: 0,
                        per_job_user_time_limit: 0,
                        limit_flags: JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                        minimum_working_set_size: 0,
                        maximum_working_set_size: 0,
                        active_process_limit: 0,
                        affinity: 0,
                        priority_class: 0,
                        scheduling_class: 0,
                    },
                    io_counters: IoCounters {
                        read_operation_count: 0,
                        write_operation_count: 0,
                        other_operation_count: 0,
                        read_transfer_count: 0,
                        write_transfer_count: 0,
                        other_transfer_count: 0,
                    },
                    process_memory_limit: 0,
                    job_memory_limit: 0,
                    peak_process_memory_used: 0,
                    peak_job_memory_used: 0,
                };

                if SetInformationJobObject(
                    handle,
                    JOB_OBJECT_EXTENDED_LIMIT_INFORMATION_CLASS,
                    &mut info as *mut _ as *mut c_void,
                    size_of::<JobObjectExtendedLimitInformation>() as u32,
                ) == 0
                {
                    let err = std::io::Error::last_os_error();
                    let _ = CloseHandle(handle);
                    return Err(format!("SetInformationJobObject falhou: {err}"));
                }

                let process = OpenProcess(
                    PROCESS_TERMINATE | PROCESS_SET_QUOTA | PROCESS_QUERY_LIMITED_INFORMATION,
                    0,
                    pid,
                );
                if process.is_null() {
                    let err = std::io::Error::last_os_error();
                    let _ = CloseHandle(handle);
                    return Err(format!("OpenProcess falhou: {err}"));
                }

                if AssignProcessToJobObject(handle, process) == 0 {
                    let err = std::io::Error::last_os_error();
                    let _ = CloseHandle(process);
                    let _ = CloseHandle(handle);
                    return Err(format!("AssignProcessToJobObject falhou: {err}"));
                }

                let _ = CloseHandle(process);
                Ok(Self { handle })
            }
        }

        pub fn terminate(&self, exit_code: u32) -> Result<(), String> {
            unsafe {
                if TerminateJobObject(self.handle, exit_code) == 0 {
                    Err(format!(
                        "TerminateJobObject falhou: {}",
                        std::io::Error::last_os_error()
                    ))
                } else {
                    Ok(())
                }
            }
        }
    }

    impl Drop for WindowsJobObject {
        fn drop(&mut self) {
            unsafe {
                if !self.handle.is_null() {
                    let _ = CloseHandle(self.handle);
                    self.handle = null_mut();
                }
            }
        }
    }
}

struct ManagedTerminal {
    pty_master: Box<dyn MasterPty + Send>,
    child: Box<dyn PtyChild + Send>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    #[cfg(windows)]
    windows_job: Option<windows_job::WindowsJobObject>,
    mission_id: Option<String>,
    pane_id: Option<String>,
    pty_owner_key: Option<String>,
    title: Arc<Mutex<Option<String>>>,
    cwd: String,
    command: String,
    args: Vec<String>,
    started_at: String,
    stop_flag: Arc<AtomicBool>,
    exit_notified: Arc<AtomicBool>,
    /// Buffer circular compartilhado entre as threads de leitura (lock curto só aqui).
    output_buffer: Arc<Mutex<VecDeque<String>>>,
    reader_thread: Option<JoinHandle<()>>,
}

#[cfg(windows)]
fn attach_windows_job_object(
    child: &Box<dyn PtyChild + Send>,
) -> Option<windows_job::WindowsJobObject> {
    child
        .process_id()
        .and_then(|pid| windows_job::WindowsJobObject::attach_process(pid).ok())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DetectedTerminalAgent {
    pty_id: String,
    pane_id: Option<String>,
    comb_id: Option<String>,
    project_id: String,
    project_name: String,
    workspace_name: Option<String>,
    agent_kind: String,
    agent_label: String,
    status: String,
    cwd: String,
    command: String,
    args: Vec<String>,
    pid: Option<u32>,
    title: Option<String>,
    provider_id: Option<String>,
    provider_name: Option<String>,
    detected_by: String,
    excerpt: Option<String>,
    started_at: String,
}

#[derive(Clone)]
struct DaemonState {
    started_at: String,
    last_tick_at: Arc<Mutex<Option<String>>>,
    running: Arc<AtomicBool>,
    system: Arc<Mutex<System>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DaemonHealthSnapshot {
    pid: u32,
    cpu_percent: f64,
    memory_mb: f64,
    last_metrics_at: String,
}

fn new_terminal_output_buffer() -> Arc<Mutex<VecDeque<String>>> {
    Arc::new(Mutex::new(VecDeque::with_capacity(
        TERMINAL_OUTPUT_MAX_LINES.min(256),
    )))
}

#[inline]
fn append_terminal_line(buffer: &Arc<Mutex<VecDeque<String>>>, line: &str) {
    if let Ok(mut guard) = buffer.lock() {
        while guard.len() >= TERMINAL_OUTPUT_MAX_LINES {
            guard.pop_front();
        }
        guard.push_back(line.to_string());
    }
}

/// Intervalo mínimo entre escritas do scrollback comprimido no SQLite (evita pressão no WAL).
const PANE_SCROLLBACK_PERSIST_INTERVAL: Duration = Duration::from_millis(1600);

fn persist_pane_scrollback_compressed(
    conn: &Connection,
    pane_id: &str,
    lines: &[String],
) -> Result<(), String> {
    if lines.is_empty() {
        conn.execute(
            "DELETE FROM pane_terminal_scrollback WHERE pane_id = ?1",
            params![pane_id],
        )
        .map_err(|e| e.to_string())?;
        return Ok(());
    }
    let json = serde_json::to_vec(lines).map_err(|e| e.to_string())?;
    let mut enc = GzEncoder::new(Vec::new(), Compression::default());
    enc.write_all(&json).map_err(|e| e.to_string())?;
    let compressed = enc.finish().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO pane_terminal_scrollback (pane_id, payload_z, updated_at) VALUES (?1, ?2, datetime('now')) \
         ON CONFLICT(pane_id) DO UPDATE SET payload_z = excluded.payload_z, updated_at = excluded.updated_at",
        params![pane_id, compressed],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn load_pane_scrollback_deque(conn: &Connection, pane_id: &str) -> Option<VecDeque<String>> {
    let blob: Vec<u8> = conn
        .query_row(
            "SELECT payload_z FROM pane_terminal_scrollback WHERE pane_id = ?1",
            params![pane_id],
            |row| row.get(0),
        )
        .ok()?;
    let mut decoder = GzDecoder::new(&blob[..]);
    let mut json = Vec::new();
    decoder.read_to_end(&mut json).ok()?;
    let lines: Vec<String> = serde_json::from_slice(&json).ok()?;
    let mut dq = VecDeque::with_capacity(lines.len().min(TERMINAL_OUTPUT_MAX_LINES));
    for line in lines {
        while dq.len() >= TERMINAL_OUTPUT_MAX_LINES {
            dq.pop_front();
        }
        dq.push_back(line);
    }
    Some(dq)
}

fn persist_managed_terminal_buffer(state: &AppState, t: &ManagedTerminal) {
    let Some(ref pane_id) = t.pane_id else {
        return;
    };
    let lines: Vec<String> = t
        .output_buffer
        .lock()
        .ok()
        .map(|g| g.iter().cloned().collect())
        .unwrap_or_default();
    if let Ok(conn) = state.conn.lock() {
        if let Err(e) = persist_pane_scrollback_compressed(&conn, pane_id, &lines) {
            eprintln!("[DCC] pane scrollback persist failed: {e}");
        }
    }
}

#[derive(Debug, Serialize)]
struct ApiError {
    code: &'static str,
    message: String,
}

type ApiResult<T> = Result<T, ApiError>;

fn db_error(message: impl Into<String>) -> ApiError {
    ApiError {
        code: "DB_ERROR",
        message: message.into(),
    }
}

fn table_has_column(conn: &Connection, table: &str, column: &str) -> Result<bool, String> {
    let pragma = format!("PRAGMA table_info({table})");
    let mut stmt = conn.prepare(&pragma).map_err(|e| e.to_string())?;
    let mut rows = stmt.query([]).map_err(|e| e.to_string())?;
    while let Some(row) = rows.next().map_err(|e| e.to_string())? {
        let name: String = row.get(1).map_err(|e| e.to_string())?;
        if name == column {
            return Ok(true);
        }
    }
    Ok(false)
}

fn table_exists(conn: &Connection, table: &str) -> Result<bool, String> {
    let mut stmt = conn
        .prepare("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1 LIMIT 1")
        .map_err(|e| e.to_string())?;
    stmt.exists(params![table]).map_err(|e| e.to_string())
}

fn ensure_column(
    conn: &Connection,
    table: &str,
    column: &str,
    sql_type: &str,
) -> Result<(), String> {
    if !table_exists(conn, table)? {
        return Ok(());
    }
    if table_has_column(conn, table, column)? {
        return Ok(());
    }
    let sql = format!("ALTER TABLE {table} ADD COLUMN {column} {sql_type}");
    conn.execute(&sql, []).map_err(|e| e.to_string())?;
    Ok(())
}

fn run_legacy_schema_migrations(conn: &Connection) -> Result<(), String> {
    // Providers
    ensure_column(conn, "providers", "api_key_encrypted", "BLOB")?;
    ensure_column(conn, "providers", "cli_path", "TEXT")?;
    ensure_column(conn, "providers", "config", "TEXT")?;

    // Projects
    ensure_column(conn, "projects", "git_remote_url", "TEXT")?;
    ensure_column(conn, "projects", "repo_config", "TEXT")?;
    ensure_column(conn, "projects", "last_opened_at", "TEXT")?;

    // Combs
    ensure_column(conn, "combs", "review_targets", "TEXT")?;
    ensure_column(conn, "combs", "branch", "TEXT")?;
    ensure_column(conn, "combs", "worktree_path", "TEXT")?;
    ensure_column(conn, "combs", "status", "TEXT NOT NULL DEFAULT 'active'")?;
    ensure_column(conn, "combs", "last_opened_at", "TEXT")?;
    ensure_column(conn, "combs", "is_pinned", "INTEGER DEFAULT 0")?;
    ensure_column(conn, "combs", "pinned_at", "TEXT")?;
    ensure_column(conn, "combs", "last_git_activity_at", "TEXT")?;
    ensure_column(conn, "combs", "forge_link", "TEXT")?;

    // Panes
    ensure_column(conn, "panes", "pty_owner_key", "TEXT")?;
    ensure_column(conn, "panes", "last_activity_at", "TEXT")?;

    Ok(())
}

fn next_id() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("dcc-{now:x}")
}

fn repo_config_file_path(project_path: &str) -> PathBuf {
    Path::new(project_path).join(REPO_CONFIG_FILENAME)
}

fn repo_config_payload_from_value(value: &Value) -> Result<RepoConfigPayload, String> {
    serde_json::from_value(value.clone()).map_err(|e| e.to_string())
}

fn repo_config_value_from_payload(payload: &RepoConfigPayload) -> Result<Value, String> {
    serde_json::to_value(payload).map_err(|e| e.to_string())
}

fn repo_config_toml_text_from_payload(payload: &RepoConfigPayload) -> Result<String, String> {
    let toml_config = repo_config_toml_from_payload(payload);
    toml::to_string_pretty(&toml_config).map_err(|e| e.to_string())
}

fn repo_config_toml_text_from_value(value: &Value) -> Result<String, String> {
    let payload = repo_config_payload_from_value(value)?;
    repo_config_toml_text_from_payload(&payload)
}

fn default_repo_config_value() -> Value {
    serde_json::json!({
        "branchPrefix": "dcc-comb",
        "defaultAgentProviderId": Value::Null,
        "setupCommand": Value::Null,
        "teardownCommand": Value::Null,
        "processes": [],
        "presets": [],
        "tasks": []
    })
}

fn repo_config_toml_from_payload(payload: &RepoConfigPayload) -> RepoConfigToml {
    RepoConfigToml {
        version: Some(1),
        branch: Some(RepoBranchToml {
            prefix: payload.branch_prefix.clone(),
        }),
        agent: Some(RepoAgentToml {
            default_provider_id: payload.default_agent_provider_id.clone(),
        }),
        scripts: Some(RepoScriptsToml {
            setup: payload.setup_command.clone(),
            teardown: payload.teardown_command.clone(),
        }),
        branch_prefix: None,
        default_agent_provider_id: None,
        setup_command: None,
        teardown_command: None,
        processes: payload
            .processes
            .iter()
            .map(|process| RepoProcessToml {
                id: process.id.clone(),
                name: process.name.clone(),
                command: process.command.clone(),
                description: process.description.clone(),
                cwd_mode: process.cwd_mode.clone(),
                auto_restart: process.auto_restart,
            })
            .collect(),
        presets: payload
            .presets
            .iter()
            .map(|preset| RepoPresetToml {
                id: preset.id.clone(),
                name: preset.name.clone(),
                command: preset.command.clone(),
                description: preset.description.clone(),
            })
            .collect(),
        tasks: payload
            .tasks
            .iter()
            .map(|task| RepoTaskToml {
                id: task.id.clone(),
                name: task.name.clone(),
                command: task.command.clone(),
                schedule: task.schedule.clone(),
                description: task.description.clone(),
                cwd_mode: task.cwd_mode.clone(),
                enabled: task.enabled,
                trigger: task.trigger.as_ref().map(|trigger| RepoTaskTriggerToml {
                    when: trigger.when.clone(),
                    prompt: trigger.prompt.clone(),
                    provider_id: trigger.provider_id.clone(),
                }),
            })
            .collect(),
    }
}

fn repo_config_payload_from_toml(config: RepoConfigToml) -> RepoConfigPayload {
    let RepoConfigToml {
        branch,
        agent,
        scripts,
        branch_prefix: flat_branch_prefix,
        default_agent_provider_id: flat_default_agent_provider_id,
        setup_command: flat_setup_command,
        teardown_command: flat_teardown_command,
        processes,
        presets,
        tasks,
        ..
    } = config;
    let branch_prefix = branch
        .and_then(|branch| branch.prefix)
        .or(flat_branch_prefix);
    let default_agent_provider_id = agent
        .and_then(|agent| agent.default_provider_id)
        .or(flat_default_agent_provider_id);
    let setup_command = scripts
        .as_ref()
        .and_then(|scripts| scripts.setup.clone())
        .or(flat_setup_command);
    let teardown_command = scripts
        .as_ref()
        .and_then(|scripts| scripts.teardown.clone())
        .or(flat_teardown_command);

    RepoConfigPayload {
        branch_prefix,
        default_agent_provider_id,
        setup_command,
        teardown_command,
        processes: processes
            .into_iter()
            .map(|process| RepoProcessPayload {
                id: process.id,
                name: process.name,
                command: process.command,
                description: process.description,
                cwd_mode: process.cwd_mode,
                auto_restart: process.auto_restart,
            })
            .collect(),
        presets: presets
            .into_iter()
            .map(|preset| RepoPresetPayload {
                id: preset.id,
                name: preset.name,
                command: preset.command,
                description: preset.description,
            })
            .collect(),
        tasks: tasks
            .into_iter()
            .map(|task| RepoTaskPayload {
                id: task.id,
                name: task.name,
                command: task.command,
                schedule: task.schedule,
                description: task.description,
                cwd_mode: task.cwd_mode,
                enabled: task.enabled,
                trigger: task.trigger.map(|trigger| RepoTaskTriggerPayload {
                    when: trigger.when,
                    prompt: trigger.prompt,
                    provider_id: trigger.provider_id,
                }),
            })
            .collect(),
    }
}

fn read_repo_config_from_disk(project_path: &str) -> Result<Option<Value>, String> {
    let path = repo_config_file_path(project_path);
    if !path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(&path).map_err(|e| format!("{}: {}", path.display(), e))?;
    if raw.trim().is_empty() {
        return Ok(None);
    }
    let toml_config: RepoConfigToml =
        toml::from_str(&raw).map_err(|e| format!("{}: {}", path.display(), e))?;
    let payload = repo_config_payload_from_toml(toml_config);
    repo_config_value_from_payload(&payload).map(Some)
}

fn read_repo_config_text_from_disk(project_path: &str) -> Result<Option<String>, String> {
    let path = repo_config_file_path(project_path);
    if !path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(&path).map_err(|e| format!("{}: {}", path.display(), e))?;
    if raw.trim().is_empty() {
        return Ok(None);
    }
    Ok(Some(raw))
}

fn write_repo_config_to_disk(project_path: &str, config: Option<&Value>) -> Result<(), String> {
    let path = repo_config_file_path(project_path);
    match config {
        Some(Value::Null) | None => {
            if path.exists() {
                fs::remove_file(&path).map_err(|e| format!("{}: {}", path.display(), e))?;
            }
            Ok(())
        }
        Some(value) => {
            let payload = repo_config_payload_from_value(value)?;
            let toml_config = repo_config_toml_from_payload(&payload);
            let toml_text = toml::to_string_pretty(&toml_config).map_err(|e| e.to_string())?;
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|e| format!("{}: {}", parent.display(), e))?;
            }
            fs::write(&path, toml_text).map_err(|e| format!("{}: {}", path.display(), e))?;
            Ok(())
        }
    }
}

fn resolve_repo_config_value(project_path: Option<&str>, db_repo_config: Option<Value>) -> Value {
    if let Some(path) = project_path {
        if let Ok(Some(value)) = read_repo_config_from_disk(path) {
            return value;
        }
    }
    if let Some(raw) = db_repo_config {
        if raw.is_string() {
            if let Some(raw_str) = raw.as_str() {
                if raw_str.trim().is_empty() {
                    return Value::Null;
                }
                return serde_json::from_str::<Value>(raw_str).unwrap_or(Value::Null);
            }
        }
        return raw;
    }
    Value::Null
}

fn sync_existing_repo_configs(conn: &Connection) -> Result<(), String> {
    let mut stmt = conn
        .prepare("SELECT id, path, repo_config FROM projects")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })
        .map_err(|e| e.to_string())?;

    for row in rows {
        let (id, path, repo_config_raw) = row.map_err(|e| e.to_string())?;
        let file_path = repo_config_file_path(&path);
        if file_path.exists() {
            match read_repo_config_from_disk(&path) {
                Ok(Some(value)) => {
                    let normalized = value.to_string();
                    if repo_config_raw.as_deref() != Some(normalized.as_str()) {
                        conn.execute(
                            "UPDATE projects SET repo_config = ?1 WHERE id = ?2",
                            params![normalized, id],
                        )
                        .map_err(|e| e.to_string())?;
                    }
                }
                Ok(None) => {}
                Err(err) => {
                    eprintln!(
                        "[DCC] Skipping invalid .dcc.toml at {}: {}",
                        file_path.display(),
                        err
                    );
                }
            }
            continue;
        }

        let Some(raw) = repo_config_raw else {
            continue;
        };
        if raw.trim().is_empty() {
            continue;
        }
        let value = match serde_json::from_str::<Value>(&raw) {
            Ok(value) => value,
            Err(err) => {
                eprintln!("[DCC] Skipping legacy repo_config for {}: {}", path, err);
                continue;
            }
        };
        let toml_text = repo_config_toml_text_from_value(&value)?;
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("{}: {}", parent.display(), e))?;
        }
        fs::write(&file_path, toml_text).map_err(|e| format!("{}: {}", file_path.display(), e))?;
        conn.execute(
            "UPDATE projects SET repo_config = ?1 WHERE id = ?2",
            params![value.to_string(), id],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn value_ref_to_json(value: ValueRef<'_>) -> Value {
    match value {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(i) => Value::from(i),
        ValueRef::Real(f) => Value::from(f),
        ValueRef::Text(t) => Value::from(String::from_utf8_lossy(t).to_string()),
        ValueRef::Blob(_) => Value::Null,
    }
}

fn row_to_json(row: &rusqlite::Row<'_>) -> rusqlite::Result<Value> {
    let mut obj = serde_json::Map::new();
    let row_ref = row.as_ref();
    for i in 0..row_ref.column_count() {
        let name = row_ref
            .column_name(i)
            .map(ToString::to_string)
            .unwrap_or_else(|_| format!("col_{i}"));
        let raw = row.get_ref(i)?;
        obj.insert(name, value_ref_to_json(raw));
    }
    Ok(Value::Object(obj))
}

fn map_project_to_renderer(mut row: Value) -> Value {
    let Some(obj) = row.as_object_mut() else {
        return row;
    };
    let project_path = obj.get("path").and_then(Value::as_str).map(str::to_string);
    let repo_config_v =
        resolve_repo_config_value(project_path.as_deref(), obj.remove("repo_config"));
    let mut out = serde_json::Map::new();
    out.insert("id".into(), obj.remove("id").unwrap_or(Value::Null));
    out.insert("name".into(), obj.remove("name").unwrap_or(Value::Null));
    out.insert("path".into(), obj.remove("path").unwrap_or(Value::Null));
    out.insert(
        "description".into(),
        obj.remove("description").unwrap_or(Value::Null),
    );
    out.insert(
        "defaultProviderId".into(),
        obj.remove("default_provider_id").unwrap_or(Value::Null),
    );
    out.insert(
        "gitRemoteUrl".into(),
        obj.remove("git_remote_url").unwrap_or(Value::Null),
    );
    out.insert("repoConfig".into(), repo_config_v);
    out.insert(
        "lastOpenedAt".into(),
        obj.remove("last_opened_at").unwrap_or(Value::Null),
    );
    out.insert(
        "createdAt".into(),
        obj.remove("created_at").unwrap_or(Value::Null),
    );
    out.insert(
        "updatedAt".into(),
        obj.remove("updated_at").unwrap_or(Value::Null),
    );
    Value::Object(out)
}

fn map_comb_to_renderer(mut row: Value) -> Value {
    let Some(obj) = row.as_object_mut() else {
        return row;
    };
    let mut review_targets_v = Value::Null;
    if let Some(v) = obj.remove("review_targets") {
        review_targets_v = match v {
            Value::String(ref s) if !s.trim().is_empty() => {
                serde_json::from_str::<Value>(s).unwrap_or(Value::Null)
            }
            Value::Null => Value::Null,
            other => other,
        };
    }
    let mut out = serde_json::Map::new();
    out.insert("id".into(), obj.remove("id").unwrap_or(Value::Null));
    out.insert(
        "projectId".into(),
        obj.remove("project_id").unwrap_or(Value::Null),
    );
    out.insert("name".into(), obj.remove("name").unwrap_or(Value::Null));
    out.insert(
        "description".into(),
        obj.remove("description").unwrap_or(Value::Null),
    );
    out.insert(
        "baseBranch".into(),
        obj.remove("base_branch").unwrap_or(Value::Null),
    );
    out.insert("branch".into(), obj.remove("branch").unwrap_or(Value::Null));
    out.insert(
        "worktreePath".into(),
        obj.remove("worktree_path").unwrap_or(Value::Null),
    );
    out.insert("reviewTargets".into(), review_targets_v);
    out.insert("status".into(), obj.remove("status").unwrap_or(Value::Null));
    out.insert(
        "lastOpenedAt".into(),
        obj.remove("last_opened_at").unwrap_or(Value::Null),
    );
    out.insert(
        "lastGitActivityAt".into(),
        obj.remove("last_git_activity_at").unwrap_or(Value::Null),
    );
    let is_pinned_val = obj.remove("is_pinned");
    let is_pinned_bool = match is_pinned_val {
        Some(Value::Number(n)) => n.as_i64().map(|i| i != 0).unwrap_or(false),
        Some(Value::Bool(b)) => b,
        _ => false,
    };
    out.insert("isPinned".into(), Value::Bool(is_pinned_bool));
    out.insert(
        "pinnedAt".into(),
        obj.remove("pinned_at").unwrap_or(Value::Null),
    );
    out.insert(
        "createdAt".into(),
        obj.remove("created_at").unwrap_or(Value::Null),
    );
    out.insert(
        "updatedAt".into(),
        obj.remove("updated_at").unwrap_or(Value::Null),
    );
    let mut forge_link_v = Value::Null;
    if let Some(v) = obj.remove("forge_link") {
        forge_link_v = match v {
            Value::String(ref s) if !s.trim().is_empty() => {
                serde_json::from_str::<Value>(s).unwrap_or(Value::Null)
            }
            Value::Null => Value::Null,
            other => other,
        };
    }
    out.insert("forgeLink".into(), forge_link_v);
    Value::Object(out)
}

fn map_pane_to_renderer(mut row: Value) -> Value {
    let Some(obj) = row.as_object_mut() else {
        return row;
    };
    let mut out = serde_json::Map::new();
    out.insert("id".into(), obj.remove("id").unwrap_or(Value::Null));
    out.insert(
        "combId".into(),
        obj.remove("comb_id").unwrap_or(Value::Null),
    );
    out.insert("type".into(), obj.remove("type").unwrap_or(Value::Null));
    out.insert(
        "providerId".into(),
        obj.remove("provider_id").unwrap_or(Value::Null),
    );
    out.insert("title".into(), obj.remove("title").unwrap_or(Value::Null));
    out.insert(
        "initialPrompt".into(),
        obj.remove("initial_prompt").unwrap_or(Value::Null),
    );
    out.insert("cwd".into(), obj.remove("cwd").unwrap_or(Value::Null));
    out.insert(
        "ptyOwnerKey".into(),
        obj.remove("pty_owner_key").unwrap_or(Value::Null),
    );
    out.insert("status".into(), obj.remove("status").unwrap_or(Value::Null));
    out.insert(
        "layoutOrder".into(),
        obj.remove("layout_order").unwrap_or_else(|| Value::from(0)),
    );
    out.insert(
        "lastActivityAt".into(),
        obj.remove("last_activity_at").unwrap_or(Value::Null),
    );
    out.insert(
        "createdAt".into(),
        obj.remove("created_at").unwrap_or(Value::Null),
    );
    out.insert(
        "updatedAt".into(),
        obj.remove("updated_at").unwrap_or(Value::Null),
    );
    Value::Object(out)
}

const PROVIDER_SELECT_SQL: &str = "SELECT id, name, type, api_key, api_key_encrypted, cli_path, config, is_active, created_at, updated_at FROM providers";

fn provider_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Value> {
    let id: String = row.get(0)?;
    let name: String = row.get(1)?;
    let typ: String = row.get(2)?;
    let api_key: Option<String> = row.get(3)?;
    let api_key_encrypted: Option<Vec<u8>> = row.get(4)?;
    let cli_path: Option<String> = row.get(5)?;
    let config_str: Option<String> = row.get(6)?;
    let is_active: i64 = row.get(7)?;
    let created_at: String = row.get(8)?;
    let updated_at: String = row.get(9)?;

    let has_api_key = api_key.is_some()
        || api_key_encrypted
            .as_ref()
            .map(|b| !b.is_empty())
            .unwrap_or(false);
    let config: Value = config_str
        .as_deref()
        .and_then(|s| serde_json::from_str::<Value>(s).ok())
        .unwrap_or(Value::Null);

    Ok(serde_json::json!({
        "id": id,
        "name": name,
        "type": typ,
        "apiKey": api_key,
        "apiKeyEncrypted": null,
        "hasApiKey": has_api_key,
        "cliPath": cli_path,
        "config": config,
        "isActive": is_active != 0,
        "createdAt": created_at,
        "updatedAt": updated_at,
    }))
}

fn iso_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{secs}")
}

fn local_now_string() -> String {
    Local::now().to_rfc3339()
}

fn collect_daemon_health_snapshot(system: &mut System) -> DaemonHealthSnapshot {
    let pid = std::process::id();
    let sysinfo_pid = Pid::from_u32(pid);

    system.refresh_process_specifics(sysinfo_pid, sysinfo::ProcessRefreshKind::new());

    let (cpu_percent, memory_mb) = system
        .process(sysinfo_pid)
        .map(|process| {
            (
                process.cpu_usage() as f64,
                process.memory() as f64 / 1_024.0 / 1_024.0,
            )
        })
        .unwrap_or((0.0, 0.0));

    DaemonHealthSnapshot {
        pid,
        cpu_percent,
        memory_mb,
        last_metrics_at: local_now_string(),
    }
}

#[derive(Debug, Clone)]
struct CronSchedule {
    seconds: Vec<u32>,
    minutes: Vec<u32>,
    hours: Vec<u32>,
    days_of_month: Vec<u32>,
    months: Vec<u32>,
    days_of_week: Vec<u32>,
}

fn expand_cron_field(raw: &str, min: u32, max: u32) -> Result<Vec<u32>, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("campo cron vazio".into());
    }
    if trimmed == "*" {
        return Ok((min..=max).collect());
    }

    let mut values = Vec::new();
    for part in trimmed.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let (range_part, step) = if let Some((range, step_raw)) = part.split_once('/') {
            let step = step_raw
                .parse::<u32>()
                .map_err(|_| format!("passo cron inválido: {step_raw}"))?;
            (range.trim(), step.max(1))
        } else {
            (part, 1)
        };

        let (start, end) = if range_part == "*" {
            (min, max)
        } else if let Some((start_raw, end_raw)) = range_part.split_once('-') {
            let start = start_raw
                .trim()
                .parse::<u32>()
                .map_err(|_| format!("valor cron inválido: {start_raw}"))?;
            let end = end_raw
                .trim()
                .parse::<u32>()
                .map_err(|_| format!("valor cron inválido: {end_raw}"))?;
            (start, end)
        } else {
            let value = range_part
                .parse::<u32>()
                .map_err(|_| format!("valor cron inválido: {range_part}"))?;
            (value, value)
        };

        if start > end {
            return Err(format!("intervalo cron inválido: {part}"));
        }

        let upper = end.min(max);
        let mut current = start.max(min);
        while current <= upper {
            values.push(current);
            current = current.saturating_add(step);
            if current == u32::MAX {
                break;
            }
        }
    }

    values.sort_unstable();
    values.dedup();
    if values.is_empty() {
        return Err(format!("campo cron sem valores válidos: {raw}"));
    }
    Ok(values)
}

fn parse_cron_schedule(raw: &str) -> Result<CronSchedule, String> {
    let parts = raw
        .split_whitespace()
        .filter(|p| !p.trim().is_empty())
        .collect::<Vec<_>>();
    match parts.len() {
        5 => Ok(CronSchedule {
            seconds: vec![0],
            minutes: expand_cron_field(parts[0], 0, 59)?,
            hours: expand_cron_field(parts[1], 0, 23)?,
            days_of_month: expand_cron_field(parts[2], 1, 31)?,
            months: expand_cron_field(parts[3], 1, 12)?,
            days_of_week: expand_cron_field(parts[4], 0, 6)?,
        }),
        6 => Ok(CronSchedule {
            seconds: expand_cron_field(parts[0], 0, 59)?,
            minutes: expand_cron_field(parts[1], 0, 59)?,
            hours: expand_cron_field(parts[2], 0, 23)?,
            days_of_month: expand_cron_field(parts[3], 1, 31)?,
            months: expand_cron_field(parts[4], 1, 12)?,
            days_of_week: expand_cron_field(parts[5], 0, 6)?,
        }),
        _ => Err(format!(
            "cron inválido: esperado 5 ou 6 campos, obtido {}",
            parts.len()
        )),
    }
}

fn cron_field_matches(value: u32, allowed: &[u32]) -> bool {
    allowed.binary_search(&value).is_ok()
}

fn cron_schedule_matches(schedule: &CronSchedule, dt: DateTime<Local>) -> bool {
    let weekday = dt.weekday().num_days_from_sunday();
    cron_field_matches(dt.second(), &schedule.seconds)
        && cron_field_matches(dt.minute(), &schedule.minutes)
        && cron_field_matches(dt.hour(), &schedule.hours)
        && cron_field_matches(dt.day(), &schedule.days_of_month)
        && cron_field_matches(dt.month(), &schedule.months)
        && cron_field_matches(weekday, &schedule.days_of_week)
}

fn next_cron_run_after(schedule: &CronSchedule, after: DateTime<Local>) -> Option<DateTime<Local>> {
    let mut candidate = after + ChronoDuration::seconds(1);
    let limit = after + ChronoDuration::days(366);
    while candidate <= limit {
        if cron_schedule_matches(schedule, candidate) {
            return Some(candidate);
        }
        candidate += ChronoDuration::seconds(1);
    }
    None
}

fn normalize_task_cwd_mode(raw: Option<&str>) -> String {
    match raw.unwrap_or("worktree").trim() {
        "project" => "project".to_string(),
        "worktree" => "worktree".to_string(),
        other if other.is_empty() => "worktree".to_string(),
        other => other.to_string(),
    }
}

#[derive(Debug, Clone)]
struct RepoTaskRuntime {
    project_id: String,
    project_name: String,
    task: RepoTaskPayload,
}

#[derive(Debug, Clone, Default)]
struct DaemonTaskRunState {
    pty_id: Option<String>,
    pane_id: Option<String>,
    comb_id: Option<String>,
    status: String,
    attached: bool,
    next_run_at: Option<String>,
    last_run_at: Option<String>,
    last_exit_code: Option<i64>,
    last_error: Option<String>,
    last_output_excerpt: Option<String>,
    updated_at: Option<String>,
}

fn collect_repo_tasks(state: &AppState) -> Result<Vec<RepoTaskRuntime>, String> {
    let conn = state
        .conn
        .lock()
        .map_err(|_| "db lock poisoned".to_string())?;
    let mut stmt = conn
        .prepare("SELECT id, name, path, repo_config FROM projects ORDER BY name ASC")
        .map_err(|e| e.to_string())?;
    let mut rows = stmt.query([]).map_err(|e| e.to_string())?;
    let mut tasks = Vec::new();

    while let Some(row) = rows.next().map_err(|e| e.to_string())? {
        let project_id: String = row.get(0).map_err(|e| e.to_string())?;
        let project_name: String = row.get(1).map_err(|e| e.to_string())?;
        let project_path: String = row.get(2).map_err(|e| e.to_string())?;
        let repo_config_raw: Option<String> = row.get(3).map_err(|e| e.to_string())?;
        let repo_config_value = resolve_repo_config_value(
            Some(project_path.as_str()),
            repo_config_raw
                .as_deref()
                .and_then(|raw| serde_json::from_str::<Value>(raw).ok()),
        );
        let payload = repo_config_payload_from_value(&repo_config_value).unwrap_or_default();
        for task in payload.tasks {
            tasks.push(RepoTaskRuntime {
                project_id: project_id.clone(),
                project_name: project_name.clone(),
                task,
            });
        }
    }

    Ok(tasks)
}

fn read_daemon_task_state(
    conn: &Connection,
    project_id: &str,
    task_id: &str,
) -> Result<Option<DaemonTaskRunState>, String> {
    conn.query_row(
        "SELECT pty_id, pane_id, comb_id, status, attached, next_run_at, last_run_at, last_exit_code, last_error, last_output_excerpt, trigger_when, trigger_prompt, trigger_provider_id, updated_at
         FROM daemon_task_runs
         WHERE project_id = ?1 AND task_id = ?2
         LIMIT 1",
        params![project_id, task_id],
        |row| {
            let attached: i64 = row.get(4)?;
            Ok(DaemonTaskRunState {
                pty_id: row.get(0)?,
                pane_id: row.get(1)?,
                comb_id: row.get(2)?,
                status: row.get(3)?,
                attached: attached != 0,
                next_run_at: row.get(5)?,
                last_run_at: row.get(6)?,
                last_exit_code: row.get(7)?,
                last_error: row.get(8)?,
                last_output_excerpt: row.get(9)?,
                updated_at: row.get(13)?,
            })
        },
    )
    .optional()
    .map_err(|e| e.to_string())
}

fn upsert_daemon_task_state(
    conn: &Connection,
    runtime: &RepoTaskRuntime,
    state: &DaemonTaskRunState,
    schedule: &str,
    cwd_mode: &str,
    enabled: bool,
) -> Result<(), String> {
    let trigger = runtime.task.trigger.clone();
    let id = format!(
        "daemon-task-state-{}-{}",
        runtime.project_id, runtime.task.id
    );
    conn.execute(
        "
        INSERT INTO daemon_task_runs (
          id, project_id, task_id, task_name, command, schedule, cwd_mode, enabled,
          trigger_when, trigger_prompt, trigger_provider_id,
          status, attached, pty_id, pane_id, comb_id,
          next_run_at, last_run_at, last_exit_code, last_output_excerpt, last_error,
          created_at, updated_at
        ) VALUES (
          ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8,
          ?9, ?10, ?11,
          ?12, ?13, ?14, ?15, ?16,
          ?17, ?18, ?19, ?20, ?21,
          datetime('now'), datetime('now')
        )
        ON CONFLICT(project_id, task_id) DO UPDATE SET
          task_name = excluded.task_name,
          command = excluded.command,
          schedule = excluded.schedule,
          cwd_mode = excluded.cwd_mode,
          enabled = excluded.enabled,
          trigger_when = excluded.trigger_when,
          trigger_prompt = excluded.trigger_prompt,
          trigger_provider_id = excluded.trigger_provider_id,
          status = excluded.status,
          attached = excluded.attached,
          pty_id = COALESCE(excluded.pty_id, daemon_task_runs.pty_id),
          pane_id = COALESCE(excluded.pane_id, daemon_task_runs.pane_id),
          comb_id = COALESCE(excluded.comb_id, daemon_task_runs.comb_id),
          next_run_at = excluded.next_run_at,
          last_run_at = COALESCE(excluded.last_run_at, daemon_task_runs.last_run_at),
          last_exit_code = COALESCE(excluded.last_exit_code, daemon_task_runs.last_exit_code),
          last_output_excerpt = COALESCE(excluded.last_output_excerpt, daemon_task_runs.last_output_excerpt),
          last_error = COALESCE(excluded.last_error, daemon_task_runs.last_error),
          updated_at = datetime('now')
        ",
        params![
            id,
            &runtime.project_id,
            &runtime.task.id,
            &runtime.task.name,
            &runtime.task.command,
            schedule,
            cwd_mode,
            if enabled { 1 } else { 0 },
            trigger.as_ref().and_then(|t| t.when.clone()),
            trigger.as_ref().and_then(|t| t.prompt.clone()),
            trigger.as_ref().and_then(|t| t.provider_id.clone()),
            state.status.clone(),
            if state.attached { 1 } else { 0 },
            state.pty_id.clone(),
            state.pane_id.clone(),
            state.comb_id.clone(),
            state.next_run_at.clone(),
            state.last_run_at.clone(),
            state.last_exit_code,
            state.last_output_excerpt.clone(),
            state.last_error.clone(),
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn daemon_task_state_to_payload(
    runtime: &RepoTaskRuntime,
    state: Option<DaemonTaskRunState>,
) -> Value {
    let state = state.unwrap_or_default();
    let status = if state.status.is_empty() {
        "idle".to_string()
    } else {
        state.status.clone()
    };
    serde_json::json!({
        "projectId": runtime.project_id.clone(),
        "projectName": runtime.project_name.clone(),
        "taskId": runtime.task.id.clone(),
        "taskName": runtime.task.name.clone(),
        "command": runtime.task.command.clone(),
        "schedule": runtime.task.schedule.clone(),
        "cwdMode": normalize_task_cwd_mode(runtime.task.cwd_mode.as_deref()),
        "enabled": runtime.task.enabled.unwrap_or(true),
        "status": status,
        "attached": state.attached,
        "ptyId": state.pty_id,
        "paneId": state.pane_id,
        "combId": state.comb_id,
        "nextRunAt": state.next_run_at,
        "lastRunAt": state.last_run_at,
        "lastExitCode": state.last_exit_code,
        "lastError": state.last_error,
        "lastOutputExcerpt": state.last_output_excerpt,
        "trigger": runtime.task.trigger.clone(),
        "updatedAt": state.updated_at,
    })
}

fn parse_local_datetime(raw: &str) -> Option<DateTime<Local>> {
    DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|dt| dt.with_timezone(&Local))
}

fn task_next_run_at(
    task: &RepoTaskPayload,
    from: DateTime<Local>,
) -> Result<Option<String>, String> {
    if !task.enabled.unwrap_or(true) {
        return Ok(None);
    }
    let schedule = parse_cron_schedule(&task.schedule)?;
    Ok(next_cron_run_after(&schedule, from).map(|dt| dt.to_rfc3339()))
}

fn task_shell_command(command: &str) -> (String, Vec<String>) {
    parse_command_payload(command)
}

fn daemon_get_status_internal(state: &AppState) -> Result<Value, String> {
    let tasks = collect_repo_tasks(state)?;
    let conn = state
        .conn
        .lock()
        .map_err(|_| "db lock poisoned".to_string())?;
    let mut total_tasks = 0i64;
    let mut enabled_tasks = 0i64;
    let mut running_tasks = 0i64;
    let mut terminals = state
        .terminals
        .lock()
        .map_err(|_| "terminals lock poisoned".to_string())?;

    for runtime in &tasks {
        total_tasks += 1;
        if runtime.task.enabled.unwrap_or(true) {
            enabled_tasks += 1;
        }
        if let Some(state_row) =
            read_daemon_task_state(&conn, &runtime.project_id, &runtime.task.id)?
        {
            let is_running = state_row.status == "running"
                && state_row
                    .pty_id
                    .as_ref()
                    .and_then(|pty_id| terminals.get_mut(pty_id))
                    .map(|terminal| terminal.child.try_wait().ok().flatten().is_none())
                    .unwrap_or(false);
            if is_running {
                running_tasks += 1;
            }
        }
    }

    let last_tick_at = state
        .daemon
        .last_tick_at
        .lock()
        .map_err(|_| "daemon lock poisoned".to_string())?
        .clone();
    let health = {
        let mut system = state
            .daemon
            .system
            .lock()
            .map_err(|_| "system lock poisoned".to_string())?;
        collect_daemon_health_snapshot(&mut system)
    };

    Ok(serde_json::json!({
        "mode": "in-process",
        "running": state.daemon.running.load(Ordering::Relaxed),
        "startedAt": state.daemon.started_at.clone(),
        "lastTickAt": last_tick_at,
        "pid": health.pid,
        "cpuPercent": health.cpu_percent,
        "memoryMb": health.memory_mb,
        "lastMetricsAt": health.last_metrics_at,
        "totalTasks": total_tasks,
        "runningTasks": running_tasks,
        "enabledTasks": enabled_tasks
    }))
}

fn daemon_health_internal(state: &AppState) -> Result<Value, String> {
    let last_tick_at = state
        .daemon
        .last_tick_at
        .lock()
        .map_err(|_| "daemon lock poisoned".to_string())?
        .clone();
    let health = {
        let mut system = state
            .daemon
            .system
            .lock()
            .map_err(|_| "system lock poisoned".to_string())?;
        collect_daemon_health_snapshot(&mut system)
    };

    Ok(serde_json::json!({
        "ok": true,
        "mode": "in-process",
        "running": state.daemon.running.load(Ordering::Relaxed),
        "startedAt": state.daemon.started_at.clone(),
        "lastTickAt": last_tick_at,
        "pid": health.pid,
        "cpuPercent": health.cpu_percent,
        "memoryMb": health.memory_mb,
        "lastMetricsAt": health.last_metrics_at,
    }))
}

fn daemon_list_tasks_internal(state: &AppState) -> Result<Value, String> {
    let tasks = collect_repo_tasks(state)?;
    let conn = state
        .conn
        .lock()
        .map_err(|_| "db lock poisoned".to_string())?;
    let mut out = Vec::new();

    for runtime in tasks {
        let state_row = read_daemon_task_state(&conn, &runtime.project_id, &runtime.task.id)?;
        out.push(daemon_task_state_to_payload(&runtime, state_row));
    }

    Ok(Value::Array(out))
}

fn daemon_list_combs_internal(state: &AppState, project_id: Option<&str>) -> Result<Value, String> {
    let conn = state
        .conn
        .lock()
        .map_err(|_| "db lock poisoned".to_string())?;
    let mut out = Vec::new();

    match project_id {
        Some(project_id) => {
            let mut stmt = conn
                .prepare(
                    "SELECT * FROM combs WHERE project_id = ?1
                     ORDER BY COALESCE(is_pinned, 0) DESC,
                              (last_git_activity_at IS NULL), last_git_activity_at DESC,
                              (last_opened_at IS NULL), last_opened_at DESC
                     LIMIT 100",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(params![project_id], row_to_json)
                .map_err(|e| e.to_string())?;
            for row in rows {
                out.push(row.map(map_comb_to_renderer).map_err(|e| e.to_string())?);
            }
        }
        None => {
            let mut stmt = conn
                .prepare(
                    "SELECT * FROM combs
                     ORDER BY COALESCE(is_pinned, 0) DESC,
                              (last_git_activity_at IS NULL), last_git_activity_at DESC,
                              (last_opened_at IS NULL), last_opened_at DESC
                     LIMIT 100",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt.query_map([], row_to_json).map_err(|e| e.to_string())?;
            for row in rows {
                out.push(row.map(map_comb_to_renderer).map_err(|e| e.to_string())?);
            }
        }
    }

    Ok(Value::Array(out))
}

fn daemon_list_panes_internal(
    state: &AppState,
    project_id: Option<&str>,
    comb_id: Option<&str>,
) -> Result<Value, String> {
    let conn = state
        .conn
        .lock()
        .map_err(|_| "db lock poisoned".to_string())?;
    let mut out = Vec::new();

    if let Some(comb_id) = comb_id {
        let mut stmt = conn
            .prepare(
                "SELECT * FROM panes WHERE comb_id = ?1 ORDER BY layout_order ASC, created_at ASC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![comb_id], row_to_json)
            .map_err(|e| e.to_string())?;
        for row in rows {
            out.push(row.map(map_pane_to_renderer).map_err(|e| e.to_string())?);
        }
        return Ok(Value::Array(out));
    }

    if let Some(project_id) = project_id {
        let mut stmt = conn
            .prepare(
                "SELECT p.*
                FROM panes p
                 JOIN combs c ON c.id = p.comb_id
                 WHERE c.project_id = ?1
                 ORDER BY p.layout_order ASC, p.created_at ASC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![project_id], row_to_json)
            .map_err(|e| e.to_string())?;
        for row in rows {
            out.push(row.map(map_pane_to_renderer).map_err(|e| e.to_string())?);
        }
        return Ok(Value::Array(out));
    }

    let mut stmt = conn
        .prepare("SELECT * FROM panes ORDER BY layout_order ASC, created_at ASC")
        .map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], row_to_json).map_err(|e| e.to_string())?;
    for row in rows {
        out.push(row.map(map_pane_to_renderer).map_err(|e| e.to_string())?);
    }

    Ok(Value::Array(out))
}

fn daemon_get_diffs_bundle_internal(
    state: &AppState,
    worktree_paths: Vec<String>,
    comb_ids: Vec<String>,
) -> Result<Value, String> {
    let mut paths = worktree_paths;

    if !comb_ids.is_empty() {
        let conn = state
            .conn
            .lock()
            .map_err(|_| "db lock poisoned".to_string())?;
        let mut stmt = conn
            .prepare("SELECT worktree_path FROM combs WHERE id = ?1 AND worktree_path IS NOT NULL AND worktree_path != ''")
            .map_err(|e| e.to_string())?;
        for comb_id in &comb_ids {
            let maybe_path: Option<String> = stmt
                .query_row(params![comb_id], |row| row.get(0))
                .optional()
                .map_err(|e| e.to_string())?;
            if let Some(path) = maybe_path {
                paths.push(path);
            }
        }
    }

    paths.sort();
    paths.dedup();

    let mut out = Vec::new();
    for worktree_path in paths {
        match build_review_diffs_for_path(&worktree_path) {
            Ok(payload) => {
                out.push(serde_json::json!({
                    "worktreePath": worktree_path,
                    "success": true,
                    "files": payload.get("files").cloned().unwrap_or(Value::Array(vec![])),
                    "summary": payload.get("summary").cloned().unwrap_or(Value::Null)
                }));
            }
            Err(e) => {
                out.push(serde_json::json!({
                    "worktreePath": worktree_path,
                    "success": false,
                    "error": e.message,
                    "files": [],
                    "summary": Value::Null
                }));
            }
        }
    }

    Ok(Value::Array(out))
}

fn daemon_spawn_task_runtime(
    app: &AppHandle,
    state: &AppState,
    runtime: &RepoTaskRuntime,
    attach: bool,
    force: bool,
) -> Result<Value, String> {
    let enabled = runtime.task.enabled.unwrap_or(true);
    if !enabled && !force {
        let conn = state
            .conn
            .lock()
            .map_err(|_| "db lock poisoned".to_string())?;
        let state_row = DaemonTaskRunState {
            status: "disabled".into(),
            ..Default::default()
        };
        upsert_daemon_task_state(
            &conn,
            runtime,
            &state_row,
            &runtime.task.schedule,
            &normalize_task_cwd_mode(runtime.task.cwd_mode.as_deref()),
            false,
        )?;
        return Ok(daemon_task_state_to_payload(runtime, Some(state_row)));
    }

    let now = Local::now();
    let next_run_at = task_next_run_at(&runtime.task, now)?;
    let cwd_mode = normalize_task_cwd_mode(runtime.task.cwd_mode.as_deref());
    let (comb_id, cwd) = choose_project_comb_and_cwd(state, &runtime.project_id, &cwd_mode)?;
    let pty_owner_key = daemon_task_owner_key(&runtime.project_id, &runtime.task.id);
    let (command, args) = task_shell_command(&runtime.task.command);
    let pane_id = comb_id.as_ref().map(|_| next_id());

    if let Some(comb_id_value) = comb_id.as_ref() {
        let conn = state
            .conn
            .lock()
            .map_err(|_| "db lock poisoned".to_string())?;
        let layout_order = conn
            .query_row(
                "SELECT COALESCE(MAX(layout_order), -1) + 1 FROM panes WHERE comb_id = ?1",
                params![comb_id_value],
                |row| row.get::<_, i64>(0),
            )
            .unwrap_or(0);
        if let Some(ref pane_id_value) = pane_id {
            conn.execute(
                "INSERT INTO panes (id, comb_id, type, provider_id, title, initial_prompt, cwd, pty_owner_key, status, layout_order, last_activity_at, created_at, updated_at)
                 VALUES (?1, ?2, 'term', NULL, ?3, ?4, ?5, ?6, 'running', ?7, datetime('now'), datetime('now'), datetime('now'))",
                params![
                    pane_id_value,
                    comb_id_value,
                    &runtime.task.name,
                    &runtime.task.command,
                    &cwd,
                    &pty_owner_key,
                    layout_order,
                ],
            )
            .map_err(|e| e.to_string())?;
        }
    }

    let pty_id = match spawn_terminal_session_with_options(
        state,
        app,
        cwd.clone(),
        command,
        args,
        pane_id.clone(),
        Some(pty_owner_key.clone()),
    ) {
        Ok(id) => id,
        Err(err) => {
            if let (Some(comb_id_value), Some(pane_id_value)) = (comb_id.as_ref(), pane_id.as_ref())
            {
                if let Ok(conn) = state.conn.lock() {
                    let _ = conn.execute(
                        "DELETE FROM panes WHERE id = ?1 AND comb_id = ?2",
                        params![pane_id_value, comb_id_value],
                    );
                }
            }
            if let Ok(conn) = state.conn.lock() {
                let failed_row = DaemonTaskRunState {
                    status: "failed".into(),
                    last_error: Some(err.message.clone()),
                    ..Default::default()
                };
                let _ = upsert_daemon_task_state(
                    &conn,
                    runtime,
                    &failed_row,
                    &runtime.task.schedule,
                    &cwd_mode,
                    enabled,
                );
            }
            return Err(err.message);
        }
    };

    let conn = state
        .conn
        .lock()
        .map_err(|_| "db lock poisoned".to_string())?;
    let mut last_run_state = DaemonTaskRunState::default();
    last_run_state.status = "running".into();
    last_run_state.attached = attach || pane_id.is_some();
    last_run_state.pty_id = Some(pty_id.clone());
    last_run_state.pane_id = pane_id.clone();
    last_run_state.comb_id = comb_id.clone();
    last_run_state.last_run_at = Some(local_now_string());
    last_run_state.next_run_at = next_run_at;
    last_run_state.last_error = None;
    last_run_state.last_exit_code = None;
    upsert_daemon_task_state(
        &conn,
        runtime,
        &last_run_state,
        &runtime.task.schedule,
        &cwd_mode,
        enabled,
    )?;

    Ok(daemon_task_state_to_payload(runtime, Some(last_run_state)))
}

fn daemon_attach_task_internal(
    state: &AppState,
    project_id: &str,
    task_id: &str,
) -> Result<Value, String> {
    let tasks = collect_repo_tasks(state)?;
    let runtime = tasks
        .into_iter()
        .find(|item| item.project_id == project_id && item.task.id == task_id)
        .ok_or_else(|| "task not found".to_string())?;
    let conn = state
        .conn
        .lock()
        .map_err(|_| "db lock poisoned".to_string())?;
    let mut row = read_daemon_task_state(&conn, project_id, task_id)?.unwrap_or_default();
    row.attached = true;
    row.status = if row.status.is_empty() {
        "idle".into()
    } else {
        row.status
    };
    upsert_daemon_task_state(
        &conn,
        &runtime,
        &row,
        &runtime.task.schedule,
        &normalize_task_cwd_mode(runtime.task.cwd_mode.as_deref()),
        runtime.task.enabled.unwrap_or(true),
    )?;
    Ok(daemon_task_state_to_payload(&runtime, Some(row)))
}

fn daemon_detach_task_internal(
    state: &AppState,
    project_id: &str,
    task_id: &str,
) -> Result<Value, String> {
    let tasks = collect_repo_tasks(state)?;
    let runtime = tasks
        .into_iter()
        .find(|item| item.project_id == project_id && item.task.id == task_id)
        .ok_or_else(|| "task not found".to_string())?;
    let conn = state
        .conn
        .lock()
        .map_err(|_| "db lock poisoned".to_string())?;
    let mut row = read_daemon_task_state(&conn, project_id, task_id)?.unwrap_or_default();
    row.attached = false;
    if row.status.is_empty() {
        row.status = "idle".into();
    }
    upsert_daemon_task_state(
        &conn,
        &runtime,
        &row,
        &runtime.task.schedule,
        &normalize_task_cwd_mode(runtime.task.cwd_mode.as_deref()),
        runtime.task.enabled.unwrap_or(true),
    )?;
    Ok(daemon_task_state_to_payload(&runtime, Some(row)))
}

fn daemon_sweep_terminal_exits_internal(app: &AppHandle, state: &AppState) -> Result<(), String> {
    let task_catalog = collect_repo_tasks(state)?;
    let mut events: Vec<(
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<i64>,
        String,
    )> = Vec::new();
    {
        let mut terminals = state
            .terminals
            .lock()
            .map_err(|_| "terminals lock poisoned".to_string())?;
        for (pty_id, terminal) in terminals.iter_mut() {
            if terminal.exit_notified.load(Ordering::Relaxed) {
                continue;
            }
            let Some(status) = terminal.child.try_wait().ok().flatten() else {
                continue;
            };
            terminal.exit_notified.store(true, Ordering::Relaxed);
            let exit_code = Some(status.exit_code() as i64);
            let excerpt = terminal.output_buffer.lock().ok().and_then(|buffer| {
                buffer
                    .iter()
                    .rev()
                    .find(|line| !line.trim().is_empty())
                    .map(|line| {
                        let trimmed = line.trim();
                        trimmed.chars().take(240).collect::<String>()
                    })
            });
            events.push((
                pty_id.clone(),
                terminal.pty_owner_key.clone(),
                terminal.pane_id.clone(),
                excerpt,
                exit_code,
                terminal.started_at.clone(),
            ));
        }
    }

    if events.is_empty() {
        return Ok(());
    }

    let conn = state
        .conn
        .lock()
        .map_err(|_| "db lock poisoned".to_string())?;
    for (pty_id, owner_key, pane_id, excerpt, exit_code, started_at) in events {
        let code = exit_code.unwrap_or(-1);
        let status = if code == 0 { "completed" } else { "failed" };
        if let Some(owner_key) = owner_key {
            if let Some(rest) = owner_key.strip_prefix("daemon-task:") {
                let mut parts = rest.splitn(2, ':');
                let project_id = parts.next().unwrap_or_default();
                let task_id = parts.next().unwrap_or_default();
                if !project_id.is_empty() && !task_id.is_empty() {
                    if let Some(runtime) = task_catalog
                        .iter()
                        .find(|item| item.project_id == project_id && item.task.id == task_id)
                    {
                        let mut row =
                            read_daemon_task_state(&conn, project_id, task_id)?.unwrap_or_default();
                        row.status = status.to_string();
                        row.attached = false;
                        row.pty_id = Some(pty_id.clone());
                        row.pane_id = pane_id.clone();
                        row.last_exit_code = Some(code);
                        row.last_output_excerpt = excerpt.clone();
                        row.last_run_at = Some(started_at.clone());
                        row.last_error = if code == 0 {
                            None
                        } else {
                            Some(format!("exit code {code}"))
                        };
                        upsert_daemon_task_state(
                            &conn,
                            &runtime,
                            &row,
                            &runtime.task.schedule,
                            &normalize_task_cwd_mode(runtime.task.cwd_mode.as_deref()),
                            runtime.task.enabled.unwrap_or(true),
                        )?;

                        let notification_body = if code == 0 {
                            format!("{} concluiu com sucesso", runtime.task.name.clone())
                        } else {
                            format!("{} terminou com código {code}", runtime.task.name.clone())
                        };
                        let _ = app
                            .notification()
                            .builder()
                            .title("Tarefa do daemon")
                            .body(notification_body)
                            .show();
                    }
                }
            }
        }
        let _ = app.emit(
            "terminal-exit",
            serde_json::json!({
                "ptyId": pty_id,
                "code": code
            }),
        );
    }

    Ok(())
}

fn daemon_tick_internal(app: &AppHandle, state: &AppState) -> Result<(), String> {
    if !state.daemon.running.load(Ordering::Relaxed) {
        return Ok(());
    }
    {
        let mut last_tick = state
            .daemon
            .last_tick_at
            .lock()
            .map_err(|_| "daemon lock poisoned".to_string())?;
        *last_tick = Some(local_now_string());
    }

    daemon_sweep_terminal_exits_internal(app, state)?;

    let tasks = collect_repo_tasks(state)?;
    let now = Local::now();

    for runtime in tasks {
        let conn = state
            .conn
            .lock()
            .map_err(|_| "db lock poisoned".to_string())?;
        let enabled = runtime.task.enabled.unwrap_or(true);
        let state_row = read_daemon_task_state(&conn, &runtime.project_id, &runtime.task.id)?
            .unwrap_or_default();
        let cwd_mode = normalize_task_cwd_mode(runtime.task.cwd_mode.as_deref());

        let mut parsed_next_run = state_row
            .next_run_at
            .as_deref()
            .and_then(parse_local_datetime);

        if state_row.status == "running" {
            continue;
        }

        if parsed_next_run.is_none() && enabled {
            parsed_next_run = task_next_run_at(&runtime.task, now)
                .ok()
                .flatten()
                .and_then(|s| parse_local_datetime(&s));
        }

        if !enabled {
            let mut disabled_row = state_row.clone();
            disabled_row.status = "disabled".into();
            disabled_row.attached = false;
            disabled_row.next_run_at = None;
            upsert_daemon_task_state(
                &conn,
                &runtime,
                &disabled_row,
                &runtime.task.schedule,
                &cwd_mode,
                false,
            )?;
            continue;
        }

        if let Some(due_at) = parsed_next_run {
            if now >= due_at {
                drop(conn);
                let _ = daemon_spawn_task_runtime(app, state, &runtime, false, false)?;
            }
        } else if state_row.next_run_at.is_none() {
            let next_run_at = task_next_run_at(&runtime.task, now).ok().flatten();
            let mut next_row = state_row.clone();
            next_row.next_run_at = next_run_at;
            next_row.status = if next_row.status.is_empty() {
                "idle".into()
            } else {
                next_row.status
            };
            next_row.attached = state_row.attached;
            upsert_daemon_task_state(
                &conn,
                &runtime,
                &next_row,
                &runtime.task.schedule,
                &cwd_mode,
                true,
            )?;
        }
    }

    Ok(())
}

fn start_daemon_worker(app: AppHandle, state: AppState) {
    state.daemon.running.store(true, Ordering::Relaxed);
    thread::spawn(move || {
        while state.daemon.running.load(Ordering::Relaxed) {
            if let Err(err) = daemon_tick_internal(&app, &state) {
                eprintln!("[DCC][daemon] tick error: {err}");
            }
            thread::sleep(Duration::from_secs(5));
        }
    });
}

fn daemon_runtime_endpoint(state: &AppState) -> Option<DaemonRuntimeInfo> {
    state
        .daemon_endpoint
        .lock()
        .ok()
        .and_then(|endpoint| endpoint.clone())
}

#[tauri::command]
fn daemon_get_status(state: State<'_, AppState>) -> ApiResult<Value> {
    if let Some(info) = daemon_runtime_endpoint(state.inner()) {
        return rpc_with_info(&info, "daemon.getStatus", serde_json::json!({})).map_err(db_error);
    }
    daemon_get_status_internal(state.inner()).map_err(db_error)
}

#[tauri::command]
fn daemon_health(state: State<'_, AppState>) -> ApiResult<Value> {
    if let Some(info) = daemon_runtime_endpoint(state.inner()) {
        return rpc_with_info(&info, "daemon.health", serde_json::json!({})).map_err(db_error);
    }
    daemon_health_internal(state.inner()).map_err(db_error)
}

#[tauri::command]
fn daemon_list_tasks(state: State<'_, AppState>) -> ApiResult<Value> {
    if let Some(info) = daemon_runtime_endpoint(state.inner()) {
        return rpc_with_info(&info, "daemon.listTasks", serde_json::json!({})).map_err(db_error);
    }
    daemon_list_tasks_internal(state.inner()).map_err(db_error)
}

#[tauri::command]
fn daemon_list_processes(
    state: State<'_, AppState>,
    project_id: Option<String>,
) -> ApiResult<Value> {
    if let Some(info) = daemon_runtime_endpoint(state.inner()) {
        return rpc_with_info(
            &info,
            "daemon.listProcesses",
            serde_json::json!({ "projectId": project_id }),
        )
        .map_err(db_error);
    }
    Err(db_error("daemon not available".to_string()))
}

#[tauri::command]
fn daemon_start_process(
    state: State<'_, AppState>,
    project_id: String,
    process_id: String,
) -> ApiResult<Value> {
    if let Some(info) = daemon_runtime_endpoint(state.inner()) {
        return rpc_with_info(
            &info,
            "daemon.startProcess",
            serde_json::json!({ "projectId": project_id, "processId": process_id }),
        )
        .map_err(db_error);
    }
    Err(db_error("daemon not available".to_string()))
}

#[tauri::command]
fn daemon_stop_process(
    state: State<'_, AppState>,
    project_id: String,
    process_id: String,
) -> ApiResult<Value> {
    if let Some(info) = daemon_runtime_endpoint(state.inner()) {
        return rpc_with_info(
            &info,
            "daemon.stopProcess",
            serde_json::json!({ "projectId": project_id, "processId": process_id }),
        )
        .map_err(db_error);
    }
    Err(db_error("daemon not available".to_string()))
}

#[tauri::command]
fn daemon_restart_process(
    state: State<'_, AppState>,
    project_id: String,
    process_id: String,
) -> ApiResult<Value> {
    if let Some(info) = daemon_runtime_endpoint(state.inner()) {
        return rpc_with_info(
            &info,
            "daemon.restartProcess",
            serde_json::json!({ "projectId": project_id, "processId": process_id }),
        )
        .map_err(db_error);
    }
    Err(db_error("daemon not available".to_string()))
}

#[tauri::command]
fn daemon_list_combs(state: State<'_, AppState>, project_id: Option<String>) -> ApiResult<Value> {
    if let Some(info) = daemon_runtime_endpoint(state.inner()) {
        return rpc_with_info(
            &info,
            "combs.list",
            serde_json::json!({ "projectId": project_id }),
        )
        .map_err(db_error);
    }
    daemon_list_combs_internal(state.inner(), project_id.as_deref()).map_err(db_error)
}

#[tauri::command]
fn daemon_list_panes(
    state: State<'_, AppState>,
    project_id: Option<String>,
    comb_id: Option<String>,
) -> ApiResult<Value> {
    if let Some(info) = daemon_runtime_endpoint(state.inner()) {
        return rpc_with_info(
            &info,
            "panes.list",
            serde_json::json!({ "projectId": project_id, "combId": comb_id }),
        )
        .map_err(db_error);
    }
    daemon_list_panes_internal(state.inner(), project_id.as_deref(), comb_id.as_deref())
        .map_err(db_error)
}

#[tauri::command]
fn daemon_get_diffs_bundle(
    state: State<'_, AppState>,
    worktree_paths: Vec<String>,
    comb_ids: Vec<String>,
) -> ApiResult<Value> {
    if let Some(info) = daemon_runtime_endpoint(state.inner()) {
        return rpc_with_info(
            &info,
            "diffs.bundle",
            serde_json::json!({ "worktreePaths": worktree_paths, "combIds": comb_ids }),
        )
        .map_err(db_error);
    }
    daemon_get_diffs_bundle_internal(state.inner(), worktree_paths, comb_ids).map_err(db_error)
}

#[tauri::command]
fn daemon_run_task(
    state: State<'_, AppState>,
    app: AppHandle,
    project_id: String,
    task_id: String,
) -> ApiResult<Value> {
    if let Some(info) = daemon_runtime_endpoint(state.inner()) {
        return rpc_with_info(
            &info,
            "daemon.runTask",
            serde_json::json!({ "projectId": project_id.clone(), "taskId": task_id.clone() }),
        )
        .map_err(db_error);
    }
    let tasks = collect_repo_tasks(state.inner()).map_err(db_error)?;
    let runtime = tasks
        .into_iter()
        .find(|item| item.project_id == project_id && item.task.id == task_id)
        .ok_or_else(|| db_error("task not found"))?;
    daemon_spawn_task_runtime(&app, state.inner(), &runtime, true, true).map_err(db_error)
}

#[tauri::command]
fn daemon_attach_task(
    state: State<'_, AppState>,
    project_id: String,
    task_id: String,
) -> ApiResult<Value> {
    if let Some(info) = daemon_runtime_endpoint(state.inner()) {
        return rpc_with_info(
            &info,
            "daemon.attachTask",
            serde_json::json!({ "projectId": project_id.clone(), "taskId": task_id.clone() }),
        )
        .map_err(db_error);
    }
    daemon_attach_task_internal(state.inner(), &project_id, &task_id).map_err(db_error)
}

#[tauri::command]
fn daemon_detach_task(
    state: State<'_, AppState>,
    project_id: String,
    task_id: String,
) -> ApiResult<Value> {
    if let Some(info) = daemon_runtime_endpoint(state.inner()) {
        return rpc_with_info(
            &info,
            "daemon.detachTask",
            serde_json::json!({ "projectId": project_id.clone(), "taskId": task_id.clone() }),
        )
        .map_err(db_error);
    }
    daemon_detach_task_internal(state.inner(), &project_id, &task_id).map_err(db_error)
}

#[allow(dead_code)]
fn copy_paths_from_worktree(
    worktree_root: &str,
    project_root: &str,
    relative_paths: &[String],
) -> ApiResult<()> {
    for rel in relative_paths {
        let src = PathBuf::from(worktree_root).join(rel);
        let dst = PathBuf::from(project_root).join(rel);
        if src.exists() {
            if src.is_dir() {
                continue;
            }
            if let Some(parent) = dst.parent() {
                std::fs::create_dir_all(parent).map_err(|e| db_error(e.to_string()))?;
            }
            std::fs::copy(&src, &dst).map_err(|e| db_error(e.to_string()))?;
        } else if dst.exists() {
            std::fs::remove_file(&dst).map_err(|e| db_error(e.to_string()))?;
        }
    }
    Ok(())
}

fn shell_command_for_text(command: &str) -> (String, Vec<String>) {
    #[cfg(target_os = "windows")]
    {
        (
            "cmd".to_string(),
            vec!["/C".to_string(), command.to_string()],
        )
    }
    #[cfg(not(target_os = "windows"))]
    {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
        (shell, vec!["-ilc".to_string(), command.to_string()])
    }
}

fn daemon_task_owner_key(project_id: &str, task_id: &str) -> String {
    format!("daemon-task:{project_id}:{task_id}")
}

fn choose_project_comb_and_cwd(
    state: &AppState,
    project_id: &str,
    cwd_mode: &str,
) -> Result<(Option<String>, String), String> {
    let conn = state
        .conn
        .lock()
        .map_err(|_| "db lock poisoned".to_string())?;
    let project_path: Option<String> = conn
        .query_row(
            "SELECT path FROM projects WHERE id = ?1",
            params![project_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    let Some(project_path) = project_path else {
        return Err("project not found".into());
    };

    let comb = conn
        .query_row(
            "SELECT id, worktree_path
             FROM combs
             WHERE project_id = ?1
             ORDER BY COALESCE(is_pinned, 0) DESC,
                      (last_git_activity_at IS NULL), last_git_activity_at DESC,
                      (last_opened_at IS NULL), last_opened_at DESC, updated_at DESC
             LIMIT 1",
            params![project_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
        )
        .optional()
        .map_err(|e| e.to_string())?;

    let comb_id = comb.as_ref().map(|(id, _)| id.clone());
    let cwd = if cwd_mode == "worktree" {
        comb.and_then(|(_, worktree_path)| worktree_path)
            .filter(|path| !path.trim().is_empty())
            .unwrap_or(project_path)
    } else {
        project_path
    };

    Ok((comb_id, cwd))
}

fn parse_command_payload(command: &str) -> (String, Vec<String>) {
    shell_command_for_text(command)
}

fn spawn_terminal_session_with_options(
    state: &AppState,
    app: &AppHandle,
    cwd: String,
    command: String,
    args: Vec<String>,
    pane_id_opt: Option<String>,
    pty_owner_key_opt: Option<String>,
) -> ApiResult<String> {
    let cols = 80u16;
    let rows = 24u16;
    let pty_system = native_pty_system();
    let pty_pair = pty_system
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| db_error(e.to_string()))?;

    let mut cmd = CommandBuilder::new(&command);
    cmd.args(&args);
    cmd.cwd(&cwd);
    #[cfg(unix)]
    {
        cmd.env("TERM", "xterm-256color");
        let git_file_path = Path::new(&cwd).join(".git");
        if git_file_path.is_file() {
            if let Ok(git_content) = std::fs::read_to_string(&git_file_path) {
                if let Some(gitdir_line) = git_content.lines().find(|l| l.starts_with("gitdir:")) {
                    let gitdir = gitdir_line.trim_start_matches("gitdir:").trim();
                    cmd.env("GIT_DIR", gitdir);
                    cmd.env("GIT_WORK_TREE", &cwd);
                }
            }
        }
    }

    let child = pty_pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| db_error(e.to_string()))?;
    #[cfg(windows)]
    let windows_job = attach_windows_job_object(&child);

    let reader = pty_pair
        .master
        .try_clone_reader()
        .map_err(|e| db_error(e.to_string()))?;
    let writer = pty_pair
        .master
        .take_writer()
        .map_err(|e| db_error(e.to_string()))?;

    let pty_id = format!("pty-{}", next_id());
    let stop_flag = Arc::new(AtomicBool::new(false));
    let title_state = Arc::new(Mutex::new(None));
    let output_buffer = new_terminal_output_buffer();
    if let Some(ref pid) = pane_id_opt {
        if let Ok(conn) = state.conn.lock() {
            if let Some(loaded) = load_pane_scrollback_deque(&conn, pid) {
                if let Ok(mut guard) = output_buffer.lock() {
                    *guard = loaded;
                }
            }
        }
    }
    let reader_thread = Some(spawn_terminal_reader_thread(
        reader,
        app.clone(),
        pty_id.clone(),
        pane_id_opt.clone(),
        title_state.clone(),
        stop_flag.clone(),
        output_buffer.clone(),
        state.conn.clone(),
    ));

    if let Some(pane_id) = pane_id_opt.as_ref() {
        if let Ok(conn) = state.conn.lock() {
            let _ = conn.execute(
                "UPDATE panes SET status = 'running', last_activity_at = datetime('now') WHERE id = ?1",
                params![pane_id],
            );
        }
    }

    let mut terminals = state
        .terminals
        .lock()
        .map_err(|_| db_error("terminals lock poisoned"))?;

    terminals.insert(
        pty_id.clone(),
        ManagedTerminal {
            pty_master: pty_pair.master,
            child,
            writer: Arc::new(Mutex::new(writer)),
            #[cfg(windows)]
            windows_job,
            mission_id: None,
            pane_id: pane_id_opt,
            pty_owner_key: pty_owner_key_opt,
            title: title_state,
            cwd,
            command,
            args,
            started_at: iso_now(),
            stop_flag,
            exit_notified: Arc::new(AtomicBool::new(false)),
            output_buffer,
            reader_thread,
        },
    );

    Ok(pty_id)
}

fn compute_stable_machine_id(app_data_dir: &Path) -> String {
    let host = System::host_name().unwrap_or_else(|| "unknown".into());
    let mut hasher = Sha256::new();
    hasher.update(std::env::consts::OS.as_bytes());
    hasher.update(b"|");
    hasher.update(std::env::consts::ARCH.as_bytes());
    hasher.update(b"|");
    hasher.update(host.as_bytes());
    hasher.update(b"|");
    hasher.update(app_data_dir.to_string_lossy().as_bytes());
    hex::encode(hasher.finalize())
}

fn dialog_result_to_button_index(result: MessageDialogResult, labels: &[String]) -> i32 {
    if !labels.is_empty() {
        if let MessageDialogResult::Custom(ref s) = result {
            if let Some(idx) = labels.iter().position(|l| l == s) {
                return idx as i32;
            }
        }
    }
    match result {
        MessageDialogResult::Ok => 0,
        MessageDialogResult::Cancel => {
            if labels.len() >= 2 {
                (labels.len() - 1) as i32
            } else {
                1
            }
        }
        MessageDialogResult::Yes => 0,
        MessageDialogResult::No => 1,
        MessageDialogResult::Custom(_) => 0,
    }
}

// ---------- App ----------
#[tauri::command]
fn app_get_version(app: AppHandle) -> String {
    app.package_info().version.to_string()
}

#[tauri::command]
async fn app_check_for_updates(app: AppHandle) -> ApiResult<Value> {
    let pkg = app.package_info().version.to_string();
    match app.updater() {
        Ok(updater) => match updater.check().await {
            Ok(Some(update)) => {
                let date_str = update.date.map(|d| d.to_string());
                Ok(serde_json::json!({
                    "available": true,
                    "version": update.version,
                    "currentVersion": update.current_version,
                    "date": date_str,
                    "body": update.body,
                }))
            }
            Ok(None) => Ok(serde_json::json!({
                "available": false,
                "currentVersion": pkg,
            })),
            Err(e) => Ok(serde_json::json!({
                "available": false,
                "currentVersion": pkg,
                "checkError": e.to_string(),
            })),
        },
        Err(e) => Ok(serde_json::json!({
            "available": false,
            "currentVersion": pkg,
            "checkError": e.to_string(),
        })),
    }
}

#[tauri::command]
async fn app_quit_and_install(app: AppHandle) -> ApiResult<Value> {
    let updater = app.updater().map_err(|e| ApiError {
        code: "UPDATER_CONFIG",
        message: e.to_string(),
    })?;
    let Some(update) = updater.check().await.map_err(|e| ApiError {
        code: "UPDATER_CHECK",
        message: e.to_string(),
    })?
    else {
        return Err(ApiError {
            code: "NO_UPDATE",
            message: "Nenhuma atualização disponível para instalar.".into(),
        });
    };
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|e| ApiError {
            code: "UPDATER_INSTALL",
            message: e.to_string(),
        })?;
    Ok(serde_json::json!({ "success": true }))
}

/// Corpo de notificação nativa: SO costuma truncar; evita payloads enormes no IPC.
fn truncate_notification_body(s: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for ch in s.chars().take(max_chars) {
        out.push(ch);
    }
    if s.chars().count() > max_chars {
        out.push('…');
    }
    out
}

fn stable_notification_id_u32(notification_id: &str) -> u32 {
    let mut hasher = DefaultHasher::new();
    notification_id.hash(&mut hasher);
    let raw = hasher.finish() as u32;
    if raw == 0 {
        1
    } else {
        raw
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn emit_notification_action(app: &AppHandle, event: DesktopNotificationActionEvent) {
    let _ = app.emit("notification-action", event);
}

#[tauri::command]
fn app_show_notification(_app: AppHandle, payload: Value) -> ApiResult<Value> {
    // Front-end: `invoke(..., { payload: { title, body } })` → este `payload` é o objeto interno.
    let inner = payload.get("payload").cloned().unwrap_or(payload);

    let request: DesktopNotificationRequest = serde_json::from_value(inner).unwrap_or_default();
    let title = request
        .title
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("Dev Command Center");
    let body = request
        .body
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|b| truncate_notification_body(b, 1800));
    let notification_id = request
        .notification_id
        .filter(|id| !id.trim().is_empty())
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let _has_actions = !request.actions.is_empty();

    let mut notification = DesktopNotification::new();
    notification.summary(title);
    if let Some(body) = body.as_deref() {
        notification.body(body);
    }
    match request
        .icon
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(icon) if icon.eq_ignore_ascii_case("auto") => {
            notification.auto_icon();
        }
        Some(icon) => {
            notification.icon(icon);
        }
        None => {
            notification.auto_icon();
        }
    }

    if let Some(sound) = request.sound.as_ref() {
        match sound {
            DesktopNotificationSoundToml::Enabled(true) => {
                notification.sound_name("default");
            }
            DesktopNotificationSoundToml::Named(name) if !name.trim().is_empty() => {
                notification.sound_name(name.trim());
            }
            _ => {}
        }
    }

    notification.id(stable_notification_id_u32(&notification_id));
    for action in &request.actions {
        let action_id = action.id.trim();
        let action_label = action.label.trim();
        if !action_id.is_empty() && !action_label.is_empty() {
            notification.action(action_id, action_label);
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let handle: DesktopNotificationHandle = match notification.show() {
            Ok(handle) => handle,
            Err(e) => {
                return Ok(serde_json::json!({
                    "ok": false,
                    "reason": "show_failed",
                    "message": e.to_string()
                }));
            }
        };

        if _has_actions {
            let app_handle = _app.clone();
            let notification_id = notification_id.clone();
            let title = title.to_string();
            let body = body.clone();
            let source = request.source.clone();
            let pane_id = request.pane_id.clone();
            let comb_id = request.comb_id.clone();
            let project_id = request.project_id.clone();
            thread::spawn(move || {
                handle.wait_for_action(|action| {
                    let action_id = if action == "__closed" {
                        "dismiss".to_string()
                    } else {
                        action.to_string()
                    };
                    emit_notification_action(
                        &app_handle,
                        DesktopNotificationActionEvent {
                            notification_id: notification_id.clone(),
                            action_id,
                            title: title.clone(),
                            source: source.clone(),
                            pane_id: pane_id.clone(),
                            comb_id: comb_id.clone(),
                            project_id: project_id.clone(),
                            body: body.clone(),
                        },
                    );
                });
            });
        }

        return Ok(serde_json::json!({
            "ok": true,
            "notificationId": notification_id
        }));
    }

    #[cfg(target_os = "macos")]
    {
        if let Err(e) = notification.show() {
            return Ok(serde_json::json!({
                "ok": false,
                "reason": "show_failed",
                "message": e.to_string()
            }));
        }
        return Ok(serde_json::json!({
            "ok": true,
            "notificationId": notification_id
        }));
    }

    #[cfg(target_os = "windows")]
    {
        if let Err(e) = notification.show() {
            return Ok(serde_json::json!({
                "ok": false,
                "reason": "show_failed",
                "message": e.to_string()
            }));
        }
        return Ok(serde_json::json!({
            "ok": true,
            "notificationId": notification_id
        }));
    }
}

// ---------- Dialog / Shell / Window ----------
#[tauri::command]
async fn dialog_select_directory(app: AppHandle) -> ApiResult<Value> {
    // Não usar `blocking_pick_*`: bloqueia o loop de eventos e o painel nativo fica inutilizável.
    let (tx, rx) = std::sync::mpsc::channel::<Option<FilePath>>();
    app.dialog()
        .file()
        .set_title("Selecionar pasta do repositório")
        .pick_folder(move |folder| {
            let _ = tx.send(folder);
        });
    let folder = tauri::async_runtime::spawn_blocking(move || match rx.recv() {
        Ok(path) => path,
        Err(_) => None,
    })
    .await
    .map_err(|e| db_error(e.to_string()))?;
    Ok(match folder {
        Some(path) => Value::String(path.to_string()),
        None => Value::Null,
    })
}

#[tauri::command]
async fn dialog_show_message(app: AppHandle, options: Value) -> ApiResult<Value> {
    let inner = options;
    let title = inner
        .get("title")
        .and_then(|t| t.as_str())
        .unwrap_or("Dev Command Center")
        .to_string();
    let message_text = inner
        .get("message")
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .to_string();
    let detail = inner.get("detail").and_then(|t| t.as_str()).unwrap_or("");
    let body = if detail.is_empty() {
        message_text
    } else {
        format!("{message_text}\n\n{detail}")
    };
    let kind = match inner.get("type").and_then(|t| t.as_str()) {
        Some("warning") => MessageDialogKind::Warning,
        Some("error") => MessageDialogKind::Error,
        _ => MessageDialogKind::Info,
    };
    let button_labels: Vec<String> = inner
        .get("buttons")
        .and_then(|b| b.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    let buttons = match button_labels.len() {
        0 => MessageDialogButtons::Ok,
        1 => MessageDialogButtons::OkCustom(button_labels[0].clone()),
        2 => {
            MessageDialogButtons::OkCancelCustom(button_labels[0].clone(), button_labels[1].clone())
        }
        _ => MessageDialogButtons::YesNoCancelCustom(
            button_labels[0].clone(),
            button_labels[1].clone(),
            button_labels[2].clone(),
        ),
    };

    let app_handle = app.clone();
    let labels_for_index = button_labels.clone();
    let idx = tauri::async_runtime::spawn_blocking(move || {
        let dialog_builder = app_handle
            .dialog()
            .message(body)
            .title(title)
            .kind(kind)
            .buttons(buttons);
        let dialog_builder = if let Some(w) = app_handle.get_webview_window("main") {
            dialog_builder.parent(&w)
        } else {
            dialog_builder
        };
        let result = dialog_builder.blocking_show_with_result();
        dialog_result_to_button_index(result, &labels_for_index)
    })
    .await
    .map_err(|e| db_error(e.to_string()))?;

    Ok(Value::Number(idx.into()))
}

#[tauri::command]
async fn dialog_confirm(app: AppHandle, message: String) -> ApiResult<bool> {
    let app_handle = app.clone();
    let ok = tauri::async_runtime::spawn_blocking(move || {
        let dialog_builder = app_handle
            .dialog()
            .message(message)
            .buttons(MessageDialogButtons::OkCancel);
        let dialog_builder = if let Some(w) = app_handle.get_webview_window("main") {
            dialog_builder.parent(&w)
        } else {
            dialog_builder
        };
        dialog_builder.blocking_show()
    })
    .await
    .map_err(|e| db_error(e.to_string()))?;
    Ok(ok)
}

// ---------- CLI path (PATH ampliado: apps GUI no macOS costumam ter PATH mínimo) ----------
fn home_dir_path() -> Option<PathBuf> {
    if let Ok(h) = std::env::var("HOME") {
        if !h.is_empty() {
            return Some(PathBuf::from(h));
        }
    }
    #[cfg(windows)]
    if let Ok(h) = std::env::var("USERPROFILE") {
        if !h.is_empty() {
            return Some(PathBuf::from(h));
        }
    }
    None
}

fn expand_user_path(s: &str) -> PathBuf {
    if s == "~" {
        if let Some(h) = home_dir_path() {
            return h;
        }
    }
    if let Some(rest) = s.strip_prefix("~/") {
        if let Some(h) = home_dir_path() {
            return h.join(rest);
        }
    }
    PathBuf::from(s)
}

fn path_looks_like_filesystem_path(s: &str) -> bool {
    s.contains('/')
        || s.contains('\\')
        || (cfg!(windows) && s.len() >= 2 && s.as_bytes()[1] == b':')
}

fn path_search_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut push = |p: PathBuf| {
        let key = p.to_string_lossy().to_string();
        if seen.insert(key) {
            dirs.push(p);
        }
    };
    if let Some(h) = home_dir_path() {
        push(h.join(".local").join("bin"));
        push(h.join(".cargo").join("bin"));
    }
    #[cfg(target_os = "macos")]
    {
        push(PathBuf::from("/opt/homebrew/bin"));
        push(PathBuf::from("/usr/local/bin"));
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        push(PathBuf::from("/usr/local/bin"));
    }
    push(PathBuf::from("/usr/bin"));
    push(PathBuf::from("/bin"));
    let sep = if cfg!(windows) { ';' } else { ':' };
    if let Ok(p) = std::env::var("PATH") {
        for part in p.split(sep) {
            if !part.is_empty() {
                push(PathBuf::from(part));
            }
        }
    }
    dirs
}

#[cfg(unix)]
fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    match std::fs::metadata(path) {
        Ok(m) => m.is_file() && m.permissions().mode() & 0o111 != 0,
        Err(_) => false,
    }
}

#[cfg(windows)]
fn is_executable_file(path: &Path) -> bool {
    path.is_file()
}

fn search_in_augmented_path(command: &str) -> Option<PathBuf> {
    #[cfg(windows)]
    let exts: &[&str] = &[".exe", ".cmd", ".bat", ".EXE", ""];
    #[cfg(not(windows))]
    let exts: &[&str] = &[""];

    for dir in path_search_dirs() {
        for ext in exts {
            let name = if ext.is_empty() {
                command.to_string()
            } else {
                format!("{command}{ext}")
            };
            let full = dir.join(&name);
            if full.is_file() && is_executable_file(&full) {
                return std::fs::canonicalize(&full).ok().or(Some(full));
            }
        }
    }
    None
}

fn resolve_cli_path_impl(command: &str) -> Option<PathBuf> {
    let command = command.trim();
    if command.is_empty() {
        return None;
    }
    if path_looks_like_filesystem_path(command) {
        let probe = expand_user_path(command);
        if probe.exists() && probe.is_file() {
            let canon = std::fs::canonicalize(&probe).unwrap_or(probe);
            if is_executable_file(&canon) {
                return Some(canon);
            }
        }
        return None;
    }
    search_in_augmented_path(command)
}

fn provider_cli_command(provider_type: &str) -> Option<&'static str> {
    match provider_type {
        "claude-code" => Some("claude"),
        "codex" => Some("codex"),
        "cursor" => Some("agent"),
        "gemini" => Some("gemini"),
        // Forge (secção 6): deteção opcional na UI / integrações
        "gh" | "github-cli" | "github_forge" => Some("gh"),
        "glab" | "gitlab-cli" | "gitlab_forge" => Some("glab"),
        _ => None,
    }
}

fn try_cli_invocation(path: &Path) -> bool {
    for args in [
        &["--version"][..],
        &["-V"][..],
        &["-v"][..],
        &["--help"][..],
    ] {
        if let Ok(out) = Command::new(path).args(args).output() {
            if out.status.success() {
                return true;
            }
        }
    }
    false
}

/// Captura o PATH e variáveis essenciais do shell de login do usuário e aplica ao processo atual.
/// Resolve o problema clássico de apps macOS GUI lançados pelo Finder/Dock receberem
/// um PATH mínimo do launchd, sem nvm/fnm/volta/homebrew.
#[cfg(not(windows))]
fn bootstrap_shell_env() {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    let (tx, rx) = std::sync::mpsc::channel();
    let shell_clone = shell.clone();
    thread::spawn(move || {
        let result = Command::new(&shell_clone).args(["-ilc", "env"]).output();
        let _ = tx.send(result);
    });

    let output = match rx.recv_timeout(Duration::from_secs(5)) {
        Ok(Ok(out)) => out,
        _ => {
            eprintln!("[DCC] bootstrap_shell_env: timeout ou erro ao capturar env do shell");
            return;
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut new_path: Option<String> = None;
    let keys_to_apply = [
        "PATH",
        "NVM_DIR",
        "NVM_BIN",
        "FNM_DIR",
        "VOLTA_HOME",
        "PNPM_HOME",
    ];

    for line in stdout.lines() {
        if let Some(idx) = line.find('=') {
            let key = &line[..idx];
            let val = &line[idx + 1..];
            if key == "PATH" {
                new_path = Some(val.to_string());
            } else if keys_to_apply.contains(&key) {
                std::env::set_var(key, val);
            }
        }
    }

    // PATH sempre é substituído pelo do shell para garantir acesso a node/yarn/etc.
    if let Some(path) = new_path {
        if !path.is_empty() {
            std::env::set_var("PATH", &path);
            eprintln!(
                "[DCC] PATH atualizado via shell de login: {}",
                &path[..path.len().min(120)]
            );
        }
    }
}

#[tauri::command]
fn shell_get_default() -> Value {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| {
        if cfg!(windows) {
            "powershell".to_string()
        } else {
            "/bin/zsh".to_string()
        }
    });
    serde_json::json!({ "shell": shell })
}

#[tauri::command]
fn shell_open_external(url: String) -> ApiResult<Value> {
    use std::process::Command;

    #[cfg(target_os = "macos")]
    let status = Command::new("open").arg(&url).status();

    #[cfg(target_os = "linux")]
    let status = Command::new("xdg-open").arg(&url).status();

    #[cfg(target_os = "windows")]
    let status = Command::new("cmd").args(["/C", "start", "", &url]).status();

    match status {
        Ok(s) if s.success() => Ok(serde_json::json!({ "success": true })),
        Ok(_) => Err(ApiError {
            code: "SHELL_ERROR",
            message: "Failed to open URL".into(),
        }),
        Err(e) => Err(ApiError {
            code: "SHELL_ERROR",
            message: e.to_string(),
        }),
    }
}

#[tauri::command]
fn shell_open_path(path: String) -> ApiResult<Value> {
    use std::process::Command;

    let expanded = expand_user_path(path.trim());

    if !expanded.exists() {
        return Err(ApiError {
            code: "PATH_NOT_FOUND",
            message: format!("Path not found: {}", expanded.display()),
        });
    }

    #[cfg(target_os = "macos")]
    let status = Command::new("open").arg(&expanded).status();

    #[cfg(target_os = "linux")]
    let status = Command::new("xdg-open").arg(&expanded).status();

    #[cfg(target_os = "windows")]
    let status = Command::new("explorer").arg(&expanded).status();

    match status {
        Ok(s) if s.success() => Ok(serde_json::json!({ "success": true })),
        Ok(_) => Err(ApiError {
            code: "SHELL_ERROR",
            message: format!("Failed to open path: {}", expanded.display()),
        }),
        Err(e) => Err(ApiError {
            code: "SHELL_ERROR",
            message: e.to_string(),
        }),
    }
}

#[tauri::command]
fn shell_open_in_editor(path: String, editor: String) -> ApiResult<Value> {
    use std::process::Command;

    let expanded = expand_user_path(path.trim());
    if !expanded.exists() {
        return Err(ApiError {
            code: "PATH_NOT_FOUND",
            message: format!("Path not found: {}", expanded.display()),
        });
    }

    let editor = editor.trim();
    if editor.is_empty() {
        return Err(ApiError {
            code: "SHELL_ERROR",
            message: "Editor is required".into(),
        });
    }

    #[cfg(target_os = "macos")]
    let status = {
        let app_name = match editor {
            "cursor" => "Cursor",
            "vscode" => "Visual Studio Code",
            "vscode-insiders" => "Visual Studio Code - Insiders",
            "zed" => "Zed",
            "trae" => "Trae",
            other => other,
        };
        Command::new("open")
            .arg("-a")
            .arg(app_name)
            .arg(&expanded)
            .status()
    };

    #[cfg(target_os = "linux")]
    let status = {
        let binary = match editor {
            "cursor" => "cursor",
            "vscode" => "code",
            "vscode-insiders" => "code-insiders",
            "zed" => "zed",
            "trae" => "trae",
            other => other,
        };
        Command::new(binary).arg(&expanded).status()
    };

    #[cfg(target_os = "windows")]
    let status = {
        let binary = match editor {
            "cursor" => "cursor",
            "vscode" => "code",
            "vscode-insiders" => "code-insiders",
            "zed" => "zed",
            "trae" => "trae",
            other => other,
        };
        Command::new(binary).arg(&expanded).status()
    };

    match status {
        Ok(s) if s.success() => Ok(serde_json::json!({ "success": true })),
        Ok(_) => Err(ApiError {
            code: "SHELL_ERROR",
            message: format!("Failed to open {} in {}", expanded.display(), editor),
        }),
        Err(e) => Err(ApiError {
            code: "SHELL_ERROR",
            message: e.to_string(),
        }),
    }
}

#[tauri::command]
fn shell_show_item_in_folder(path: String) -> ApiResult<Value> {
    use std::process::Command;

    let expanded = expand_user_path(path.trim());

    if !expanded.exists() {
        return Err(ApiError {
            code: "PATH_NOT_FOUND",
            message: format!("Path not found: {}", expanded.display()),
        });
    }

    #[cfg(target_os = "macos")]
    let status = Command::new("open")
        .args(["-R", &expanded.to_string_lossy()])
        .status();

    #[cfg(target_os = "linux")]
    let status = {
        // Try dbus for better file manager integration
        let dbus_result = Command::new("dbus-send")
            .args([
                "--session",
                "--dest=org.freedesktop.FileManager1",
                "--type=method_call",
                "/org/freedesktop/FileManager1",
                "org.freedesktop.FileManager1.ShowItems",
                &format!("array:string:file://{}", expanded.display()),
                "string:",
            ])
            .status();

        // Fallback to xdg-open on parent folder if dbus fails
        if dbus_result.is_err() || !dbus_result.as_ref().unwrap().success() {
            let parent = expanded.parent().unwrap_or(&expanded);
            Command::new("xdg-open").arg(parent).status()
        } else {
            dbus_result
        }
    };

    #[cfg(target_os = "windows")]
    let status = Command::new("explorer")
        .args(["/select,", &expanded.to_string_lossy()])
        .status();

    match status {
        Ok(s) if s.success() => Ok(serde_json::json!({ "success": true })),
        Ok(_) => Err(ApiError {
            code: "SHELL_ERROR",
            message: format!("Failed to show item in folder: {}", expanded.display()),
        }),
        Err(e) => Err(ApiError {
            code: "SHELL_ERROR",
            message: e.to_string(),
        }),
    }
}

#[tauri::command]
fn shell_resolve_cli_path(command: String) -> ApiResult<Value> {
    let path = resolve_cli_path_impl(&command);
    let path_str = path.as_ref().map(|p| p.to_string_lossy().to_string());
    Ok(serde_json::json!({ "path": path_str }))
}

#[tauri::command]
fn shell_detect_cli_for_provider(provider_type: String) -> ApiResult<Value> {
    let Some(cmd) = provider_cli_command(provider_type.trim()) else {
        return Ok(serde_json::json!({ "path": null }));
    };
    let path = resolve_cli_path_impl(cmd);
    let path_str = path.as_ref().map(|p| p.to_string_lossy().to_string());
    Ok(serde_json::json!({ "path": path_str }))
}

#[tauri::command]
fn shell_validate_cli_path(_provider_type: String, cli_path: String) -> ApiResult<Value> {
    let trimmed = cli_path.trim();
    if trimmed.is_empty() {
        return Ok(serde_json::json!({
            "valid": false,
            "message": "Caminho vazio"
        }));
    }
    let candidate = resolve_cli_path_impl(trimmed).unwrap_or_else(|| expand_user_path(trimmed));
    if !candidate.exists() {
        return Ok(serde_json::json!({
            "valid": false,
            "message": "Executável não encontrado"
        }));
    }
    if !candidate.is_file() || !is_executable_file(&candidate) {
        return Ok(serde_json::json!({
            "valid": false,
            "message": "Não é um executável válido"
        }));
    }
    if try_cli_invocation(&candidate) {
        return Ok(serde_json::json!({ "valid": true }));
    }
    Ok(serde_json::json!({
        "valid": false,
        "message": "Executável encontrado, mas não respondeu a --version / --help"
    }))
}

#[tauri::command]
fn shell_open_terminal_at_path(
    dir_path: String,
    _suggested_command: Option<Value>,
) -> ApiResult<Value> {
    let path = expand_user_path(dir_path.trim());
    if !path.exists() || !path.is_dir() {
        return Ok(serde_json::json!({
            "success": false,
            "error": format!("Diretório inválido: {}", path.display())
        }));
    }

    #[cfg(target_os = "macos")]
    {
        let status = Command::new("open")
            .arg("-a")
            .arg("Terminal")
            .arg(&path)
            .status()
            .map_err(|e| ApiError {
                code: "SHELL_ERROR",
                message: format!("Falha ao abrir Terminal.app: {e}"),
            })?;

        if status.success() {
            return Ok(serde_json::json!({ "success": true }));
        }
        return Ok(serde_json::json!({
            "success": false,
            "error": "Terminal.app retornou erro ao abrir o diretório"
        }));
    }

    #[cfg(target_os = "linux")]
    {
        let attempts: &[(&str, &[&str])] = &[
            ("x-terminal-emulator", &["--working-directory"]),
            ("gnome-terminal", &["--working-directory"]),
            ("konsole", &["--workdir"]),
            ("xfce4-terminal", &["--working-directory"]),
        ];
        for (bin, flag) in attempts {
            let status = Command::new(bin).arg(flag[0]).arg(&path).status();
            if let Ok(s) = status {
                if s.success() {
                    return Ok(serde_json::json!({ "success": true }));
                }
            }
        }
        return Ok(serde_json::json!({
            "success": false,
            "error": "Nenhum terminal suportado encontrado no Linux"
        }));
    }

    #[cfg(target_os = "windows")]
    {
        let status = Command::new("cmd")
            .args(["/C", "start", "wt", "-d"])
            .arg(&path)
            .status()
            .map_err(|e| ApiError {
                code: "SHELL_ERROR",
                message: format!("Falha ao abrir Windows Terminal: {e}"),
            })?;
        if status.success() {
            return Ok(serde_json::json!({ "success": true }));
        }
        return Ok(serde_json::json!({
            "success": false,
            "error": "Windows Terminal retornou erro ao abrir o diretório"
        }));
    }

    #[allow(unreachable_code)]
    Ok(serde_json::json!({
        "success": false,
        "error": "Plataforma não suportada para abertura de terminal"
    }))
}

#[tauri::command]
async fn terminal_save_temp_image(image_data: Vec<u8>, extension: String) -> ApiResult<Value> {
    use uuid::Uuid;

    let temp_dir = std::env::temp_dir();
    let filename = format!("dcc_img_{}.{}", Uuid::new_v4(), extension);
    let path = temp_dir.join(&filename);

    std::fs::write(&path, image_data).map_err(|e| ApiError {
        code: "FS_ERROR",
        message: format!("Failed to save temp image: {}", e),
    })?;

    Ok(serde_json::json!({
        "path": path.to_string_lossy().to_string(),
        "filename": filename
    }))
}

#[tauri::command]
fn window_minimize(app: AppHandle) -> ApiResult<Value> {
    let window = app.get_webview_window("main").ok_or_else(|| ApiError {
        code: "WINDOW_NOT_FOUND",
        message: "Main window not found".into(),
    })?;

    window.minimize().map_err(|e| ApiError {
        code: "WINDOW_ERROR",
        message: e.to_string(),
    })?;

    Ok(serde_json::json!({ "success": true }))
}

#[tauri::command]
fn window_maximize(app: AppHandle) -> ApiResult<Value> {
    let window = app.get_webview_window("main").ok_or_else(|| ApiError {
        code: "WINDOW_NOT_FOUND",
        message: "Main window not found".into(),
    })?;

    // Toggle maximize/unmaximize
    if window.is_maximized().unwrap_or(false) {
        window.unmaximize().map_err(|e| ApiError {
            code: "WINDOW_ERROR",
            message: e.to_string(),
        })?;
    } else {
        window.maximize().map_err(|e| ApiError {
            code: "WINDOW_ERROR",
            message: e.to_string(),
        })?;
    }

    Ok(serde_json::json!({ "success": true }))
}

#[tauri::command]
fn window_close(app: AppHandle) -> ApiResult<Value> {
    let window = app.get_webview_window("main").ok_or_else(|| ApiError {
        code: "WINDOW_NOT_FOUND",
        message: "Main window not found".into(),
    })?;

    window.close().map_err(|e| ApiError {
        code: "WINDOW_ERROR",
        message: e.to_string(),
    })?;

    Ok(serde_json::json!({ "success": true }))
}

#[tauri::command]
fn window_focus(app: AppHandle) -> ApiResult<Value> {
    let window = app.get_webview_window("main").ok_or_else(|| ApiError {
        code: "WINDOW_NOT_FOUND",
        message: "Main window not found".into(),
    })?;

    window.show().map_err(|e| ApiError {
        code: "WINDOW_ERROR",
        message: e.to_string(),
    })?;
    let _ = window.unminimize();
    window.set_focus().map_err(|e| ApiError {
        code: "WINDOW_ERROR",
        message: e.to_string(),
    })?;

    Ok(serde_json::json!({ "success": true }))
}

#[tauri::command]
fn window_is_maximized(app: AppHandle) -> ApiResult<bool> {
    let window = app.get_webview_window("main").ok_or_else(|| ApiError {
        code: "WINDOW_NOT_FOUND",
        message: "Main window not found".into(),
    })?;
    window.is_maximized().map_err(|e| ApiError {
        code: "WINDOW_ERROR",
        message: e.to_string(),
    })
}

// ---------- License ----------
#[tauri::command]
fn license_get_status(state: State<'_, AppState>) -> ApiResult<Value> {
    let conn = state
        .conn
        .lock()
        .map_err(|_| db_error("db lock poisoned"))?;
    let row: Option<(String, i64, Option<String>)> = conn
        .query_row(
            "SELECT email, activated, activated_at FROM activation WHERE id = 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .map_err(|e| db_error(e.to_string()))?;
    Ok(match row {
        Some((email, activated, activated_at)) if activated != 0 => serde_json::json!({
            "activated": true,
            "email": email,
            "activatedAt": activated_at,
            "tier": "beta"
        }),
        Some((email, _, _)) => serde_json::json!({
            "activated": false,
            "email": email
        }),
        None => serde_json::json!({
            "activated": false,
            "tier": "dev"
        }),
    })
}

#[tauri::command]
fn license_get_machine_id(state: State<'_, AppState>) -> ApiResult<String> {
    Ok(compute_stable_machine_id(state.app_data_dir.as_ref()))
}

#[tauri::command]
async fn license_activate(state: State<'_, AppState>, email: String) -> ApiResult<Value> {
    let email = email.trim().to_string();
    if email.is_empty() {
        return Ok(serde_json::json!({
            "success": false,
            "message": "Indique um e-mail válido."
        }));
    }
    let machine_id = compute_stable_machine_id(state.app_data_dir.as_ref());
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(25))
        .build()
        .map_err(|e| db_error(e.to_string()))?;
    let response = client
        .post("https://www.devcommandcenter.com/api/beta-activate")
        .json(&serde_json::json!({ "email": email, "machineId": machine_id }))
        .send()
        .await
        .map_err(|e| ApiError {
            code: "NETWORK_ERROR",
            message: e.to_string(),
        })?;
    let status = response.status();
    let body_text = response.text().await.map_err(|e| db_error(e.to_string()))?;
    let parsed: Value = serde_json::from_str(&body_text).unwrap_or_else(|_| serde_json::json!({}));
    let ok_flag = parsed.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
    let server_msg = parsed
        .get("message")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let token = parsed
        .get("token")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    if !status.is_success() {
        return Ok(serde_json::json!({
            "success": false,
            "message": server_msg.unwrap_or_else(|| format!("Servidor respondeu HTTP {}.", status.as_u16()))
        }));
    }
    if !ok_flag {
        return Ok(serde_json::json!({
            "success": false,
            "message": server_msg.unwrap_or_else(|| "Activation refused.".into())
        }));
    }

    let now = local_now_string();
    {
        let conn = state
            .conn
            .lock()
            .map_err(|_| db_error("db lock poisoned"))?;
        conn.execute(
            "INSERT INTO activation (id, email, machine_id, activated, token, activated_at, created_at, updated_at)
             VALUES (1, ?1, ?2, 1, ?3, ?4, ?4, ?4)
             ON CONFLICT(id) DO UPDATE SET
               email = excluded.email,
               machine_id = excluded.machine_id,
               activated = 1,
               token = excluded.token,
               activated_at = excluded.activated_at,
               updated_at = excluded.updated_at",
            params![email, machine_id, token, now],
        )
        .map_err(|e| db_error(e.to_string()))?;
    }

    Ok(serde_json::json!({ "success": true, "message": server_msg }))
}

#[tauri::command]
fn license_skip_activation(state: State<'_, AppState>) -> ApiResult<Value> {
    if !cfg!(debug_assertions) {
        return Err(ApiError {
            code: "SKIP_FORBIDDEN",
            message: "Pular ativação só está disponível em builds de desenvolvimento.".into(),
        });
    }
    let machine_id = compute_stable_machine_id(state.app_data_dir.as_ref());
    let now = local_now_string();
    let conn = state
        .conn
        .lock()
        .map_err(|_| db_error("db lock poisoned"))?;
    conn.execute(
        "INSERT INTO activation (id, email, machine_id, activated, token, activated_at, created_at, updated_at)
         VALUES (1, 'dev@local', ?1, 1, NULL, ?2, ?2, ?2)
         ON CONFLICT(id) DO UPDATE SET
           email = 'dev@local',
           machine_id = excluded.machine_id,
           activated = 1,
           updated_at = excluded.updated_at",
        params![machine_id, now],
    )
    .map_err(|e| db_error(e.to_string()))?;
    Ok(serde_json::json!({ "success": true }))
}

// ---------- Providers / Projects / Missions / Logs / Combs / Panes ----------
#[tauri::command]
fn db_providers_find_all(state: State<'_, AppState>) -> ApiResult<Value> {
    let conn = state
        .conn
        .lock()
        .map_err(|_| db_error("db lock poisoned"))?;
    let mut stmt = conn
        .prepare(&format!(
            "{PROVIDER_SELECT_SQL} ORDER BY name ASC LIMIT 100 OFFSET 0"
        ))
        .map_err(|e| db_error(e.to_string()))?;
    let rows = stmt
        .query_map([], provider_from_row)
        .map_err(|e| db_error(e.to_string()))?;
    let items: Result<Vec<_>, _> = rows.collect();
    Ok(Value::Array(items.map_err(|e| db_error(e.to_string()))?))
}

#[tauri::command]
fn db_providers_find_by_id(state: State<'_, AppState>, id: String) -> ApiResult<Value> {
    let conn = state
        .conn
        .lock()
        .map_err(|_| db_error("db lock poisoned"))?;
    let mut stmt = conn
        .prepare(&format!("{PROVIDER_SELECT_SQL} WHERE id = ?1"))
        .map_err(|e| db_error(e.to_string()))?;
    let row = stmt
        .query_row(params![id], provider_from_row)
        .optional()
        .map_err(|e| db_error(e.to_string()))?;
    Ok(row.unwrap_or(Value::Null))
}

#[tauri::command]
fn db_providers_find_by_type(state: State<'_, AppState>, kind: String) -> ApiResult<Value> {
    let conn = state
        .conn
        .lock()
        .map_err(|_| db_error("db lock poisoned"))?;
    let mut stmt = conn
        .prepare(&format!(
            "{PROVIDER_SELECT_SQL} WHERE type = ?1 ORDER BY name ASC"
        ))
        .map_err(|e| db_error(e.to_string()))?;
    let rows = stmt
        .query_map(params![kind], provider_from_row)
        .map_err(|e| db_error(e.to_string()))?;
    let items: Result<Vec<_>, _> = rows.collect();
    Ok(Value::Array(items.map_err(|e| db_error(e.to_string()))?))
}

#[tauri::command]
fn db_providers_find_active(state: State<'_, AppState>) -> ApiResult<Value> {
    let conn = state
        .conn
        .lock()
        .map_err(|_| db_error("db lock poisoned"))?;
    let mut stmt = conn
        .prepare(&format!(
            "{PROVIDER_SELECT_SQL} WHERE is_active = 1 ORDER BY name ASC"
        ))
        .map_err(|e| db_error(e.to_string()))?;
    let rows = stmt
        .query_map([], provider_from_row)
        .map_err(|e| db_error(e.to_string()))?;
    let items: Result<Vec<_>, _> = rows.collect();
    Ok(Value::Array(items.map_err(|e| db_error(e.to_string()))?))
}

#[tauri::command]
fn db_providers_create(state: State<'_, AppState>, data: Value) -> ApiResult<Value> {
    let obj = data
        .as_object()
        .ok_or_else(|| db_error("invalid provider payload"))?;
    let id = Uuid::new_v4().to_string();
    let name = obj
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| db_error("name is required"))?;
    let typ = obj
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| db_error("type is required"))?;
    let api_key = obj
        .get("apiKey")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty());
    let cli_path = obj
        .get("cliPath")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty());
    let config_str: Option<String> = match obj.get("config") {
        None | Some(Value::Null) => None,
        Some(v) => Some(serde_json::to_string(v).map_err(|e| db_error(e.to_string()))?),
    };
    let is_active = obj.get("isActive").and_then(Value::as_bool).unwrap_or(true);
    let is_active_i: i64 = if is_active { 1 } else { 0 };

    {
        let conn = state
            .conn
            .lock()
            .map_err(|_| db_error("db lock poisoned"))?;
        conn.execute(
            "INSERT INTO providers (id, name, type, api_key, api_key_encrypted, cli_path, config, is_active)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                id,
                name,
                typ,
                api_key,
                None::<Vec<u8>>,
                cli_path,
                config_str,
                is_active_i
            ],
        )
        .map_err(|e| db_error(e.to_string()))?;
    }
    db_providers_find_by_id(state, id)
}

#[tauri::command]
fn db_providers_update(state: State<'_, AppState>, id: String, data: Value) -> ApiResult<Value> {
    use rusqlite::types::Value as SqlValue;

    let obj = data
        .as_object()
        .ok_or_else(|| db_error("invalid provider payload"))?;
    let mut sets: Vec<String> = Vec::new();
    let mut bind: Vec<SqlValue> = Vec::new();

    if let Some(name) = obj.get("name").and_then(Value::as_str) {
        sets.push("name = ?".into());
        bind.push(SqlValue::Text(name.to_string()));
    }
    if let Some(typ) = obj.get("type").and_then(Value::as_str) {
        sets.push("type = ?".into());
        bind.push(SqlValue::Text(typ.to_string()));
    }
    if obj.contains_key("apiKey") {
        sets.push("api_key = ?".into());
        let plain = obj.get("apiKey").and_then(|v| {
            if v.is_null() {
                return None;
            }
            v.as_str().filter(|s| !s.is_empty()).map(|s| s.to_string())
        });
        bind.push(match plain {
            Some(s) => SqlValue::Text(s),
            None => SqlValue::Null,
        });
        sets.push("api_key_encrypted = ?".into());
        bind.push(SqlValue::Null);
    }
    if obj.contains_key("cliPath") {
        sets.push("cli_path = ?".into());
        let v = obj.get("cliPath").and_then(|v| {
            if v.is_null() {
                return None;
            }
            v.as_str().map(|s| s.to_string())
        });
        bind.push(match v {
            Some(s) if !s.is_empty() => SqlValue::Text(s),
            _ => SqlValue::Null,
        });
    }
    if obj.contains_key("config") {
        sets.push("config = ?".into());
        let cfg = match obj.get("config") {
            None | Some(Value::Null) => SqlValue::Null,
            Some(v) => {
                let s = serde_json::to_string(v).map_err(|e| db_error(e.to_string()))?;
                SqlValue::Text(s)
            }
        };
        bind.push(cfg);
    }
    if let Some(ia) = obj.get("isActive").and_then(Value::as_bool) {
        sets.push("is_active = ?".into());
        bind.push(SqlValue::Integer(if ia { 1 } else { 0 }));
    }

    if sets.is_empty() {
        return db_providers_find_by_id(state, id);
    }

    bind.push(SqlValue::Text(id.clone()));
    let sql = format!("UPDATE providers SET {} WHERE id = ?", sets.join(", "));
    {
        let conn = state
            .conn
            .lock()
            .map_err(|_| db_error("db lock poisoned"))?;
        conn.execute(&sql, params_from_iter(bind))
            .map_err(|e| db_error(e.to_string()))?;
    }
    db_providers_find_by_id(state, id)
}

#[tauri::command]
fn db_providers_delete(state: State<'_, AppState>, id: String) -> ApiResult<Value> {
    let conn = state
        .conn
        .lock()
        .map_err(|_| db_error("db lock poisoned"))?;
    let changed = conn
        .execute("DELETE FROM providers WHERE id = ?1", params![id])
        .map_err(|e| db_error(e.to_string()))?;
    Ok(Value::Bool(changed > 0))
}

#[tauri::command]
fn db_providers_set_active(
    state: State<'_, AppState>,
    id: String,
    is_active: bool,
) -> ApiResult<Value> {
    {
        let conn = state
            .conn
            .lock()
            .map_err(|_| db_error("db lock poisoned"))?;
        conn.execute(
            "UPDATE providers SET is_active = ?1 WHERE id = ?2",
            params![if is_active { 1 } else { 0 }, id],
        )
        .map_err(|e| db_error(e.to_string()))?;
    }
    db_providers_find_by_id(state, id)
}

#[tauri::command]
fn db_providers_test_connection(state: State<'_, AppState>, id: String) -> ApiResult<Value> {
    let conn = state
        .conn
        .lock()
        .map_err(|_| db_error("db lock poisoned"))?;
    let mut stmt = conn
        .prepare(&format!("{PROVIDER_SELECT_SQL} WHERE id = ?1"))
        .map_err(|e| db_error(e.to_string()))?;
    let row = stmt
        .query_row(params![id], provider_from_row)
        .optional()
        .map_err(|e| db_error(e.to_string()))?;
    let Some(provider) = row else {
        return Ok(serde_json::json!({
            "success": false,
            "error": "Provider not found"
        }));
    };
    let typ = provider.get("type").and_then(|v| v.as_str()).unwrap_or("");
    let has_key = provider
        .get("hasApiKey")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let cli_path = provider
        .get("cliPath")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty());

    let ok = match typ {
        "openai" | "anthropic" => has_key,
        "claude-code" | "codex" | "cursor" | "gemini" => cli_path.is_some(),
        _ => true,
    };
    if ok {
        Ok(serde_json::json!({ "success": true }))
    } else {
        Ok(serde_json::json!({
            "success": false,
            "error": "Provider not fully configured"
        }))
    }
}

#[tauri::command]
fn db_providers_is_encryption_available() -> ApiResult<bool> {
    Ok(false)
}

/// Fragmento dinâmico `coluna = ?` para UPDATEs (evita closure que captura `sets`/`bind_values`).
#[allow(dead_code)]
fn push_sql_set(
    column: &'static str,
    value: Option<String>,
    sets: &mut Vec<String>,
    bind_values: &mut Vec<String>,
) {
    if let Some(v) = value {
        sets.push(format!("{column} = ?"));
        bind_values.push(v);
    }
}

#[tauri::command]
fn db_projects_find_all(state: State<'_, AppState>) -> ApiResult<Value> {
    let conn = state
        .conn
        .lock()
        .map_err(|_| db_error("db lock poisoned"))?;
    let mut stmt = conn
        .prepare(
            "SELECT * FROM projects
             ORDER BY (last_opened_at IS NULL), last_opened_at DESC
             LIMIT 100 OFFSET 0",
        )
        .map_err(|e| db_error(e.to_string()))?;
    let rows = stmt
        .query_map([], row_to_json)
        .map_err(|e| db_error(e.to_string()))?;
    let items: Result<Vec<_>, _> = rows.collect();
    let mapped = items
        .map_err(|e| db_error(e.to_string()))?
        .into_iter()
        .map(map_project_to_renderer)
        .collect::<Vec<_>>();
    Ok(Value::Array(mapped))
}
#[tauri::command]
fn db_projects_find_by_id(state: State<'_, AppState>, id: String) -> ApiResult<Value> {
    let conn = state
        .conn
        .lock()
        .map_err(|_| db_error("db lock poisoned"))?;
    let mut stmt = conn
        .prepare("SELECT * FROM projects WHERE id = ?1")
        .map_err(|e| db_error(e.to_string()))?;
    let row = stmt
        .query_row(params![id], row_to_json)
        .optional()
        .map_err(|e| db_error(e.to_string()))?;
    Ok(row.map(map_project_to_renderer).unwrap_or(Value::Null))
}
#[tauri::command]
fn db_projects_find_by_path(state: State<'_, AppState>, path: String) -> ApiResult<Value> {
    let conn = state
        .conn
        .lock()
        .map_err(|_| db_error("db lock poisoned"))?;
    let mut stmt = conn
        .prepare("SELECT * FROM projects WHERE path = ?1")
        .map_err(|e| db_error(e.to_string()))?;
    let row = stmt
        .query_row(params![path], row_to_json)
        .optional()
        .map_err(|e| db_error(e.to_string()))?;
    Ok(row.map(map_project_to_renderer).unwrap_or(Value::Null))
}

#[tauri::command]
fn db_projects_get_repo_config_toml(state: State<'_, AppState>, id: String) -> ApiResult<Value> {
    let conn = state
        .conn
        .lock()
        .map_err(|_| db_error("db lock poisoned"))?;
    let mut stmt = conn
        .prepare("SELECT path, repo_config FROM projects WHERE id = ?1")
        .map_err(|e| db_error(e.to_string()))?;
    let row = stmt
        .query_row(params![id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
        })
        .optional()
        .map_err(|e| db_error(e.to_string()))?;
    let Some((project_path, repo_config_raw)) = row else {
        return Ok(serde_json::json!({
            "exists": false,
            "source": "missing",
            "path": null,
            "content": ""
        }));
    };
    let file_path = repo_config_file_path(&project_path);
    if let Some(content) = read_repo_config_text_from_disk(&project_path).map_err(db_error)? {
        return Ok(serde_json::json!({
            "exists": true,
            "source": "disk",
            "path": file_path.to_string_lossy().to_string(),
            "content": content
        }));
    }

    let default_value = default_repo_config_value();
    let has_db_repo_config = repo_config_raw.is_some();

    let content = if let Some(raw) = repo_config_raw {
        if raw.trim().is_empty() {
            repo_config_toml_text_from_value(&default_value).map_err(db_error)?
        } else {
            let value = serde_json::from_str::<Value>(&raw).unwrap_or(Value::Null);
            if value.is_null() {
                repo_config_toml_text_from_value(&default_value).map_err(db_error)?
            } else {
                repo_config_toml_text_from_value(&value).map_err(db_error)?
            }
        }
    } else {
        repo_config_toml_text_from_value(&default_value).map_err(db_error)?
    };

    Ok(serde_json::json!({
        "exists": false,
        "source": if has_db_repo_config { "db" } else { "generated" },
        "path": file_path.to_string_lossy().to_string(),
        "content": content
    }))
}

#[tauri::command]
fn db_projects_save_repo_config_toml(
    state: State<'_, AppState>,
    id: String,
    content: String,
) -> ApiResult<Value> {
    let project_path = {
        let conn = state
            .conn
            .lock()
            .map_err(|_| db_error("db lock poisoned"))?;
        conn.query_row(
            "SELECT path FROM projects WHERE id = ?1",
            params![id.clone()],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|e| db_error(e.to_string()))?
    };
    let Some(project_path) = project_path else {
        return Ok(serde_json::json!({
            "success": false,
            "error": "Project not found"
        }));
    };

    let parsed: RepoConfigToml = toml::from_str(&content).map_err(|e| {
        db_error(format!(
            "{}: {}",
            repo_config_file_path(&project_path).display(),
            e
        ))
    })?;
    let payload = repo_config_payload_from_toml(parsed);
    let normalized = repo_config_value_from_payload(&payload).map_err(db_error)?;

    let file_path = repo_config_file_path(&project_path);
    if let Some(parent) = file_path.parent() {
        fs::create_dir_all(parent).map_err(|e| db_error(format!("{}: {}", parent.display(), e)))?;
    }
    fs::write(&file_path, content)
        .map_err(|e| db_error(format!("{}: {}", file_path.display(), e)))?;

    {
        let conn = state
            .conn
            .lock()
            .map_err(|_| db_error("db lock poisoned"))?;
        conn.execute(
            "UPDATE projects SET repo_config = ?1 WHERE id = ?2",
            params![normalized.to_string(), id],
        )
        .map_err(|e| db_error(e.to_string()))?;
    }

    Ok(serde_json::json!({
        "success": true,
        "path": file_path.to_string_lossy().to_string()
    }))
}

#[tauri::command]
fn db_projects_search(state: State<'_, AppState>, query: String) -> ApiResult<Value> {
    let conn = state
        .conn
        .lock()
        .map_err(|_| db_error("db lock poisoned"))?;
    let term = format!("%{query}%");
    let mut stmt = conn
        .prepare(
            "SELECT * FROM projects
             WHERE name LIKE ?1 OR description LIKE ?1 OR path LIKE ?1
             ORDER BY (last_opened_at IS NULL), last_opened_at DESC
             LIMIT 20",
        )
        .map_err(|e| db_error(e.to_string()))?;
    let rows = stmt
        .query_map(params![term], row_to_json)
        .map_err(|e| db_error(e.to_string()))?;
    let items: Result<Vec<_>, _> = rows.collect();
    Ok(Value::Array(
        items
            .map_err(|e| db_error(e.to_string()))?
            .into_iter()
            .map(map_project_to_renderer)
            .collect(),
    ))
}
#[tauri::command]
fn db_projects_create(state: State<'_, AppState>, data: Value) -> ApiResult<Value> {
    let obj = data
        .as_object()
        .ok_or_else(|| db_error("invalid project payload"))?;
    let id = next_id();
    let name = obj
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| db_error("name is required"))?;
    let path = obj
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| db_error("path is required"))?;
    let description = obj.get("description").and_then(Value::as_str);
    let default_provider_id = obj.get("defaultProviderId").and_then(Value::as_str);
    let git_remote_url = obj.get("gitRemoteUrl").and_then(Value::as_str);
    let repo_config = if let Some(explicit_repo_config) = obj.get("repoConfig") {
        if !explicit_repo_config.is_null() {
            write_repo_config_to_disk(path, Some(explicit_repo_config)).map_err(db_error)?;
            Some(explicit_repo_config.to_string())
        } else {
            write_repo_config_to_disk(path, Some(&Value::Null)).map_err(db_error)?;
            None
        }
    } else {
        read_repo_config_from_disk(path)
            .map_err(db_error)?
            .map(|v| v.to_string())
    };

    {
        let conn = state
            .conn
            .lock()
            .map_err(|_| db_error("db lock poisoned"))?;
        conn.execute(
            "INSERT INTO projects (id, name, path, description, default_provider_id, git_remote_url, repo_config)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                id,
                name,
                path,
                description,
                default_provider_id,
                git_remote_url,
                repo_config
            ],
        )
        .map_err(|e| db_error(e.to_string()))?;
    }
    db_projects_find_by_id(state, id)
}
#[tauri::command]
fn db_projects_update(state: State<'_, AppState>, id: String, data: Value) -> ApiResult<Value> {
    let obj = data
        .as_object()
        .ok_or_else(|| db_error("invalid project payload"))?;
    let mut sets = Vec::new();
    let mut values: Vec<String> = Vec::new();

    if let Some(name) = obj.get("name").and_then(Value::as_str) {
        sets.push("name = ?");
        values.push(name.to_string());
    }
    if obj.get("description").is_some() {
        sets.push("description = ?");
        values.push(
            obj.get("description")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        );
    }
    if obj.get("defaultProviderId").is_some() {
        sets.push("default_provider_id = ?");
        values.push(
            obj.get("defaultProviderId")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        );
    }
    if obj.get("gitRemoteUrl").is_some() {
        sets.push("git_remote_url = ?");
        values.push(
            obj.get("gitRemoteUrl")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        );
    }
    if obj.get("repoConfig").is_some() {
        if let Some(project_path) = {
            let conn = state
                .conn
                .lock()
                .map_err(|_| db_error("db lock poisoned"))?;
            conn.query_row(
                "SELECT path FROM projects WHERE id = ?1",
                params![id.clone()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|e| db_error(e.to_string()))?
        } {
            let repo_config = obj.get("repoConfig").cloned();
            if let Some(ref config) = repo_config {
                if !config.is_null() {
                    write_repo_config_to_disk(&project_path, Some(config)).map_err(db_error)?;
                } else {
                    write_repo_config_to_disk(&project_path, Some(&Value::Null))
                        .map_err(db_error)?;
                }
            }
        }
        sets.push("repo_config = ?");
        values.push(
            obj.get("repoConfig")
                .and_then(|v| {
                    if v.is_null() {
                        None
                    } else {
                        Some(v.to_string())
                    }
                })
                .unwrap_or_default(),
        );
    }
    if sets.is_empty() {
        return db_projects_find_by_id(state, id);
    }

    let mut sql = format!("UPDATE projects SET {} WHERE id = ?", sets.join(", "));
    sql.push_str("");
    {
        let conn = state
            .conn
            .lock()
            .map_err(|_| db_error("db lock poisoned"))?;
        let mut dyn_params = values.iter().map(|v| v.as_str()).collect::<Vec<_>>();
        dyn_params.push(id.as_str());
        conn.execute(&sql, params_from_iter(dyn_params))
            .map_err(|e| db_error(e.to_string()))?;
    }
    db_projects_find_by_id(state, id)
}
#[tauri::command]
fn db_projects_delete(state: State<'_, AppState>, id: String) -> ApiResult<Value> {
    let conn = state
        .conn
        .lock()
        .map_err(|_| db_error("db lock poisoned"))?;
    let changed = conn
        .execute("DELETE FROM projects WHERE id = ?1", params![id])
        .map_err(|e| db_error(e.to_string()))?;
    Ok(Value::Bool(changed > 0))
}
#[tauri::command]
fn db_projects_get_stats(state: State<'_, AppState>, id: String) -> ApiResult<Value> {
    let conn = state
        .conn
        .lock()
        .map_err(|_| db_error("db lock poisoned"))?;
    let mut stmt = conn
        .prepare(
            "SELECT
                COUNT(*) as total,
                SUM(CASE WHEN status = 'applied' THEN 1 ELSE 0 END) as applied,
                SUM(CASE WHEN status = 'active' THEN 1 ELSE 0 END) as active,
                SUM(CASE WHEN status = 'error' THEN 1 ELSE 0 END) as failed
             FROM combs WHERE project_id = ?1",
        )
        .map_err(|e| db_error(e.to_string()))?;
    let value = stmt
        .query_row(params![id], |r| {
            Ok(serde_json::json!({
              "totalWorkspaces": r.get::<_, i64>(0).unwrap_or(0),
              "appliedWorkspaces": r.get::<_, i64>(1).unwrap_or(0),
              "activeWorkspaces": r.get::<_, i64>(2).unwrap_or(0),
              "failedWorkspaces": r.get::<_, i64>(3).unwrap_or(0),
            }))
        })
        .map_err(|e| db_error(e.to_string()))?;
    Ok(value)
}
#[tauri::command]
fn db_projects_update_last_opened(state: State<'_, AppState>, id: String) -> ApiResult<Value> {
    {
        let conn = state
            .conn
            .lock()
            .map_err(|_| db_error("db lock poisoned"))?;
        conn.execute(
            "UPDATE projects SET last_opened_at = datetime('now') WHERE id = ?1",
            params![id.clone()],
        )
        .map_err(|e| db_error(e.to_string()))?;
    }
    db_projects_find_by_id(state, id)
}

#[tauri::command]
fn db_panes_find_by_comb(state: State<'_, AppState>, comb_id: String) -> ApiResult<Value> {
    let conn = state
        .conn
        .lock()
        .map_err(|_| db_error("db lock poisoned"))?;
    let mut stmt = conn
        .prepare("SELECT * FROM panes WHERE comb_id = ?1 ORDER BY layout_order ASC")
        .map_err(|e| db_error(e.to_string()))?;
    let rows = stmt
        .query_map(params![comb_id], row_to_json)
        .map_err(|e| db_error(e.to_string()))?;
    let items: Result<Vec<_>, _> = rows.collect();
    Ok(Value::Array(
        items
            .map_err(|e| db_error(e.to_string()))?
            .into_iter()
            .map(map_pane_to_renderer)
            .collect(),
    ))
}
#[tauri::command]
fn db_panes_find_by_id(state: State<'_, AppState>, id: String) -> ApiResult<Value> {
    let conn = state
        .conn
        .lock()
        .map_err(|_| db_error("db lock poisoned"))?;
    let mut stmt = conn
        .prepare("SELECT * FROM panes WHERE id = ?1")
        .map_err(|e| db_error(e.to_string()))?;
    let row = stmt
        .query_row(params![id], row_to_json)
        .optional()
        .map_err(|e| db_error(e.to_string()))?;
    Ok(row.map(map_pane_to_renderer).unwrap_or(Value::Null))
}
#[tauri::command]
fn db_panes_create(state: State<'_, AppState>, data: Value) -> ApiResult<Value> {
    let obj = data
        .as_object()
        .ok_or_else(|| db_error("invalid pane payload"))?;
    let id = next_id();
    let comb_id = obj
        .get("combId")
        .and_then(Value::as_str)
        .ok_or_else(|| db_error("combId is required"))?;
    let typ = obj
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| db_error("type is required"))?;
    let provider_id = obj.get("providerId").and_then(Value::as_str);
    let title = obj.get("title").and_then(Value::as_str);
    let initial_prompt = obj.get("initialPrompt").and_then(Value::as_str);
    let layout_order_opt = obj.get("layoutOrder").and_then(|v| v.as_i64());

    let layout_order: i64 = if let Some(lo) = layout_order_opt {
        lo
    } else {
        let conn = state
            .conn
            .lock()
            .map_err(|_| db_error("db lock poisoned"))?;
        conn.query_row(
            "SELECT COALESCE(MAX(layout_order), -1) + 1 FROM panes WHERE comb_id = ?1",
            params![comb_id],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0)
    };

    {
        let conn = state
            .conn
            .lock()
            .map_err(|_| db_error("db lock poisoned"))?;
        conn.execute(
            "INSERT INTO panes (id, comb_id, type, provider_id, title, initial_prompt, status, layout_order, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'idle', ?7, datetime('now'), datetime('now'))",
            params![id, comb_id, typ, provider_id, title, initial_prompt, layout_order],
        )
        .map_err(|e| db_error(e.to_string()))?;
    }
    db_panes_find_by_id(state, id)
}
#[tauri::command]
fn db_panes_update(state: State<'_, AppState>, id: String, data: Value) -> ApiResult<Value> {
    use rusqlite::types::Value as SqlValue;

    let obj = data
        .as_object()
        .ok_or_else(|| db_error("invalid pane payload"))?;
    let mut sets: Vec<String> = Vec::new();
    let mut bind: Vec<SqlValue> = Vec::new();

    if obj.contains_key("title") {
        sets.push("title = ?".into());
        let v = obj.get("title").and_then(|v| {
            if v.is_null() {
                return None;
            }
            v.as_str().map(|s| s.to_string())
        });
        bind.push(match v {
            Some(s) => SqlValue::Text(s),
            None => SqlValue::Null,
        });
    }
    if obj.contains_key("initialPrompt") {
        sets.push("initial_prompt = ?".into());
        let v = obj.get("initialPrompt").and_then(|v| {
            if v.is_null() {
                return None;
            }
            v.as_str().map(|s| s.to_string())
        });
        bind.push(match v {
            Some(s) => SqlValue::Text(s),
            None => SqlValue::Null,
        });
    }
    if obj.contains_key("providerId") {
        sets.push("provider_id = ?".into());
        let v = obj.get("providerId").and_then(|v| {
            if v.is_null() {
                return None;
            }
            v.as_str().map(|s| s.to_string())
        });
        bind.push(match v {
            Some(s) => SqlValue::Text(s),
            None => SqlValue::Null,
        });
    }
    if obj.contains_key("cwd") {
        sets.push("cwd = ?".into());
        let v = obj.get("cwd").and_then(|v| {
            if v.is_null() {
                return None;
            }
            v.as_str().map(|s| s.to_string())
        });
        bind.push(match v {
            Some(s) => SqlValue::Text(s),
            None => SqlValue::Null,
        });
    }
    if obj.contains_key("ptyOwnerKey") {
        sets.push("pty_owner_key = ?".into());
        let v = obj.get("ptyOwnerKey").and_then(|v| {
            if v.is_null() {
                return None;
            }
            v.as_str().map(|s| s.to_string())
        });
        bind.push(match v {
            Some(s) => SqlValue::Text(s),
            None => SqlValue::Null,
        });
    }
    if let Some(st) = obj.get("status").and_then(Value::as_str) {
        sets.push("status = ?".into());
        bind.push(SqlValue::Text(st.to_string()));
    }
    if let Some(lo) = obj.get("layoutOrder").and_then(|v| v.as_i64()) {
        sets.push("layout_order = ?".into());
        bind.push(SqlValue::Integer(lo));
    }
    if obj.contains_key("lastActivityAt") {
        sets.push("last_activity_at = ?".into());
        let v = obj.get("lastActivityAt").and_then(|v| {
            if v.is_null() {
                return None;
            }
            v.as_str().map(|s| s.to_string())
        });
        bind.push(match v {
            Some(s) => SqlValue::Text(s),
            None => SqlValue::Null,
        });
    }

    if sets.is_empty() {
        return db_panes_find_by_id(state, id);
    }

    bind.push(SqlValue::Text(id.clone()));
    let sql = format!("UPDATE panes SET {} WHERE id = ?", sets.join(", "));
    {
        let conn = state
            .conn
            .lock()
            .map_err(|_| db_error("db lock poisoned"))?;
        conn.execute(&sql, params_from_iter(bind))
            .map_err(|e| db_error(e.to_string()))?;
    }
    db_panes_find_by_id(state, id)
}
#[tauri::command]
fn db_panes_delete(state: State<'_, AppState>, id: String) -> ApiResult<Value> {
    let conn = state
        .conn
        .lock()
        .map_err(|_| db_error("db lock poisoned"))?;
    let changed = conn
        .execute("DELETE FROM panes WHERE id = ?1", params![id])
        .map_err(|e| db_error(e.to_string()))?;
    Ok(Value::Bool(changed > 0))
}

// ---------- DB Utils ----------
#[tauri::command]
fn db_utils_get_path(state: State<'_, AppState>) -> String {
    state.db_path.to_string_lossy().to_string()
}

#[tauri::command]
fn db_utils_get_size(state: State<'_, AppState>) -> ApiResult<u64> {
    std::fs::metadata(state.db_path.as_path())
        .map(|m| m.len())
        .map_err(|e| ApiError {
            code: "DB_SIZE_ERROR",
            message: e.to_string(),
        })
}

#[tauri::command]
fn db_utils_backup(state: State<'_, AppState>, dest_path: String) -> ApiResult<bool> {
    std::fs::copy(state.db_path.as_path(), dest_path)
        .map(|_| true)
        .map_err(|e| ApiError {
            code: "DB_BACKUP_ERROR",
            message: e.to_string(),
        })
}

// ---------- Terminal ----------
#[tauri::command]
fn terminal_spawn(state: State<'_, AppState>, app: AppHandle, options: Value) -> ApiResult<Value> {
    let cwd = options
        .get("cwd")
        .and_then(Value::as_str)
        .ok_or_else(|| db_error("cwd is required"))?;
    let command = options
        .get("command")
        .and_then(Value::as_str)
        .unwrap_or("bash");
    let args = options
        .get("args")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect::<Vec<_>>();

    let cols = options.get("cols").and_then(Value::as_u64).unwrap_or(80) as u16;
    let rows = options.get("rows").and_then(Value::as_u64).unwrap_or(24) as u16;

    let pty_system = native_pty_system();
    let pty_pair = pty_system
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| db_error(e.to_string()))?;

    let mut cmd = CommandBuilder::new(command);
    cmd.args(&args);
    cmd.cwd(cwd);
    // Alinha o PTY com o que o xterm.js emula; sem TERM adequado, `clear` e escapes podem falhar
    // quando o app é lançado pela GUI (env mínima).
    #[cfg(unix)]
    {
        cmd.env("TERM", "xterm-256color");

        // Detecta worktree e configura Git corretamente
        // Worktrees têm um arquivo .git (não diretório) apontando para gitdir
        let git_file_path = Path::new(cwd).join(".git");
        if git_file_path.is_file() {
            // Lê o arquivo .git para obter o gitdir
            if let Ok(git_content) = std::fs::read_to_string(&git_file_path) {
                // Formato: "gitdir: /path/to/.git/worktrees/name"
                if let Some(gitdir_line) = git_content.lines().find(|l| l.starts_with("gitdir:")) {
                    let gitdir = gitdir_line.trim_start_matches("gitdir:").trim();
                    // Configura GIT_DIR para apontar para o gitdir do worktree
                    cmd.env("GIT_DIR", gitdir);
                    // GIT_WORK_TREE aponta para o diretório de trabalho atual (o worktree)
                    cmd.env("GIT_WORK_TREE", cwd);
                }
            }
        }
    }

    let child = pty_pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| db_error(e.to_string()))?;
    #[cfg(windows)]
    let windows_job = attach_windows_job_object(&child);

    let reader = pty_pair
        .master
        .try_clone_reader()
        .map_err(|e| db_error(e.to_string()))?;
    let writer = pty_pair
        .master
        .take_writer()
        .map_err(|e| db_error(e.to_string()))?;

    let pty_id = format!("pty-{}", next_id());
    let stop_flag = Arc::new(AtomicBool::new(false));

    let pane_id_opt = options
        .get("paneId")
        .and_then(Value::as_str)
        .map(|s| s.to_string());
    let skip_scrollback_restore = options
        .get("restart")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let pty_owner_key_opt = options
        .get("ptyOwnerKey")
        .and_then(Value::as_str)
        .map(|s| s.to_string());

    let output_buffer = new_terminal_output_buffer();
    let title_state = Arc::new(Mutex::new(None));
    if skip_scrollback_restore {
        if let Some(ref pid) = pane_id_opt {
            if let Ok(conn) = state.conn.lock() {
                let _ = conn.execute(
                    "DELETE FROM pane_terminal_scrollback WHERE pane_id = ?1",
                    params![pid],
                );
            }
        }
    } else if let Some(ref pid) = pane_id_opt {
        if let Ok(conn) = state.conn.lock() {
            if let Some(loaded) = load_pane_scrollback_deque(&conn, pid) {
                if let Ok(mut guard) = output_buffer.lock() {
                    *guard = loaded;
                }
            }
        }
    }

    let reader_thread = Some(spawn_terminal_reader_thread(
        reader,
        app.clone(),
        pty_id.clone(),
        pane_id_opt.clone(),
        title_state.clone(),
        stop_flag.clone(),
        output_buffer.clone(),
        state.conn.clone(),
    ));

    if let Some(pane_id) = pane_id_opt.as_ref() {
        if let Ok(conn) = state.conn.lock() {
            let _ = conn.execute(
                "UPDATE panes SET status = 'running', last_activity_at = datetime('now') WHERE id = ?1",
                params![pane_id],
            );
        }
    }

    let mut terminals = state
        .terminals
        .lock()
        .map_err(|_| db_error("terminals lock poisoned"))?;

    terminals.insert(
        pty_id.clone(),
        ManagedTerminal {
            pty_master: pty_pair.master,
            child,
            writer: Arc::new(Mutex::new(writer)),
            #[cfg(windows)]
            windows_job,
            mission_id: None,
            pane_id: pane_id_opt,
            pty_owner_key: pty_owner_key_opt,
            title: title_state,
            cwd: cwd.to_string(),
            command: command.to_string(),
            args,
            started_at: iso_now(),
            stop_flag,
            exit_notified: Arc::new(AtomicBool::new(false)),
            output_buffer,
            reader_thread,
        },
    );

    Ok(serde_json::json!({ "ptyId": pty_id }))
}

fn terminal_session_json(pty_id: &str, t: &mut ManagedTerminal) -> Value {
    let wait = t.child.try_wait().ok().flatten();
    let (status, exited_at, exit_code) = if let Some(s) = wait {
        ("exited", Some(iso_now()), Some(s.exit_code()))
    } else {
        ("running", None, None)
    };
    serde_json::json!({
      "ptyId": pty_id,
      "cwd": t.cwd,
      "command": t.command,
      "args": t.args,
      "ptyOwnerKey": t.pty_owner_key,
      "status": status,
      "startedAt": t.started_at,
      "exitedAt": exited_at,
      "lastExitCode": exit_code
    })
}

#[derive(Debug, Clone)]
struct PaneActivityContext {
    project_id: String,
    project_name: String,
    workspace_name: Option<String>,
    pane_id: String,
    comb_id: String,
    pane_type: Option<String>,
    pane_title: Option<String>,
    provider_id: Option<String>,
    provider_type: Option<String>,
    provider_name: Option<String>,
}

fn terminal_command_signature(command: &str, args: &[String]) -> String {
    let mut parts = Vec::with_capacity(args.len() + 1);
    parts.push(command.trim().to_string());
    parts.extend(args.iter().map(|arg| arg.trim().to_string()));
    parts.join(" ").to_lowercase()
}

fn command_basename(command: &str) -> String {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    Path::new(trimmed)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(trimmed)
        .to_lowercase()
}

fn command_matches_any_source(
    signature: &str,
    basename: &str,
    title: &str,
    patterns: &[&str],
) -> Option<&'static str> {
    patterns.iter().find_map(|pattern| {
        let pattern_lower = pattern.to_lowercase();
        if basename == pattern_lower
            || basename.contains(&pattern_lower)
            || signature.contains(&pattern_lower)
        {
            Some("command")
        } else if title.contains(&pattern_lower) {
            Some("title")
        } else {
            None
        }
    })
}

fn detect_terminal_agent_kind(
    command: &str,
    args: &[String],
    pane_type: Option<&str>,
    provider_type: Option<&str>,
    provider_name: Option<&str>,
    title: Option<&str>,
) -> Option<(String, String, String)> {
    let signature = terminal_command_signature(command, args);
    let basename = command_basename(command);
    let title = title.unwrap_or("").to_lowercase();
    let provider_type = provider_type.unwrap_or("").to_lowercase();
    let provider_name = provider_name.unwrap_or("").to_lowercase();
    let pane_type = pane_type.unwrap_or("").to_lowercase();

    let classify = |kind: &str, label: &str, detected_by: &str| {
        Some((kind.to_string(), label.to_string(), detected_by.to_string()))
    };

    let claude_source = command_matches_any_source(
        &signature,
        &basename,
        &title,
        &[
            "claude",
            "claude-code",
            "@anthropic-ai/claude-code",
            "anthropic-ai/claude-code",
        ],
    );
    if claude_source.is_some() || provider_type == "claude-code" || provider_name.contains("claude")
    {
        return classify("claude", "Claude", claude_source.unwrap_or("provider"));
    }

    let codex_source = command_matches_any_source(
        &signature,
        &basename,
        &title,
        &["codex", "@openai/codex", "openai/codex"],
    );
    if codex_source.is_some() || provider_type == "codex" || provider_name.contains("codex") {
        return classify("codex", "Codex", codex_source.unwrap_or("provider"));
    }

    let opencode_source = command_matches_any_source(
        &signature,
        &basename,
        &title,
        &["opencode", "open-code", "open code"],
    );
    if opencode_source.is_some()
        || provider_name.contains("opencode")
        || provider_name.contains("open code")
    {
        return classify(
            "opencode",
            "OpenCode",
            opencode_source.unwrap_or("provider"),
        );
    }

    let cursor_source =
        command_matches_any_source(&signature, &basename, &title, &["cursor-agent", "cursor"]);
    if cursor_source.is_some() || provider_type == "cursor" || provider_name.contains("cursor") {
        return classify("cursor", "Cursor", cursor_source.unwrap_or("provider"));
    }

    let gemini_source =
        command_matches_any_source(&signature, &basename, &title, &["gemini", "gemini-cli"]);
    if gemini_source.is_some() || provider_type == "gemini" || provider_name.contains("gemini") {
        return classify("gemini", "Gemini", gemini_source.unwrap_or("provider"));
    }

    let aider_source = command_matches_any_source(&signature, &basename, &title, &["aider"]);
    if aider_source.is_some() || provider_name.contains("aider") {
        return classify("aider", "Aider", aider_source.unwrap_or("provider"));
    }

    let continue_source = command_matches_any_source(&signature, &basename, &title, &["continue"]);
    if continue_source.is_some() || provider_name.contains("continue") {
        return classify(
            "continue",
            "Continue",
            continue_source.unwrap_or("provider"),
        );
    }

    if pane_type == "agent" {
        let label = if !provider_name.is_empty() {
            provider_name
        } else {
            "agent".to_string()
        };
        return classify("agent", &label, "pane-type");
    }

    None
}

fn normalize_terminal_title(raw: &str) -> Option<String> {
    let stripped = strip_ansi_codes(raw);

    // Remove OSC sequence residues (e.g., "11;rgb:1a1a/1a1a/1a1a")
    // Pattern: digit(s) followed by semicolon and rgb/hex color data
    let osc_cleaned = stripped
        .split_whitespace()
        .filter(|word| {
            // Filter out OSC color code patterns like "11;rgb:..." or "10;#..."
            !word.chars().next().map_or(false, |c| c.is_ascii_digit())
                || !word.contains(';')
                || (!word.contains("rgb:") && !word.contains('#'))
        })
        .collect::<Vec<_>>()
        .join(" ");

    let collapsed = osc_cleaned
        .chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect::<String>();
    let normalized = collapsed.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = normalized.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn update_terminal_title_from_bytes(
    data: &[u8],
    carry: &mut Vec<u8>,
    title_state: &Arc<Mutex<Option<String>>>,
) {
    let mut bytes = Vec::with_capacity(carry.len() + data.len());
    bytes.extend_from_slice(carry);
    bytes.extend_from_slice(data);
    carry.clear();

    let mut i = 0usize;
    let mut latest_title: Option<String> = None;
    while i < bytes.len() {
        if bytes[i] != 0x1b {
            i += 1;
            continue;
        }
        if i + 1 >= bytes.len() {
            carry.extend_from_slice(&bytes[i..]);
            break;
        }
        if bytes[i + 1] != b']' {
            i += 1;
            continue;
        }

        let seq_start = i;
        i += 2;
        let kind_start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i >= bytes.len() {
            carry.extend_from_slice(&bytes[seq_start..]);
            break;
        }
        if bytes[i] != b';' {
            i = seq_start + 1;
            continue;
        }

        let kind = std::str::from_utf8(&bytes[kind_start..i]).ok();
        i += 1;
        let payload_start = i;
        let mut terminated_at: Option<usize> = None;
        while i < bytes.len() {
            if bytes[i] == 0x07 {
                terminated_at = Some(i);
                break;
            }
            if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'\\' {
                terminated_at = Some(i);
                break;
            }
            i += 1;
        }
        let Some(term_idx) = terminated_at else {
            carry.extend_from_slice(&bytes[seq_start..]);
            break;
        };

        if matches!(kind, Some("0") | Some("2")) {
            if let Ok(raw_title) = std::str::from_utf8(&bytes[payload_start..term_idx]) {
                if let Some(title) = normalize_terminal_title(raw_title) {
                    latest_title = Some(title);
                }
            }
        }

        i = if bytes[term_idx] == 0x1b {
            term_idx + 2
        } else {
            term_idx + 1
        };
    }

    if let Some(title) = latest_title {
        if let Ok(mut guard) = title_state.lock() {
            *guard = Some(title);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_terminal_title_collapses_whitespace_and_strips_controls() {
        let title = normalize_terminal_title("  Claude\tCode  \u{1b}[31m- Dev \u{1b}[0m  ");
        assert_eq!(title.as_deref(), Some("Claude Code - Dev"));
    }

    #[test]
    fn normalize_terminal_title_removes_osc_color_codes() {
        let title = normalize_terminal_title("service-dashboard git:(main) 11;rgb:1a1a/1a1a/1a1a");
        assert_eq!(title.as_deref(), Some("service-dashboard git:(main)"));
    }

    #[test]
    fn normalize_terminal_title_removes_multiple_osc_patterns() {
        let title = normalize_terminal_title("myapp 10;#ffffff 11;rgb:0000/0000/0000 test");
        assert_eq!(title.as_deref(), Some("myapp test"));
    }

    #[test]
    fn parses_bel_terminated_osc_title_in_single_chunk() {
        let title_state = Arc::new(Mutex::new(None));
        let mut carry = Vec::new();
        update_terminal_title_from_bytes(b"\x1b]2;Codex - project\x07", &mut carry, &title_state);

        assert!(carry.is_empty());
        assert_eq!(
            title_state
                .lock()
                .ok()
                .and_then(|guard| guard.clone())
                .as_deref(),
            Some("Codex - project")
        );
    }

    #[test]
    fn parses_st_terminated_osc_title_across_chunks() {
        let title_state = Arc::new(Mutex::new(None));
        let mut carry = Vec::new();

        update_terminal_title_from_bytes(b"\x1b]0;Claude ", &mut carry, &title_state);
        assert!(!carry.is_empty());
        assert!(title_state
            .lock()
            .ok()
            .and_then(|guard| guard.clone())
            .is_none());

        update_terminal_title_from_bytes(b"Code\x1b\\", &mut carry, &title_state);
        assert!(carry.is_empty());
        assert_eq!(
            title_state
                .lock()
                .ok()
                .and_then(|guard| guard.clone())
                .as_deref(),
            Some("Claude Code")
        );
    }
}

fn waiting_excerpt_from_buffer(output_buffer: &Arc<Mutex<VecDeque<String>>>) -> Option<String> {
    let buffer = output_buffer.lock().ok()?;
    let tail: String = buffer
        .iter()
        .rev()
        .take(5)
        .map(|s| s.as_str())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n");
    let stripped = strip_ansi_codes(&tail);
    if WAIT_PATTERN.is_match(&stripped) {
        return buffer.iter().rev().find(|s| !s.trim().is_empty()).map(|s| {
            let trimmed = strip_ansi_codes(s).trim().to_string();
            const MAX_EXCERPT_CHARS: usize = 220;
            if trimmed.chars().count() > MAX_EXCERPT_CHARS {
                format!(
                    "{}…",
                    trimmed.chars().take(MAX_EXCERPT_CHARS).collect::<String>()
                )
            } else {
                trimmed
            }
        });
    }
    None
}

fn collect_pane_activity_contexts(
    state: &AppState,
    project_id: &str,
) -> Result<Vec<PaneActivityContext>, String> {
    let conn = state
        .conn
        .lock()
        .map_err(|_| "db lock poisoned".to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT p.id, p.comb_id, p.type, p.title, p.provider_id, c.project_id, c.name, prj.name, pr.name, pr.type
             FROM panes p
             JOIN combs c ON c.id = p.comb_id
             JOIN projects prj ON prj.id = c.project_id
             LEFT JOIN providers pr ON pr.id = p.provider_id
             WHERE c.project_id = ?1",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![project_id], |row| {
            Ok(PaneActivityContext {
                pane_id: row.get::<_, String>(0)?,
                comb_id: row.get::<_, String>(1)?,
                pane_type: row.get::<_, Option<String>>(2)?,
                pane_title: row.get::<_, Option<String>>(3)?,
                provider_id: row.get::<_, Option<String>>(4)?,
                project_id: row.get::<_, String>(5)?,
                workspace_name: row.get::<_, Option<String>>(6)?,
                project_name: row.get::<_, String>(7)?,
                provider_name: row.get::<_, Option<String>>(8)?,
                provider_type: row.get::<_, Option<String>>(9)?,
            })
        })
        .map_err(|e| e.to_string())?;
    let mut contexts = Vec::new();
    for row in rows {
        contexts.push(row.map_err(|e| e.to_string())?);
    }

    Ok(contexts)
}

fn collect_project_agent_activity(state: &AppState, project_id: &str) -> Result<Value, String> {
    let pane_contexts = collect_pane_activity_contexts(state, project_id)?;
    let pane_context_by_id = pane_contexts
        .into_iter()
        .map(|ctx| (ctx.pane_id.clone(), ctx))
        .collect::<HashMap<_, _>>();

    let mut system = System::new_all();
    system.refresh_all();

    let mut running_by_comb: HashMap<String, i64> = HashMap::new();
    let mut active_agents_by_comb: HashMap<String, i64> = HashMap::new();
    let mut total_running = 0i64;
    let mut working_agents = 0i64;
    let mut waiting_agents = 0i64;
    let mut active_agents: Vec<DetectedTerminalAgent> = Vec::new();

    let mut terminals = state
        .terminals
        .lock()
        .map_err(|_| "terminals lock poisoned".to_string())?;

    for (pty_id, terminal) in terminals.iter_mut() {
        let Some(pane_id) = terminal.pane_id.as_ref() else {
            continue;
        };
        let Some(ctx) = pane_context_by_id.get(pane_id) else {
            continue;
        };
        if ctx.project_id != project_id {
            continue;
        }

        let is_running = terminal.child.try_wait().ok().flatten().is_none();
        if !is_running {
            continue;
        }

        total_running += 1;
        *running_by_comb.entry(ctx.comb_id.clone()).or_insert(0) += 1;

        let shell_pid = terminal.child.process_id();
        let runtime_title = terminal.title.lock().ok().and_then(|guard| guard.clone());
        let activity_title = runtime_title.clone().or_else(|| ctx.pane_title.clone());
        let mut detected = detect_terminal_agent_kind(
            &terminal.command,
            &terminal.args,
            ctx.pane_type.as_deref(),
            ctx.provider_type.as_deref(),
            ctx.provider_name.as_deref(),
            activity_title.as_deref(),
        );

        if detected.is_none() {
            if let Some(shell_pid) = shell_pid {
                let shell_pid = Pid::from_u32(shell_pid);
                let mut descendants: Vec<String> = Vec::new();
                for process in system.processes().values() {
                    if process.parent() == Some(shell_pid) {
                        let name = process.name().to_string();
                        let cmd = process
                            .cmd()
                            .iter()
                            .map(|arg| arg.to_string())
                            .collect::<Vec<_>>()
                            .join(" ");
                        descendants.push(format!("{name} {cmd}"));
                    }
                }
                for signature in &descendants {
                    if let Some(next) = detect_terminal_agent_kind(
                        signature,
                        &[],
                        ctx.pane_type.as_deref(),
                        ctx.provider_type.as_deref(),
                        ctx.provider_name.as_deref(),
                        activity_title.as_deref(),
                    ) {
                        detected = Some(next);
                        break;
                    }
                }
            }
        }

        let Some((agent_kind, agent_label, detected_by)) = detected else {
            continue;
        };

        let excerpt = waiting_excerpt_from_buffer(&terminal.output_buffer);
        let status = if excerpt.is_some() {
            waiting_agents += 1;
            "waiting"
        } else {
            working_agents += 1;
            "working"
        };
        *active_agents_by_comb
            .entry(ctx.comb_id.clone())
            .or_insert(0) += 1;

        active_agents.push(DetectedTerminalAgent {
            pty_id: pty_id.clone(),
            pane_id: Some(ctx.pane_id.clone()),
            comb_id: Some(ctx.comb_id.clone()),
            project_id: ctx.project_id.clone(),
            project_name: ctx.project_name.clone(),
            workspace_name: ctx.workspace_name.clone(),
            agent_kind,
            agent_label,
            status: status.to_string(),
            cwd: terminal.cwd.clone(),
            command: terminal.command.clone(),
            args: terminal.args.clone(),
            pid: shell_pid,
            title: activity_title,
            provider_id: ctx.provider_id.clone(),
            provider_name: ctx.provider_name.clone(),
            detected_by,
            excerpt,
            started_at: terminal.started_at.clone(),
        });
    }

    active_agents.sort_by(|a, b| {
        a.project_name
            .cmp(&b.project_name)
            .then_with(|| a.workspace_name.cmp(&b.workspace_name))
            .then_with(|| a.agent_label.cmp(&b.agent_label))
            .then_with(|| a.command.cmp(&b.command))
    });

    Ok(serde_json::json!({
        "totalRunningPanes": total_running,
        "runningPanesByCombId": running_by_comb,
        "activeAgentsByCombId": active_agents_by_comb,
        "workingAgents": working_agents,
        "waitingAgents": waiting_agents,
        "activeAgents": active_agents,
    }))
}

/// Verifica se o terminal está aguardando input do usuário
/// Retorna (needs_attention, excerpt_opcional)
fn check_needs_attention(
    data: &str,
    output_buffer: &Arc<Mutex<VecDeque<String>>>,
) -> (bool, Option<String>) {
    // OSC 9;9; protocol (Ghostty)
    if data.contains("\x1b]9;9;") {
        return (true, Some("Notification request".to_string()));
    }

    // Verificar últimas linhas do buffer para padrões de espera
    if let Ok(buffer) = output_buffer.lock() {
        // Pegar últimas 5 linhas
        let tail: String = buffer
            .iter()
            .rev()
            .take(5)
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n");

        // Strip ANSI para análise mais limpa
        let stripped = strip_ansi_codes(&tail);

        if WAIT_PATTERN.is_match(&stripped) {
            // Extrair excerpt da última linha não vazia
            let excerpt = buffer
                .iter()
                .rev()
                .find(|s| !s.trim().is_empty())
                .map(|s| {
                    let trimmed = strip_ansi_codes(s).trim().to_string();
                    const MAX_EXCERPT_CHARS: usize = 220;
                    if trimmed.chars().count() > MAX_EXCERPT_CHARS {
                        format!(
                            "{}…",
                            trimmed.chars().take(MAX_EXCERPT_CHARS).collect::<String>()
                        )
                    } else {
                        trimmed
                    }
                })
                .unwrap_or_default();

            return (true, Some(excerpt));
        }
    }

    (false, None)
}

/// Remove códigos ANSI básicos para análise de texto
fn strip_ansi_codes(input: &str) -> String {
    // Remove SGR sequences: ESC [ ... m
    let re_sgr = Regex::new(r"\x1b\[[\d;?]*[A-Za-z]").unwrap();
    let s = re_sgr.replace_all(input, "");

    // Remove OSC sequences: ESC ] ... BEL or ESC ]... ESC \
    let re_osc1 = Regex::new(r"\x1b\][^\x07]*\x07").unwrap();
    let s = re_osc1.replace_all(&s, "");

    let re_osc2 = Regex::new(r"\x1b\][^\x1b\\]*\\").unwrap();
    re_osc2.replace_all(&s, "").to_string()
}

fn spawn_terminal_reader_thread(
    mut reader: Box<dyn Read + Send>,
    app: AppHandle,
    pty_id: String,
    pane_id: Option<String>,
    title_state: Arc<Mutex<Option<String>>>,
    stop_flag: Arc<AtomicBool>,
    output_buffer: Arc<Mutex<VecDeque<String>>>,
    conn: Arc<Mutex<Connection>>,
) -> JoinHandle<()> {
    /// Batching alinhado a superset-sh (`apps/desktop/src/main/terminal-host/pty-subprocess.ts`):
    /// ~60fps + limite por frame evita inundar o IPC com micro-chunks e mantém redesenhos de TUI (`\r`, escapes)
    /// mais coerentes num único `terminal-output` antes do xterm.js — sem alterar o stream final concatenado.
    const FLUSH_INTERVAL: Duration = Duration::from_millis(16);
    const MAX_BATCH_BYTES: usize = 128 * 1024;

    thread::spawn(move || {
        let (tx, rx) = mpsc::channel::<Vec<u8>>();
        let last_persist = Arc::new(Mutex::new(
            Instant::now() - PANE_SCROLLBACK_PERSIST_INTERVAL,
        ));
        let mut title_remainder: Vec<u8> = Vec::new();

        let read_thread = thread::spawn(move || {
            let mut buffer = [0u8; 8192];
            loop {
                if stop_flag.load(Ordering::Relaxed) {
                    break;
                }
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(n) => {
                        if tx.send(buffer[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        let mut pending: Vec<u8> = Vec::new();

        let mut flush_pending = |pending: &mut Vec<u8>| {
            if pending.is_empty() {
                return;
            }
            update_terminal_title_from_bytes(pending, &mut title_remainder, &title_state);
            let data = String::from_utf8_lossy(pending).to_string();
            pending.clear();

            append_terminal_line(&output_buffer, &data);

            if let Some(ref pid) = pane_id {
                if let Ok(mut lp) = last_persist.lock() {
                    if lp.elapsed() >= PANE_SCROLLBACK_PERSIST_INTERVAL {
                        *lp = Instant::now();
                        let lines: Vec<String> = output_buffer
                            .lock()
                            .ok()
                            .map(|g| g.iter().cloned().collect())
                            .unwrap_or_default();
                        if let Ok(c) = conn.lock() {
                            if let Err(e) = persist_pane_scrollback_compressed(&c, pid, &lines) {
                                eprintln!("[DCC] pane scrollback persist (throttled): {e}");
                            }
                        }
                    }
                }
            }

            let (needs_attention, excerpt) = check_needs_attention(&data, &output_buffer);

            if needs_attention {
                let excerpt_key = excerpt
                    .as_ref()
                    .map(|s| s.chars().take(64).collect::<String>())
                    .unwrap_or_default();
                let activity_excerpt = excerpt.clone();
                let notification_id = format!(
                    "{}:{}:{}",
                    pane_id.as_deref().unwrap_or_else(|| pty_id.as_str()),
                    "needs_input",
                    excerpt_key
                );
                let mut payload = serde_json::json!({
                    "ptyId": pty_id,
                    "status": "waiting",
                    "phase": "needs_input",
                    "notificationId": notification_id
                });

                if let Some(ref pid) = pane_id {
                    payload["paneId"] = serde_json::json!(pid);
                }

                if let Some(ref exc) = excerpt {
                    payload["excerpt"] = serde_json::json!(exc);
                }

                let _ = app.emit("terminal-attention", payload);
                let mut activity_payload = serde_json::json!({
                    "ptyId": pty_id,
                    "status": "waiting"
                });
                if let Some(ref pid) = pane_id {
                    activity_payload["paneId"] = serde_json::json!(pid);
                }
                if let Some(exc) = activity_excerpt {
                    activity_payload["excerpt"] = serde_json::json!(exc);
                }
                let _ = app.emit("terminal-activity", activity_payload);
            }

            let _ = app.emit(
                "terminal-output",
                serde_json::json!({
                  "ptyId": pty_id,
                  "data": data,
                  "stream": "stdout"
                }),
            );
        };

        loop {
            match rx.recv_timeout(FLUSH_INTERVAL) {
                Ok(chunk) => {
                    pending.extend_from_slice(&chunk);
                    while let Ok(more) = rx.try_recv() {
                        pending.extend_from_slice(&more);
                        if pending.len() >= MAX_BATCH_BYTES {
                            flush_pending(&mut pending);
                        }
                    }
                    if pending.len() >= MAX_BATCH_BYTES {
                        flush_pending(&mut pending);
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if !pending.is_empty() {
                        flush_pending(&mut pending);
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    if !pending.is_empty() {
                        flush_pending(&mut pending);
                    }
                    break;
                }
            }
        }

        let _ = read_thread.join();

        if let Some(ref pid) = pane_id {
            let lines: Vec<String> = output_buffer
                .lock()
                .ok()
                .map(|g| g.iter().cloned().collect())
                .unwrap_or_default();
            if let Ok(c) = conn.lock() {
                if let Err(e) = persist_pane_scrollback_compressed(&c, pid, &lines) {
                    eprintln!("[DCC] pane scrollback persist (final): {e}");
                }
            }
        }
    })
}

#[tauri::command]
fn terminal_get_or_create(
    state: State<'_, AppState>,
    app: AppHandle,
    mission_id: String,
    options: Value,
) -> ApiResult<Value> {
    let mut terminals = state
        .terminals
        .lock()
        .map_err(|_| db_error("terminals lock poisoned"))?;

    let existing_id = terminals
        .iter_mut()
        .find(|(_, t)| t.mission_id.as_deref() == Some(mission_id.as_str()))
        .map(|(id, _)| id.clone());

    if let Some(pty_id) = existing_id {
        if let Some(t) = terminals.get_mut(&pty_id) {
            let session = terminal_session_json(&pty_id, t);
            return Ok(serde_json::json!({ "ptyId": pty_id, "session": session }));
        }
    }

    // Drop lock before spawn to avoid deadlock or long stalls
    drop(terminals);

    let spawn_result = terminal_spawn(state.clone(), app, options)?;
    let pty_id = spawn_result
        .get("ptyId")
        .and_then(Value::as_str)
        .unwrap()
        .to_string();

    let mut terminals = state
        .terminals
        .lock()
        .map_err(|_| db_error("terminals lock poisoned"))?;

    if let Some(t) = terminals.get_mut(&pty_id) {
        t.mission_id = Some(mission_id);
        let session = terminal_session_json(&pty_id, t);
        return Ok(serde_json::json!({ "ptyId": pty_id, "session": session }));
    }

    Err(db_error("Failed to re-acquire spawned terminal"))
}

#[tauri::command]
fn terminal_get_session(state: State<'_, AppState>, mission_id: String) -> ApiResult<Value> {
    let mut terminals = state
        .terminals
        .lock()
        .map_err(|_| db_error("terminals lock poisoned"))?;
    if let Some((id, t)) = terminals
        .iter_mut()
        .find(|(_, t)| t.mission_id.as_deref() == Some(mission_id.as_str()))
    {
        return Ok(terminal_session_json(id, t));
    }
    Ok(Value::Null)
}

/// Escreve no PTY em pedaços para reduzir EIO/backpressure em colagens grandes (um único `write` enorme).
const PTY_INPUT_CHUNK_BYTES: usize = 8192;

fn pty_write_all_chunked(writer: &mut dyn Write, data: &[u8]) -> std::io::Result<()> {
    if data.is_empty() {
        return Ok(());
    }
    for chunk in data.chunks(PTY_INPUT_CHUNK_BYTES) {
        writer.write_all(chunk)?;
    }
    Ok(())
}

#[tauri::command]
fn terminal_write(state: State<'_, AppState>, pty_id: String, data: String) -> ApiResult<Value> {
    let terminals = state
        .terminals
        .lock()
        .map_err(|_| db_error("terminals lock poisoned"))?;

    if let Some(t) = terminals.get(&pty_id) {
        let mut writer = t
            .writer
            .lock()
            .map_err(|_| db_error("writer lock poisoned"))?;
        pty_write_all_chunked(&mut *writer, data.as_bytes())
            .map_err(|e| db_error(e.to_string()))?;
        return Ok(serde_json::json!({ "ok": true }));
    }
    Ok(serde_json::json!({ "ok": false }))
}

#[tauri::command]
fn terminal_resize(
    state: State<'_, AppState>,
    pty_id: String,
    cols: u16,
    rows: u16,
) -> ApiResult<Value> {
    let terminals = state
        .terminals
        .lock()
        .map_err(|_| db_error("terminals lock poisoned"))?;

    if let Some(t) = terminals.get(&pty_id) {
        t.pty_master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| db_error(e.to_string()))?;
        return Ok(serde_json::json!({ "ok": true }));
    }
    Ok(serde_json::json!({ "ok": false }))
}

/// Envia sinal ao **grupo de processos** ligado ao PTY (Unix: `kill(-pgid, …)`; Windows: Ctrl+C no stream e Job Object para TERM/KILL).
fn send_signal_to_managed_terminal(t: &ManagedTerminal, signal: &str) -> Result<(), String> {
    let Some(pid_u) = t.child.process_id() else {
        return Err("PID da sessão PTY indisponível".into());
    };
    let normalized = signal.trim().to_ascii_uppercase();
    let name = normalized
        .strip_prefix("SIG")
        .map(str::to_string)
        .unwrap_or(normalized);

    #[cfg(unix)]
    {
        let sig = match name.as_str() {
            "INT" => libc::SIGINT,
            "TERM" => libc::SIGTERM,
            "KILL" => libc::SIGKILL,
            _ => {
                return Err(format!(
                    "Sinal não suportado: {signal} (use SIGINT, SIGTERM ou SIGKILL)"
                ));
            }
        };
        let pid = pid_u as i32;
        let pgrp = unsafe { libc::getpgid(pid) };
        if pgrp < 0 {
            return Err(format!("getpgid: {}", std::io::Error::last_os_error()));
        }
        let rc = unsafe { libc::kill(-pgrp, sig) };
        if rc == 0 {
            return Ok(());
        }
        let err = std::io::Error::last_os_error();
        let rc2 = unsafe { libc::kill(pid, sig) };
        if rc2 == 0 {
            return Ok(());
        }
        return Err(format!("kill: {err}"));
    }

    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        match name.as_str() {
            "INT" => {
                let mut w = t
                    .writer
                    .lock()
                    .map_err(|_| "writer lock poisoned".to_string())?;
                w.write_all(b"\x03")
                    .map_err(|e| format!("enviar Ctrl+C ao PTY: {e}"))?;
                Ok(())
            }
            "TERM" => terminate_managed_terminal_tree(t, false),
            "KILL" => terminate_managed_terminal_tree(t, true),
            _ => Err(format!(
                "Sinal não suportado: {signal} (use SIGINT, SIGTERM ou SIGKILL)"
            )),
        }
    }

    #[cfg(all(not(unix), not(windows)))]
    {
        let _ = (pid_u, name);
        Err("envio de sinais PTY não suportado nesta plataforma".into())
    }
}

#[cfg(windows)]
fn terminate_managed_terminal_tree(t: &ManagedTerminal, force: bool) -> Result<(), String> {
    if let Some(job) = t.windows_job.as_ref() {
        if job.terminate(if force { 1 } else { 0 }).is_ok() {
            return Ok(());
        }
    }

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let pid_u = t
        .child
        .process_id()
        .ok_or_else(|| "PID da sessão PTY indisponível".to_string())?;
    let mut args = vec!["/PID".to_string(), pid_u.to_string(), "/T".to_string()];
    if force {
        args.push("/F".to_string());
    }
    let status = std::process::Command::new("taskkill")
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("taskkill falhou: {status}"))
    }
}

#[cfg(not(windows))]
fn terminate_managed_terminal_tree(_t: &ManagedTerminal, _force: bool) -> Result<(), String> {
    Ok(())
}

fn finalize_managed_terminal_removal(
    state: &AppState,
    app: &AppHandle,
    pty_id: String,
    mut terminal: ManagedTerminal,
    emit_exit_event: bool,
) {
    persist_managed_terminal_buffer(state, &terminal);
    terminal.stop_flag.store(true, Ordering::Relaxed);
    let _ = terminate_managed_terminal_tree(&terminal, true);
    drop(terminal.pty_master);
    // Não aguardamos o reader thread aqui. Se ele demorar a encerrar,
    // o delete ficaria preso e a UI pareceria travada.
    let _ = terminal.reader_thread.take();

    if emit_exit_event {
        let _ = app.emit(
            "terminal-exit",
            serde_json::json!({ "ptyId": pty_id, "code": -1 }),
        );
    }
}

#[tauri::command]
fn terminal_send_signal(
    state: State<'_, AppState>,
    pty_id: String,
    signal: String,
) -> ApiResult<Value> {
    let terminals = state
        .terminals
        .lock()
        .map_err(|_| db_error("terminals lock poisoned"))?;
    let Some(t) = terminals.get(&pty_id) else {
        return Ok(serde_json::json!({ "ok": false, "error": "sessão PTY não encontrada" }));
    };
    match send_signal_to_managed_terminal(t, &signal) {
        Ok(()) => Ok(serde_json::json!({ "ok": true })),
        Err(e) => Ok(serde_json::json!({ "ok": false, "error": e })),
    }
}

#[tauri::command]
fn terminal_kill(state: State<'_, AppState>, app: AppHandle, pty_id: String) -> ApiResult<Value> {
    let terminal = {
        let mut terminals = state
            .terminals
            .lock()
            .map_err(|_| db_error("terminals lock poisoned"))?;
        terminals.remove(&pty_id)
    };

    if let Some(t) = terminal {
        finalize_managed_terminal_removal(&state, &app, pty_id, t, true);
        return Ok(serde_json::json!({ "ok": true }));
    }
    Ok(serde_json::json!({ "ok": false }))
}

#[tauri::command]
fn terminal_kill_by_mission_id(
    state: State<'_, AppState>,
    app: AppHandle,
    mission_id: String,
) -> ApiResult<Value> {
    let ids = {
        let terminals = state
            .terminals
            .lock()
            .map_err(|_| db_error("terminals lock poisoned"))?;
        terminals
            .iter()
            .filter(|(_, t)| t.mission_id.as_deref() == Some(mission_id.as_str()))
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>()
    };

    for id in ids {
        let terminal = {
            let mut terminals = state
                .terminals
                .lock()
                .map_err(|_| db_error("terminals lock poisoned"))?;
            terminals.remove(&id)
        };
        if let Some(t) = terminal {
            finalize_managed_terminal_removal(&state, &app, id, t, true);
        }
    }
    Ok(serde_json::json!({ "ok": true }))
}

#[tauri::command]
fn terminal_get_or_create_for_pane(
    state: State<'_, AppState>,
    app: AppHandle,
    pane_id: String,
    options: Value,
) -> ApiResult<Value> {
    let mut terminals = state
        .terminals
        .lock()
        .map_err(|_| db_error("terminals lock poisoned"))?;

    let existing_id = terminals
        .iter_mut()
        .find(|(_, t)| t.pane_id.as_deref() == Some(pane_id.as_str()))
        .map(|(id, _)| id.clone());

    if let Some(pty_id) = existing_id {
        if let Some(t) = terminals.get_mut(&pty_id) {
            let session = terminal_session_json(&pty_id, t);
            return Ok(serde_json::json!({ "ptyId": pty_id, "session": session }));
        }
    }

    drop(terminals);

    let merged_options = match options {
        Value::Object(mut m) => {
            m.insert("paneId".to_string(), Value::String(pane_id.clone()));
            Value::Object(m)
        }
        _ => serde_json::json!({ "paneId": pane_id.clone() }),
    };
    let spawn_result = terminal_spawn(state.clone(), app, merged_options)?;
    let pty_id = spawn_result
        .get("ptyId")
        .and_then(Value::as_str)
        .unwrap()
        .to_string();

    let mut terminals = state
        .terminals
        .lock()
        .map_err(|_| db_error("terminals lock poisoned"))?;

    if let Some(t) = terminals.get_mut(&pty_id) {
        let session = terminal_session_json(&pty_id, t);
        return Ok(serde_json::json!({ "ptyId": pty_id, "session": session }));
    }

    Err(db_error("Failed to re-acquire spawned terminal for pane"))
}

#[tauri::command]
fn terminal_get_pane_session(state: State<'_, AppState>, pane_id: String) -> ApiResult<Value> {
    let mut terminals = state
        .terminals
        .lock()
        .map_err(|_| db_error("terminals lock poisoned"))?;
    if let Some((id, t)) = terminals
        .iter_mut()
        .find(|(_, t)| t.pane_id.as_deref() == Some(pane_id.as_str()))
    {
        return Ok(terminal_session_json(id, t));
    }
    Ok(Value::Null)
}

#[tauri::command]
fn terminal_kill_by_pane_id(
    state: State<'_, AppState>,
    app: AppHandle,
    pane_id: String,
) -> ApiResult<Value> {
    if let Ok(conn) = state.conn.lock() {
        let _ = conn.execute(
            "UPDATE panes SET status = 'exited', last_activity_at = datetime('now') WHERE id = ?1",
            params![pane_id.clone()],
        );
    }

    let ids = {
        let terminals = state
            .terminals
            .lock()
            .map_err(|_| db_error("terminals lock poisoned"))?;
        terminals
            .iter()
            .filter(|(_, t)| t.pane_id.as_deref() == Some(pane_id.as_str()))
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>()
    };

    for id in ids {
        let terminal = {
            let mut terminals = state
                .terminals
                .lock()
                .map_err(|_| db_error("terminals lock poisoned"))?;
            terminals.remove(&id)
        };
        if let Some(t) = terminal {
            finalize_managed_terminal_removal(&state, &app, id, t, true);
        }
    }
    Ok(serde_json::json!({ "ok": true }))
}

/// Remove histórico persistido e o buffer em memória do painel (alinhado a "limpar scrollback" na UI).
#[tauri::command]
fn terminal_clear_persisted_scrollback(
    state: State<'_, AppState>,
    pane_id: String,
) -> ApiResult<Value> {
    if let Ok(conn) = state.conn.lock() {
        let _ = conn.execute(
            "DELETE FROM pane_terminal_scrollback WHERE pane_id = ?1",
            params![pane_id.clone()],
        );
    }
    let mut terminals = state
        .terminals
        .lock()
        .map_err(|_| db_error("terminals lock poisoned"))?;
    for t in terminals.values_mut() {
        if t.pane_id.as_deref() == Some(pane_id.as_str()) {
            if let Ok(mut g) = t.output_buffer.lock() {
                g.clear();
            }
        }
    }
    Ok(serde_json::json!({ "ok": true }))
}

#[tauri::command]
fn terminal_get_backlog(state: State<'_, AppState>, pty_id: String) -> ApiResult<Value> {
    let terminals = state
        .terminals
        .lock()
        .map_err(|_| db_error("terminals lock poisoned"))?;
    if let Some(t) = terminals.get(&pty_id) {
        let lines = t
            .output_buffer
            .lock()
            .map_err(|_| db_error("terminal output buffer lock poisoned"))?
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        return Ok(serde_json::json!({ "lines": lines }));
    }
    Ok(serde_json::json!({ "lines": [] }))
}

#[tauri::command]
fn terminal_get_project_activity(
    state: State<'_, AppState>,
    project_id: String,
) -> ApiResult<Value> {
    collect_project_agent_activity(&state, &project_id).map_err(|e| db_error(e))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(not(windows))]
    bootstrap_shell_env();

    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            app_get_version,
            app_check_for_updates,
            app_quit_and_install,
            app_show_notification,
            dialog_select_directory,
            dialog_show_message,
            dialog_confirm,
            shell_get_default,
            shell_open_external,
            shell_open_path,
            shell_open_in_editor,
            shell_show_item_in_folder,
            shell_resolve_cli_path,
            shell_detect_cli_for_provider,
            shell_validate_cli_path,
            shell_open_terminal_at_path,
            window_minimize,
            window_maximize,
            window_close,
            window_focus,
            window_is_maximized,
            license_get_status,
            license_get_machine_id,
            license_activate,
            license_skip_activation,
            daemon_get_status,
            daemon_health,
            daemon_list_tasks,
            daemon_list_processes,
            daemon_start_process,
            daemon_stop_process,
            daemon_restart_process,
            daemon_list_combs,
            daemon_list_panes,
            daemon_get_diffs_bundle,
            daemon_run_task,
            daemon_attach_task,
            daemon_detach_task,
            db_providers_find_all,
            db_providers_find_by_id,
            db_providers_find_by_type,
            db_providers_find_active,
            db_providers_create,
            db_providers_update,
            db_providers_delete,
            db_providers_set_active,
            db_providers_test_connection,
            db_providers_is_encryption_available,
            db_projects_find_all,
            db_projects_find_by_id,
            db_projects_find_by_path,
            db_projects_get_repo_config_toml,
            db_projects_save_repo_config_toml,
            db_projects_search,
            db_projects_create,
            db_projects_update,
            db_projects_delete,
            db_projects_get_stats,
            db_projects_update_last_opened,
            db_panes_find_by_comb,
            db_panes_find_by_id,
            db_panes_create,
            db_panes_update,
            db_panes_delete,
            db_utils_backup,
            db_utils_get_path,
            db_utils_get_size,
            terminal_spawn,
            terminal_get_or_create,
            terminal_get_session,
            terminal_write,
            terminal_resize,
            terminal_send_signal,
            terminal_kill,
            terminal_kill_by_mission_id,
            terminal_get_or_create_for_pane,
            terminal_get_pane_session,
            terminal_kill_by_pane_id,
            terminal_get_backlog,
            terminal_clear_persisted_scrollback,
            terminal_get_project_activity,
            terminal_save_temp_image,
            create_workspace_for_repo,
            create_workspace_from_url,
            archive_workspace,
            restore_workspace,
            delete_repository,
            delete_workspace,
            list_local_branches,
            list_git_tracked_files,
            list_child_directories,
            list_repositories,
            list_workspaces,
            workspace_forge_cli_status,
            workspace_forge_cli_accounts,
            workspace_forge_cli_select_login,
            workspace_change_request_create,
            workspace_change_request_merge,
            workspace_change_request_view_web,
            workspace_github_cli_status,
            workspace_gh_pr_create_fill,
            workspace_gh_pr_merge,
            workspace_gh_pr_view_web,
            workspace_git_file_preview,
            workspace_git_file_preview_content,
            workspace_git_branch_diff,
            workspace_git_commit_push,
            workspace_git_discard_file,
            workspace_git_push,
            workspace_git_stage_all,
            workspace_git_stage_file,
            workspace_git_status,
            workspace_git_unstage_file,
            workspace_pr_status,
            workspace_run_setup,
            workspace_continue_from_base_branch,
            session_commands::start_thread,
            session_commands::send_turn,
            session_commands::abort_run,
            session_commands::resume_session,
            session_commands::close_session,
            session_commands::restore_session,
            session_commands::list_thread_events,
            session_commands::list_workspace_sessions,
            session_commands::respond_to_user_input,
            session_commands::respond_to_permission_request
        ])
        .setup(|app| {
            let app_data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| PathBuf::from("."));
            let _ = std::fs::create_dir_all(&app_data_dir);
            let db_path = app_data_dir.join("database.sqlite");
            let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;
            conn.execute_batch(
                "
                PRAGMA journal_mode = WAL;
                PRAGMA synchronous = NORMAL;
                PRAGMA foreign_keys = ON;
                PRAGMA busy_timeout = 5000;
                ",
            )
            .map_err(|e| e.to_string())?;
            run_legacy_schema_migrations(&conn)
                .map_err(|e| format!("failed to migrate legacy schema: {e}"))?;
            conn.execute_batch(APP_SCHEMA_SQL)
                .map_err(|e| format!("failed to apply schema: {e}"))?;
            sync_existing_repo_configs(&conn)
                .map_err(|e| format!("failed to sync repo configs: {e}"))?;
            eprintln!("[DCC] Database ready at {:?}", db_path);
            app.manage(WorkspaceCommandState::new(db_path.clone()));
            app.manage(SessionCommandState::new(
                app.handle().clone(),
                db_path.clone(),
            ));
            let state = AppState {
                db_path: Arc::new(db_path),
                app_data_dir: Arc::new(app_data_dir.clone()),
                conn: Arc::new(Mutex::new(conn)),
                terminals: Arc::new(Mutex::new(HashMap::new())),
                daemon: Arc::new(DaemonState {
                    started_at: local_now_string(),
                    last_tick_at: Arc::new(Mutex::new(None)),
                    running: Arc::new(AtomicBool::new(true)),
                    system: Arc::new(Mutex::new(System::new_all())),
                }),
                daemon_endpoint: Arc::new(Mutex::new(None)),
            };
            let app_handle = app.handle().clone();
            match ensure_sidecar_running(
                &app_handle,
                state.db_path.as_ref(),
                state.app_data_dir.as_ref(),
            ) {
                Ok(runtime) => {
                    if let Ok(mut endpoint) = state.daemon_endpoint.lock() {
                        *endpoint = Some(runtime);
                    }
                    state.daemon.running.store(false, Ordering::Relaxed);
                }
                Err(err) => {
                    eprintln!(
                        "[DCC][daemon] sidecar unavailable, using in-process fallback: {err}"
                    );
                    let state_for_daemon = state.clone();
                    start_daemon_worker(app_handle, state_for_daemon);
                }
            }
            app.manage(state);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn main() {
    run();
}

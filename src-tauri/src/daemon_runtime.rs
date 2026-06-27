use chrono::{DateTime, Datelike, Duration as ChronoDuration, Local, Timelike};
use dcc_tauri::git::configure_git_command;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use sysinfo::{Pid, ProcessRefreshKind, System};

const APP_SCHEMA_SQL: &str = include_str!("../sql/schema.sql");

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoTaskTriggerPayload {
    #[serde(default)]
    pub when: Option<String>,
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub provider_id: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoTaskPayload {
    pub id: String,
    pub name: String,
    pub command: String,
    pub schedule: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub cwd_mode: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub trigger: Option<RepoTaskTriggerPayload>,
}

#[derive(Debug, Clone)]
struct ProviderRow {
    provider_type: String,
    api_key: String,
    base_url: Option<String>,
    model: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoProcessPayload {
    pub id: String,
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub cwd_mode: Option<String>,
    #[serde(default)]
    pub auto_restart: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct RepoConfigPayload {
    #[serde(default)]
    tasks: Vec<RepoTaskPayload>,
    #[serde(default)]
    processes: Vec<RepoProcessPayload>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct RepoConfigTomlLite {
    #[serde(default)]
    tasks: Vec<RepoTaskTomlLite>,
    #[serde(default)]
    processes: Vec<RepoProcessTomlLite>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct RepoTaskTomlLite {
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
    trigger: Option<RepoTaskTriggerTomlLite>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct RepoTaskTriggerTomlLite {
    #[serde(default)]
    when: Option<String>,
    #[serde(default)]
    prompt: Option<String>,
    #[serde(default)]
    provider_id: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct RepoProcessTomlLite {
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

#[derive(Debug, Clone)]
struct RepoTaskRuntime {
    project_id: String,
    project_name: String,
    project_path: String,
    task: RepoTaskPayload,
}

#[derive(Debug, Clone)]
struct RepoProcessRuntime {
    project_id: String,
    project_name: String,
    project_path: String,
    process: RepoProcessPayload,
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

#[derive(Debug, Clone, Default)]
struct DaemonProcessState {
    pty_id: Option<String>,
    pane_id: Option<String>,
    comb_id: Option<String>,
    status: String,
    pid: Option<u32>,
    exit_code: Option<i32>,
    restart_count: i32,
    last_restart_at: Option<String>,
    backoff_seconds: i32,
    cpu_percent: f64,
    memory_mb: f64,
    last_metrics_at: Option<String>,
    last_error: Option<String>,
    last_output_excerpt: Option<String>,
    started_at: Option<String>,
    stopped_at: Option<String>,
    updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DaemonHealthSnapshot {
    pid: u32,
    cpu_percent: f64,
    memory_mb: f64,
    last_metrics_at: String,
}

#[derive(Debug)]
struct RunningTask {
    child: Child,
    output_buffer: Arc<Mutex<VecDeque<String>>>,
    started_at: String,
    runtime: RepoTaskRuntime,
    attached: bool,
    next_run_at: Option<String>,
}

#[derive(Debug)]
struct ManagedProcess {
    child: Child,
    output_buffer: Arc<Mutex<VecDeque<String>>>,
    started_at: String,
    runtime: RepoProcessRuntime,
    restart_count: i32,
    backoff_seconds: i32,
    should_restart: bool,
}

#[derive(Debug, Clone)]
pub struct DaemonHealth {
    pub started_at: String,
    pub last_tick_at: Arc<Mutex<Option<String>>>,
    pub running: Arc<AtomicBool>,
}

#[derive(Debug)]
pub struct DaemonService {
    pub db_path: Arc<PathBuf>,
    pub started_at: String,
    pub last_tick_at: Arc<Mutex<Option<String>>>,
    pub running: Arc<AtomicBool>,
    conn: Arc<Mutex<Connection>>,
    active_runs: Arc<Mutex<HashMap<String, RunningTask>>>,
    managed_processes: Arc<Mutex<HashMap<String, ManagedProcess>>>,
    system: Arc<Mutex<System>>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RpcRequest {
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RpcResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

fn local_now_string() -> String {
    Local::now().to_rfc3339()
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
    conn.execute_batch(APP_SCHEMA_SQL)
        .map_err(|e| e.to_string())?;
    Ok(conn)
}

fn path_with_node_modules_bin(project_path: &str) -> String {
    let bin_path = format!("{}/node_modules/.bin", project_path);
    let current_path = std::env::var("PATH").unwrap_or_default();
    if current_path.is_empty() {
        bin_path
    } else {
        format!("{}:{}", bin_path, current_path)
    }
}

fn local_shell_command(command: &str) -> (String, Vec<String>) {
    if cfg!(windows) {
        (
            "cmd".to_string(),
            vec!["/C".to_string(), command.to_string()],
        )
    } else {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
        // -i: interactive (garante leitura do ~/.zshrc com nvm/fnm/volta)
        // -l: login shell (garante leitura do ~/.zprofile e /etc/profile)
        (shell, vec!["-ilc".to_string(), command.to_string()])
    }
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
struct CronSchedule {
    seconds: Vec<u32>,
    minutes: Vec<u32>,
    hours: Vec<u32>,
    days_of_month: Vec<u32>,
    months: Vec<u32>,
    days_of_week: Vec<u32>,
}

fn expand_cron_field(raw: &str, min: u32, max: u32) -> Result<Vec<u32>, String> {
    let mut values = Vec::new();
    for part in raw.split(',').map(|p| p.trim()).filter(|p| !p.is_empty()) {
        let (range_part, step) = if let Some((lhs, rhs)) = part.split_once('/') {
            let step = rhs
                .parse::<u32>()
                .map_err(|_| format!("passo cron inválido: {rhs}"))?;
            (lhs.trim(), step.max(1))
        } else {
            (part, 1)
        };

        let (start, end) = if range_part == "*" {
            (min, max)
        } else if let Some((start_raw, end_raw)) = range_part.split_once('-') {
            let start = start_raw
                .parse::<u32>()
                .map_err(|_| format!("valor cron inválido: {start_raw}"))?;
            let end = end_raw
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
        .filter(|part| !part.trim().is_empty())
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

fn parse_task_toml_from_repo_config(path: &Path) -> Vec<RepoTaskPayload> {
    let config_path = path.join(".dcc.toml");
    let raw = match fs::read_to_string(config_path) {
        Ok(content) => content,
        Err(_) => return Vec::new(),
    };
    let parsed = toml::from_str::<RepoConfigTomlLite>(&raw).ok();
    parsed
        .map(|config| {
            config
                .tasks
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
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn parse_repo_config_tasks(raw: Option<&str>, project_path: &Path) -> Vec<RepoTaskPayload> {
    if let Some(raw) = raw {
        if let Ok(parsed) = serde_json::from_str::<RepoConfigPayload>(raw) {
            if !parsed.tasks.is_empty() {
                return parsed.tasks;
            }
        }
    }
    parse_task_toml_from_repo_config(project_path)
}

fn parse_process_toml_from_repo_config(path: &Path) -> Vec<RepoProcessPayload> {
    let config_path = path.join(".dcc.toml");
    let raw = match fs::read_to_string(config_path) {
        Ok(content) => content,
        Err(_) => return Vec::new(),
    };
    let parsed = toml::from_str::<RepoConfigTomlLite>(&raw).ok();
    parsed
        .map(|config| {
            config
                .processes
                .into_iter()
                .map(|process| RepoProcessPayload {
                    id: process.id,
                    name: process.name,
                    command: process.command,
                    description: process.description,
                    cwd_mode: process.cwd_mode,
                    auto_restart: process.auto_restart,
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn parse_repo_config_processes(raw: Option<&str>, project_path: &Path) -> Vec<RepoProcessPayload> {
    if let Some(raw) = raw {
        if let Ok(parsed) = serde_json::from_str::<RepoConfigPayload>(raw) {
            if !parsed.processes.is_empty() {
                return parsed.processes;
            }
        }
    }
    parse_process_toml_from_repo_config(project_path)
}

fn read_daemon_task_state(
    conn: &Connection,
    project_id: &str,
    task_id: &str,
) -> Result<Option<DaemonTaskRunState>, String> {
    conn.query_row(
        "SELECT pty_id, pane_id, comb_id, status, attached, next_run_at, last_run_at, last_exit_code, last_error, last_output_excerpt, updated_at
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
                updated_at: row.get(10)?,
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
            &runtime.task.schedule,
            cwd_mode,
            if enabled { 1 } else { 0 },
            trigger.as_ref().and_then(|trigger| trigger.when.clone()),
            trigger.as_ref().and_then(|trigger| trigger.prompt.clone()),
            trigger.as_ref().and_then(|trigger| trigger.provider_id.clone()),
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

fn read_daemon_process_state(
    conn: &Connection,
    project_id: &str,
    process_id: &str,
) -> Result<Option<DaemonProcessState>, String> {
    conn.query_row(
        "SELECT pty_id, pane_id, comb_id, status, pid, exit_code, restart_count, last_restart_at,
                backoff_seconds, cpu_percent, memory_mb, last_metrics_at, last_error, last_output_excerpt,
                started_at, stopped_at, updated_at
         FROM daemon_processes
         WHERE project_id = ?1 AND process_id = ?2
         LIMIT 1",
        params![project_id, process_id],
        |row| {
            let pid_i64: Option<i64> = row.get(4)?;
            let exit_code_i64: Option<i64> = row.get(5)?;
            let restart_count_i64: i64 = row.get(6).unwrap_or(0);
            let backoff_i64: i64 = row.get(8).unwrap_or(1);
            Ok(DaemonProcessState {
                pty_id: row.get(0)?,
                pane_id: row.get(1)?,
                comb_id: row.get(2)?,
                status: row.get(3)?,
                pid: pid_i64.map(|p| p as u32),
                exit_code: exit_code_i64.map(|c| c as i32),
                restart_count: restart_count_i64 as i32,
                last_restart_at: row.get(7)?,
                backoff_seconds: backoff_i64 as i32,
                cpu_percent: row.get(9).unwrap_or(0.0),
                memory_mb: row.get(10).unwrap_or(0.0),
                last_metrics_at: row.get(11)?,
                last_error: row.get(12)?,
                last_output_excerpt: row.get(13)?,
                started_at: row.get(14)?,
                stopped_at: row.get(15)?,
                updated_at: row.get(16)?,
            })
        },
    )
    .optional()
    .map_err(|e| e.to_string())
}

fn upsert_daemon_process_state(
    conn: &Connection,
    runtime: &RepoProcessRuntime,
    state: &DaemonProcessState,
    cwd_mode: &str,
    auto_restart: bool,
) -> Result<(), String> {
    let id = format!(
        "daemon-process-state-{}-{}",
        runtime.project_id, runtime.process.id
    );
    conn.execute(
        "
        INSERT INTO daemon_processes (
          id, project_id, process_id, process_name, command, cwd_mode, auto_restart,
          status, pty_id, pane_id, comb_id, pid, exit_code, restart_count, last_restart_at,
          backoff_seconds, cpu_percent, memory_mb, last_metrics_at, last_output_excerpt, last_error,
          started_at, stopped_at, created_at, updated_at
        ) VALUES (
          ?1, ?2, ?3, ?4, ?5, ?6, ?7,
          ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15,
          ?16, ?17, ?18, ?19, ?20, ?21,
          ?22, ?23, datetime('now'), datetime('now')
        )
        ON CONFLICT(project_id, process_id) DO UPDATE SET
          process_name = excluded.process_name,
          command = excluded.command,
          cwd_mode = excluded.cwd_mode,
          auto_restart = excluded.auto_restart,
          status = excluded.status,
          pty_id = COALESCE(excluded.pty_id, daemon_processes.pty_id),
          pane_id = COALESCE(excluded.pane_id, daemon_processes.pane_id),
          comb_id = COALESCE(excluded.comb_id, daemon_processes.comb_id),
          pid = excluded.pid,
          exit_code = COALESCE(excluded.exit_code, daemon_processes.exit_code),
          restart_count = excluded.restart_count,
          last_restart_at = COALESCE(excluded.last_restart_at, daemon_processes.last_restart_at),
          backoff_seconds = excluded.backoff_seconds,
          cpu_percent = excluded.cpu_percent,
          memory_mb = excluded.memory_mb,
          last_metrics_at = COALESCE(excluded.last_metrics_at, daemon_processes.last_metrics_at),
          last_output_excerpt = COALESCE(excluded.last_output_excerpt, daemon_processes.last_output_excerpt),
          last_error = COALESCE(excluded.last_error, daemon_processes.last_error),
          started_at = COALESCE(excluded.started_at, daemon_processes.started_at),
          stopped_at = COALESCE(excluded.stopped_at, daemon_processes.stopped_at),
          updated_at = datetime('now')
        ",
        params![
            id,
            &runtime.project_id,
            &runtime.process.id,
            &runtime.process.name,
            &runtime.process.command,
            cwd_mode,
            if auto_restart { 1 } else { 0 },
            state.status.clone(),
            state.pty_id.clone(),
            state.pane_id.clone(),
            state.comb_id.clone(),
            state.pid.map(|p| p as i64),
            state.exit_code.map(|c| c as i64),
            state.restart_count as i64,
            state.last_restart_at.clone(),
            state.backoff_seconds as i64,
            state.cpu_percent,
            state.memory_mb,
            state.last_metrics_at.clone(),
            state.last_output_excerpt.clone(),
            state.last_error.clone(),
            state.started_at.clone(),
            state.stopped_at.clone(),
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn daemon_process_state_to_payload(
    runtime: &RepoProcessRuntime,
    state: Option<DaemonProcessState>,
) -> Value {
    let state = state.unwrap_or_default();
    let status = if state.status.is_empty() {
        "stopped".to_string()
    } else {
        state.status.clone()
    };
    serde_json::json!({
        "projectId": runtime.project_id.clone(),
        "projectName": runtime.project_name.clone(),
        "processId": runtime.process.id.clone(),
        "processName": runtime.process.name.clone(),
        "command": runtime.process.command.clone(),
        "cwdMode": normalize_task_cwd_mode(runtime.process.cwd_mode.as_deref()),
        "autoRestart": runtime.process.auto_restart.unwrap_or(true),
        "status": status,
        "ptyId": state.pty_id,
        "paneId": state.pane_id,
        "combId": state.comb_id,
        "pid": state.pid,
        "exitCode": state.exit_code,
        "restartCount": state.restart_count,
        "lastRestartAt": state.last_restart_at,
        "backoffSeconds": state.backoff_seconds,
        "cpuPercent": state.cpu_percent,
        "memoryMb": state.memory_mb,
        "lastMetricsAt": state.last_metrics_at,
        "lastError": state.last_error,
        "lastOutputExcerpt": state.last_output_excerpt,
        "startedAt": state.started_at,
        "stoppedAt": state.stopped_at,
        "updatedAt": state.updated_at,
    })
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
    local_shell_command(command)
}

fn task_key(project_id: &str, task_id: &str) -> String {
    format!("{project_id}:{task_id}")
}

fn process_key(project_id: &str, process_id: &str) -> String {
    format!("{project_id}:{process_id}")
}

fn collect_process_metrics(system: &mut System, pid: u32) -> Option<(f64, f64)> {
    let sysinfo_pid = Pid::from_u32(pid);

    // Atualizar informações do processo específico, incluindo CPU
    // Nota: Para medições precisas de CPU, a sysinfo precisa de múltiplas amostragens.
    // A primeira chamada inicializa o estado, as subsequentes retornam valores reais.
    system.refresh_process_specifics(
        sysinfo_pid,
        ProcessRefreshKind::new().with_cpu().with_memory(),
    );

    if let Some(process) = system.process(sysinfo_pid) {
        // CPU em porcentagem
        let cpu_percent = process.cpu_usage() as f64;

        // Memória em MB
        let memory_mb = process.memory() as f64 / 1_024.0 / 1_024.0;

        Some((cpu_percent, memory_mb))
    } else {
        None
    }
}

fn collect_daemon_health_snapshot(system: &mut System) -> DaemonHealthSnapshot {
    let pid = std::process::id();
    let (cpu_percent, memory_mb) = collect_process_metrics(system, pid).unwrap_or((0.0, 0.0));

    DaemonHealthSnapshot {
        pid,
        cpu_percent,
        memory_mb,
        last_metrics_at: local_now_string(),
    }
}

fn git_output_in_path(cwd: &Path, args: &[&str]) -> Result<String, String> {
    let mut command = Command::new("git");
    configure_git_command(&mut command);
    command.current_dir(cwd).args(args);
    let output = command.output().map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

fn parse_json_value(raw: Option<&str>) -> Option<Value> {
    raw.and_then(|value| serde_json::from_str::<Value>(value).ok())
}

fn buffer_append_line(buffer: &Arc<Mutex<VecDeque<String>>>, label: &str, line: &str) {
    if let Ok(mut guard) = buffer.lock() {
        let formatted = if label.is_empty() {
            line.to_string()
        } else {
            format!("{label}{line}")
        };
        while guard.len() >= 160 {
            guard.pop_front();
        }
        guard.push_back(formatted);
    }
}

fn spawn_stream_reader<R: Read + Send + 'static>(
    reader: R,
    buffer: Arc<Mutex<VecDeque<String>>>,
    label: &'static str,
) {
    thread::spawn(move || {
        let reader = BufReader::new(reader);
        for line in reader.lines() {
            match line {
                Ok(line) => buffer_append_line(&buffer, label, &line),
                Err(err) => {
                    buffer_append_line(&buffer, label, &format!("[stream error] {err}"));
                    break;
                }
            }
        }
    });
}

impl DaemonService {
    pub fn new(db_path: PathBuf) -> Result<Arc<Self>, String> {
        let conn = open_connection(&db_path)?;
        Ok(Arc::new(Self {
            db_path: Arc::new(db_path),
            started_at: local_now_string(),
            last_tick_at: Arc::new(Mutex::new(None)),
            running: Arc::new(AtomicBool::new(true)),
            conn: Arc::new(Mutex::new(conn)),
            active_runs: Arc::new(Mutex::new(HashMap::new())),
            managed_processes: Arc::new(Mutex::new(HashMap::new())),
            system: Arc::new(Mutex::new(System::new_all())),
        }))
    }

    fn with_conn<T>(&self, f: impl FnOnce(&Connection) -> Result<T, String>) -> Result<T, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "daemon db lock poisoned".to_string())?;
        f(&conn)
    }

    fn load_tasks(&self) -> Result<Vec<RepoTaskRuntime>, String> {
        self.with_conn(|conn| {
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
                let project_path_buf = PathBuf::from(&project_path);
                for task in parse_repo_config_tasks(repo_config_raw.as_deref(), &project_path_buf) {
                    tasks.push(RepoTaskRuntime {
                        project_id: project_id.clone(),
                        project_name: project_name.clone(),
                        project_path: project_path.clone(),
                        task,
                    });
                }
            }
            Ok(tasks)
        })
    }

    fn load_processes(&self) -> Result<Vec<RepoProcessRuntime>, String> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare("SELECT id, name, path, repo_config FROM projects ORDER BY name ASC")
                .map_err(|e| e.to_string())?;
            let mut rows = stmt.query([]).map_err(|e| e.to_string())?;
            let mut processes = Vec::new();
            while let Some(row) = rows.next().map_err(|e| e.to_string())? {
                let project_id: String = row.get(0).map_err(|e| e.to_string())?;
                let project_name: String = row.get(1).map_err(|e| e.to_string())?;
                let project_path: String = row.get(2).map_err(|e| e.to_string())?;
                let repo_config_raw: Option<String> = row.get(3).map_err(|e| e.to_string())?;
                let project_path_buf = PathBuf::from(&project_path);
                for process in
                    parse_repo_config_processes(repo_config_raw.as_deref(), &project_path_buf)
                {
                    processes.push(RepoProcessRuntime {
                        project_id: project_id.clone(),
                        project_name: project_name.clone(),
                        project_path: project_path.clone(),
                        process,
                    });
                }
            }
            Ok(processes)
        })
    }

    fn resolve_task_cwd(
        &self,
        runtime: &RepoTaskRuntime,
        cwd_mode: &str,
    ) -> Result<String, String> {
        if cwd_mode == "project" {
            return Ok(runtime.project_path.clone());
        }

        self.with_conn(|conn| {
            let worktree = conn
                .query_row(
                    "SELECT worktree_path FROM combs WHERE project_id = ?1 AND worktree_path IS NOT NULL AND worktree_path != '' ORDER BY COALESCE(last_git_activity_at, last_opened_at, created_at) DESC LIMIT 1",
                    params![runtime.project_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            Ok(worktree.unwrap_or_else(|| runtime.project_path.clone()))
        })
    }

    fn resolve_process_cwd(
        &self,
        runtime: &RepoProcessRuntime,
        cwd_mode: &str,
    ) -> Result<String, String> {
        if cwd_mode == "project" {
            return Ok(runtime.project_path.clone());
        }

        self.with_conn(|conn| {
            let worktree = conn
                .query_row(
                    "SELECT worktree_path FROM combs WHERE project_id = ?1 AND worktree_path IS NOT NULL AND worktree_path != '' ORDER BY COALESCE(last_git_activity_at, last_opened_at, created_at) DESC LIMIT 1",
                    params![runtime.project_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            Ok(worktree.unwrap_or_else(|| runtime.project_path.clone()))
        })
    }

    fn create_running_task(
        &self,
        runtime: RepoTaskRuntime,
        attached: bool,
    ) -> Result<Value, String> {
        let cwd_mode = normalize_task_cwd_mode(runtime.task.cwd_mode.as_deref());
        let cwd = self.resolve_task_cwd(&runtime, &cwd_mode)?;
        let (program, args) = task_shell_command(&runtime.task.command);
        let mut command = Command::new(program);
        command.args(args);
        command.current_dir(&cwd);
        command.env("PATH", path_with_node_modules_bin(&runtime.project_path));
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());

        let mut child = command.spawn().map_err(|e| e.to_string())?;
        let pid = child.id();
        let pty_id = format!(
            "daemon-task-{}-{}-{pid}",
            runtime.project_id, runtime.task.id
        );
        let output_buffer = Arc::new(Mutex::new(VecDeque::with_capacity(160)));

        if let Some(stdout) = child.stdout.take() {
            spawn_stream_reader(stdout, output_buffer.clone(), "");
        }
        if let Some(stderr) = child.stderr.take() {
            spawn_stream_reader(stderr, output_buffer.clone(), "[stderr] ");
        }

        let next_run_at = task_next_run_at(&runtime.task, Local::now())?;
        let state = DaemonTaskRunState {
            pty_id: Some(pty_id.clone()),
            pane_id: None,
            comb_id: None,
            status: "running".to_string(),
            attached,
            next_run_at,
            last_run_at: Some(local_now_string()),
            last_exit_code: None,
            last_error: None,
            last_output_excerpt: None,
            updated_at: Some(local_now_string()),
        };
        self.with_conn(|conn| {
            upsert_daemon_task_state(
                conn,
                &runtime,
                &state,
                &cwd_mode,
                runtime.task.enabled.unwrap_or(true),
            )
        })?;

        let mut active_runs = self
            .active_runs
            .lock()
            .map_err(|_| "daemon run lock poisoned".to_string())?;
        active_runs.insert(
            task_key(&runtime.project_id, &runtime.task.id),
            RunningTask {
                child,
                output_buffer,
                started_at: state.last_run_at.clone().unwrap_or_else(local_now_string),
                runtime: runtime.clone(),
                attached,
                next_run_at: state.next_run_at.clone(),
            },
        );
        Ok(daemon_task_state_to_payload(&runtime, Some(state)))
    }

    fn create_managed_process(&self, runtime: RepoProcessRuntime) -> Result<Value, String> {
        let cwd_mode = normalize_task_cwd_mode(runtime.process.cwd_mode.as_deref());
        let cwd = self.resolve_process_cwd(&runtime, &cwd_mode)?;
        let (program, args) = task_shell_command(&runtime.process.command);
        let mut command = Command::new(program);
        command.args(args);
        command.current_dir(&cwd);
        command.env("PATH", path_with_node_modules_bin(&runtime.project_path));
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());

        let mut child = command.spawn().map_err(|e| e.to_string())?;
        let pid = child.id();
        let pty_id = format!(
            "daemon-process-{}-{}-{pid}",
            runtime.project_id, runtime.process.id
        );
        let output_buffer = Arc::new(Mutex::new(VecDeque::with_capacity(160)));

        if let Some(stdout) = child.stdout.take() {
            spawn_stream_reader(stdout, output_buffer.clone(), "");
        }
        if let Some(stderr) = child.stderr.take() {
            spawn_stream_reader(stderr, output_buffer.clone(), "[stderr] ");
        }

        let state = DaemonProcessState {
            pty_id: Some(pty_id.clone()),
            pane_id: None,
            comb_id: None,
            status: "running".to_string(),
            pid: Some(pid),
            exit_code: None,
            restart_count: 0,
            last_restart_at: None,
            backoff_seconds: 1,
            cpu_percent: 0.0,
            memory_mb: 0.0,
            last_metrics_at: None,
            last_error: None,
            last_output_excerpt: None,
            started_at: Some(local_now_string()),
            stopped_at: None,
            updated_at: Some(local_now_string()),
        };
        self.with_conn(|conn| {
            upsert_daemon_process_state(
                conn,
                &runtime,
                &state,
                &cwd_mode,
                runtime.process.auto_restart.unwrap_or(true),
            )
        })?;

        let mut managed = self
            .managed_processes
            .lock()
            .map_err(|_| "daemon process lock poisoned".to_string())?;
        managed.insert(
            process_key(&runtime.project_id, &runtime.process.id),
            ManagedProcess {
                child,
                output_buffer,
                started_at: state.started_at.clone().unwrap_or_else(local_now_string),
                runtime: runtime.clone(),
                restart_count: 0,
                backoff_seconds: 1,
                should_restart: runtime.process.auto_restart.unwrap_or(true),
            },
        );
        Ok(daemon_process_state_to_payload(&runtime, Some(state)))
    }

    fn sweep_finished_tasks(&self) -> Result<(), String> {
        let mut finished = Vec::new();
        {
            let mut active = self
                .active_runs
                .lock()
                .map_err(|_| "daemon run lock poisoned".to_string())?;
            for (key, task) in active.iter_mut() {
                if let Some(status) = task.child.try_wait().map_err(|e| e.to_string())? {
                    finished.push((
                        key.clone(),
                        task.runtime.clone(),
                        task.started_at.clone(),
                        status.code().unwrap_or(-1),
                        task.output_buffer.clone(),
                        task.attached,
                        task.next_run_at.clone(),
                    ));
                }
            }
            for (key, _, _, _, _, _, _) in &finished {
                active.remove(key);
            }
        }

        if finished.is_empty() {
            return Ok(());
        }

        let mut triggers_to_execute = Vec::new();

        self.with_conn(|conn| {
            for (_, runtime, started_at, code, output_buffer, _, _) in finished {
                let excerpt = output_buffer.lock().ok().and_then(|buffer| {
                    buffer
                        .iter()
                        .rev()
                        .find(|line| !line.trim().is_empty())
                        .map(|line| line.trim().chars().take(240).collect::<String>())
                });
                let cwd_mode = normalize_task_cwd_mode(runtime.task.cwd_mode.as_deref());
                let mut row = read_daemon_task_state(conn, &runtime.project_id, &runtime.task.id)?
                    .unwrap_or_default();
                row.status = if code == 0 {
                    "completed".to_string()
                } else {
                    "failed".to_string()
                };
                row.attached = false;
                row.last_exit_code = Some(i64::from(code));
                row.last_output_excerpt = excerpt;
                row.last_error = if code == 0 {
                    None
                } else {
                    Some(format!("exit code {code}"))
                };
                row.last_run_at = Some(started_at);
                upsert_daemon_task_state(
                    conn,
                    &runtime,
                    &row,
                    &cwd_mode,
                    runtime.task.enabled.unwrap_or(true),
                )?;

                // Coletar trigger para execução posterior
                if let Some(trigger) = &runtime.task.trigger {
                    if should_trigger_execute(trigger, code) {
                        triggers_to_execute.push((
                            runtime.clone(),
                            trigger.clone(),
                            code,
                            row.last_output_excerpt.clone(),
                        ));
                    }
                }
            }
            Ok(())
        })?;

        // Executar triggers coletados (fora do with_conn para evitar deadlock)
        for (runtime, trigger, exit_code, output_excerpt) in triggers_to_execute {
            if let Err(err) = execute_trigger(&runtime, &trigger, exit_code, &output_excerpt, self)
            {
                eprintln!(
                    "[DCC][trigger] Erro ao executar trigger para task '{}': {}",
                    runtime.task.name, err
                );
            }
        }

        Ok(())
    }

    fn sweep_managed_processes(&self) -> Result<(), String> {
        let mut finished = Vec::new();
        let mut to_restart = Vec::new();
        let mut metrics_updates = Vec::new();

        // Obter lock do system para coleta de métricas
        let mut system = self
            .system
            .lock()
            .map_err(|_| "system lock poisoned".to_string())?;

        // Atualizar todos os processos para ter baseline de CPU
        // Isso é necessário para que as medições de CPU sejam precisas
        system.refresh_processes_specifics(ProcessRefreshKind::new().with_cpu());

        {
            let mut managed = self
                .managed_processes
                .lock()
                .map_err(|_| "daemon process lock poisoned".to_string())?;

            for (key, process) in managed.iter_mut() {
                let pid = process.child.id();

                // Coletar métricas de processos em execução
                if let Some(status) = process.child.try_wait().map_err(|e| e.to_string())? {
                    // Processo terminou
                    let exit_code = status.code().unwrap_or(-1);

                    finished.push((
                        key.clone(),
                        process.runtime.clone(),
                        process.started_at.clone(),
                        exit_code,
                        process.output_buffer.clone(),
                        process.restart_count,
                        process.backoff_seconds,
                        process.should_restart,
                    ));
                } else {
                    // Processo ainda está rodando - coletar métricas
                    if let Some((cpu_percent, memory_mb)) =
                        collect_process_metrics(&mut system, pid)
                    {
                        metrics_updates.push((
                            process.runtime.project_id.clone(),
                            process.runtime.process.id.clone(),
                            cpu_percent,
                            memory_mb,
                        ));
                    }
                }
            }

            for (key, _, _, _, _, _, _, _) in &finished {
                managed.remove(key);
            }
        }

        // Atualizar métricas de processos em execução
        if !metrics_updates.is_empty() {
            self.with_conn(|conn| {
                for (project_id, process_id, cpu_percent, memory_mb) in &metrics_updates {
                    // Atualizar apenas os campos de métricas
                    conn.execute(
                        "UPDATE daemon_processes
                         SET cpu_percent = ?1, memory_mb = ?2, last_metrics_at = datetime('now')
                         WHERE project_id = ?3 AND process_id = ?4",
                        params![cpu_percent, memory_mb, project_id, process_id],
                    )
                    .map_err(|e| e.to_string())?;
                }
                Ok::<(), String>(())
            })?;
        }

        if finished.is_empty() {
            return Ok(());
        }

        self.with_conn(|conn| {
            for (
                _,
                runtime,
                _started_at,
                code,
                output_buffer,
                restart_count,
                backoff_seconds,
                should_restart,
            ) in finished
            {
                let excerpt = output_buffer.lock().ok().and_then(|buffer| {
                    buffer
                        .iter()
                        .rev()
                        .find(|line| !line.trim().is_empty())
                        .map(|line| line.trim().chars().take(240).collect::<String>())
                });

                let cwd_mode = normalize_task_cwd_mode(runtime.process.cwd_mode.as_deref());
                let mut row =
                    read_daemon_process_state(conn, &runtime.project_id, &runtime.process.id)?
                        .unwrap_or_default();

                let crashed = code != 0;

                row.status = if crashed {
                    "crashed".to_string()
                } else {
                    "stopped".to_string()
                };
                row.pid = None;
                row.exit_code = Some(code);
                row.last_output_excerpt = excerpt;
                row.last_error = if crashed {
                    Some(format!("exit code {code}"))
                } else {
                    None
                };
                row.stopped_at = Some(local_now_string());

                // Auto-restart logic com backoff exponencial
                if should_restart && crashed {
                    let new_restart_count = restart_count + 1;
                    let new_backoff = (backoff_seconds * 2).min(300); // max 5 minutos

                    row.restart_count = new_restart_count;
                    row.backoff_seconds = new_backoff;
                    row.last_restart_at = Some(local_now_string());
                    row.status = "restarting".to_string();

                    println!(
                        "[daemon] Process {} crashed (exit {}), will restart in {}s (restart #{})",
                        runtime.process.name, code, new_backoff, new_restart_count
                    );

                    // Preparar para restart após backoff
                    to_restart.push((runtime.clone(), new_backoff, new_restart_count));
                } else if crashed {
                    row.restart_count = restart_count;
                    println!(
                        "[daemon] Process {} crashed (exit {}) but auto_restart is disabled",
                        runtime.process.name, code
                    );
                }

                upsert_daemon_process_state(
                    conn,
                    &runtime,
                    &row,
                    &cwd_mode,
                    runtime.process.auto_restart.unwrap_or(true),
                )?;
            }
            Ok(())
        })?;

        // Restart processes após backoff
        for (runtime, backoff_seconds, restart_count) in to_restart {
            thread::sleep(Duration::from_secs(backoff_seconds as u64));

            // Re-criar o processo
            match self.restart_managed_process(runtime.clone(), restart_count, backoff_seconds) {
                Ok(_) => {
                    println!(
                        "[daemon] Process {} restarted successfully",
                        runtime.process.name
                    );
                }
                Err(e) => {
                    println!(
                        "[daemon] Failed to restart process {}: {}",
                        runtime.process.name, e
                    );
                    // Marcar como failed no banco
                    let _ = self.with_conn(|conn| {
                        let cwd_mode = normalize_task_cwd_mode(runtime.process.cwd_mode.as_deref());
                        let mut row = read_daemon_process_state(
                            conn,
                            &runtime.project_id,
                            &runtime.process.id,
                        )?
                        .unwrap_or_default();
                        row.status = "failed".to_string();
                        row.last_error = Some(format!("restart failed: {}", e));
                        upsert_daemon_process_state(
                            conn,
                            &runtime,
                            &row,
                            &cwd_mode,
                            runtime.process.auto_restart.unwrap_or(true),
                        )
                    });
                }
            }
        }

        Ok(())
    }

    fn restart_managed_process(
        &self,
        runtime: RepoProcessRuntime,
        restart_count: i32,
        backoff_seconds: i32,
    ) -> Result<Value, String> {
        let cwd_mode = normalize_task_cwd_mode(runtime.process.cwd_mode.as_deref());
        let cwd = self.resolve_process_cwd(&runtime, &cwd_mode)?;
        let (program, args) = task_shell_command(&runtime.process.command);
        let mut command = Command::new(program);
        command.args(args);
        command.current_dir(&cwd);
        command.env("PATH", path_with_node_modules_bin(&runtime.project_path));
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());

        let mut child = command.spawn().map_err(|e| e.to_string())?;
        let pid = child.id();
        let pty_id = format!(
            "daemon-process-{}-{}-{pid}",
            runtime.project_id, runtime.process.id
        );
        let output_buffer = Arc::new(Mutex::new(VecDeque::with_capacity(160)));

        if let Some(stdout) = child.stdout.take() {
            spawn_stream_reader(stdout, output_buffer.clone(), "");
        }
        if let Some(stderr) = child.stderr.take() {
            spawn_stream_reader(stderr, output_buffer.clone(), "[stderr] ");
        }

        let state = DaemonProcessState {
            pty_id: Some(pty_id.clone()),
            pane_id: None,
            comb_id: None,
            status: "running".to_string(),
            pid: Some(pid),
            exit_code: None,
            restart_count,
            last_restart_at: Some(local_now_string()),
            backoff_seconds,
            cpu_percent: 0.0,
            memory_mb: 0.0,
            last_metrics_at: None,
            last_error: None,
            last_output_excerpt: None,
            started_at: Some(local_now_string()),
            stopped_at: None,
            updated_at: Some(local_now_string()),
        };

        self.with_conn(|conn| {
            upsert_daemon_process_state(
                conn,
                &runtime,
                &state,
                &cwd_mode,
                runtime.process.auto_restart.unwrap_or(true),
            )
        })?;

        let mut managed = self
            .managed_processes
            .lock()
            .map_err(|_| "daemon process lock poisoned".to_string())?;
        managed.insert(
            process_key(&runtime.project_id, &runtime.process.id),
            ManagedProcess {
                child,
                output_buffer,
                started_at: state.started_at.clone().unwrap_or_else(local_now_string),
                runtime: runtime.clone(),
                restart_count,
                backoff_seconds,
                should_restart: runtime.process.auto_restart.unwrap_or(true),
            },
        );
        Ok(daemon_process_state_to_payload(&runtime, Some(state)))
    }

    pub fn status(&self) -> Result<Value, String> {
        self.sweep_finished_tasks()?;
        self.sweep_managed_processes()?;
        let health = {
            let mut system = self
                .system
                .lock()
                .map_err(|_| "system lock poisoned".to_string())?;
            // sweep_managed_processes já fez refresh, mas garantimos aqui também
            system.refresh_processes_specifics(ProcessRefreshKind::new().with_cpu());
            collect_daemon_health_snapshot(&mut system)
        };
        let tasks = self.load_tasks()?;
        let mut total_tasks = 0i64;
        let mut enabled_tasks = 0i64;
        let mut running_tasks = 0i64;

        self.with_conn(|conn| {
            for runtime in &tasks {
                total_tasks += 1;
                if runtime.task.enabled.unwrap_or(true) {
                    enabled_tasks += 1;
                }
                if let Some(state_row) =
                    read_daemon_task_state(conn, &runtime.project_id, &runtime.task.id)?
                {
                    if state_row.status == "running" {
                        running_tasks += 1;
                    }
                }
            }
            Ok(())
        })?;

        let last_tick_at = self
            .last_tick_at
            .lock()
            .map_err(|_| "daemon lock poisoned".to_string())?
            .clone();

        Ok(serde_json::json!({
            "mode": "sidecar",
            "running": self.running.load(Ordering::Relaxed),
            "startedAt": self.started_at.clone(),
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

    pub fn health(&self) -> Result<Value, String> {
        let health = {
            let mut system = self
                .system
                .lock()
                .map_err(|_| "system lock poisoned".to_string())?;
            // Atualizar processos para ter baseline de CPU atualizada
            system.refresh_processes_specifics(ProcessRefreshKind::new().with_cpu());
            collect_daemon_health_snapshot(&mut system)
        };
        let last_tick_at = self
            .last_tick_at
            .lock()
            .map_err(|_| "daemon lock poisoned".to_string())?
            .clone();

        Ok(serde_json::json!({
            "ok": true,
            "mode": "sidecar",
            "running": self.running.load(Ordering::Relaxed),
            "startedAt": self.started_at.clone(),
            "lastTickAt": last_tick_at,
            "pid": health.pid,
            "cpuPercent": health.cpu_percent,
            "memoryMb": health.memory_mb,
            "lastMetricsAt": health.last_metrics_at,
        }))
    }

    pub fn list_tasks(&self) -> Result<Value, String> {
        self.sweep_finished_tasks()?;
        self.sweep_managed_processes()?;
        let tasks = self.load_tasks()?;
        self.with_conn(|conn| {
            let mut out = Vec::new();
            for runtime in tasks {
                let state_row =
                    read_daemon_task_state(conn, &runtime.project_id, &runtime.task.id)?;
                out.push(daemon_task_state_to_payload(&runtime, state_row));
            }
            Ok(Value::Array(out))
        })
    }

    pub fn list_combs(&self, project_id: Option<&str>) -> Result<Value, String> {
        // The live worktrees live in `dcc_workspaces` (the legacy `combs` table
        // is empty in current installs). The mobile companion reads this via
        // `/api/v1/combs`, so we map dcc_workspaces onto the comb shape it
        // expects. The project group label is the basename of `root_path`
        // (e.g. ".../vendeagora-app" → "vendeagora-app"); `dcc_workspaces`
        // carries no FK into `projects`, so we derive it.
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT w.id, w.project_id, w.name, w.base_branch, w.worktree_path,
                            w.root_path, w.state, w.created_at, w.updated_at
                     FROM dcc_workspaces w
                     ORDER BY w.updated_at DESC, w.created_at DESC",
                )
                .map_err(|e| e.to_string())?;
            let mut rows = stmt.query([]).map_err(|e| e.to_string())?;
            let mut out = Vec::new();
            while let Some(row) = rows.next().map_err(|e| e.to_string())? {
                let row_project_id: String = row.get(1).map_err(|e| e.to_string())?;
                if let Some(filter) = project_id {
                    if filter != row_project_id {
                        continue;
                    }
                }
                let root_path: Option<String> = row.get(5).map_err(|e| e.to_string())?;
                let project_name = root_path
                    .as_deref()
                    .map(|p| p.trim_end_matches('/'))
                    .and_then(|p| p.rsplit('/').next())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| row_project_id.clone());
                let base_branch: Option<String> = row.get(3).map_err(|e| e.to_string())?;
                out.push(serde_json::json!({
                    "id": row.get::<_, String>(0).map_err(|e| e.to_string())?,
                    "projectId": row_project_id,
                    "projectName": project_name,
                    "projectPath": root_path,
                    "name": row.get::<_, String>(2).map_err(|e| e.to_string())?,
                    "description": Value::Null,
                    "baseBranch": base_branch.clone(),
                    "branch": base_branch,
                    "worktreePath": row.get::<_, Option<String>>(4).map_err(|e| e.to_string())?,
                    "reviewTargets": Value::Null,
                    "forgeLink": Value::Null,
                    "status": row.get::<_, String>(6).map_err(|e| e.to_string())?,
                    "isPinned": false,
                    "pinnedAt": Value::Null,
                    "lastOpenedAt": row.get::<_, Option<String>>(8).map_err(|e| e.to_string())?,
                    "createdAt": row.get::<_, Option<String>>(7).map_err(|e| e.to_string())?,
                    "updatedAt": row.get::<_, Option<String>>(8).map_err(|e| e.to_string())?,
                    "lastGitActivityAt": Value::Null,
                }));
            }
            Ok(Value::Array(out))
        })
    }

    /// Live session list for the mobile companion, mapped onto the
    /// `SessionSearchResult` shape. Sources from `dcc_sessions` joined to the
    /// live `dcc_workspaces` — the INNER JOIN drops orphaned sessions whose
    /// worktree was deleted/recreated (the stale FTS index keeps those around
    /// and is what made the phone show threads the desktop no longer lists).
    pub fn list_live_sessions(&self, limit: u64) -> Result<Value, String> {
        let safe_limit = limit.clamp(1, 200) as i64;
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT s.id, COALESCE(NULLIF(t.title, ''), w.name) AS thread_title,
                            s.workspace_id, w.name AS workspace_name, w.base_branch,
                            w.project_id, w.root_path, s.provider_id, s.model,
                            s.updated_at, t.archived_at
                     FROM dcc_sessions s
                     JOIN dcc_workspaces w ON w.id = s.workspace_id
                     LEFT JOIN dcc_threads t ON t.session_id = s.id
                     WHERE w.state != 'archived'
                     ORDER BY s.updated_at DESC
                     LIMIT ?1",
                )
                .map_err(|e| e.to_string())?;
            let mut rows = stmt.query([safe_limit]).map_err(|e| e.to_string())?;
            let mut out = Vec::new();
            while let Some(row) = rows.next().map_err(|e| e.to_string())? {
                let root_path: Option<String> = row.get(6).map_err(|e| e.to_string())?;
                let project_name = root_path
                    .as_deref()
                    .map(|p| p.trim_end_matches('/'))
                    .and_then(|p| p.rsplit('/').next())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string());
                out.push(serde_json::json!({
                    "sessionId": row.get::<_, String>(0).map_err(|e| e.to_string())?,
                    "threadTitle": row.get::<_, Option<String>>(1).map_err(|e| e.to_string())?,
                    "snippet": Value::Null,
                    "providerId": row.get::<_, Option<String>>(7).map_err(|e| e.to_string())?,
                    "model": row.get::<_, Option<String>>(8).map_err(|e| e.to_string())?,
                    "workspaceName": row.get::<_, Option<String>>(3).map_err(|e| e.to_string())?,
                    "workspaceBranch": row.get::<_, Option<String>>(4).map_err(|e| e.to_string())?,
                    "workspaceId": row.get::<_, Option<String>>(2).map_err(|e| e.to_string())?,
                    "projectId": row.get::<_, Option<String>>(5).map_err(|e| e.to_string())?,
                    "projectName": project_name,
                    "updatedAt": row.get::<_, Option<String>>(9).map_err(|e| e.to_string())?,
                    "archivedAt": row.get::<_, Option<String>>(10).map_err(|e| e.to_string())?,
                }));
            }
            Ok(Value::Array(out))
        })
    }

    pub fn list_processes(&self, project_id: Option<&str>) -> Result<Value, String> {
        self.sweep_managed_processes()?;
        let processes = self.load_processes()?;
        self.with_conn(|conn| {
            let mut out = Vec::new();
            for runtime in processes {
                if let Some(filter) = project_id {
                    if filter != runtime.project_id {
                        continue;
                    }
                }
                let state_row =
                    read_daemon_process_state(conn, &runtime.project_id, &runtime.process.id)?;
                out.push(daemon_process_state_to_payload(&runtime, state_row));
            }
            Ok(Value::Array(out))
        })
    }

    pub fn start_process(&self, project_id: &str, process_id: &str) -> Result<Value, String> {
        self.sweep_managed_processes()?;
        let processes = self.load_processes()?;
        let runtime = processes
            .into_iter()
            .find(|item| item.project_id == project_id && item.process.id == process_id)
            .ok_or_else(|| "process not found".to_string())?;

        // Verificar se já está rodando
        let managed = self
            .managed_processes
            .lock()
            .map_err(|_| "daemon process lock poisoned".to_string())?;
        let key = process_key(project_id, process_id);
        if managed.contains_key(&key) {
            return Err("process already running".to_string());
        }
        drop(managed);

        self.create_managed_process(runtime)
    }

    pub fn stop_process(&self, project_id: &str, process_id: &str) -> Result<Value, String> {
        let key = process_key(project_id, process_id);
        let mut managed = self
            .managed_processes
            .lock()
            .map_err(|_| "daemon process lock poisoned".to_string())?;

        if let Some(mut process) = managed.remove(&key) {
            // Matar o processo
            let _ = process.child.kill();
            drop(managed);

            // Atualizar estado no banco
            let processes = self.load_processes()?;
            let runtime = processes
                .into_iter()
                .find(|item| item.project_id == project_id && item.process.id == process_id)
                .ok_or_else(|| "process not found".to_string())?;

            self.with_conn(|conn| {
                let cwd_mode = normalize_task_cwd_mode(runtime.process.cwd_mode.as_deref());
                let mut state =
                    read_daemon_process_state(conn, project_id, process_id)?.unwrap_or_default();
                state.status = "stopped".to_string();
                state.pid = None;
                state.stopped_at = Some(local_now_string());
                upsert_daemon_process_state(conn, &runtime, &state, &cwd_mode, false)?;
                Ok(daemon_process_state_to_payload(&runtime, Some(state)))
            })
        } else {
            Err("process not running".to_string())
        }
    }

    pub fn restart_process(&self, project_id: &str, process_id: &str) -> Result<Value, String> {
        // Parar o processo se estiver rodando
        let _ = self.stop_process(project_id, process_id);

        // Aguardar um pouco para garantir que o processo foi encerrado
        thread::sleep(Duration::from_millis(500));

        // Iniciar novamente
        self.start_process(project_id, process_id)
    }

    pub fn list_panes(
        &self,
        project_id: Option<&str>,
        comb_id: Option<&str>,
    ) -> Result<Value, String> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT p.id, p.comb_id, p.type, p.provider_id, p.title, p.initial_prompt, p.cwd,
                            p.pty_owner_key, p.status, p.layout_order, p.last_activity_at,
                            p.created_at, p.updated_at, c.project_id, c.name, pr.name
                     FROM panes p
                     LEFT JOIN combs c ON c.id = p.comb_id
                     LEFT JOIN projects pr ON pr.id = c.project_id
                     ORDER BY p.layout_order ASC, p.created_at ASC",
                )
                .map_err(|e| e.to_string())?;
            let mut rows = stmt.query([]).map_err(|e| e.to_string())?;
            let mut out = Vec::new();
            while let Some(row) = rows.next().map_err(|e| e.to_string())? {
                let row_comb_id: String = row.get(1).map_err(|e| e.to_string())?;
                let row_project_id: Option<String> = row.get(13).map_err(|e| e.to_string())?;
                if let Some(filter) = comb_id {
                    if filter != row_comb_id {
                        continue;
                    }
                }
                if let Some(filter) = project_id {
                    if row_project_id.as_deref() != Some(filter) {
                        continue;
                    }
                }
                out.push(serde_json::json!({
                    "id": row.get::<_, String>(0).map_err(|e| e.to_string())?,
                    "combId": row_comb_id,
                    "type": row.get::<_, String>(2).map_err(|e| e.to_string())?,
                    "providerId": row.get::<_, Option<String>>(3).map_err(|e| e.to_string())?,
                    "title": row.get::<_, Option<String>>(4).map_err(|e| e.to_string())?,
                    "initialPrompt": row.get::<_, Option<String>>(5).map_err(|e| e.to_string())?,
                    "cwd": row.get::<_, Option<String>>(6).map_err(|e| e.to_string())?,
                    "ptyOwnerKey": row.get::<_, Option<String>>(7).map_err(|e| e.to_string())?,
                    "status": row.get::<_, String>(8).map_err(|e| e.to_string())?,
                    "layoutOrder": row.get::<_, i64>(9).map_err(|e| e.to_string())?,
                    "lastActivityAt": row.get::<_, Option<String>>(10).map_err(|e| e.to_string())?,
                    "createdAt": row.get::<_, Option<String>>(11).map_err(|e| e.to_string())?,
                    "updatedAt": row.get::<_, Option<String>>(12).map_err(|e| e.to_string())?,
                    "projectId": row_project_id,
                    "projectName": row.get::<_, Option<String>>(15).map_err(|e| e.to_string())?,
                    "combName": row.get::<_, Option<String>>(14).map_err(|e| e.to_string())?,
                }));
            }
            Ok(Value::Array(out))
        })
    }

    pub fn diffs_bundle(
        &self,
        worktree_paths: Vec<String>,
        comb_ids: Vec<String>,
    ) -> Result<Value, String> {
        let mut paths = worktree_paths;
        if !comb_ids.is_empty() {
            let extra_paths = self.with_conn(|conn| {
                let mut stmt = conn
                    .prepare("SELECT worktree_path FROM dcc_workspaces WHERE id = ?1 AND worktree_path IS NOT NULL AND worktree_path != ''")
                    .map_err(|e| e.to_string())?;
                let mut out = Vec::new();
                for comb_id in &comb_ids {
                    let maybe_path: Option<String> = stmt
                        .query_row(params![comb_id], |row| row.get(0))
                        .optional()
                        .map_err(|e| e.to_string())?;
                    if let Some(path) = maybe_path {
                        out.push(path);
                    }
                }
                Ok(out)
            })?;
            paths.extend(extra_paths);
        }

        paths.sort();
        paths.dedup();

        let mut out = Vec::new();
        for worktree_path in paths {
            let cwd = PathBuf::from(&worktree_path);
            let branch = git_output_in_path(&cwd, &["branch", "--show-current"]);
            let status = git_output_in_path(&cwd, &["status", "--short", "--untracked-files=all"]);
            let stat = git_output_in_path(&cwd, &["diff", "--stat"]);
            let name_status = git_output_in_path(&cwd, &["diff", "--name-status"]);
            let error = branch
                .as_ref()
                .err()
                .or_else(|| status.as_ref().err())
                .or_else(|| stat.as_ref().err())
                .or_else(|| name_status.as_ref().err())
                .map(|value| value.to_string());

            out.push(serde_json::json!({
                "worktreePath": worktree_path,
                "branch": branch.ok(),
                "status": status.ok(),
                "stat": stat.ok(),
                "nameStatus": name_status.ok(),
                "error": error,
            }));
        }

        Ok(Value::Array(out))
    }

    pub fn run_task(&self, project_id: &str, task_id: &str) -> Result<Value, String> {
        self.sweep_finished_tasks()?;
        self.sweep_managed_processes()?;
        let tasks = self.load_tasks()?;
        let runtime = tasks
            .into_iter()
            .find(|item| item.project_id == project_id && item.task.id == task_id)
            .ok_or_else(|| "task not found".to_string())?;
        self.create_running_task(runtime, true)
    }

    pub fn attach_task(&self, project_id: &str, task_id: &str) -> Result<Value, String> {
        let tasks = self.load_tasks()?;
        let runtime = tasks
            .into_iter()
            .find(|item| item.project_id == project_id && item.task.id == task_id)
            .ok_or_else(|| "task not found".to_string())?;
        self.with_conn(|conn| {
            let mut row = read_daemon_task_state(conn, project_id, task_id)?.unwrap_or_default();
            row.attached = true;
            if row.status.is_empty() {
                row.status = "idle".into();
            }
            upsert_daemon_task_state(
                conn,
                &runtime,
                &row,
                &normalize_task_cwd_mode(runtime.task.cwd_mode.as_deref()),
                runtime.task.enabled.unwrap_or(true),
            )
        })?;
        self.with_conn(|conn| {
            let state_row = read_daemon_task_state(conn, project_id, task_id)?;
            Ok(daemon_task_state_to_payload(&runtime, state_row))
        })
    }

    pub fn detach_task(&self, project_id: &str, task_id: &str) -> Result<Value, String> {
        let tasks = self.load_tasks()?;
        let runtime = tasks
            .into_iter()
            .find(|item| item.project_id == project_id && item.task.id == task_id)
            .ok_or_else(|| "task not found".to_string())?;
        self.with_conn(|conn| {
            let mut row = read_daemon_task_state(conn, project_id, task_id)?.unwrap_or_default();
            row.attached = false;
            if row.status.is_empty() {
                row.status = "idle".into();
            }
            upsert_daemon_task_state(
                conn,
                &runtime,
                &row,
                &normalize_task_cwd_mode(runtime.task.cwd_mode.as_deref()),
                runtime.task.enabled.unwrap_or(true),
            )
        })?;
        self.with_conn(|conn| {
            let state_row = read_daemon_task_state(conn, project_id, task_id)?;
            Ok(daemon_task_state_to_payload(&runtime, state_row))
        })
    }

    fn tick(&self) -> Result<(), String> {
        if !self.running.load(Ordering::Relaxed) {
            return Ok(());
        }

        {
            let mut last_tick = self
                .last_tick_at
                .lock()
                .map_err(|_| "daemon lock poisoned".to_string())?;
            *last_tick = Some(local_now_string());
        }

        self.sweep_finished_tasks()?;
        self.sweep_managed_processes()?;
        let now = Local::now();
        let tasks = self.load_tasks()?;

        for runtime in tasks {
            let enabled = runtime.task.enabled.unwrap_or(true);
            let cwd_mode = normalize_task_cwd_mode(runtime.task.cwd_mode.as_deref());
            let mut should_run = false;
            let mut next_run_at = None;

            self.with_conn(|conn| {
                let mut state_row =
                    read_daemon_task_state(conn, &runtime.project_id, &runtime.task.id)?
                        .unwrap_or_default();
                if !enabled {
                    state_row.status = "disabled".to_string();
                    state_row.attached = false;
                    state_row.next_run_at = None;
                    upsert_daemon_task_state(conn, &runtime, &state_row, &cwd_mode, false)?;
                    return Ok(());
                }

                if state_row.status == "running" {
                    return Ok(());
                }

                if state_row.next_run_at.is_none() {
                    state_row.next_run_at = task_next_run_at(&runtime.task, now).ok().flatten();
                    upsert_daemon_task_state(conn, &runtime, &state_row, &cwd_mode, true)?;
                    next_run_at = state_row.next_run_at.clone();
                    return Ok(());
                }

                if let Some(due_at) = state_row
                    .next_run_at
                    .as_deref()
                    .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
                    .map(|dt| dt.with_timezone(&Local))
                {
                    if now >= due_at {
                        should_run = true;
                        next_run_at = task_next_run_at(&runtime.task, now).ok().flatten();
                    }
                }
                Ok(())
            })?;

            if should_run {
                let _ = self.create_running_task(runtime.clone(), false)?;
                if let Some(next_run_at) = next_run_at {
                    self.with_conn(|conn| {
                        let mut state_row =
                            read_daemon_task_state(conn, &runtime.project_id, &runtime.task.id)?
                                .unwrap_or_default();
                        state_row.next_run_at = Some(next_run_at);
                        upsert_daemon_task_state(conn, &runtime, &state_row, &cwd_mode, true)
                    })?;
                }
            }
        }

        Ok(())
    }
}

fn handle_rpc(service: &DaemonService, request: RpcRequest) -> RpcResponse {
    let result = match request.method.as_str() {
        "daemon.health" => service.health(),
        "daemon.getStatus" => service.status(),
        "daemon.listTasks" => service.list_tasks(),
        "daemon.runTask" => {
            match (
                request
                    .params
                    .get("projectId")
                    .and_then(|value| value.as_str()),
                request
                    .params
                    .get("taskId")
                    .and_then(|value| value.as_str()),
            ) {
                (Some(project_id), Some(task_id)) => service.run_task(project_id, task_id),
                (None, _) => Err("missing projectId".to_string()),
                (_, None) => Err("missing taskId".to_string()),
            }
        }
        "daemon.attachTask" => {
            match (
                request
                    .params
                    .get("projectId")
                    .and_then(|value| value.as_str()),
                request
                    .params
                    .get("taskId")
                    .and_then(|value| value.as_str()),
            ) {
                (Some(project_id), Some(task_id)) => service.attach_task(project_id, task_id),
                (None, _) => Err("missing projectId".to_string()),
                (_, None) => Err("missing taskId".to_string()),
            }
        }
        "daemon.detachTask" => {
            match (
                request
                    .params
                    .get("projectId")
                    .and_then(|value| value.as_str()),
                request
                    .params
                    .get("taskId")
                    .and_then(|value| value.as_str()),
            ) {
                (Some(project_id), Some(task_id)) => service.detach_task(project_id, task_id),
                (None, _) => Err("missing projectId".to_string()),
                (_, None) => Err("missing taskId".to_string()),
            }
        }
        "daemon.listProcesses" => {
            let project_id = request
                .params
                .get("projectId")
                .and_then(|value| value.as_str());
            service.list_processes(project_id)
        }
        "daemon.startProcess" => {
            match (
                request
                    .params
                    .get("projectId")
                    .and_then(|value| value.as_str()),
                request
                    .params
                    .get("processId")
                    .and_then(|value| value.as_str()),
            ) {
                (Some(project_id), Some(process_id)) => {
                    service.start_process(project_id, process_id)
                }
                (None, _) => Err("missing projectId".to_string()),
                (_, None) => Err("missing processId".to_string()),
            }
        }
        "daemon.stopProcess" => {
            match (
                request
                    .params
                    .get("projectId")
                    .and_then(|value| value.as_str()),
                request
                    .params
                    .get("processId")
                    .and_then(|value| value.as_str()),
            ) {
                (Some(project_id), Some(process_id)) => {
                    service.stop_process(project_id, process_id)
                }
                (None, _) => Err("missing projectId".to_string()),
                (_, None) => Err("missing processId".to_string()),
            }
        }
        "daemon.restartProcess" => {
            match (
                request
                    .params
                    .get("projectId")
                    .and_then(|value| value.as_str()),
                request
                    .params
                    .get("processId")
                    .and_then(|value| value.as_str()),
            ) {
                (Some(project_id), Some(process_id)) => {
                    service.restart_process(project_id, process_id)
                }
                (None, _) => Err("missing projectId".to_string()),
                (_, None) => Err("missing processId".to_string()),
            }
        }
        "combs.list" => {
            let project_id = request
                .params
                .get("projectId")
                .and_then(|value| value.as_str());
            service.list_combs(project_id)
        }
        "panes.list" => {
            let project_id = request
                .params
                .get("projectId")
                .and_then(|value| value.as_str());
            let comb_id = request
                .params
                .get("combId")
                .and_then(|value| value.as_str());
            service.list_panes(project_id, comb_id)
        }
        "diffs.bundle" => {
            let worktree_paths = request
                .params
                .get("worktreePaths")
                .and_then(|value| value.as_array())
                .map(|values| {
                    values
                        .iter()
                        .filter_map(|value| value.as_str().map(|s| s.to_string()))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let comb_ids = request
                .params
                .get("combIds")
                .and_then(|value| value.as_array())
                .map(|values| {
                    values
                        .iter()
                        .filter_map(|value| value.as_str().map(|s| s.to_string()))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            service.diffs_bundle(worktree_paths, comb_ids)
        }
        "sessions.live" => {
            let limit = request
                .params
                .get("limit")
                .and_then(|value| value.as_u64())
                .unwrap_or(60);
            service.list_live_sessions(limit)
        }
        _ => Err(format!("unknown method: {}", request.method)),
    };

    match result {
        Ok(result) => RpcResponse {
            ok: true,
            result: Some(result),
            error: None,
        },
        Err(error) => RpcResponse {
            ok: false,
            result: None,
            error: Some(error),
        },
    }
}

impl DaemonService {
    fn process_pending_requests(&self) -> Result<(), String> {
        let pending = self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT id, method, params_json FROM daemon_rpc_requests
                     WHERE status = 'pending'
                     ORDER BY created_at ASC
                     LIMIT 32",
                )
                .map_err(|e| e.to_string())?;
            let mut rows = stmt.query([]).map_err(|e| e.to_string())?;
            let mut out = Vec::new();
            while let Some(row) = rows.next().map_err(|e| e.to_string())? {
                let request_id: String = row.get(0).map_err(|e| e.to_string())?;
                let method: String = row.get(1).map_err(|e| e.to_string())?;
                let params_json: Option<String> = row.get(2).map_err(|e| e.to_string())?;
                out.push((request_id, method, params_json));
            }
            Ok(out)
        })?;

        if pending.is_empty() {
            return Ok(());
        }

        for (request_id, method, params_json) in pending {
            let params_value = params_json
                .as_deref()
                .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
                .unwrap_or(Value::Null);
            let request = RpcRequest {
                method,
                params: params_value,
            };
            let response = handle_rpc(self, request);
            let response_json = serde_json::to_string(&response).map_err(|e| e.to_string())?;
            self.with_conn(|conn| {
                if response.ok {
                    conn.execute(
                        "UPDATE daemon_rpc_requests SET status = 'done', response_json = ?2, error = NULL WHERE id = ?1",
                        params![request_id, response_json],
                    )
                    .map_err(|e| e.to_string())?;
                } else {
                    conn.execute(
                        "UPDATE daemon_rpc_requests SET status = 'error', response_json = ?2, error = ?3 WHERE id = ?1",
                        params![request_id, response_json, response.error.clone()],
                    )
                    .map_err(|e| e.to_string())?;
                }
                Ok(())
            })?;
        }
        Ok(())
    }

    fn get_provider(&self, provider_id: &str) -> Result<ProviderRow, String> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT provider_type, api_key, base_url, model
                 FROM providers WHERE id = ?1",
                params![provider_id],
                |row| {
                    Ok(ProviderRow {
                        provider_type: row.get(0)?,
                        api_key: row.get(1)?,
                        base_url: row.get(2)?,
                        model: row.get(3)?,
                    })
                },
            )
            .map_err(|e| format!("Provider '{}' não encontrado: {}", provider_id, e))
        })
    }
}

fn should_trigger_execute(trigger: &RepoTaskTriggerPayload, exit_code: i32) -> bool {
    match trigger.when.as_deref() {
        Some("success") => exit_code == 0,
        Some("failure") => exit_code != 0,
        Some("complete") => true,
        _ => false,
    }
}

fn render_trigger_prompt(
    template: &str,
    task_name: &str,
    command: &str,
    exit_code: i32,
    output: &str,
) -> String {
    template
        .replace("{{task_name}}", task_name)
        .replace("{{command}}", command)
        .replace("{{exit_code}}", &exit_code.to_string())
        .replace("{{output}}", output)
        .replace(
            "{{status}}",
            if exit_code == 0 { "success" } else { "failure" },
        )
}

fn call_anthropic_api(provider: &ProviderRow, prompt: &str) -> Result<String, String> {
    let client = reqwest::blocking::Client::new();
    let model = provider
        .model
        .as_deref()
        .unwrap_or("claude-3-5-sonnet-20241022");
    let base_url = provider
        .base_url
        .as_deref()
        .unwrap_or("https://api.anthropic.com");

    let response = client
        .post(format!("{}/v1/messages", base_url))
        .header("x-api-key", &provider.api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&serde_json::json!({
            "model": model,
            "max_tokens": 1024,
            "messages": [{
                "role": "user",
                "content": prompt
            }]
        }))
        .send()
        .map_err(|e| format!("Erro HTTP Anthropic: {e}"))?;

    if !response.status().is_success() {
        return Err(format!(
            "Anthropic API erro {}: {}",
            response.status(),
            response.text().unwrap_or_default()
        ));
    }

    let data: Value = response
        .json()
        .map_err(|e| format!("Erro ao parsear response Anthropic: {e}"))?;

    let text = data["content"][0]["text"]
        .as_str()
        .ok_or("Response sem campo 'content[0].text'")?;

    Ok(text.to_string())
}

fn call_openai_api(provider: &ProviderRow, prompt: &str) -> Result<String, String> {
    let client = reqwest::blocking::Client::new();
    let model = provider.model.as_deref().unwrap_or("gpt-4");
    let base_url = provider
        .base_url
        .as_deref()
        .unwrap_or("https://api.openai.com");

    let response = client
        .post(format!("{}/v1/chat/completions", base_url))
        .header("Authorization", format!("Bearer {}", provider.api_key))
        .header("content-type", "application/json")
        .json(&serde_json::json!({
            "model": model,
            "max_tokens": 1024,
            "messages": [{
                "role": "user",
                "content": prompt
            }]
        }))
        .send()
        .map_err(|e| format!("Erro HTTP OpenAI: {e}"))?;

    if !response.status().is_success() {
        return Err(format!(
            "OpenAI API erro {}: {}",
            response.status(),
            response.text().unwrap_or_default()
        ));
    }

    let data: Value = response
        .json()
        .map_err(|e| format!("Erro ao parsear response OpenAI: {e}"))?;

    let text = data["choices"][0]["message"]["content"]
        .as_str()
        .ok_or("Response sem campo 'choices[0].message.content'")?;

    Ok(text.to_string())
}

fn call_ai_provider(provider: &ProviderRow, prompt: &str) -> Result<String, String> {
    match provider.provider_type.as_str() {
        "anthropic" => call_anthropic_api(provider, prompt),
        "openai" => call_openai_api(provider, prompt),
        _ => Err(format!(
            "Provider type '{}' não suportado",
            provider.provider_type
        )),
    }
}

fn log_trigger_execution(
    runtime: &RepoTaskRuntime,
    trigger: &RepoTaskTriggerPayload,
    response: &str,
) -> Result<(), String> {
    let response_preview = if response.len() > 200 {
        format!("{}... (truncated)", &response[..200])
    } else {
        response.to_string()
    };

    println!(
        "[DCC][trigger] Task '{}' trigger executado: provider={} when={} response=\"{}\"",
        runtime.task.name,
        trigger.provider_id.as_deref().unwrap_or("(none)"),
        trigger.when.as_deref().unwrap_or("(none)"),
        response_preview
    );
    Ok(())
}

fn execute_trigger(
    runtime: &RepoTaskRuntime,
    trigger: &RepoTaskTriggerPayload,
    exit_code: i32,
    output_excerpt: &Option<String>,
    service: &DaemonService,
) -> Result<(), String> {
    // 1. Validar se há prompt e provider_id
    let prompt_template = trigger
        .prompt
        .as_ref()
        .ok_or("Trigger sem prompt configurado")?;
    let provider_id = trigger
        .provider_id
        .as_ref()
        .ok_or("Trigger sem provider_id configurado")?;

    // 2. Buscar provider no banco
    let provider = service.get_provider(provider_id)?;

    // 3. Substituir variáveis no prompt
    let prompt = render_trigger_prompt(
        prompt_template,
        &runtime.task.name,
        &runtime.task.command,
        exit_code,
        output_excerpt.as_deref().unwrap_or("(sem output)"),
    );

    // 4. Enviar para provider
    let response = call_ai_provider(&provider, &prompt)?;

    // 5. Logar execução
    log_trigger_execution(runtime, trigger, &response)?;

    Ok(())
}

fn request_loop(service: Arc<DaemonService>) {
    while service.running.load(Ordering::Relaxed) {
        if let Err(error) = service.process_pending_requests() {
            eprintln!("[DCC][dccd] request error: {error}");
        }
        thread::sleep(Duration::from_millis(200));
    }
}

fn sweep_loop(service: Arc<DaemonService>) {
    while service.running.load(Ordering::Relaxed) {
        if let Err(error) = service.tick() {
            eprintln!("[DCC][dccd] tick error: {error}");
        }
        thread::sleep(Duration::from_secs(5));
    }
}

pub fn serve(service: Arc<DaemonService>, runtime_file: &Path) -> Result<(), String> {
    service.running.store(true, Ordering::Relaxed);
    let runtime = serde_json::json!({
        "pid": std::process::id(),
        "startedAt": service.started_at.clone(),
        "dbPath": service.db_path.to_string_lossy(),
    });
    eprintln!("[DCC][dccd] runtime file: {}", runtime_file.display());
    if let Some(parent) = runtime_file.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(
        runtime_file,
        serde_json::to_string_pretty(&runtime).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    let request_service = service.clone();
    thread::spawn(move || request_loop(request_service));

    let sweep_service = service.clone();
    thread::spawn(move || sweep_loop(sweep_service));

    eprintln!("[DCC][dccd] sidecar running");
    loop {
        thread::sleep(Duration::from_secs(60));
    }
}

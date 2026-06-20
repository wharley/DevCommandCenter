mod fingerprint;
mod parser;
mod process;

use std::{
    collections::HashMap,
    ffi::OsString,
    io::{BufRead, BufReader, Read},
    path::{Component, Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use specta::Type;
use tauri::{AppHandle, Emitter, State};

use crate::{commands::workspace_support::preflight_workspace_root, state::WorkspaceCommandState};

use self::{
    fingerprint::build_diff_fingerprint,
    parser::{
        apply_agent_event, parse_agent_jsonl, parse_agent_jsonl_line, ParsedCodeRabbitAgentEvent,
        ParsedCodeRabbitAgentOutput,
    },
    process::{
        command_output_detail, command_stderr, command_stdout, detect_coderabbit_cli,
        kill_process_group, run_command_with_timeout, spawn_command, CODERABBIT_DEFAULT_TIMEOUT,
        CODERABBIT_DOCTOR_TIMEOUT, CODERABBIT_STATUS_TIMEOUT,
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum CodeRabbitCliStatusState {
    Ready,
    Unavailable,
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum CodeRabbitReviewType {
    All,
    Committed,
    Uncommitted,
}

impl CodeRabbitReviewType {
    pub(crate) fn as_cli_value(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Committed => "committed",
            Self::Uncommitted => "uncommitted",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum CodeRabbitFindingSeverity {
    Critical,
    Major,
    Minor,
    Trivial,
    Info,
    Unknown,
}

impl CodeRabbitFindingSeverity {
    pub(crate) fn from_cli_value(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "critical" => Self::Critical,
            "major" => Self::Major,
            "minor" => Self::Minor,
            "trivial" => Self::Trivial,
            "info" => Self::Info,
            _ => Self::Unknown,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceCodeRabbitCliStatusInput {
    pub workspace_root: Option<String>,
    pub cli_path: Option<String>,
    pub include_auth_status: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CodeRabbitAuthStatusOutput {
    pub checked: bool,
    pub success: bool,
    pub exit_code: Option<i32>,
    pub authenticated: Option<bool>,
    pub login: Option<String>,
    pub organization: Option<String>,
    pub message: Option<String>,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceCodeRabbitCliStatusOutput {
    pub cli_name: String,
    pub cli_path: Option<String>,
    pub installed: bool,
    pub status: CodeRabbitCliStatusState,
    pub version: Option<String>,
    pub message: String,
    pub login_command: String,
    pub auth: Option<CodeRabbitAuthStatusOutput>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceCodeRabbitDoctorInput {
    pub workspace_root: String,
    pub cli_path: Option<String>,
    pub timeout_seconds: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceCodeRabbitDoctorOutput {
    pub cli_name: String,
    pub cli_path: String,
    pub success: bool,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub started_at: String,
    pub completed_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceCodeRabbitFingerprintInput {
    pub workspace_root: String,
    pub review_type: Option<CodeRabbitReviewType>,
    pub base: Option<String>,
    pub base_commit: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CodeRabbitDiffFingerprint {
    pub review_type: CodeRabbitReviewType,
    pub head: Option<String>,
    pub current_branch: Option<String>,
    pub base_ref: Option<String>,
    pub merge_base: Option<String>,
    pub staged_diff_hash: Option<String>,
    pub unstaged_diff_hash: Option<String>,
    pub untracked_files_hash: Option<String>,
    pub committed_diff_hash: Option<String>,
    pub combined_hash: String,
    pub generated_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceCodeRabbitReviewInput {
    pub workspace_root: String,
    pub cli_path: Option<String>,
    pub review_type: Option<CodeRabbitReviewType>,
    pub base: Option<String>,
    pub base_commit: Option<String>,
    pub light: Option<bool>,
    #[serde(default)]
    pub config_paths: Vec<String>,
    pub timeout_seconds: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CodeRabbitFinding {
    pub id: String,
    pub severity: CodeRabbitFindingSeverity,
    pub severity_raw: String,
    pub path: String,
    pub start_line: Option<u32>,
    pub end_line: Option<u32>,
    pub side: Option<String>,
    pub comment: Option<String>,
    pub codegen_instructions: Option<String>,
    pub suggestions: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CodeRabbitReviewStatusEvent {
    pub event_type: String,
    pub status: Option<String>,
    pub message: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CodeRabbitReviewComplete {
    pub status: Option<String>,
    pub findings: Option<u32>,
    pub message: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceCodeRabbitReviewOutput {
    pub cli_name: String,
    pub cli_path: String,
    pub success: bool,
    pub exit_code: Option<i32>,
    pub review_type: CodeRabbitReviewType,
    pub fingerprint: CodeRabbitDiffFingerprint,
    pub findings: Vec<CodeRabbitFinding>,
    pub statuses: Vec<CodeRabbitReviewStatusEvent>,
    pub complete: Option<CodeRabbitReviewComplete>,
    pub errors: Vec<String>,
    pub event_count: u32,
    pub stdout: String,
    pub stderr: String,
    pub started_at: String,
    pub completed_at: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum CodeRabbitReviewJobStatus {
    Starting,
    Running,
    Succeeded,
    Failed,
    Canceled,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceCodeRabbitReviewStartOutput {
    pub job_id: String,
    pub status: CodeRabbitReviewJobStatus,
    pub started_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceCodeRabbitReviewJobInput {
    pub job_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceCodeRabbitReviewJobSnapshot {
    pub job_id: String,
    pub workspace_root: String,
    pub review_type: CodeRabbitReviewType,
    pub status: CodeRabbitReviewJobStatus,
    pub pid: Option<u32>,
    pub started_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
    pub cancel_requested: bool,
    pub message: Option<String>,
    pub result: Option<WorkspaceCodeRabbitReviewOutput>,
    pub errors: Vec<String>,
}

pub const CODERABBIT_REVIEW_EVENT_NAME: &str = "dcc/coderabbit/review/event";

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CodeRabbitReviewStreamEvent {
    pub job_id: String,
    pub workspace_root: String,
    pub event_type: String,
    pub status: Option<String>,
    pub message: Option<String>,
    pub finding: Option<CodeRabbitFinding>,
    pub complete: Option<CodeRabbitReviewComplete>,
    pub result: Option<WorkspaceCodeRabbitReviewOutput>,
    pub errors: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceCodeRabbitStoredReviewInput {
    pub workspace_root: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceCodeRabbitSaveReviewInput {
    pub workspace_root: String,
    pub review: WorkspaceCodeRabbitReviewOutput,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceCodeRabbitStoredReviewOutput {
    pub workspace_root: String,
    pub review: Option<WorkspaceCodeRabbitReviewOutput>,
    pub updated_at: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceCodeRabbitReviewHistoryInput {
    pub workspace_root: String,
    pub limit: Option<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceCodeRabbitReviewHistoryEntry {
    pub review_id: String,
    pub workspace_root: String,
    pub review: WorkspaceCodeRabbitReviewOutput,
    pub review_type: Option<String>,
    pub success: bool,
    pub findings_count: u32,
    pub fingerprint_hash: Option<String>,
    pub completed_at: Option<String>,
    pub saved_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceCodeRabbitReviewHistoryOutput {
    pub workspace_root: String,
    pub entries: Vec<WorkspaceCodeRabbitReviewHistoryEntry>,
}

#[derive(Clone, Default)]
pub struct CodeRabbitReviewJobsState {
    jobs: Arc<Mutex<HashMap<String, CodeRabbitReviewJobRecord>>>,
}

#[derive(Clone)]
struct CodeRabbitReviewJobRecord {
    snapshot: WorkspaceCodeRabbitReviewJobSnapshot,
    cancel_requested: Arc<AtomicBool>,
}

#[tauri::command]
pub async fn workspace_coderabbit_cli_status(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceCodeRabbitCliStatusInput,
) -> Result<WorkspaceCodeRabbitCliStatusOutput, String> {
    if let Some(root) = input
        .workspace_root
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        preflight_workspace_root(&state, root).await?;
    }

    let workspace_root = input
        .workspace_root
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);
    let cli_path = input.cli_path.clone();
    let include_auth_status = input.include_auth_status.unwrap_or(false);

    tauri::async_runtime::spawn_blocking(move || {
        let current_dir = workspace_root.as_deref();
        let cli = match detect_coderabbit_cli(cli_path.as_deref(), current_dir) {
            Ok(cli) => cli,
            Err(message) => {
                return Ok(WorkspaceCodeRabbitCliStatusOutput {
                    cli_name: "cr".to_string(),
                    cli_path: None,
                    installed: false,
                    status: CodeRabbitCliStatusState::Unavailable,
                    version: None,
                    message,
                    login_command: "cr auth login".to_string(),
                    auth: None,
                });
            }
        };

        let auth = if include_auth_status {
            Some(run_auth_status(&cli.path, current_dir))
        } else {
            None
        };
        let status = match auth.as_ref() {
            Some(auth) if !auth.success => CodeRabbitCliStatusState::Error,
            _ => CodeRabbitCliStatusState::Ready,
        };
        let message = match auth.as_ref() {
            Some(auth) if !auth.success => auth.message.clone().unwrap_or_else(|| {
                "CodeRabbit CLI is installed, but auth status failed".to_string()
            }),
            _ => "CodeRabbit CLI is installed".to_string(),
        };

        Ok(WorkspaceCodeRabbitCliStatusOutput {
            cli_name: cli.name,
            cli_path: Some(cli.path),
            installed: true,
            status,
            version: cli.version,
            message,
            login_command: "cr auth login".to_string(),
            auth,
        })
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn workspace_coderabbit_review_start(
    app: AppHandle,
    workspace_state: State<'_, WorkspaceCommandState>,
    jobs_state: State<'_, CodeRabbitReviewJobsState>,
    input: WorkspaceCodeRabbitReviewInput,
) -> Result<WorkspaceCodeRabbitReviewStartOutput, String> {
    preflight_workspace_root(&workspace_state, &input.workspace_root).await?;
    let workspace_root = normalize_workspace_root(&input.workspace_root)?;
    let review_type = input.review_type.unwrap_or(CodeRabbitReviewType::All);
    let job_id = format!("coderabbit-review-{}", uuid::Uuid::new_v4().simple());
    let started_at = Utc::now().to_rfc3339();
    let cancel_requested = Arc::new(AtomicBool::new(false));

    let snapshot = WorkspaceCodeRabbitReviewJobSnapshot {
        job_id: job_id.clone(),
        workspace_root: workspace_root.clone(),
        review_type,
        status: CodeRabbitReviewJobStatus::Starting,
        pid: None,
        started_at: started_at.clone(),
        updated_at: started_at.clone(),
        completed_at: None,
        cancel_requested: false,
        message: Some("CodeRabbit review queued".to_string()),
        result: None,
        errors: Vec::new(),
    };

    {
        let mut jobs = jobs_state
            .jobs
            .lock()
            .map_err(|_| "CodeRabbit review jobs lock poisoned".to_string())?;
        jobs.insert(
            job_id.clone(),
            CodeRabbitReviewJobRecord {
                snapshot,
                cancel_requested: cancel_requested.clone(),
            },
        );
    }

    let jobs_state = jobs_state.inner().clone();
    std::thread::spawn({
        let job_id = job_id.clone();
        move || {
            run_review_job(
                &jobs_state,
                app,
                job_id,
                input,
                workspace_root,
                cancel_requested,
            );
        }
    });

    Ok(WorkspaceCodeRabbitReviewStartOutput {
        job_id,
        status: CodeRabbitReviewJobStatus::Starting,
        started_at,
    })
}

#[tauri::command]
pub async fn workspace_coderabbit_review_job(
    jobs_state: State<'_, CodeRabbitReviewJobsState>,
    input: WorkspaceCodeRabbitReviewJobInput,
) -> Result<WorkspaceCodeRabbitReviewJobSnapshot, String> {
    get_review_job_snapshot(jobs_state.inner(), &input.job_id)
}

#[tauri::command]
pub async fn workspace_coderabbit_review_cancel(
    jobs_state: State<'_, CodeRabbitReviewJobsState>,
    input: WorkspaceCodeRabbitReviewJobInput,
) -> Result<WorkspaceCodeRabbitReviewJobSnapshot, String> {
    let pid = {
        let mut jobs = jobs_state
            .jobs
            .lock()
            .map_err(|_| "CodeRabbit review jobs lock poisoned".to_string())?;
        let record = jobs
            .get_mut(&input.job_id)
            .ok_or_else(|| format!("CodeRabbit review job not found: {}", input.job_id))?;
        record.cancel_requested.store(true, Ordering::Relaxed);
        record.snapshot.cancel_requested = true;
        record.snapshot.updated_at = Utc::now().to_rfc3339();
        record.snapshot.message = Some("Canceling CodeRabbit review".to_string());
        if matches!(
            record.snapshot.status,
            CodeRabbitReviewJobStatus::Starting | CodeRabbitReviewJobStatus::Running
        ) {
            record.snapshot.status = CodeRabbitReviewJobStatus::Running;
        }
        record.snapshot.pid
    };

    if let Some(pid) = pid {
        kill_process_group(pid);
    }

    get_review_job_snapshot(jobs_state.inner(), &input.job_id)
}

#[tauri::command]
pub async fn workspace_coderabbit_review_load(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceCodeRabbitStoredReviewInput,
) -> Result<WorkspaceCodeRabbitStoredReviewOutput, String> {
    preflight_workspace_root(&state, &input.workspace_root).await?;
    let workspace_root = normalize_workspace_root(&input.workspace_root)?;
    let db_path = state.db_path.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let conn = open_coderabbit_reviews_db(&db_path)?;
        let row = conn
            .query_row(
                "SELECT review_json, updated_at FROM workspace_coderabbit_reviews WHERE workspace_root = ?1",
                params![&workspace_root],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        let Some((review_json, updated_at)) = row else {
            return Ok(WorkspaceCodeRabbitStoredReviewOutput {
                workspace_root,
                review: None,
                updated_at: None,
            });
        };
        let review = serde_json::from_str::<WorkspaceCodeRabbitReviewOutput>(&review_json)
            .map_err(|error| error.to_string())?;
        Ok(WorkspaceCodeRabbitStoredReviewOutput {
            workspace_root,
            review: Some(review),
            updated_at,
        })
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn workspace_coderabbit_review_save(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceCodeRabbitSaveReviewInput,
) -> Result<WorkspaceCodeRabbitStoredReviewOutput, String> {
    preflight_workspace_root(&state, &input.workspace_root).await?;
    let workspace_root = normalize_workspace_root(&input.workspace_root)?;
    let db_path = state.db_path.clone();
    let review = input.review;

    tauri::async_runtime::spawn_blocking(move || {
        let conn = open_coderabbit_reviews_db(&db_path)?;
        let review_json = serde_json::to_string(&review).map_err(|error| error.to_string())?;
        conn.execute(
            "INSERT INTO workspace_coderabbit_reviews (
                workspace_root, review_json, fingerprint_hash, completed_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, datetime('now'))
             ON CONFLICT(workspace_root) DO UPDATE SET
                review_json = excluded.review_json,
                fingerprint_hash = excluded.fingerprint_hash,
                completed_at = excluded.completed_at,
                updated_at = datetime('now')",
            params![
                &workspace_root,
                &review_json,
                &review.fingerprint.combined_hash,
                &review.completed_at
            ],
        )
        .map_err(|error| error.to_string())?;
        conn.execute(
            "INSERT INTO workspace_coderabbit_review_history (
                review_id, workspace_root, review_json, review_type, success,
                findings_count, fingerprint_hash, completed_at, saved_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, datetime('now'))",
            params![
                format!("cr-review-{}", uuid::Uuid::new_v4().simple()),
                &workspace_root,
                &review_json,
                review.review_type.as_cli_value(),
                if review.success { 1 } else { 0 },
                review.findings.len() as i64,
                &review.fingerprint.combined_hash,
                &review.completed_at
            ],
        )
        .map_err(|error| error.to_string())?;
        let updated_at = conn
            .query_row(
                "SELECT updated_at FROM workspace_coderabbit_reviews WHERE workspace_root = ?1",
                params![&workspace_root],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()
            .map_err(|error| error.to_string())?
            .flatten();
        Ok(WorkspaceCodeRabbitStoredReviewOutput {
            workspace_root,
            review: Some(review),
            updated_at,
        })
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn workspace_coderabbit_review_history(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceCodeRabbitReviewHistoryInput,
) -> Result<WorkspaceCodeRabbitReviewHistoryOutput, String> {
    preflight_workspace_root(&state, &input.workspace_root).await?;
    let workspace_root = normalize_workspace_root(&input.workspace_root)?;
    let db_path = state.db_path.clone();
    let limit = input.limit.unwrap_or(20).clamp(1, 100);

    tauri::async_runtime::spawn_blocking(move || {
        let conn = open_coderabbit_reviews_db(&db_path)?;
        let mut stmt = conn
            .prepare(
                "SELECT review_id, workspace_root, review_json, review_type, success,
                        findings_count, fingerprint_hash, completed_at, saved_at
                 FROM workspace_coderabbit_review_history
                 WHERE workspace_root = ?1
                 ORDER BY saved_at DESC
                 LIMIT ?2",
            )
            .map_err(|error| error.to_string())?;
        let rows = stmt
            .query_map(params![&workspace_root, limit as i64], |row| {
                let review_json: String = row.get(2)?;
                let review = serde_json::from_str::<WorkspaceCodeRabbitReviewOutput>(&review_json)
                    .map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            2,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })?;
                Ok(WorkspaceCodeRabbitReviewHistoryEntry {
                    review_id: row.get(0)?,
                    workspace_root: row.get(1)?,
                    review,
                    review_type: row.get(3)?,
                    success: row.get::<_, i64>(4)? != 0,
                    findings_count: row.get::<_, i64>(5)? as u32,
                    fingerprint_hash: row.get(6)?,
                    completed_at: row.get(7)?,
                    saved_at: row.get(8)?,
                })
            })
            .map_err(|error| error.to_string())?;
        let entries = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        Ok(WorkspaceCodeRabbitReviewHistoryOutput {
            workspace_root,
            entries,
        })
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn workspace_coderabbit_review_clear(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceCodeRabbitStoredReviewInput,
) -> Result<(), String> {
    preflight_workspace_root(&state, &input.workspace_root).await?;
    let workspace_root = normalize_workspace_root(&input.workspace_root)?;
    let db_path = state.db_path.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let conn = open_coderabbit_reviews_db(&db_path)?;
        conn.execute(
            "DELETE FROM workspace_coderabbit_reviews WHERE workspace_root = ?1",
            params![&workspace_root],
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn workspace_coderabbit_doctor(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceCodeRabbitDoctorInput,
) -> Result<WorkspaceCodeRabbitDoctorOutput, String> {
    preflight_workspace_root(&state, &input.workspace_root).await?;
    let workspace_root = normalize_workspace_root(&input.workspace_root)?;
    let cli_path = input.cli_path.clone();
    let timeout = timeout_from_seconds(input.timeout_seconds, CODERABBIT_DOCTOR_TIMEOUT);

    tauri::async_runtime::spawn_blocking(move || {
        let current_dir = Path::new(&workspace_root);
        let cli = detect_coderabbit_cli(cli_path.as_deref(), Some(current_dir))?;
        let started_at = Utc::now().to_rfc3339();
        let output = run_command_with_timeout(
            &cli.path,
            [OsString::from("doctor")],
            Some(current_dir),
            timeout,
        )?;
        let completed_at = Utc::now().to_rfc3339();

        Ok(WorkspaceCodeRabbitDoctorOutput {
            cli_name: cli.name,
            cli_path: cli.path,
            success: output.status.success(),
            exit_code: output.status.code(),
            stdout: command_stdout(&output),
            stderr: command_stderr(&output),
            started_at,
            completed_at,
        })
    })
    .await
    .map_err(|error| error.to_string())?
}

fn run_review_job(
    jobs_state: &CodeRabbitReviewJobsState,
    app: AppHandle,
    job_id: String,
    input: WorkspaceCodeRabbitReviewInput,
    workspace_root: String,
    cancel_requested: Arc<AtomicBool>,
) {
    let review_type = input.review_type.unwrap_or(CodeRabbitReviewType::All);
    let cli_path = input.cli_path.clone();
    let base = normalize_optional(input.base.as_deref());
    let base_commit = normalize_optional(input.base_commit.as_deref());
    let config_paths = match validate_config_paths(&input.config_paths) {
        Ok(paths) => paths,
        Err(error) => {
            finish_review_job_error(jobs_state, &job_id, error);
            return;
        }
    };
    let light = input.light.unwrap_or(false);
    let timeout = timeout_from_seconds(input.timeout_seconds, CODERABBIT_DEFAULT_TIMEOUT);

    if cancel_requested.load(Ordering::Relaxed) {
        finish_review_job_canceled(jobs_state, &job_id);
        return;
    }

    let current_dir = Path::new(&workspace_root);
    let cli = match detect_coderabbit_cli(cli_path.as_deref(), Some(current_dir)) {
        Ok(cli) => cli,
        Err(error) => {
            finish_review_job_error(jobs_state, &job_id, error);
            return;
        }
    };
    let fingerprint = match build_diff_fingerprint(
        &workspace_root,
        review_type,
        base.as_deref(),
        base_commit.as_deref(),
    ) {
        Ok(fingerprint) => fingerprint,
        Err(error) => {
            finish_review_job_error(jobs_state, &job_id, error);
            return;
        }
    };
    if cancel_requested.load(Ordering::Relaxed) {
        finish_review_job_canceled(jobs_state, &job_id);
        return;
    }
    let args = build_review_args(
        &workspace_root,
        review_type,
        base.as_deref(),
        base_commit.as_deref(),
        light,
        &config_paths,
    );

    let started_at = Utc::now().to_rfc3339();
    let mut child = match spawn_command(&cli.path, args, Some(current_dir)) {
        Ok(child) => child,
        Err(error) => {
            finish_review_job_error(jobs_state, &job_id, error);
            return;
        }
    };
    let pid = child.id();
    let timed_out = Arc::new(AtomicBool::new(false));
    let (finished_tx, finished_rx) = mpsc::channel();
    {
        let timed_out = timed_out.clone();
        std::thread::spawn(move || {
            if finished_rx.recv_timeout(timeout).is_err() {
                timed_out.store(true, Ordering::Relaxed);
                kill_process_group(pid);
            }
        });
    }
    update_review_job(jobs_state, &job_id, |snapshot| {
        snapshot.status = CodeRabbitReviewJobStatus::Running;
        snapshot.pid = Some(pid);
        snapshot.updated_at = Utc::now().to_rfc3339();
        snapshot.message = Some("CodeRabbit review running".to_string());
    });
    emit_review_event(
        &app,
        CodeRabbitReviewStreamEvent {
            job_id: job_id.clone(),
            workspace_root: workspace_root.clone(),
            event_type: "running".to_string(),
            status: Some("running".to_string()),
            message: Some("CodeRabbit review running".to_string()),
            finding: None,
            complete: None,
            result: None,
            errors: Vec::new(),
        },
    );

    let Some(stdout_pipe) = child.stdout.take() else {
        finish_review_job_error(
            jobs_state,
            &job_id,
            "CodeRabbit review stdout pipe unavailable".to_string(),
        );
        return;
    };
    let stderr_pipe = child.stderr.take();
    let (stdout_tx, stdout_rx) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        let reader = BufReader::new(stdout_pipe);
        for line in reader.lines() {
            match line {
                Ok(line) => {
                    let _ = stdout_tx.send(line);
                }
                Err(_) => break,
            }
        }
    });
    let stderr_handle = stderr_pipe.map(|mut stderr| {
        std::thread::spawn(move || {
            let mut value = String::new();
            let _ = stderr.read_to_string(&mut value);
            value
        })
    });

    let mut parsed = ParsedCodeRabbitAgentOutput::default();
    let mut stdout = String::new();
    let mut line_number = 0usize;
    let started = Instant::now();
    let exit_status = loop {
        while let Ok(line) = stdout_rx.try_recv() {
            line_number += 1;
            stdout.push_str(&line);
            stdout.push('\n');
            match parse_agent_jsonl_line(&line, line_number, parsed.findings.len()) {
                Ok(Some(event)) => {
                    let stream_event =
                        stream_event_from_agent_event(&job_id, &workspace_root, &event);
                    apply_agent_event(&mut parsed, event);
                    update_review_job_from_parsed(jobs_state, &job_id, &parsed);
                    emit_review_event(&app, stream_event);
                }
                Ok(None) => {}
                Err(error) => {
                    parsed.errors.push(error.clone());
                    emit_review_event(
                        &app,
                        CodeRabbitReviewStreamEvent {
                            job_id: job_id.clone(),
                            workspace_root: workspace_root.clone(),
                            event_type: "error".to_string(),
                            status: Some("error".to_string()),
                            message: Some(error.clone()),
                            finding: None,
                            complete: None,
                            result: None,
                            errors: vec![error],
                        },
                    );
                }
            }
        }

        if cancel_requested.load(Ordering::Relaxed) {
            kill_process_group(pid);
        } else if started.elapsed() >= timeout {
            timed_out.store(true, Ordering::Relaxed);
            kill_process_group(pid);
        }

        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(error) => break Err(error),
        }
    };
    let _ = finished_tx.send(());
    while let Ok(line) = stdout_rx.try_recv() {
        line_number += 1;
        stdout.push_str(&line);
        stdout.push('\n');
        match parse_agent_jsonl_line(&line, line_number, parsed.findings.len()) {
            Ok(Some(event)) => {
                let stream_event = stream_event_from_agent_event(&job_id, &workspace_root, &event);
                apply_agent_event(&mut parsed, event);
                update_review_job_from_parsed(jobs_state, &job_id, &parsed);
                emit_review_event(&app, stream_event);
            }
            Ok(None) => {}
            Err(error) => parsed.errors.push(error),
        }
    }
    let completed_at = Utc::now().to_rfc3339();
    let canceled = cancel_requested.load(Ordering::Relaxed);
    let timed_out = timed_out.load(Ordering::Relaxed);
    let stderr = stderr_handle
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default()
        .trim()
        .to_string();

    match exit_status {
        Ok(status) => {
            if !status.success() && !canceled {
                if !stderr.is_empty() {
                    parsed.errors.push(stderr.clone());
                } else {
                    parsed.errors.push("CodeRabbit review failed".to_string());
                }
            }
            if timed_out {
                parsed.errors.push(format!(
                    "CodeRabbit CLI command timed out after {}s",
                    timeout.as_secs()
                ));
            }
            let result = WorkspaceCodeRabbitReviewOutput {
                cli_name: cli.name,
                cli_path: cli.path,
                success: status.success() && !canceled && !timed_out,
                exit_code: status.code(),
                review_type,
                fingerprint,
                findings: parsed.findings,
                statuses: parsed.statuses,
                complete: parsed.complete,
                errors: parsed.errors,
                event_count: parsed.event_count,
                stdout,
                stderr,
                started_at,
                completed_at: completed_at.clone(),
            };
            update_review_job(jobs_state, &job_id, |snapshot| {
                snapshot.status = if canceled {
                    CodeRabbitReviewJobStatus::Canceled
                } else if result.success {
                    CodeRabbitReviewJobStatus::Succeeded
                } else {
                    CodeRabbitReviewJobStatus::Failed
                };
                snapshot.pid = None;
                snapshot.updated_at = completed_at.clone();
                snapshot.completed_at = Some(completed_at.clone());
                snapshot.cancel_requested = canceled;
                snapshot.message = Some(match snapshot.status {
                    CodeRabbitReviewJobStatus::Canceled => "CodeRabbit review canceled",
                    CodeRabbitReviewJobStatus::Succeeded => "CodeRabbit review completed",
                    CodeRabbitReviewJobStatus::Failed => "CodeRabbit review failed",
                    CodeRabbitReviewJobStatus::Starting | CodeRabbitReviewJobStatus::Running => {
                        "CodeRabbit review updated"
                    }
                }
                .to_string());
                snapshot.errors = result.errors.clone();
                snapshot.result = Some(result);
            });
            let snapshot = get_review_job_snapshot(jobs_state, &job_id).ok();
            let result = snapshot.and_then(|snapshot| snapshot.result);
            if let Some(result) = result {
                emit_review_event(
                    &app,
                    CodeRabbitReviewStreamEvent {
                        job_id: job_id.clone(),
                        workspace_root: workspace_root.clone(),
                        event_type: if canceled {
                            "canceled"
                        } else if result.success {
                            "succeeded"
                        } else {
                            "failed"
                        }
                        .to_string(),
                        status: Some(if canceled {
                            "canceled"
                        } else if result.success {
                            "succeeded"
                        } else {
                            "failed"
                        }
                        .to_string()),
                        message: None,
                        finding: None,
                        complete: result.complete.clone(),
                        result: Some(result.clone()),
                        errors: result.errors.clone(),
                    },
                );
            }
        }
        Err(error) => {
            update_review_job(jobs_state, &job_id, |snapshot| {
                snapshot.status = if canceled {
                    CodeRabbitReviewJobStatus::Canceled
                } else {
                    CodeRabbitReviewJobStatus::Failed
                };
                snapshot.pid = None;
                snapshot.updated_at = completed_at.clone();
                snapshot.completed_at = Some(completed_at.clone());
                snapshot.cancel_requested = canceled;
                snapshot.message = Some(if canceled {
                    "CodeRabbit review canceled".to_string()
                } else {
                    "CodeRabbit review failed".to_string()
                });
                if !canceled {
                    if timed_out {
                        snapshot.errors.push(format!(
                            "CodeRabbit CLI command timed out after {}s",
                            timeout.as_secs()
                        ));
                    } else {
                        snapshot.errors.push(error.to_string());
                    }
                }
            });
            emit_review_event(
                &app,
                CodeRabbitReviewStreamEvent {
                    job_id: job_id.clone(),
                    workspace_root: workspace_root.clone(),
                    event_type: if canceled { "canceled" } else { "failed" }.to_string(),
                    status: Some(if canceled { "canceled" } else { "failed" }.to_string()),
                    message: Some(if canceled {
                        "CodeRabbit review canceled".to_string()
                    } else {
                        error.to_string()
                    }),
                    finding: None,
                    complete: None,
                    result: None,
                    errors: if canceled {
                        Vec::new()
                    } else {
                        vec![error.to_string()]
                    },
                },
            );
        }
    }
}

fn finish_review_job_canceled(jobs_state: &CodeRabbitReviewJobsState, job_id: &str) {
    update_review_job(jobs_state, job_id, |snapshot| {
        let now = Utc::now().to_rfc3339();
        snapshot.status = CodeRabbitReviewJobStatus::Canceled;
        snapshot.updated_at = now.clone();
        snapshot.completed_at = Some(now);
        snapshot.cancel_requested = true;
        snapshot.message = Some("CodeRabbit review canceled".to_string());
    });
}

fn update_review_job_from_parsed(
    jobs_state: &CodeRabbitReviewJobsState,
    job_id: &str,
    parsed: &ParsedCodeRabbitAgentOutput,
) {
    update_review_job(jobs_state, job_id, |snapshot| {
        snapshot.updated_at = Utc::now().to_rfc3339();
        if let Some(status) = parsed.statuses.last() {
            snapshot.message = status
                .message
                .clone()
                .or_else(|| status.status.clone())
                .or_else(|| Some(status.event_type.clone()));
        }
        if !parsed.errors.is_empty() {
            snapshot.errors = parsed.errors.clone();
        }
    });
}

fn stream_event_from_agent_event(
    job_id: &str,
    workspace_root: &str,
    event: &ParsedCodeRabbitAgentEvent,
) -> CodeRabbitReviewStreamEvent {
    match event {
        ParsedCodeRabbitAgentEvent::Finding(finding) => CodeRabbitReviewStreamEvent {
            job_id: job_id.to_string(),
            workspace_root: workspace_root.to_string(),
            event_type: "finding".to_string(),
            status: None,
            message: finding.comment.clone(),
            finding: Some(finding.clone()),
            complete: None,
            result: None,
            errors: Vec::new(),
        },
        ParsedCodeRabbitAgentEvent::Status(status) => CodeRabbitReviewStreamEvent {
            job_id: job_id.to_string(),
            workspace_root: workspace_root.to_string(),
            event_type: status.event_type.clone(),
            status: status.status.clone(),
            message: status.message.clone(),
            finding: None,
            complete: None,
            result: None,
            errors: Vec::new(),
        },
        ParsedCodeRabbitAgentEvent::Complete(complete) => CodeRabbitReviewStreamEvent {
            job_id: job_id.to_string(),
            workspace_root: workspace_root.to_string(),
            event_type: "complete".to_string(),
            status: complete.status.clone(),
            message: complete.message.clone(),
            finding: None,
            complete: Some(complete.clone()),
            result: None,
            errors: Vec::new(),
        },
        ParsedCodeRabbitAgentEvent::Error(error) => CodeRabbitReviewStreamEvent {
            job_id: job_id.to_string(),
            workspace_root: workspace_root.to_string(),
            event_type: "error".to_string(),
            status: Some("error".to_string()),
            message: Some(error.clone()),
            finding: None,
            complete: None,
            result: None,
            errors: vec![error.clone()],
        },
    }
}

fn emit_review_event(app: &AppHandle, event: CodeRabbitReviewStreamEvent) {
    let _ = app.emit(CODERABBIT_REVIEW_EVENT_NAME, event);
}

fn finish_review_job_error(jobs_state: &CodeRabbitReviewJobsState, job_id: &str, error: String) {
    update_review_job(jobs_state, job_id, |snapshot| {
        let now = Utc::now().to_rfc3339();
        snapshot.status = CodeRabbitReviewJobStatus::Failed;
        snapshot.updated_at = now.clone();
        snapshot.completed_at = Some(now);
        snapshot.message = Some("CodeRabbit review failed".to_string());
        snapshot.errors.push(error);
    });
}

fn update_review_job(
    jobs_state: &CodeRabbitReviewJobsState,
    job_id: &str,
    update: impl FnOnce(&mut WorkspaceCodeRabbitReviewJobSnapshot),
) {
    if let Ok(mut jobs) = jobs_state.jobs.lock() {
        if let Some(record) = jobs.get_mut(job_id) {
            update(&mut record.snapshot);
        }
    }
}

fn get_review_job_snapshot(
    jobs_state: &CodeRabbitReviewJobsState,
    job_id: &str,
) -> Result<WorkspaceCodeRabbitReviewJobSnapshot, String> {
    let jobs = jobs_state
        .jobs
        .lock()
        .map_err(|_| "CodeRabbit review jobs lock poisoned".to_string())?;
    jobs.get(job_id)
        .map(|record| record.snapshot.clone())
        .ok_or_else(|| format!("CodeRabbit review job not found: {job_id}"))
}

fn open_coderabbit_reviews_db(db_path: &Path) -> Result<Connection, String> {
    let conn = Connection::open(db_path).map_err(|error| error.to_string())?;
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS workspace_coderabbit_reviews (
          workspace_root TEXT PRIMARY KEY,
          review_json TEXT NOT NULL,
          fingerprint_hash TEXT,
          completed_at TEXT,
          created_at TEXT DEFAULT (datetime('now')),
          updated_at TEXT DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_workspace_coderabbit_reviews_updated
          ON workspace_coderabbit_reviews(updated_at DESC);
        CREATE TABLE IF NOT EXISTS workspace_coderabbit_review_history (
          review_id TEXT PRIMARY KEY,
          workspace_root TEXT NOT NULL,
          review_json TEXT NOT NULL,
          review_type TEXT,
          success INTEGER DEFAULT 0,
          findings_count INTEGER DEFAULT 0,
          fingerprint_hash TEXT,
          completed_at TEXT,
          saved_at TEXT DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_workspace_coderabbit_review_history_workspace
          ON workspace_coderabbit_review_history(workspace_root, saved_at DESC);
        CREATE TRIGGER IF NOT EXISTS update_workspace_coderabbit_reviews_timestamp
        AFTER UPDATE ON workspace_coderabbit_reviews
        BEGIN
          UPDATE workspace_coderabbit_reviews
          SET updated_at = datetime('now')
          WHERE workspace_root = NEW.workspace_root;
        END;
        ",
    )
    .map_err(|error| error.to_string())?;
    Ok(conn)
}

#[tauri::command]
pub async fn workspace_coderabbit_diff_fingerprint(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceCodeRabbitFingerprintInput,
) -> Result<CodeRabbitDiffFingerprint, String> {
    preflight_workspace_root(&state, &input.workspace_root).await?;
    let workspace_root = normalize_workspace_root(&input.workspace_root)?;
    let review_type = input.review_type.unwrap_or(CodeRabbitReviewType::All);
    let base = input.base.clone();
    let base_commit = input.base_commit.clone();

    tauri::async_runtime::spawn_blocking(move || {
        build_diff_fingerprint(
            &workspace_root,
            review_type,
            base.as_deref(),
            base_commit.as_deref(),
        )
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn workspace_coderabbit_review(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceCodeRabbitReviewInput,
) -> Result<WorkspaceCodeRabbitReviewOutput, String> {
    preflight_workspace_root(&state, &input.workspace_root).await?;
    let workspace_root = normalize_workspace_root(&input.workspace_root)?;
    let review_type = input.review_type.unwrap_or(CodeRabbitReviewType::All);
    let cli_path = input.cli_path.clone();
    let base = normalize_optional(input.base.as_deref());
    let base_commit = normalize_optional(input.base_commit.as_deref());
    let config_paths = validate_config_paths(&input.config_paths)?;
    let light = input.light.unwrap_or(false);
    let timeout = timeout_from_seconds(input.timeout_seconds, CODERABBIT_DEFAULT_TIMEOUT);

    tauri::async_runtime::spawn_blocking(move || {
        let current_dir = Path::new(&workspace_root);
        let cli = detect_coderabbit_cli(cli_path.as_deref(), Some(current_dir))?;
        let fingerprint = build_diff_fingerprint(
            &workspace_root,
            review_type,
            base.as_deref(),
            base_commit.as_deref(),
        )?;
        let args = build_review_args(
            &workspace_root,
            review_type,
            base.as_deref(),
            base_commit.as_deref(),
            light,
            &config_paths,
        );

        let started_at = Utc::now().to_rfc3339();
        let output = run_command_with_timeout(&cli.path, args, Some(current_dir), timeout)?;
        let completed_at = Utc::now().to_rfc3339();
        let stdout = command_stdout(&output);
        let stderr = command_stderr(&output);
        let mut parsed = parse_agent_jsonl(&stdout);
        if !output.status.success() {
            parsed
                .errors
                .push(command_output_detail(&output, "CodeRabbit review failed"));
        }

        Ok(WorkspaceCodeRabbitReviewOutput {
            cli_name: cli.name,
            cli_path: cli.path,
            success: output.status.success(),
            exit_code: output.status.code(),
            review_type,
            fingerprint,
            findings: parsed.findings,
            statuses: parsed.statuses,
            complete: parsed.complete,
            errors: parsed.errors,
            event_count: parsed.event_count,
            stdout,
            stderr,
            started_at,
            completed_at,
        })
    })
    .await
    .map_err(|error| error.to_string())?
}

fn run_auth_status(cli_path: &str, current_dir: Option<&Path>) -> CodeRabbitAuthStatusOutput {
    let output = run_command_with_timeout(
        cli_path,
        [
            OsString::from("auth"),
            OsString::from("status"),
            OsString::from("--agent"),
        ],
        current_dir,
        CODERABBIT_STATUS_TIMEOUT,
    );

    match output {
        Ok(output) => {
            let stdout = command_stdout(&output);
            let stderr = command_stderr(&output);
            let parsed = parse_auth_status_stdout(&stdout);
            let message = parsed
                .message
                .or_else(|| (!stderr.is_empty()).then(|| stderr.clone()))
                .or_else(|| (!stdout.is_empty()).then(|| stdout.clone()));
            CodeRabbitAuthStatusOutput {
                checked: true,
                success: output.status.success(),
                exit_code: output.status.code(),
                authenticated: parsed.authenticated,
                login: parsed.login,
                organization: parsed.organization,
                message,
                stdout,
                stderr,
            }
        }
        Err(error) => CodeRabbitAuthStatusOutput {
            checked: true,
            success: false,
            exit_code: None,
            authenticated: None,
            login: None,
            organization: None,
            message: Some(error),
            stdout: String::new(),
            stderr: String::new(),
        },
    }
}

#[derive(Default)]
struct ParsedAuthStatus {
    authenticated: Option<bool>,
    login: Option<String>,
    organization: Option<String>,
    message: Option<String>,
}

fn parse_auth_status_stdout(stdout: &str) -> ParsedAuthStatus {
    let mut parsed = ParsedAuthStatus::default();
    for line in stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        parsed.authenticated = parsed
            .authenticated
            .or_else(|| bool_field(&value, &["authenticated", "isAuthenticated", "loggedIn"]));
        parsed.login = parsed
            .login
            .or_else(|| string_field(&value, &["login", "user", "username", "email"]));
        parsed.organization = parsed.organization.or_else(|| {
            string_field(
                &value,
                &["organization", "org", "organizationName", "orgName"],
            )
        });
        parsed.message = parsed
            .message
            .or_else(|| string_field(&value, &["message", "status", "detail"]));
    }
    parsed
}

fn build_review_args(
    workspace_root: &str,
    review_type: CodeRabbitReviewType,
    base: Option<&str>,
    base_commit: Option<&str>,
    light: bool,
    config_paths: &[String],
) -> Vec<OsString> {
    let mut args = vec![
        OsString::from("review"),
        OsString::from("--agent"),
        OsString::from("--dir"),
        OsString::from(workspace_root),
        OsString::from("--type"),
        OsString::from(review_type.as_cli_value()),
    ];

    if light {
        args.push(OsString::from("--light"));
    }
    if let Some(base) = base {
        args.push(OsString::from("--base"));
        args.push(OsString::from(base));
    }
    if let Some(base_commit) = base_commit {
        args.push(OsString::from("--base-commit"));
        args.push(OsString::from(base_commit));
    }
    for path in config_paths {
        args.push(OsString::from("--config"));
        args.push(OsString::from(path));
    }

    args
}

fn normalize_workspace_root(workspace_root: &str) -> Result<String, String> {
    let root = workspace_root.trim();
    if root.is_empty() {
        Err("workspace_root is empty".to_string())
    } else {
        Ok(root.to_string())
    }
}

fn normalize_optional(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
}

fn timeout_from_seconds(value: Option<u64>, default: Duration) -> Duration {
    match value {
        Some(seconds) if seconds > 0 => Duration::from_secs(seconds.clamp(5, 7200)),
        _ => default,
    }
}

fn validate_config_paths(paths: &[String]) -> Result<Vec<String>, String> {
    let mut out = Vec::with_capacity(paths.len());
    for path in paths {
        let path = path.trim();
        if path.is_empty() {
            continue;
        }
        validate_relative_path(path)?;
        out.push(path.to_string());
    }
    Ok(out)
}

fn validate_relative_path(path: &str) -> Result<(), String> {
    let path_buf = PathBuf::from(path);
    if path_buf.is_absolute() {
        return Err(format!(
            "config path must be relative to the workspace: {path}"
        ));
    }
    for component in path_buf.components() {
        if matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        ) {
            return Err(format!("invalid config path outside workspace: {path}"));
        }
    }
    Ok(())
}

fn string_field(value: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(raw) = value.get(*key).and_then(Value::as_str) {
            let raw = raw.trim();
            if !raw.is_empty() {
                return Some(raw.to_string());
            }
        }
    }
    None
}

fn bool_field(value: &Value, keys: &[&str]) -> Option<bool> {
    for key in keys {
        if let Some(raw) = value.get(*key).and_then(Value::as_bool) {
            return Some(raw);
        }
    }
    None
}

mod errors;
mod fingerprint;
mod jobs;
mod parser;
mod process;
mod storage;

use std::{
    ffi::OsString,
    path::{Component, Path, PathBuf},
    time::Duration,
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use specta::Type;


pub use self::jobs::CodeRabbitReviewJobsState;

use self::{
    parser::ParsedCodeRabbitAgentEvent,
    process::{
        command_stderr, command_stdout, detect_coderabbit_cli, run_command_with_timeout, CODERABBIT_STATUS_TIMEOUT,
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
pub struct WorkspaceCodeRabbitLogoutInput {
    pub cli_path: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceCodeRabbitLogoutOutput {
    pub success: bool,
    pub exit_code: Option<i32>,
    pub message: String,
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
    pub error_kind: Option<CodeRabbitReviewErrorKind>,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum CodeRabbitReviewErrorKind {
    Auth,
    Permission,
    RateLimit,
    NoDiff,
    Timeout,
    Network,
    Git,
    Config,
    Parse,
    CliUnavailable,
    Unknown,
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
    pub error_kind: Option<CodeRabbitReviewErrorKind>,
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
    pub error_kind: Option<CodeRabbitReviewErrorKind>,
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

pub async fn workspace_coderabbit_logout(
    input: WorkspaceCodeRabbitLogoutInput,
) -> Result<WorkspaceCodeRabbitLogoutOutput, String> {
    crate::rt::spawn_blocking(move || {
        let cli = detect_coderabbit_cli(input.cli_path.as_deref(), None)?;
        let output = run_command_with_timeout(
            &cli.path,
            [OsString::from("auth"), OsString::from("logout")],
            None,
            CODERABBIT_STATUS_TIMEOUT,
        )?;
        let stdout = command_stdout(&output);
        let stderr = command_stderr(&output);
        let success = output.status.success();
        let message = if !stdout.is_empty() {
            stdout
        } else if !stderr.is_empty() {
            stderr
        } else if success {
            "CodeRabbit disconnected".to_string()
        } else {
            "CodeRabbit logout failed".to_string()
        };

        Ok(WorkspaceCodeRabbitLogoutOutput {
            success,
            exit_code: output.status.code(),
            message,
        })
    })
    .await
    .map_err(|error| error.to_string())?
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
            error_kind: None,
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
            error_kind: None,
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
            error_kind: None,
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
            error_kind: Some(CodeRabbitReviewErrorKind::Unknown),
            errors: vec![error.clone()],
        },
    }
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

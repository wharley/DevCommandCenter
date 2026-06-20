mod fingerprint;
mod parser;
mod process;

use std::{
    ffi::OsString,
    path::{Component, Path, PathBuf},
    time::Duration,
};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use specta::Type;
use tauri::State;

use crate::{commands::workspace_support::preflight_workspace_root, state::WorkspaceCommandState};

use self::{
    fingerprint::build_diff_fingerprint,
    parser::parse_agent_jsonl,
    process::{
        command_output_detail, command_stderr, command_stdout, detect_coderabbit_cli,
        run_command_with_timeout, CODERABBIT_DEFAULT_TIMEOUT, CODERABBIT_DOCTOR_TIMEOUT,
        CODERABBIT_STATUS_TIMEOUT,
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

use std::path::Path;

use chrono::Utc;
use dcc_core::domain::workspace::WorkspacePushTarget;
use dcc_infra::db::SqliteWorkspaceRepo;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;
use uuid::Uuid;

use crate::{
    commands::workspace_support::{
        find_workspace_by_root, resolve_current_branch_name, resolve_current_commit_sha,
        resolve_default_remote_name,
    },
    git::{parse_git_status_porcelain_z, run_git_output},
    state::WorkspaceCommandState,
};

const FAILURE_OUTPUT_MAX_BYTES: usize = 16 * 1024;
const FAILURE_CHANGED_FILES_MAX: usize = 100;
const TRUNCATION_MARKER: &str = "\n… [truncated by DCC]";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "kebab-case")]
pub enum WorkspaceDeliveryFailureOperation {
    Fetch,
    Pull,
    Push,
    Pipeline,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "kebab-case")]
pub enum WorkspaceDeliveryFailureClassification {
    Authentication,
    NonFastForward,
    ProtectedBranch,
    LocalHookOrLint,
    ConflictOrDivergence,
    Transport,
    PipelineOrJob,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceDeliveryPushTarget {
    pub remote: String,
    pub branch: String,
    pub url: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceDeliveryFailureSnapshot {
    pub attempt_token: String,
    pub workspace_root: String,
    pub branch: Option<String>,
    pub head_sha: Option<String>,
    pub operation: WorkspaceDeliveryFailureOperation,
    pub classification: WorkspaceDeliveryFailureClassification,
    pub remote: Option<String>,
    pub push_target: Option<WorkspaceDeliveryPushTarget>,
    pub output: String,
    pub output_truncated: bool,
    pub changed_files: Vec<String>,
    pub changed_files_truncated: bool,
    pub external_url: Option<String>,
    pub created_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceDeliveryFailureInput {
    pub workspace_root: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceDeliveryFailureOutput {
    pub snapshot: Option<WorkspaceDeliveryFailureSnapshot>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct CaptureDeliveryFailureOptions {
    pub(crate) remote: Option<String>,
    pub(crate) external_url: Option<String>,
}

fn strip_terminal_controls(raw: &str) -> String {
    #[derive(Clone, Copy)]
    enum EscapeState {
        Normal,
        Escape,
        Csi,
        Osc,
        OscEscape,
    }

    let mut state = EscapeState::Normal;
    let mut cleaned = String::with_capacity(raw.len());
    for character in raw.chars() {
        state = match state {
            EscapeState::Normal => match character {
                '\u{1b}' => EscapeState::Escape,
                '\r' => EscapeState::Normal,
                '\n' | '\t' => {
                    cleaned.push(character);
                    EscapeState::Normal
                }
                value if !value.is_control() => {
                    cleaned.push(value);
                    EscapeState::Normal
                }
                _ => EscapeState::Normal,
            },
            EscapeState::Escape => match character {
                '[' => EscapeState::Csi,
                ']' => EscapeState::Osc,
                _ => EscapeState::Normal,
            },
            EscapeState::Csi => {
                if ('@'..='~').contains(&character) {
                    EscapeState::Normal
                } else {
                    EscapeState::Csi
                }
            }
            EscapeState::Osc => match character {
                '\u{7}' => EscapeState::Normal,
                '\u{1b}' => EscapeState::OscEscape,
                _ => EscapeState::Osc,
            },
            EscapeState::OscEscape => {
                if character == '\\' {
                    EscapeState::Normal
                } else {
                    EscapeState::Osc
                }
            }
        };
    }
    cleaned
}

fn redact_url_credentials(raw: &str) -> String {
    let mut redacted = raw.to_string();
    let mut cursor = 0usize;
    while let Some(relative_scheme) = redacted[cursor..].find("://") {
        let authority_start = cursor + relative_scheme + 3;
        let authority_end = redacted[authority_start..]
            .find(|character: char| {
                character.is_whitespace() || matches!(character, '/' | '?' | '#')
            })
            .map(|relative| authority_start + relative)
            .unwrap_or(redacted.len());
        let Some(relative_at) = redacted[authority_start..authority_end].rfind('@') else {
            cursor = authority_end;
            continue;
        };
        let at = authority_start + relative_at;
        redacted.replace_range(authority_start..at, "[redacted]");
        cursor = authority_start + "[redacted]@".len();
    }
    redacted
}

fn line_contains_credential(line: &str) -> bool {
    let upper = line.to_ascii_uppercase();
    [
        "AUTHORIZATION:",
        "PRIVATE-TOKEN:",
        "JOB-TOKEN:",
        "GH_TOKEN=",
        "GITHUB_TOKEN=",
        "GITLAB_TOKEN=",
        "GLAB_TOKEN=",
        "ACCESS_TOKEN=",
        "PASSWORD=",
        "PASS=",
    ]
    .iter()
    .any(|marker| upper.contains(marker))
}

pub(crate) fn sanitize_delivery_failure_output(raw: &str) -> (String, bool) {
    let without_controls = strip_terminal_controls(raw);
    let redacted = without_controls
        .lines()
        .map(|line| {
            if line_contains_credential(line) {
                "[redacted credential]".to_string()
            } else {
                redact_url_credentials(line)
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    if redacted.len() <= FAILURE_OUTPUT_MAX_BYTES {
        return (redacted, false);
    }

    let max_content_bytes = FAILURE_OUTPUT_MAX_BYTES.saturating_sub(TRUNCATION_MARKER.len());
    let mut boundary = max_content_bytes.min(redacted.len());
    while !redacted.is_char_boundary(boundary) {
        boundary = boundary.saturating_sub(1);
    }
    let mut bounded = redacted[..boundary].to_string();
    bounded.push_str(TRUNCATION_MARKER);
    (bounded, true)
}

fn contains_any(text: &str, markers: &[&str]) -> bool {
    markers.iter().any(|marker| text.contains(marker))
}

pub(crate) fn classify_delivery_failure(
    operation: WorkspaceDeliveryFailureOperation,
    output: &str,
) -> WorkspaceDeliveryFailureClassification {
    let text = output.to_ascii_lowercase();

    if contains_any(
        &text,
        &[
            "protected branch",
            "protected ref",
            "gh006",
            "branch policy",
            "not allowed to push code to protected",
            "cannot push to this protected",
        ],
    ) {
        return WorkspaceDeliveryFailureClassification::ProtectedBranch;
    }

    if contains_any(
        &text,
        &[
            "non-fast-forward",
            "fetch first",
            "tip of your current branch is behind",
            "updates were rejected because the remote contains work",
            "remote contains work that you do not have locally",
        ],
    ) {
        return WorkspaceDeliveryFailureClassification::NonFastForward;
    }

    if contains_any(
        &text,
        &[
            "authentication failed",
            "not authenticated",
            "unauthorized",
            "invalid credentials",
            "bad credentials",
            "invalid username or password",
            "could not read username",
            "http basic: access denied",
            "permission denied (publickey)",
            "token expired",
            "invalid token",
            "401 unauthorized",
            "http 401",
            "status code: 401",
        ],
    ) {
        return WorkspaceDeliveryFailureClassification::Authentication;
    }

    if contains_any(
        &text,
        &[
            "merge conflict",
            "automatic merge failed",
            "conflicting file",
            "unmerged files",
            "needs merge",
            "have diverged",
            "branches have diverged",
            "not possible to fast-forward",
            "changed after this workspace was opened",
        ],
    ) {
        return WorkspaceDeliveryFailureClassification::ConflictOrDivergence;
    }

    if operation == WorkspaceDeliveryFailureOperation::Push
        && contains_any(
            &text,
            &[
                "pre-commit hook",
                "pre-push hook",
                "commit-msg hook",
                "lint-staged",
                "lint failed",
                "eslint",
                "husky",
            ],
        )
    {
        return WorkspaceDeliveryFailureClassification::LocalHookOrLint;
    }

    if contains_any(
        &text,
        &[
            "could not resolve host",
            "couldn't resolve host",
            "failed to connect",
            "connection timed out",
            "operation timed out",
            "connection refused",
            "connection reset",
            "network is unreachable",
            "ssl certificate problem",
            "tls handshake",
            "rpc failed",
            "remote end hung up unexpectedly",
            "early eof",
            "http 500",
            "http 502",
            "http 503",
            "http 504",
        ],
    ) {
        return WorkspaceDeliveryFailureClassification::Transport;
    }

    if operation == WorkspaceDeliveryFailureOperation::Pipeline
        && contains_any(
            &text,
            &[
                "pipeline #",
                "pipeline failed",
                "failed pipeline",
                "failed jobs:",
                "job failed",
            ],
        )
    {
        return WorkspaceDeliveryFailureClassification::PipelineOrJob;
    }

    WorkspaceDeliveryFailureClassification::Unknown
}

fn collect_changed_files(root: &str) -> (Vec<String>, bool) {
    let Ok(output) = run_git_output(root, &["status", "--porcelain=v1", "-z"]) else {
        return (Vec::new(), false);
    };
    if !output.status.success() {
        return (Vec::new(), false);
    }
    let mut paths = parse_git_status_porcelain_z(&output.stdout)
        .into_iter()
        .map(|entry| entry.path)
        .collect::<Vec<_>>();
    paths.sort();
    paths.dedup();
    let truncated = paths.len() > FAILURE_CHANGED_FILES_MAX;
    paths.truncate(FAILURE_CHANGED_FILES_MAX);
    (paths, truncated)
}

fn remote_url(root: &str, remote: &str) -> Option<String> {
    let output = run_git_output(root, &["remote", "get-url", remote]).ok()?;
    if !output.status.success() {
        return None;
    }
    let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if raw.is_empty() {
        None
    } else {
        Some(redact_url_credentials(&raw))
    }
}

fn normalize_push_target(root: &str, target: WorkspacePushTarget) -> WorkspaceDeliveryPushTarget {
    WorkspaceDeliveryPushTarget {
        url: target
            .remote_url
            .map(|url| redact_url_credentials(url.trim()))
            .filter(|url| !url.is_empty())
            .or_else(|| remote_url(root, &target.remote_name)),
        remote: target.remote_name,
        branch: target.branch_name,
    }
}

async fn resolve_push_target(
    state: &WorkspaceCommandState,
    root: &str,
    branch: Option<&str>,
) -> Option<WorkspaceDeliveryPushTarget> {
    let repo = SqliteWorkspaceRepo::open(&state.db_path).ok()?;
    if let Ok(Some(workspace)) = find_workspace_by_root(&repo, root).await {
        if let Some(source) = workspace.source {
            let target = source.push_target.unwrap_or(WorkspacePushTarget {
                remote_name: source.remote_name,
                branch_name: source.head_branch,
                remote_url: None,
                remote_created: false,
            });
            return Some(normalize_push_target(root, target));
        }
    }

    let remote = resolve_default_remote_name(root).ok()?;
    let branch = branch?.trim();
    if branch.is_empty() || branch == "HEAD" {
        return None;
    }
    Some(WorkspaceDeliveryPushTarget {
        url: remote_url(root, &remote),
        remote,
        branch: branch.to_string(),
    })
}

pub(crate) async fn capture_workspace_delivery_failure(
    state: &WorkspaceCommandState,
    root: &str,
    operation: WorkspaceDeliveryFailureOperation,
    output: &str,
    options: CaptureDeliveryFailureOptions,
) -> WorkspaceDeliveryFailureSnapshot {
    let root = root.trim();
    let branch = resolve_current_branch_name(root).ok();
    let head_sha = resolve_current_commit_sha(root).ok().flatten();
    let push_target = resolve_push_target(state, root, branch.as_deref()).await;
    let remote = options
        .remote
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| push_target.as_ref().map(|target| target.remote.clone()));
    let (output, output_truncated) = sanitize_delivery_failure_output(output);
    let classification = classify_delivery_failure(operation, &output);
    let (changed_files, changed_files_truncated) = collect_changed_files(root);
    let snapshot = WorkspaceDeliveryFailureSnapshot {
        attempt_token: Uuid::new_v4().to_string(),
        workspace_root: root.to_string(),
        branch,
        head_sha,
        operation,
        classification,
        remote,
        push_target,
        output,
        output_truncated,
        changed_files,
        changed_files_truncated,
        external_url: options
            .external_url
            .map(|value| redact_url_credentials(value.trim()))
            .filter(|value| !value.is_empty()),
        created_at: Utc::now().to_rfc3339(),
    };
    state.record_delivery_failure(snapshot)
}

pub(crate) fn clear_workspace_delivery_failure(
    state: &WorkspaceCommandState,
    root: &str,
    operation: WorkspaceDeliveryFailureOperation,
) {
    state.clear_delivery_failure(root, operation);
}

#[tauri::command]
pub async fn workspace_delivery_failure_snapshot(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceDeliveryFailureInput,
) -> Result<WorkspaceDeliveryFailureOutput, String> {
    let root = input.workspace_root.trim();
    if root.is_empty() || !Path::new(root).is_dir() || !state.has_delivery_failure(root) {
        return Ok(WorkspaceDeliveryFailureOutput { snapshot: None });
    }
    let branch = resolve_current_branch_name(root).ok();
    let head_sha = resolve_current_commit_sha(root).ok().flatten();
    Ok(WorkspaceDeliveryFailureOutput {
        snapshot: state.latest_delivery_failure(root, branch.as_deref(), head_sha.as_deref()),
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{
        classify_delivery_failure, sanitize_delivery_failure_output,
        WorkspaceDeliveryFailureClassification, WorkspaceDeliveryFailureOperation,
        WorkspaceDeliveryFailureSnapshot, FAILURE_OUTPUT_MAX_BYTES, TRUNCATION_MARKER,
    };
    use crate::state::WorkspaceCommandState;

    #[test]
    fn sanitizes_controls_credentials_and_authenticated_urls() {
        let raw = "\u{1b}[31mfailed\u{1b}[0m\r\nAuthorization: Bearer secret\n\
                   GH_TOKEN=secret\nhttps://oauth2:token@gitlab.example/group/repo.git";
        let (output, truncated) = sanitize_delivery_failure_output(raw);

        assert!(!truncated);
        assert!(!output.contains('\u{1b}'));
        assert!(!output.contains("secret"));
        assert!(!output.contains("oauth2:token"));
        assert!(output.contains("[redacted credential]"));
        assert!(output.contains("https://[redacted]@gitlab.example/group/repo.git"));
    }

    #[test]
    fn bounds_failure_output_on_a_utf8_boundary() {
        let raw = "á".repeat(FAILURE_OUTPUT_MAX_BYTES);
        let (output, truncated) = sanitize_delivery_failure_output(&raw);

        assert!(truncated);
        assert!(output.len() <= FAILURE_OUTPUT_MAX_BYTES);
        assert!(output.ends_with(TRUNCATION_MARKER));
    }

    fn snapshot(token: &str, branch: &str, sha: &str) -> WorkspaceDeliveryFailureSnapshot {
        WorkspaceDeliveryFailureSnapshot {
            attempt_token: token.to_string(),
            workspace_root: "/tmp/dcc-delivery-test".to_string(),
            branch: Some(branch.to_string()),
            head_sha: Some(sha.to_string()),
            operation: WorkspaceDeliveryFailureOperation::Push,
            classification: WorkspaceDeliveryFailureClassification::Unknown,
            remote: Some("origin".to_string()),
            push_target: None,
            output: "push failed".to_string(),
            output_truncated: false,
            changed_files: vec!["src/main.rs".to_string()],
            changed_files_truncated: false,
            external_url: None,
            created_at: format!("2026-07-24T12:00:0{token}Z"),
        }
    }

    #[test]
    fn deduplicates_failures_and_rejects_stale_branch_or_sha() {
        let state = WorkspaceCommandState::new(PathBuf::from("/tmp/dcc-delivery-test.sqlite"));
        let first = state.record_delivery_failure(snapshot("1", "feature/a", "abc"));
        let duplicate = state.record_delivery_failure(snapshot("2", "feature/a", "abc"));

        assert_eq!(duplicate.attempt_token, first.attempt_token);
        assert!(state
            .latest_delivery_failure("/tmp/dcc-delivery-test", Some("feature/a"), Some("abc"))
            .is_some());
        assert!(state
            .latest_delivery_failure("/tmp/dcc-delivery-test", Some("feature/b"), Some("abc"))
            .is_none());
        assert!(state
            .latest_delivery_failure("/tmp/dcc-delivery-test", Some("feature/a"), Some("def"))
            .is_none());

        state.clear_delivery_failure(
            "/tmp/dcc-delivery-test",
            WorkspaceDeliveryFailureOperation::Push,
        );
        assert!(state
            .latest_delivery_failure("/tmp/dcc-delivery-test", Some("feature/a"), Some("abc"))
            .is_none());
    }

    #[test]
    fn classifies_only_strong_delivery_failure_signals() {
        let cases = [
            (
                WorkspaceDeliveryFailureOperation::Push,
                "remote: You are not allowed to push code to protected branches",
                WorkspaceDeliveryFailureClassification::ProtectedBranch,
            ),
            (
                WorkspaceDeliveryFailureOperation::Push,
                "! [rejected] feature -> feature (non-fast-forward)",
                WorkspaceDeliveryFailureClassification::NonFastForward,
            ),
            (
                WorkspaceDeliveryFailureOperation::Fetch,
                "fatal: Authentication failed for repository",
                WorkspaceDeliveryFailureClassification::Authentication,
            ),
            (
                WorkspaceDeliveryFailureOperation::Pull,
                "Automatic merge failed; fix conflicts and then commit the result",
                WorkspaceDeliveryFailureClassification::ConflictOrDivergence,
            ),
            (
                WorkspaceDeliveryFailureOperation::Push,
                "husky - pre-push hook exited with code 1: ESLint found problems",
                WorkspaceDeliveryFailureClassification::LocalHookOrLint,
            ),
            (
                WorkspaceDeliveryFailureOperation::Fetch,
                "fatal: unable to access repository: Could not resolve host",
                WorkspaceDeliveryFailureClassification::Transport,
            ),
            (
                WorkspaceDeliveryFailureOperation::Pipeline,
                "GitLab pipeline #42 failed for commit abc",
                WorkspaceDeliveryFailureClassification::PipelineOrJob,
            ),
        ];

        for (operation, output, expected) in cases {
            assert_eq!(
                classify_delivery_failure(operation, output),
                expected,
                "{output}"
            );
        }
    }

    #[test]
    fn keeps_ambiguous_failures_unknown_and_preserves_specific_precedence() {
        assert_eq!(
            classify_delivery_failure(
                WorkspaceDeliveryFailureOperation::Push,
                "remote rejected: pre-receive hook declined"
            ),
            WorkspaceDeliveryFailureClassification::Unknown
        );
        assert_eq!(
            classify_delivery_failure(
                WorkspaceDeliveryFailureOperation::Pipeline,
                "HTTP 401 Unauthorized while loading pipeline"
            ),
            WorkspaceDeliveryFailureClassification::Authentication
        );
        assert_eq!(
            classify_delivery_failure(
                WorkspaceDeliveryFailureOperation::Push,
                "unexpected failure"
            ),
            WorkspaceDeliveryFailureClassification::Unknown
        );
        assert_eq!(
            classify_delivery_failure(
                WorkspaceDeliveryFailureOperation::Pipeline,
                "GitLab pipeline lookup failed: 403 Forbidden"
            ),
            WorkspaceDeliveryFailureClassification::Unknown
        );
    }
}

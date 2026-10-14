//! Delivering a task from somewhere other than its desktop panel (the phone,
//! through `GET/POST /api/v1/mobile/workspaces/:id/delivery…`).
//!
//! Every step is one of the desktop's own delivery functions (`*_with_state`
//! of `workspace_git_commit_push`, `workspace_change_request_create`, …), in
//! the order the desktop's `useWorkspaceDelivery`
//! (`apps/desktop/src/features/commit/use-workspace-delivery.ts`) runs them:
//! configured before-push checks, `git add -A` only when nothing is staged,
//! the deterministic staged-snapshot commit message (never a provider),
//! commit/push with the staged fingerprint, the "update and push" recovery
//! after a non-fast-forward push, the PR with an optional title and body.
//! Commit rules, push guards (protected branch, imported branches, push
//! identity), forge calls and the delivery-failure store are the desktop's.
//!
//! The task is chosen by id; its folder comes from the database, never from
//! the caller.
use std::path::Path;

use dcc_core::{
    domain::workspace::{Workspace, WorkspaceId, WorkspaceState},
    ports::WorkspaceRepo,
};
use dcc_infra::db::SqliteWorkspaceRepo;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use specta::Type;

use crate::{
    commands::{
        forge_commands::{
            workspace_change_request_create_with_state, workspace_change_request_title,
            workspace_pipeline_status_with_state, workspace_pr_status_with_state,
            WorkspacePipelineStatusInput, WorkspacePrStatusInput, WorkspacePrStatusOutput,
        },
        workspace_commands::{
            archive_workspace_with_state, complete_workspace_with_state,
            derive_staged_commit_subject, workspace_delivery_recovery_execute_with_state,
            workspace_git_branch_diff_with_state, workspace_git_commit_push_with_state,
            workspace_git_commit_suggestion_with_state, workspace_git_commit_with_state,
            workspace_git_push_with_state, workspace_git_stage_all_with_state,
            workspace_git_status_with_state, workspace_project_automation_config_with_state,
            workspace_run_project_tasks_with_state, WorkspaceChangeRequestCreateInput,
            WorkspaceGitBranchDiffInput, WorkspaceGitChangeEntry, WorkspaceGitCommitInput,
            WorkspaceGitCommitPushInput, WorkspaceGitCommitSuggestionInput,
            WorkspaceGitConflictStateInput, WorkspaceGitPathInput, WorkspaceGitPushInput,
            WorkspaceGitStatusInput, WorkspaceGitStatusOutput, WorkspaceGitValidationStatus,
            WorkspaceIdInput, WorkspaceRunProjectTasksInput,
        },
    },
    delivery_failure::{
        workspace_delivery_failure_snapshot_with_state, WorkspaceDeliveryFailureClassification,
        WorkspaceDeliveryFailureInput, WorkspaceDeliveryFailureOperation,
        WorkspaceDeliveryFailureSnapshot, WorkspaceDeliveryRecoveryAction,
        WorkspaceDeliveryRecoveryInput,
    },
    state::{SessionCommandState, WorkspaceCommandState},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeliveryError {
    /// No such task, or its folder is gone.
    NotFound(String),
    /// The local changes are not the ones the caller saw.
    Stale,
    /// The project's configured before-push checks did not pass.
    ChecksFailed,
    /// The push was refused because the remote branch moved; "update and
    /// push" (`sync_and_push`) can fix it. Carries `remote/branch`.
    BehindRemote(String),
    /// Wrong input (no changes to commit, task bundled with others…).
    Invalid(String),
    Failed(String),
}

impl std::fmt::Display for DeliveryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(message) | Self::Invalid(message) | Self::Failed(message) => {
                f.write_str(message)
            }
            Self::Stale => f.write_str(
                "As mudanças da tarefa mudaram desde que você abriu. Confira de novo antes de enviar.",
            ),
            Self::ChecksFailed => f.write_str(
                "Os checks configurados para antes do push falharam no computador.",
            ),
            Self::BehindRemote(target) => write!(
                f,
                "O envio foi recusado: {target} tem commits que este branch não tem. Use \"Atualizar e enviar\"."
            ),
        }
    }
}

impl From<String> for DeliveryError {
    fn from(message: String) -> Self {
        Self::Failed(message)
    }
}

/// One changed file, as the phone lists it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryChange {
    pub path: String,
    pub status: String,
    pub insertions: u32,
    pub deletions: u32,
    pub staged: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryPushFailure {
    pub classification: WorkspaceDeliveryFailureClassification,
    /// `remote/branch` the push went to.
    pub target: Option<String>,
    /// "Atualizar e enviar" is offered (non-fast-forward, safe target).
    pub can_sync: bool,
}

/// The task's git state, read locally (no forge call).
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DeliverySummary {
    pub workspace_id: String,
    pub workspace_state: WorkspaceState,
    pub branch: Option<String>,
    pub base_branch: String,
    /// Uncommitted changes (staged and not).
    pub changes: Vec<DeliveryChange>,
    pub staged_count: u32,
    /// Committed on this branch and not on its base (`git diff base...HEAD`).
    pub branch_changes: Vec<DeliveryChange>,
    pub ahead_of_remote_count: u32,
    pub behind_of_remote_count: u32,
    pub conflict_count: u32,
    pub merge_in_progress: bool,
    /// Guards a commit against changes that moved since this was read.
    pub changes_token: String,
    /// The desktop's deterministic message for what the commit would take.
    pub commit_message: String,
    /// Checks the project runs before each push (`.dcc.toml`).
    pub before_push: Vec<String>,
    pub push_failure: Option<DeliveryPushFailure>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryPipeline {
    pub status: String,
    pub web_url: Option<String>,
    pub failed_jobs: Vec<String>,
}

/// The task's PR and CI, from the forge (slower; read on its own).
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryForge {
    pub pull_request: Option<WorkspacePrStatusOutput>,
    pub pipeline: Option<DeliveryPipeline>,
    /// The desktop's default title for a new PR (task name or thread title).
    pub request_title: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeliveryCommitInput {
    /// Edited subject; the default message when absent.
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub push: bool,
    pub changes_token: String,
}

#[derive(Clone, Debug, Default, Deserialize, Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeliveryRequestInput {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub draft: bool,
    /// Commit and push uncommitted changes first (the desktop modal's box).
    #[serde(default)]
    pub include_local_changes: bool,
    #[serde(default)]
    pub changes_token: Option<String>,
}

struct Target {
    workspace: Workspace,
    root: String,
}

async fn target(state: &WorkspaceCommandState, workspace_id: &str) -> Result<Target, DeliveryError> {
    let repo = SqliteWorkspaceRepo::open(&state.db_path).map_err(|error| error.to_string())?;
    let workspace = repo
        .get_workspace(&WorkspaceId(workspace_id.to_string()))
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| DeliveryError::NotFound("Workspace não encontrado.".into()))?;
    let root = workspace
        .worktree_path
        .clone()
        .unwrap_or_else(|| workspace.root_path.clone());
    if !Path::new(&root).is_dir() {
        return Err(DeliveryError::NotFound(
            "A pasta desta tarefa não existe mais no computador.".into(),
        ));
    }
    Ok(Target { workspace, root })
}

fn change(entry: &WorkspaceGitChangeEntry, staged: bool) -> DeliveryChange {
    DeliveryChange {
        path: entry.path.clone(),
        status: entry.status.clone(),
        insertions: entry.insertions,
        deletions: entry.deletions,
        staged,
    }
}

/// Staged and unstaged as one list per path, the way the phone shows them.
fn local_changes(status: &WorkspaceGitStatusOutput) -> Vec<DeliveryChange> {
    let mut changes: Vec<DeliveryChange> = status.staged.iter().map(|e| change(e, true)).collect();
    for entry in &status.unstaged {
        match changes.iter_mut().find(|c| c.path == entry.path) {
            Some(existing) => {
                existing.insertions += entry.insertions;
                existing.deletions += entry.deletions;
            }
            None => changes.push(change(entry, false)),
        }
    }
    changes.sort_by(|a, b| a.path.cmp(&b.path));
    changes
}

fn changes_token(status: &WorkspaceGitStatusOutput) -> String {
    let mut hasher = Sha256::new();
    for (side, entries) in [("s", &status.staged), ("u", &status.unstaged)] {
        // Untracked files report no line counts: their size and mtime
        // catch an edit the counts would miss.
        let mut lines: Vec<String> = entries
            .iter()
            .map(|e| {
                let stamp = std::fs::metadata(&e.absolute_path)
                    .map(|meta| {
                        let modified = meta
                            .modified()
                            .ok()
                            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                            .map(|since| since.as_nanos())
                            .unwrap_or(0);
                        format!("{}:{modified}", meta.len())
                    })
                    .unwrap_or_default();
                format!(
                    "{side}\t{}\t{}\t{}\t{}\t{stamp}",
                    e.path, e.status, e.insertions, e.deletions
                )
            })
            .collect();
        lines.sort();
        for line in lines {
            hasher.update(line.as_bytes());
            hasher.update(b"\n");
        }
    }
    hasher.update(status.staged_fingerprint.as_bytes());
    hasher.update(status.current_branch.as_deref().unwrap_or("").as_bytes());
    format!("{:x}", hasher.finalize())
}

/// What a commit takes: the staged files, or everything when nothing is
/// staged (the desktop stages all first), hence its default message.
fn default_commit_message(status: &WorkspaceGitStatusOutput) -> String {
    if !status.staged.is_empty() {
        return derive_staged_commit_subject(&status.staged);
    }
    derive_staged_commit_subject(&status.unstaged)
}

async fn git_status(state: &WorkspaceCommandState, root: &str) -> Result<WorkspaceGitStatusOutput, DeliveryError> {
    Ok(workspace_git_status_with_state(
        state,
        WorkspaceGitStatusInput {
            workspace_root: root.to_string(),
        },
    )
    .await?)
}

async fn push_failure(state: &WorkspaceCommandState, root: &str) -> Option<WorkspaceDeliveryFailureSnapshot> {
    workspace_delivery_failure_snapshot_with_state(
        state,
        WorkspaceDeliveryFailureInput {
            workspace_root: root.to_string(),
        },
    )
    .await
    .ok()
    .and_then(|output| output.snapshot)
    .filter(|snapshot| snapshot.operation == WorkspaceDeliveryFailureOperation::Push)
}

fn push_target(snapshot: &WorkspaceDeliveryFailureSnapshot) -> String {
    snapshot
        .push_target
        .as_ref()
        .map(|target| format!("{}/{}", target.remote, target.branch))
        .or_else(|| snapshot.branch.clone())
        .unwrap_or_default()
}

/// `capturedNonFastForwardPush`: the push failed because the remote moved
/// and the backend offers to synchronize.
fn can_sync(snapshot: &WorkspaceDeliveryFailureSnapshot) -> bool {
    snapshot.classification == WorkspaceDeliveryFailureClassification::NonFastForward
        && snapshot
            .available_actions
            .contains(&WorkspaceDeliveryRecoveryAction::Synchronize)
}

/// A failed push becomes `BehindRemote` when "update and push" applies.
async fn explain_push_error(state: &WorkspaceCommandState, root: &str, error: String) -> DeliveryError {
    match push_failure(state, root).await {
        Some(snapshot) if can_sync(&snapshot) => DeliveryError::BehindRemote(push_target(&snapshot)),
        _ => DeliveryError::Failed(error),
    }
}

async fn before_push_ids(state: &WorkspaceCommandState, root: &str) -> Vec<String> {
    workspace_project_automation_config_with_state(
        state,
        WorkspaceGitConflictStateInput {
            workspace_root: root.to_string(),
        },
    )
    .await
    .map(|config| config.before_push)
    .unwrap_or_default()
}

/// `runBeforePushChecks`.
async fn run_before_push_checks(state: &WorkspaceCommandState, root: &str) -> Result<(), DeliveryError> {
    let config = workspace_project_automation_config_with_state(
        state,
        WorkspaceGitConflictStateInput {
            workspace_root: root.to_string(),
        },
    )
    .await?;
    if config.before_push.is_empty() {
        return Ok(());
    }
    let output = workspace_run_project_tasks_with_state(
        state,
        WorkspaceRunProjectTasksInput {
            workspace_root: root.to_string(),
            task_ids: config.before_push,
            expected_config_hash: config.config_hash,
        },
    )
    .await?;
    match output.report.status {
        WorkspaceGitValidationStatus::Passed => Ok(()),
        _ => Err(DeliveryError::ChecksFailed),
    }
}

pub async fn delivery_summary(
    state: &WorkspaceCommandState,
    workspace_id: &str,
) -> Result<DeliverySummary, DeliveryError> {
    let target = target(state, workspace_id).await?;
    let status = git_status(state, &target.root).await?;
    let branch_changes = workspace_git_branch_diff_with_state(
        state,
        WorkspaceGitBranchDiffInput {
            workspace_root: target.root.clone(),
        },
    )
    .await
    .map(|diff| diff.changes.iter().map(|e| change(e, false)).collect())
    .unwrap_or_default();
    let push_failure = push_failure(state, &target.root).await.map(|snapshot| DeliveryPushFailure {
        classification: snapshot.classification,
        target: Some(push_target(&snapshot)).filter(|t| !t.is_empty()),
        can_sync: can_sync(&snapshot),
    });
    Ok(DeliverySummary {
        workspace_id: target.workspace.id.0.clone(),
        workspace_state: target.workspace.state.clone(),
        branch: status.current_branch.clone(),
        base_branch: target.workspace.base_branch.clone(),
        changes: local_changes(&status),
        staged_count: status.staged.len() as u32,
        branch_changes,
        ahead_of_remote_count: status.ahead_of_remote_count,
        behind_of_remote_count: status.behind_of_remote_count,
        conflict_count: status.conflict_count,
        merge_in_progress: status.merge_in_progress,
        changes_token: changes_token(&status),
        commit_message: default_commit_message(&status),
        before_push: before_push_ids(state, &target.root).await,
        push_failure,
    })
}

pub async fn delivery_forge(
    state: &WorkspaceCommandState,
    workspace_id: &str,
) -> Result<DeliveryForge, DeliveryError> {
    let target = target(state, workspace_id).await?;
    let status = git_status(state, &target.root).await?;
    let pull_request = workspace_pr_status_with_state(
        state,
        WorkspacePrStatusInput {
            workspace_root: target.root.clone(),
            branch: status.current_branch.clone(),
            forge_login: None,
        },
    )
    .await
    .ok()
    .filter(|pr| pr.number.is_some());
    let pipeline = if pull_request.is_some() {
        workspace_pipeline_status_with_state(
            state,
            WorkspacePipelineStatusInput {
                workspace_root: target.root.clone(),
                forge_login: None,
            },
        )
        .await
        .ok()
        .and_then(|output| output.pipeline)
        .map(|pipeline| DeliveryPipeline {
            failed_jobs: pipeline
                .jobs
                .iter()
                .filter(|job| job.status == "failed" && !job.allow_failure)
                .map(|job| job.name.clone())
                .collect(),
            status: pipeline.status,
            web_url: pipeline.web_url,
        })
    } else {
        None
    };
    let request_title = workspace_change_request_title(state, &target.root)
        .await
        .ok()
        .flatten();
    Ok(DeliveryForge {
        pull_request,
        pipeline,
        request_title,
    })
}

/// Stages everything when nothing is staged, then reads the staged
/// snapshot's deterministic message and fingerprint.
async fn stage_and_suggest(
    state: &WorkspaceCommandState,
    sessions: &SessionCommandState,
    root: &str,
    status: &WorkspaceGitStatusOutput,
) -> Result<(String, String), DeliveryError> {
    if status.staged.is_empty() {
        workspace_git_stage_all_with_state(
            state,
            WorkspaceGitPathInput {
                workspace_root: root.to_string(),
                relative_path: ".".to_string(),
            },
        )
        .await?;
    }
    // Automatic delivery never asks a provider (no provider id).
    let suggestion = workspace_git_commit_suggestion_with_state(
        state,
        sessions,
        WorkspaceGitCommitSuggestionInput {
            workspace_root: root.to_string(),
            provider_id: None,
            model: None,
            provider_runtime: None,
        },
    )
    .await?;
    Ok((suggestion.subject, suggestion.staged_fingerprint))
}

fn edited(text: Option<String>) -> Option<String> {
    text.map(|value| value.trim().to_string()).filter(|value| !value.is_empty())
}

/// "Commit" or "Commit e enviar".
pub async fn delivery_commit(
    state: &WorkspaceCommandState,
    sessions: &SessionCommandState,
    workspace_id: &str,
    input: DeliveryCommitInput,
) -> Result<(), DeliveryError> {
    let target = target(state, workspace_id).await?;
    let status = git_status(state, &target.root).await?;
    if changes_token(&status) != input.changes_token {
        return Err(DeliveryError::Stale);
    }
    if status.staged.is_empty() && status.unstaged.is_empty() {
        return Err(DeliveryError::Invalid("Não há mudanças para commitar.".into()));
    }
    if input.push {
        run_before_push_checks(state, &target.root).await?;
    }
    let (subject, staged_fingerprint) =
        stage_and_suggest(state, sessions, &target.root, &status).await?;
    let message = edited(input.message).unwrap_or(subject);
    let body = edited(input.body);
    if input.push {
        if let Err(error) = workspace_git_commit_push_with_state(
            state,
            WorkspaceGitCommitPushInput {
                workspace_root: target.root.clone(),
                message,
                body,
                staged_fingerprint,
                forge_login: None,
            },
        )
        .await
        {
            return Err(explain_push_error(state, &target.root, error).await);
        }
        return Ok(());
    }
    Ok(workspace_git_commit_with_state(
        state,
        WorkspaceGitCommitInput {
            workspace_root: target.root.clone(),
            message,
            body,
            staged_fingerprint,
        },
    )
    .await?)
}

/// "Enviar": push the commits the branch has.
pub async fn delivery_push(state: &WorkspaceCommandState, workspace_id: &str) -> Result<(), DeliveryError> {
    let target = target(state, workspace_id).await?;
    run_before_push_checks(state, &target.root).await?;
    if let Err(error) = workspace_git_push_with_state(
        state,
        WorkspaceGitPushInput {
            workspace_root: target.root.clone(),
            forge_login: None,
        },
    )
    .await
    {
        return Err(explain_push_error(state, &target.root, error).await);
    }
    Ok(())
}

/// "Atualizar e enviar" (`syncAndPush`): bring the remote branch's commits
/// in (rebase when safe, merge otherwise), rerun the checks, push.
pub async fn delivery_sync_and_push(
    state: &WorkspaceCommandState,
    workspace_id: &str,
) -> Result<(), DeliveryError> {
    let target = target(state, workspace_id).await?;
    let Some(snapshot) = push_failure(state, &target.root).await.filter(can_sync) else {
        return Err(DeliveryError::Invalid(
            "Não há um envio recusado para atualizar. Tente enviar de novo.".into(),
        ));
    };
    workspace_delivery_recovery_execute_with_state(
        state,
        WorkspaceDeliveryRecoveryInput {
            workspace_root: target.root.clone(),
            attempt_token: snapshot.attempt_token,
            action: WorkspaceDeliveryRecoveryAction::Synchronize,
            forge_login: None,
        },
    )
    .await?;
    // The merge brought in code the configured checks have not seen yet.
    run_before_push_checks(state, &target.root).await?;
    if let Err(error) = workspace_git_push_with_state(
        state,
        WorkspaceGitPushInput {
            workspace_root: target.root.clone(),
            forge_login: None,
        },
    )
    .await
    {
        return Err(explain_push_error(state, &target.root, error).await);
    }
    Ok(())
}

/// The desktop's "Criar PR" modal: optional title/body (empty body lets the
/// forge fill it from the commits), draft or not, local changes first.
pub async fn delivery_create_request(
    state: &WorkspaceCommandState,
    sessions: &SessionCommandState,
    workspace_id: &str,
    input: DeliveryRequestInput,
) -> Result<(), DeliveryError> {
    let target = target(state, workspace_id).await?;
    let status = git_status(state, &target.root).await?;
    let has_local = !status.staged.is_empty() || !status.unstaged.is_empty();
    if input.include_local_changes && has_local {
        if input.changes_token.as_deref() != Some(changes_token(&status).as_str()) {
            return Err(DeliveryError::Stale);
        }
    }
    // Creating a request also pushes the branch: the checks run even when
    // local files are left out.
    run_before_push_checks(state, &target.root).await?;
    if input.include_local_changes && has_local {
        let (subject, staged_fingerprint) =
            stage_and_suggest(state, sessions, &target.root, &status).await?;
        if let Err(error) = workspace_git_commit_push_with_state(
            state,
            WorkspaceGitCommitPushInput {
                workspace_root: target.root.clone(),
                message: subject,
                body: None,
                staged_fingerprint,
                forge_login: None,
            },
        )
        .await
        {
            return Err(explain_push_error(state, &target.root, error).await);
        }
    }
    Ok(workspace_change_request_create_with_state(
        state,
        WorkspaceChangeRequestCreateInput {
            workspace_root: target.root.clone(),
            forge_login: None,
            title: edited(input.title),
            body: edited(input.body),
            draft: input.draft,
        },
    )
    .await?)
}

pub async fn delivery_complete(state: &WorkspaceCommandState, workspace_id: &str) -> Result<(), DeliveryError> {
    // Unknown ids are refused by the command itself; check first for a 404.
    let repo = SqliteWorkspaceRepo::open(&state.db_path).map_err(|error| error.to_string())?;
    if repo
        .get_workspace(&WorkspaceId(workspace_id.to_string()))
        .await
        .map_err(|error| error.to_string())?
        .is_none()
    {
        return Err(DeliveryError::NotFound("Workspace não encontrado.".into()));
    }
    complete_workspace_with_state(
        state,
        WorkspaceIdInput {
            workspace_id: workspace_id.to_string(),
        },
    )
    .await
    .map_err(DeliveryError::Invalid)
}

pub async fn delivery_archive(state: &WorkspaceCommandState, workspace_id: &str) -> Result<(), DeliveryError> {
    let repo = SqliteWorkspaceRepo::open(&state.db_path).map_err(|error| error.to_string())?;
    if repo
        .get_workspace(&WorkspaceId(workspace_id.to_string()))
        .await
        .map_err(|error| error.to_string())?
        .is_none()
    {
        return Err(DeliveryError::NotFound("Workspace não encontrado.".into()));
    }
    archive_workspace_with_state(
        state,
        WorkspaceIdInput {
            workspace_id: workspace_id.to_string(),
        },
    )
    .await
    .map_err(DeliveryError::Invalid)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str, status: &str, insertions: u32, deletions: u32) -> WorkspaceGitChangeEntry {
        WorkspaceGitChangeEntry {
            path: path.to_string(),
            name: path.rsplit('/').next().unwrap_or(path).to_string(),
            absolute_path: format!("/repo/{path}"),
            status: status.to_string(),
            insertions,
            deletions,
        }
    }

    fn status(staged: Vec<WorkspaceGitChangeEntry>, unstaged: Vec<WorkspaceGitChangeEntry>) -> WorkspaceGitStatusOutput {
        WorkspaceGitStatusOutput {
            staged,
            unstaged,
            staged_fingerprint: "fp".to_string(),
            current_branch: Some("dcc/task".to_string()),
            ahead_of_remote_count: 0,
            behind_of_remote_count: 0,
            conflict_count: 0,
            merge_in_progress: false,
            has_remote: true,
        }
    }

    #[test]
    fn local_changes_merge_staged_and_unstaged_per_path() {
        let s = status(
            vec![entry("src/a.ts", "M", 2, 1)],
            vec![entry("src/a.ts", "M", 3, 0), entry("README.md", "??", 4, 0)],
        );
        let changes = local_changes(&s);
        assert_eq!(changes.len(), 2);
        assert_eq!(changes[0].path, "README.md");
        assert!(!changes[0].staged);
        assert_eq!((changes[1].insertions, changes[1].deletions, changes[1].staged), (5, 1, true));
    }

    #[test]
    fn the_token_moves_with_any_change_and_the_message_follows_what_commits() {
        let unstaged = status(vec![], vec![entry("docs/guide.md", "M", 1, 0)]);
        let more = status(vec![], vec![entry("docs/guide.md", "M", 2, 0)]);
        assert_ne!(changes_token(&unstaged), changes_token(&more));
        assert_eq!(changes_token(&unstaged), changes_token(&unstaged.clone()));
        // Nothing staged: the commit takes everything.
        assert_eq!(default_commit_message(&unstaged), derive_staged_commit_subject(&unstaged.unstaged));
        // Something staged: only that.
        let staged = status(vec![entry("src/x.rs", "A", 1, 0)], vec![entry("docs/guide.md", "M", 1, 0)]);
        assert_eq!(default_commit_message(&staged), derive_staged_commit_subject(&staged.staged));
    }
}

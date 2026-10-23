//! Deleting a task, a multi-project task or a project, the same way for the
//! desktop and the native app. What only a running process holds (the memory
//! checkpoint, the git mutation coordinator, delivery failures kept in
//! memory) comes from [`DeletionRuntime`].

pub mod delegation;
pub mod remote_branch;

use std::collections::BTreeSet;
use std::future::Future;
use std::path::{Path, PathBuf};

use dcc_core::domain::delegation_apply::DelegationApplyTransactionState;
use dcc_core::domain::delegation_worktree::DelegationWorktreeOperationState;
use dcc_core::domain::repository::RepositoryId;
use dcc_core::domain::workspace::{Workspace, WorkspaceId};
use dcc_core::domain::workspace_bundle::WorkspaceBundleId;
pub use dcc_core::domain::workspace_git::WorkspaceRemoteBranchDeletionTarget;
use dcc_core::ports::{
    DelegationApplyTransactionRepo, DelegationRepo, DelegationWorktreeOperationRepo,
    RepositoryRepo, WorkspaceBundleRepo, WorkspaceRepo,
};
use serde::{Deserialize, Serialize};

use crate::db::{SqliteSessionRepo, SqliteWorkspaceRepo};
use crate::git::{broken_worktree_reason, is_git_repo, remove_worktree};
use crate::workspace_actions::ensure_workspace_is_not_a_bundle_member;
use crate::workspace_git::resolve_workspace_active_root;
use delegation::{
    delegation_uncommitted_files, remove_journaled_delegation_worktree,
    validate_delegation_worktree_path, UNAPPLIED_DELEGATION_EDITS,
};
use remote_branch::{
    cleanup_unused_workspace_push_target, delete_workspace_remote_branch,
    delete_workspace_remote_branch_after_worktree_loss, workspace_remote_branch_deletion_target,
};

/// What a deletion needs from the process that runs it.
pub trait DeletionRuntime: Sync {
    fn db_path(&self) -> &Path;
    fn app_data_dir(&self) -> &Path;
    /// The tasks' conversations reach ai-memory before they are erased; an
    /// error leaves the task whole.
    fn checkpoint_memory(
        &self,
        workspace_ids: &[WorkspaceId],
    ) -> impl Future<Output = Result<(), String>> + Send;
    /// Runs a git change on `root` admitted by the process's coordinator.
    fn git_mutation<T, F>(
        &self,
        root: &str,
        operation: F,
    ) -> impl Future<Output = Result<T, String>> + Send
    where
        T: Send + 'static,
        F: FnOnce(&Path) -> Result<T, String> + Send + 'static;
    /// The same for a task whose worktree may already be gone.
    fn git_mutation_for_workspace<T, F>(
        &self,
        workspace_id: &WorkspaceId,
        root: &str,
        operation: F,
    ) -> impl Future<Output = Result<T, String>> + Send
    where
        T: Send + 'static,
        F: FnOnce(&Path) -> Result<T, String> + Send + 'static;
    /// A change on a worktree and a child task's worktree together.
    fn git_pair_mutation<T, F>(
        &self,
        root: &str,
        secondary: PathBuf,
        operation: F,
    ) -> impl Future<Output = Result<T, String>> + Send
    where
        T: Send + 'static,
        F: FnOnce(&Path, &Path) -> Result<T, String> + Send + 'static;
    /// Forgets delivery failures the process keeps for `root`.
    fn clear_delivery_failures(&self, root: &str);
}

/// Reads the child tasks a refused deletion listed (`unapplied_delegation_edits:` + JSON).
pub fn parse_unapplied_delegation_edits(message: &str) -> Option<Vec<UnappliedDelegationEdits>> {
    let json = message.split_once(UNAPPLIED_DELEGATION_EDITS)?.1;
    serde_json::from_str(json).ok()
}

/// How a person confirmed deleting one task.
#[derive(Clone, Debug, Default)]
pub struct DeleteTaskRequest {
    pub workspace_id: String,
    pub delete_remote_branch: bool,
    pub expected_remote_target: Option<WorkspaceRemoteBranchDeletionTarget>,
    /// The person confirmed losing child-task edits that never reached this task.
    pub discard_delegation_changes: bool,
}

/// How a person confirmed deleting a multi-project task.
#[derive(Clone, Debug, Default)]
pub struct DeleteBundleRequest {
    pub bundle_id: String,
    pub delete_remote_branches: bool,
    pub expected_remote_targets: Vec<WorkspaceRemoteBranchDeletionTarget>,
}

/// The branches a deletion can also remove from the remote, as the dialog
/// offers them (`remoteDeletionTargets`).
pub fn remote_deletion_targets(
    workspaces: &[Workspace],
) -> Vec<WorkspaceRemoteBranchDeletionTarget> {
    workspaces
        .iter()
        .filter_map(|workspace| {
            workspace_remote_branch_deletion_target(workspace)
                .ok()
                .flatten()
        })
        .collect()
}

/// Deletes one task: worktree, child task worktrees, conversations and,
/// when confirmed, its remote branch. A refusal leaves it whole.
pub async fn delete_workspace<R: DeletionRuntime>(
    runtime: &R,
    input: DeleteTaskRequest,
) -> Result<(), String> {
    let repo = SqliteWorkspaceRepo::open(runtime.db_path()).map_err(|e| e.to_string())?;
    let session_repo = SqliteSessionRepo::open(runtime.db_path()).map_err(|e| e.to_string())?;
    let id = WorkspaceId(input.workspace_id);
    ensure_workspace_is_not_a_bundle_member(&repo, &id, "delete").await?;
    let workspace = repo
        .get_workspace(&id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("workspace not found: {}", id.0))?;
    // A destructive delete must not erase a session before its final memory
    // checkpoint has reached ai-memory. If the sidecar is unavailable, leave
    // the task intact and let the user retry after it recovers.
    runtime
        .checkpoint_memory(std::slice::from_ref(&id))
        .await
        .map_err(|error| format!("memory export must finish before deleting this task: {error}"))?;
    ensure_workspace_history_deletable(&session_repo, std::slice::from_ref(&workspace))?;
    if !input.discard_delegation_changes {
        // Before anything is removed, so a refusal leaves the task whole.
        let pending = unapplied_delegation_edits(&session_repo, &workspace).await?;
        if !pending.is_empty() {
            return Err(format!(
                "{UNAPPLIED_DELEGATION_EDITS}{}",
                serde_json::to_string(&pending).map_err(|error| error.to_string())?
            ));
        }
    }
    cleanup_delegation_worktrees(
        runtime,
        &session_repo,
        &workspace,
        input.discard_delegation_changes,
    )
    .await?;
    if input.delete_remote_branch {
        let expected_target = input.expected_remote_target.ok_or_else(|| {
            "The remote branch was not confirmed. Reopen the deletion dialog and try again."
                .to_string()
        })?;
        let active_root = resolve_workspace_active_root(&workspace).to_string();
        let remote_workspace = workspace.clone();
        if Path::new(&active_root).exists() {
            runtime
                .git_mutation(&active_root, move |trusted_root| {
                    let trusted_root = trusted_root
                        .to_str()
                        .ok_or_else(|| "workspace root is not valid UTF-8".to_string())?;
                    delete_workspace_remote_branch(
                        &remote_workspace,
                        trusted_root,
                        &expected_target,
                    )
                })
                .await?;
        } else {
            let repository_root = workspace.root_path.clone();
            runtime
                .git_mutation_for_workspace(&workspace.id, &repository_root, move |trusted_root| {
                    let trusted_root = trusted_root
                        .to_str()
                        .ok_or_else(|| "repository root is not valid UTF-8".to_string())?;
                    delete_workspace_remote_branch_after_worktree_loss(
                        trusted_root,
                        &remote_workspace,
                        &expected_target,
                    )
                })
                .await?;
        }
    }
    cleanup_unused_workspace_push_target(runtime, &repo, &workspace, &BTreeSet::new()).await?;
    cleanup_workspace_files(&workspace)?;
    cleanup_workspace_session_records(
        &session_repo,
        std::slice::from_ref(&workspace),
        runtime.db_path(),
    )
    .await?;
    repo.delete_workspace(&id)
        .await
        .map_err(|e| e.to_string())?;
    runtime.clear_delivery_failures(&workspace.root_path);
    if let Some(worktree_path) = workspace.worktree_path.as_deref() {
        runtime.clear_delivery_failures(worktree_path);
    }
    Ok(())
}

/// The members a multi-project task created for itself: what deleting it
/// removes (members that were the person's own checkouts stay).
pub async fn bundle_created_workspaces(
    repo: &SqliteWorkspaceRepo,
    bundle_id: &WorkspaceBundleId,
) -> Result<Vec<Workspace>, String> {
    let summary = repo
        .get_workspace_bundle(bundle_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("workspace bundle not found: {}", bundle_id.0))?;
    let mut created_workspaces = Vec::new();
    for member in &summary.members {
        if !member.created_for_bundle {
            continue;
        }
        let workspace = repo
            .get_workspace(&member.workspace_id)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("workspace not found: {}", member.workspace_id.0))?;
        created_workspaces.push(workspace);
    }
    Ok(created_workspaces)
}

/// Deletes a multi-project task and the members it created.
pub async fn delete_workspace_bundle<R: DeletionRuntime>(
    runtime: &R,
    input: DeleteBundleRequest,
) -> Result<(), String> {
    let repo = SqliteWorkspaceRepo::open(runtime.db_path()).map_err(|error| error.to_string())?;
    let session_repo =
        SqliteSessionRepo::open(runtime.db_path()).map_err(|error| error.to_string())?;
    let bundle_id = WorkspaceBundleId(input.bundle_id);
    let created_workspaces = bundle_created_workspaces(&repo, &bundle_id).await?;

    let workspace_ids = created_workspaces
        .iter()
        .map(|workspace| workspace.id.clone())
        .collect::<Vec<_>>();
    runtime
        .checkpoint_memory(&workspace_ids)
        .await
        .map_err(|error| format!("memory export must finish before deleting this task: {error}"))?;

    ensure_workspace_history_deletable(&session_repo, &created_workspaces)?;

    let mut delegation_cleanup_errors = Vec::new();
    for workspace in created_workspaces.iter().rev() {
        if let Err(error) =
            cleanup_delegation_worktrees(runtime, &session_repo, workspace, false).await
        {
            delegation_cleanup_errors.push(format!("{} delegations: {error}", workspace.id.0));
        }
    }
    if !delegation_cleanup_errors.is_empty() {
        return Err(format!(
            "multi-workspace delegation cleanup was incomplete: {}",
            delegation_cleanup_errors.join("; ")
        ));
    }
    if input.delete_remote_branches {
        let mut targets = Vec::new();
        // Local members work on the person's own branch, which DCC never
        // created, so deleting the task must never reach their remote branch.
        for workspace in created_workspaces
            .iter()
            .filter(|workspace| workspace.worktree_path.is_some())
        {
            if let Some(target) = workspace_remote_branch_deletion_target(workspace)? {
                targets.push((workspace, target));
            }
        }
        let mut actual_targets = targets
            .iter()
            .map(|(_, target)| {
                (
                    target.remote.clone(),
                    target.branch.clone(),
                    target.expected_oid.clone(),
                    target.push_url.clone(),
                )
            })
            .collect::<Vec<_>>();
        let mut expected_targets = input
            .expected_remote_targets
            .iter()
            .map(|target| {
                (
                    target.remote.trim().to_string(),
                    target.branch.trim().to_string(),
                    target.expected_oid.trim().to_ascii_lowercase(),
                    target.push_url.trim().to_string(),
                )
            })
            .filter(|(remote, branch, expected_oid, push_url)| {
                !remote.is_empty()
                    && !branch.is_empty()
                    && !expected_oid.is_empty()
                    && !push_url.is_empty()
            })
            .collect::<Vec<_>>();
        actual_targets.sort();
        expected_targets.sort();
        if actual_targets != expected_targets {
            return Err(
                "The worktree branches changed after the deletion dialog opened. Review the deletion confirmation and try again."
                    .to_string(),
            );
        }
        for (workspace, target) in targets {
            let active_root = resolve_workspace_active_root(workspace).to_string();
            let workspace = workspace.clone();
            runtime
                .git_mutation(&active_root, move |trusted_root| {
                    let trusted_root = trusted_root
                        .to_str()
                        .ok_or_else(|| "workspace root is not valid UTF-8".to_string())?;
                    delete_workspace_remote_branch(&workspace, trusted_root, &target)
                })
                .await?;
        }
    }

    let retiring_workspace_ids = created_workspaces
        .iter()
        .map(|workspace| workspace.id.0.clone())
        .collect::<BTreeSet<_>>();
    for workspace in &created_workspaces {
        cleanup_unused_workspace_push_target(runtime, &repo, workspace, &retiring_workspace_ids)
            .await?;
    }

    let mut cleanup_errors = Vec::new();
    for workspace in created_workspaces.iter().rev() {
        if let Err(error) = cleanup_workspace_files(workspace) {
            cleanup_errors.push(format!("{}: {error}", workspace.id.0));
        }
    }
    if !cleanup_errors.is_empty() {
        return Err(format!(
            "multi-workspace cleanup was incomplete: {}",
            cleanup_errors.join("; ")
        ));
    }

    cleanup_workspace_session_records(&session_repo, &created_workspaces, runtime.db_path())
        .await?;

    repo.delete_workspace_bundle(&bundle_id)
        .await
        .map_err(|error| error.to_string())?;
    for workspace in created_workspaces {
        repo.delete_workspace(&workspace.id)
            .await
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

/// A child task whose edits never reached this one: what deleting loses.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnappliedDelegationEdits {
    pub title: String,
    pub files: u32,
}

/// Child tasks whose uncommitted edits never reached this task: what deleting
/// it would throw away. Read-only, so it runs before anything is removed.
pub async fn unapplied_delegation_edits(
    session_repo: &SqliteSessionRepo,
    workspace: &Workspace,
) -> Result<Vec<UnappliedDelegationEdits>, String> {
    let active_root = resolve_workspace_active_root(workspace).to_string();
    let operations = session_repo
        .list_delegation_worktree_operations_by_workspace(&workspace.id)
        .await
        .map_err(|e| e.to_string())?;
    let mut pending = Vec::new();
    for operation in operations {
        if matches!(
            operation.state,
            DelegationWorktreeOperationState::Removed | DelegationWorktreeOperationState::Preparing
        ) {
            continue;
        }
        let applied = session_repo
            .get_delegation_apply_transaction_by_operation_id(&operation.operation_id)
            .await
            .map_err(|e| e.to_string())?
            .is_some_and(|apply| apply.state == DelegationApplyTransactionState::Applied);
        if applied {
            continue;
        }
        let Ok(worktree_path) =
            validate_delegation_worktree_path(&active_root, &operation.worktree_path)
        else {
            continue;
        };
        if !worktree_path.exists() {
            continue;
        }
        let files = delegation_uncommitted_files(&worktree_path)?;
        if files == 0 {
            continue;
        }
        let title = match operation.delegation_id.as_ref() {
            Some(delegation_id) => DelegationRepo::get_delegation(session_repo, delegation_id)
                .await
                .map_err(|e| e.to_string())?
                .and_then(|delegation| delegation.title)
                .filter(|title| !title.trim().is_empty()),
            None => None,
        }
        .unwrap_or_else(|| operation.branch.clone());
        pending.push(UnappliedDelegationEdits { title, files });
    }
    Ok(pending)
}

pub async fn cleanup_delegation_worktrees<R: DeletionRuntime>(
    runtime: &R,
    session_repo: &SqliteSessionRepo,
    workspace: &Workspace,
    discard_changes: bool,
) -> Result<(), String> {
    let operations = session_repo
        .list_delegation_worktree_operations_by_workspace(&workspace.id)
        .await
        .map_err(|e| e.to_string())?;
    let active_root = resolve_workspace_active_root(workspace);
    for operation in operations {
        remove_journaled_delegation_worktree(
            runtime,
            session_repo,
            active_root,
            operation,
            discard_changes,
        )
        .await?;
    }

    Ok(())
}

pub async fn delete_repository_with_workspaces<R: DeletionRuntime>(
    runtime: &R,
    repo: &SqliteWorkspaceRepo,
    session_repo: &SqliteSessionRepo,
    id: &RepositoryId,
    coderabbit_db_path: &Path,
) -> Result<Vec<Workspace>, String> {
    let repository = repo
        .get_repository(id)
        .await
        .map_err(|error| error.to_string())?;
    let repository_root = repository
        .as_ref()
        .map(|repository| repository.root_path.trim())
        .filter(|root| !root.is_empty())
        .unwrap_or(id.0.trim());
    let mut workspaces = repo
        .list_workspaces()
        .await
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter(|workspace| workspace.root_path.trim() == repository_root)
        .collect::<Vec<_>>();
    workspaces.sort_unstable_by(|left, right| {
        let left_depth = left
            .worktree_path
            .as_deref()
            .map(|path| Path::new(path).components().count())
            .unwrap_or_default();
        let right_depth = right
            .worktree_path
            .as_deref()
            .map(|path| Path::new(path).components().count())
            .unwrap_or_default();
        right_depth.cmp(&left_depth)
    });

    ensure_workspace_history_deletable(session_repo, &workspaces)?;

    for workspace in &workspaces {
        cleanup_delegation_worktrees(runtime, session_repo, workspace, false).await?;
        cleanup_workspace_files(workspace)?;
    }

    cleanup_workspace_session_records(session_repo, &workspaces, coderabbit_db_path).await?;

    repo.delete_repository(id)
        .await
        .map_err(|error| error.to_string())?;
    Ok(workspaces)
}

pub async fn cleanup_workspace_session_records(
    session_repo: &SqliteSessionRepo,
    workspaces: &[Workspace],
    coderabbit_db_path: &Path,
) -> Result<(), String> {
    if workspaces.is_empty() {
        return Ok(());
    }

    let workspace_ids = workspaces
        .iter()
        .map(|workspace| workspace.id.clone())
        .collect::<Vec<_>>();
    session_repo
        .delete_workspace_history(&workspace_ids)
        .map_err(|error| error.to_string())?;

    let mut coderabbit_roots = BTreeSet::new();
    for workspace in workspaces {
        coderabbit_roots.insert(workspace.root_path.trim().to_string());
        if let Some(worktree_path) = workspace.worktree_path.as_deref() {
            coderabbit_roots.insert(worktree_path.trim().to_string());
        }
    }
    for root in coderabbit_roots {
        if root.is_empty() {
            continue;
        }
        clear_coderabbit_artifacts(coderabbit_db_path, &root)?;
    }
    Ok(())
}

pub fn ensure_workspace_history_deletable(
    session_repo: &SqliteSessionRepo,
    workspaces: &[Workspace],
) -> Result<(), String> {
    let workspace_ids = workspaces
        .iter()
        .map(|workspace| workspace.id.clone())
        .collect::<Vec<_>>();
    session_repo
        .ensure_workspace_history_deletable(&workspace_ids)
        .map_err(|error| error.to_string())
}
pub fn cleanup_workspace_files(workspace: &Workspace) -> Result<(), String> {
    let Some(worktree_root) = workspace.worktree_path.as_deref().map(str::trim) else {
        return Ok(());
    };
    if worktree_root.is_empty() {
        return Ok(());
    }

    let root_path = workspace.root_path.trim();
    if !root_path.is_empty() && Path::new(root_path) == Path::new(worktree_root) {
        return Ok(());
    }

    let worktree_path = Path::new(worktree_root);
    if !worktree_path.exists() {
        return Ok(());
    }

    let repo_root = Path::new(root_path);
    let should_use_git_worktree_removal =
        !root_path.is_empty() && is_git_repo(repo_root) && worktree_path.join(".git").exists();

    if should_use_git_worktree_removal {
        match remove_worktree(repo_root, worktree_path) {
            Ok(()) => return Ok(()),
            Err(error) if broken_worktree_reason(worktree_path).is_none() => {
                return Err(error.to_string());
            }
            Err(_) => {
                // Fall back to the exact broken directory only. Repository-wide
                // metadata cleanup is never an implicit side effect of deletion.
            }
        }
    }

    std::fs::remove_dir_all(worktree_path).map_err(|error| {
        format!(
            "failed to remove workspace path {}: {}",
            worktree_path.display(),
            error
        )
    })?;

    Ok(())
}

/// Drops the CodeRabbit reviews kept for `root` (tables the desktop creates;
/// nothing to do when they were never made).
pub fn clear_coderabbit_artifacts(db_path: &Path, root: &str) -> Result<(), String> {
    let conn = rusqlite::Connection::open(db_path).map_err(|error| error.to_string())?;
    for table in [
        "workspace_coderabbit_reviews",
        "workspace_coderabbit_review_history",
    ] {
        let exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
                [table],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        if exists {
            conn.execute(
                &format!("DELETE FROM {table} WHERE workspace_root = ?1"),
                [root],
            )
            .map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use dcc_core::domain::session::SessionId;

    #[test]
    fn sessions_whose_memory_is_behind_are_listed() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("database.sqlite");
        let repo = SqliteSessionRepo::open(&db).unwrap();
        let conn = rusqlite::Connection::open(&db).unwrap();
        let now = "2026-10-23T12:00:00Z";
        for (session, workspace) in [
            ("exported", "w"),
            ("behind", "w"),
            ("queued", "w"),
            ("empty", "w"),
            ("other", "x"),
        ] {
            conn.execute(
                "INSERT INTO dcc_sessions (id, project_id, workspace_id, provider_id, state, created_at, updated_at)
                 VALUES (?1, 'p', ?2, 'claude', 'idle', ?3, ?3)",
                rusqlite::params![session, workspace, now],
            )
            .unwrap();
        }
        for (session, count) in [("exported", 2), ("behind", 3), ("queued", 1), ("other", 1)] {
            for sequence in 0..count {
                conn.execute(
                    "INSERT INTO dcc_session_events (event_id, session_id, sequence, occurred_at, kind_json)
                     VALUES (?1, ?2, ?3, ?4, '{}')",
                    rusqlite::params![format!("{session}-{sequence}"), session, sequence, now],
                )
                .unwrap();
            }
        }
        for (session, events) in [("exported", 2), ("behind", 2), ("queued", 1)] {
            conn.execute(
                "INSERT INTO dcc_ai_memory_export_history
                    (session_id, status, attempts, event_count, accepted_count, started_at, finished_at, created_at)
                 VALUES (?1, 'completed', 1, ?2, ?2, ?3, ?3, ?3)",
                rusqlite::params![session, events, now],
            )
            .unwrap();
        }
        repo.enqueue_ai_memory_export(&SessionId("queued".into()))
            .unwrap();

        let mut missing: Vec<String> = repo
            .sessions_missing_memory_export(&[WorkspaceId("w".into())])
            .unwrap()
            .into_iter()
            .map(|session| session.0)
            .collect();
        missing.sort();
        assert_eq!(missing, ["behind", "queued"]);
    }

    #[test]
    fn a_refusal_lists_the_child_tasks_edits() {
        let message = format!(
            "{UNAPPLIED_DELEGATION_EDITS}{}",
            serde_json::to_string(&[UnappliedDelegationEdits {
                title: "Fix".into(),
                files: 2
            }])
            .unwrap()
        );
        assert_eq!(
            parse_unapplied_delegation_edits(&message),
            Some(vec![UnappliedDelegationEdits {
                title: "Fix".into(),
                files: 2
            }])
        );
        assert_eq!(
            parse_unapplied_delegation_edits("unapplied_delegation_edits:3 uncommitted file(s)"),
            None
        );
    }
}

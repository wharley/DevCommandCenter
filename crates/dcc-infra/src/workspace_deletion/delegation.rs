//! Removing a child task's worktree through its journal: the edits that never
//! reached the parent are kept unless the person discarded them.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{Duration, Utc};
use dcc_core::domain::delegation_apply::DelegationApplyTransactionState;
use dcc_core::domain::delegation_worktree::{
    DelegationWorktreeOperation, DelegationWorktreeOperationState,
};
use dcc_core::ports::{
    DelegationApplyTransactionRepo, DelegationWorktreeOperationRepo, WorkspaceRepo,
};
use uuid::Uuid;

use super::DeletionRuntime;
use crate::db::{SqliteSessionRepo, SqliteWorkspaceRepo};
use crate::delegation_apply::try_lock_apply_operation;
use crate::git::remove_worktree;
use crate::git_command::{git_output_err, run_git_output};
use crate::workspace_git::{resolve_current_branch_name, resolve_workspace_active_root};

pub fn delegation_worktrees_root(active_root: &Path) -> PathBuf {
    let parent = active_root.parent().unwrap_or(active_root);
    let worktrees_root =
        if parent.file_name().and_then(|name| name.to_str()) == Some(".dcc-worktrees") {
            parent.to_path_buf()
        } else {
            parent.join(".dcc-worktrees")
        };
    worktrees_root.join(".dcc-delegations")
}

pub fn validate_delegation_worktree_path(root: &str, value: &str) -> Result<PathBuf, String> {
    let worktree_path = PathBuf::from(value.trim());
    if !worktree_path.is_absolute() {
        return Err("delegation worktree path must be absolute".to_string());
    }
    let allowed_root = delegation_worktrees_root(Path::new(root));
    let canonical_allowed_root =
        fs::canonicalize(&allowed_root).unwrap_or_else(|_| allowed_root.clone());
    let canonical_worktree_path =
        fs::canonicalize(&worktree_path).unwrap_or_else(|_| worktree_path.clone());
    if !canonical_worktree_path.starts_with(&canonical_allowed_root) {
        return Err("refusing to use a path outside the DCC delegation worktree root".to_string());
    }
    Ok(worktree_path)
}

pub async fn remove_journaled_delegation_worktree<R: DeletionRuntime>(
    runtime: &R,
    journal_repo: &SqliteSessionRepo,
    requested_root: &str,
    mut operation: DelegationWorktreeOperation,
    discard_changes: bool,
) -> Result<(), String> {
    validate_delegation_operation_workspace_scope(runtime, requested_root, &operation).await?;
    if matches!(operation.state, DelegationWorktreeOperationState::Removed) {
        return Ok(());
    }
    let artifact_root = runtime
        .app_data_dir()
        .join("delegation-apply")
        .join("transactions");
    let _operation_lock = try_lock_apply_operation(&artifact_root, &operation.operation_id.0)?
        .ok_or_else(|| "delegation operation is owned by another live process".to_string())?;
    // Edits already applied to the parent are not lost when the worktree goes.
    let mut edits_reached_parent = false;
    if let Some(apply) = journal_repo
        .get_delegation_apply_transaction_by_operation_id(&operation.operation_id)
        .await
        .map_err(|error| error.to_string())?
    {
        edits_reached_parent = apply.state == DelegationApplyTransactionState::Applied;
        match apply.state {
            DelegationApplyTransactionState::Preparing
            | DelegationApplyTransactionState::Prepared
            | DelegationApplyTransactionState::Applying => {
                return Err("delegation apply is still active; retry cleanup after recovery".to_string())
            }
            DelegationApplyTransactionState::RecoveryRequired => {
                return Err(
                    "delegation apply changed the destination ambiguously; recover it before removing the worktree"
                        .to_string(),
                )
            }
            DelegationApplyTransactionState::Applied
            | DelegationApplyTransactionState::RolledBack => {}
        }
    }
    // Validate immutable ownership before recording destructive intent. A bad
    // journal row must not get stuck in Removing without touching the worktree.
    let worktree_path =
        validate_delegation_worktree_path(requested_root, &operation.worktree_path)?;
    let expected_branch = operation.branch.clone();
    let expected_oid = operation
        .expected_branch_oid
        .clone()
        .ok_or_else(|| "delegation worktree journal has no expected branch OID".to_string())?;
    let keep_unapplied_edits = !discard_changes
        && !edits_reached_parent
        && !matches!(operation.state, DelegationWorktreeOperationState::Preparing);
    // Checked before the journal records destructive intent: a refusal must
    // leave an open task exactly as it was, still reviewable and applicable.
    if keep_unapplied_edits && worktree_path.exists() {
        let files = delegation_uncommitted_files(&worktree_path)?;
        if files > 0 {
            return Err(unapplied_delegation_edits_error(files));
        }
    }
    if matches!(operation.state, DelegationWorktreeOperationState::Preparing) {
        operation.state = DelegationWorktreeOperationState::CleanupRequired;
        operation.last_error = Some("delegation worktree preparation was interrupted".to_string());
        operation.updated_at = Utc::now().to_rfc3339();
        if !journal_repo
            .compare_and_swap_delegation_worktree_operation(
                DelegationWorktreeOperationState::Preparing,
                &operation,
            )
            .await
            .map_err(|error| error.to_string())?
        {
            return Err("delegation worktree journal changed; retry cleanup".to_string());
        }
    }
    if !matches!(
        operation.state,
        DelegationWorktreeOperationState::Prepared
            | DelegationWorktreeOperationState::Bound
            | DelegationWorktreeOperationState::ReviewPending
            | DelegationWorktreeOperationState::Applied
            | DelegationWorktreeOperationState::Removing
            | DelegationWorktreeOperationState::CleanupRequired
    ) {
        return Err(format!(
            "delegation worktree cannot be removed while it is {:?}",
            operation.state
        ));
    }

    let recovery_owner = Uuid::new_v4().to_string();
    let claimed_at = Utc::now();
    let lease_until = claimed_at + Duration::minutes(2);
    operation = match journal_repo
        .claim_delegation_worktree_removal(
            &operation.operation_id,
            &recovery_owner,
            &claimed_at.to_rfc3339(),
            &lease_until.to_rfc3339(),
        )
        .await
        .map_err(|error| error.to_string())?
    {
        Some(operation) => operation,
        None => {
            let current = journal_repo
                .get_delegation_worktree_operation(&operation.operation_id)
                .await
                .map_err(|error| error.to_string())?;
            if current
                .as_ref()
                .is_some_and(|current| current.state == DelegationWorktreeOperationState::Removed)
            {
                return Ok(());
            }
            if current
                .as_ref()
                .is_some_and(|current| current.state == DelegationWorktreeOperationState::Removing)
            {
                return Err(
                    "delegation worktree cleanup is already running in another process".to_string(),
                );
            }
            return Err("delegation worktree journal changed; retry cleanup".to_string());
        }
    };

    let removal = if worktree_path.exists() {
        runtime
            .git_pair_mutation(
                requested_root,
                worktree_path,
                move |trusted_root, trusted_worktree| {
                    remove_journaled_delegation_worktree_inner(
                        trusted_root,
                        trusted_worktree,
                        &expected_branch,
                        &expected_oid,
                        keep_unapplied_edits,
                    )
                },
            )
            .await
    } else {
        runtime
            .git_mutation(requested_root, move |trusted_root| {
                release_delegation_branch(
                    trusted_root,
                    &expected_branch,
                    &expected_oid,
                    keep_unapplied_edits,
                )
            })
            .await
    };

    if let Err(error) = removal {
        let _ = journal_repo
            .finalize_delegation_worktree_removal(
                &operation.operation_id,
                &recovery_owner,
                DelegationWorktreeOperationState::CleanupRequired,
                Some(error.clone()),
                &Utc::now().to_rfc3339(),
            )
            .await;
        return Err(error);
    }
    if journal_repo
        .finalize_delegation_worktree_removal(
            &operation.operation_id,
            &recovery_owner,
            DelegationWorktreeOperationState::Removed,
            None,
            &Utc::now().to_rfc3339(),
        )
        .await
        .map_err(|error| error.to_string())?
        .is_none()
    {
        if journal_repo
            .get_delegation_worktree_operation(&operation.operation_id)
            .await
            .map_err(|error| error.to_string())?
            .is_some_and(|current| current.state == DelegationWorktreeOperationState::Removed)
        {
            return Ok(());
        }
        return Err("delegation worktree was removed but its journal needs recovery".to_string());
    }
    Ok(())
}

pub async fn validate_delegation_operation_workspace_scope<R: DeletionRuntime>(
    runtime: &R,
    requested_root: &str,
    operation: &DelegationWorktreeOperation,
) -> Result<(), String> {
    if operation.source_root.trim() != requested_root.trim() {
        return Err("delegation worktree journal does not belong to workspace_root".to_string());
    }
    let workspace_repo =
        SqliteWorkspaceRepo::open(&runtime.db_path()).map_err(|error| error.to_string())?;
    let workspace = workspace_repo
        .get_workspace(&operation.workspace_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| {
            format!(
                "delegation worktree workspace no longer exists: {}",
                operation.workspace_id.0
            )
        })?;
    if resolve_workspace_active_root(&workspace).trim() != requested_root.trim() {
        return Err("delegation worktree workspace mapping changed; refusing cleanup".to_string());
    }
    Ok(())
}

pub fn delete_delegation_branch_ref(
    root: &Path,
    branch: &str,
    expected_oid: &str,
) -> Result<(), String> {
    if !branch.starts_with("dcc/delegation/") {
        return Err("refusing to remove a non-DCC delegation branch".to_string());
    }
    let root = root
        .to_str()
        .ok_or_else(|| "workspace root is not valid UTF-8".to_string())?;
    validate_branch_for_fetch(root, branch)?;
    let reference = format!("refs/heads/{branch}");
    let observed = run_git_output(root, &["rev-parse", "--verify", "--quiet", &reference])?;
    if !observed.status.success() {
        if observed.status.code() == Some(1) {
            return Ok(());
        }
        return Err(git_output_err("git rev-parse --verify", &observed.stderr));
    }
    let observed_oid = String::from_utf8_lossy(&observed.stdout)
        .trim()
        .to_ascii_lowercase();
    if observed_oid != expected_oid.trim().to_ascii_lowercase() {
        return Err(
            "delegation branch advanced after its journal identity was captured".to_string(),
        );
    }
    let output = run_git_output(root, &["update-ref", "-d", &reference, expected_oid])?;
    if output.status.success() {
        Ok(())
    } else {
        // Another process may have completed the same compare-delete between
        // rev-parse and update-ref. Absence is success; a successor remains
        // protected and must never be deleted.
        let after = run_git_output(root, &["rev-parse", "--verify", "--quiet", &reference])?;
        if !after.status.success() && after.status.code() == Some(1) {
            return Ok(());
        }
        if after.status.success()
            && String::from_utf8_lossy(&after.stdout)
                .trim()
                .eq_ignore_ascii_case(expected_oid.trim())
        {
            return Err(git_output_err("git update-ref -d", &output.stderr));
        }
        Err("delegation branch advanced while its journaled ref was being removed".to_string())
    }
}

/// Prefix of the error a cleanup returns instead of deleting edits that never
/// reached the parent. The frontend reads it to offer "discard" explicitly.
pub const UNAPPLIED_DELEGATION_EDITS: &str = "unapplied_delegation_edits:";

pub fn unapplied_delegation_edits_error(files: u32) -> String {
    format!(
        "{UNAPPLIED_DELEGATION_EDITS}{files} uncommitted file(s) in a child task's worktree were never applied to the parent"
    )
}

/// Uncommitted files, untracked included: what `worktree remove --force` loses.
pub fn delegation_uncommitted_files(worktree_path: &Path) -> Result<u32, String> {
    let worktree = worktree_path
        .to_str()
        .ok_or_else(|| "delegation worktree path is not valid UTF-8".to_string())?;
    let output = run_git_output(
        worktree,
        &[
            "status",
            "--porcelain=v1",
            "--no-renames",
            "--untracked-files=normal",
        ],
    )?;
    if !output.status.success() {
        return Err(git_output_err("git status", &output.stderr));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count() as u32)
}

/// Whether `oid` is in the history of any branch or remote-tracking ref other
/// than the delegation's own: then deleting that branch loses nothing.
pub fn delegation_tip_reachable_elsewhere(
    root: &str,
    branch: &str,
    oid: &str,
) -> Result<bool, String> {
    let output = run_git_output(
        root,
        &[
            "for-each-ref",
            "--format=%(refname)",
            "--contains",
            oid,
            "refs/heads",
            "refs/remotes",
        ],
    )?;
    if !output.status.success() {
        return Err(git_output_err(
            "git for-each-ref --contains",
            &output.stderr,
        ));
    }
    let own = format!("refs/heads/{branch}");
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .any(|reference| !reference.is_empty() && reference != own))
}

/// Drops the delegation branch once its worktree is gone. A branch that moved
/// past its journaled OID holds the child's own commits: it is deleted when
/// they live on elsewhere or the person discarded them, and kept otherwise,
/// so freeing the disk never costs a commit.
pub fn release_delegation_branch(
    root: &Path,
    branch: &str,
    expected_oid: &str,
    keep_unapplied_edits: bool,
) -> Result<(), String> {
    if !branch.starts_with("dcc/delegation/") {
        return Err("refusing to remove a non-DCC delegation branch".to_string());
    }
    let root_str = root
        .to_str()
        .ok_or_else(|| "workspace root is not valid UTF-8".to_string())?;
    validate_branch_for_fetch(root_str, branch)?;
    let reference = format!("refs/heads/{branch}");
    let observed = run_git_output(root_str, &["rev-parse", "--verify", "--quiet", &reference])?;
    if !observed.status.success() {
        if observed.status.code() == Some(1) {
            return Ok(());
        }
        return Err(git_output_err("git rev-parse --verify", &observed.stderr));
    }
    let observed_oid = String::from_utf8_lossy(&observed.stdout)
        .trim()
        .to_ascii_lowercase();
    if observed_oid == expected_oid.trim().to_ascii_lowercase() {
        return delete_delegation_branch_ref(root, branch, expected_oid);
    }
    if keep_unapplied_edits && !delegation_tip_reachable_elsewhere(root_str, branch, &observed_oid)?
    {
        return Ok(());
    }
    delete_delegation_branch_ref(root, branch, &observed_oid)
}

pub fn remove_journaled_delegation_worktree_inner(
    root: &Path,
    worktree_path: &Path,
    expected_branch: &str,
    expected_oid: &str,
    keep_unapplied_edits: bool,
) -> Result<(), String> {
    let worktree = worktree_path
        .to_str()
        .ok_or_else(|| "delegation worktree path is not valid UTF-8".to_string())?;
    // A different branch checked out means the folder is no longer the
    // child's; commits on the child's own branch are handled below.
    if resolve_current_branch_name(worktree)? != expected_branch {
        return Err(
            "delegation worktree branch identity changed after it was journaled".to_string(),
        );
    }
    let expected_name = expected_branch.replace('/', "-");
    if worktree_path.file_name().and_then(|name| name.to_str()) != Some(expected_name.as_str()) {
        return Err("delegation worktree path does not match its journaled branch".to_string());
    }
    if keep_unapplied_edits {
        let files = delegation_uncommitted_files(worktree_path)?;
        if files > 0 {
            return Err(unapplied_delegation_edits_error(files));
        }
    }
    remove_worktree(root, worktree_path).map_err(|error| error.to_string())?;
    if worktree_path.exists() {
        return Err(format!(
            "git removed delegation metadata but the worktree path still exists: {}",
            worktree_path.display()
        ));
    }
    release_delegation_branch(root, expected_branch, expected_oid, keep_unapplied_edits)
}

pub fn validate_branch_for_fetch(root: &str, branch: &str) -> Result<(), String> {
    let output = run_git_output(root, &["check-ref-format", "--branch", branch])?;
    if output.status.success() {
        return Ok(());
    }
    Err(git_output_err(
        "git check-ref-format --branch",
        &output.stderr,
    ))
}

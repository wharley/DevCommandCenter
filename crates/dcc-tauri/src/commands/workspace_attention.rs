//! Blockers the sidebar's "Needs you" section shows for every active task.
//!
//! The rail rows poll git and the forge only for the rows on screen, so a
//! blocker in a collapsed project would never surface. This computes the few
//! states that truly block the person — local merge conflicts, a PR in
//! conflict with its base, failing checks, delegated edits awaiting review —
//! for all active tasks in one call, with a short PR-status cache so the
//! forge is not hit once per task per poll.

use std::{
    collections::HashMap,
    path::Path,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;

use dcc_core::{
    domain::{delegation::DelegationStatus, workspace::WorkspaceState},
    ports::{DelegationRepo, WorkspaceRepo},
};
use dcc_infra::db::{SqliteSessionRepo, SqliteWorkspaceRepo};

use crate::{
    commands::{
        forge_commands::{workspace_pr_status_with_state, WorkspacePrStatusInput},
        workspace_commands::workspace_git_status_inner,
    },
    state::WorkspaceCommandState,
};

const PR_STATUS_TTL: Duration = Duration::from_secs(120);

/// The fields of a task's PR status the blocker rules read.
#[derive(Clone, Debug, Default)]
pub struct PrSnapshot {
    pub number: Option<u64>,
    pub head_branch: Option<String>,
    pub state: Option<String>,
    pub mergeable: Option<String>,
    pub merge_state_status: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceBlockerKind {
    /// Local merge conflicts in the task's checkout.
    Conflicts,
    /// The task's PR conflicts with its base branch.
    PrConflicts,
    /// The task's PR has failing checks.
    ChecksFailing,
    /// Edits from a delegation wait for the person to apply or discard.
    DelegatedEditsReview,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceBlocker {
    pub workspace_id: String,
    pub kind: WorkspaceBlockerKind,
    /// PR number for PR blockers; count for delegated reviews.
    pub count: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceAttentionBlockersOutput {
    pub blockers: Vec<WorkspaceBlocker>,
}

/// Classifies one task's git and PR state; pure so the rules are testable.
pub fn classify_git_blockers(
    conflict_count: u32,
    merge_in_progress: bool,
    current_branch: Option<&str>,
    pr: Option<&PrSnapshot>,
) -> Option<(WorkspaceBlockerKind, Option<u64>)> {
    // A merge with every conflict resolved only needs to be completed; the
    // recap already offers that, so only open conflicts block.
    let _ = merge_in_progress;
    if conflict_count > 0 {
        return Some((WorkspaceBlockerKind::Conflicts, None));
    }
    let pr = pr?;
    let belongs = match (pr.head_branch.as_deref(), current_branch) {
        (Some(head), Some(current)) => head.trim() == current.trim(),
        (None, _) => true,
        _ => false,
    };
    if !belongs || pr.state.as_deref().map(str::to_lowercase).as_deref() != Some("open") {
        return None;
    }
    let number = pr.number;
    if pr.mergeable.as_deref() == Some("CONFLICTING")
        || pr.merge_state_status.as_deref().map(str::to_uppercase).as_deref() == Some("DIRTY")
    {
        return Some((WorkspaceBlockerKind::PrConflicts, number));
    }
    // UNSTABLE is GitHub's "failing commit status". BLOCKED also covers
    // "review required", which is not a failure, so it is left out.
    if pr.merge_state_status.as_deref().map(str::to_uppercase).as_deref() == Some("UNSTABLE") {
        return Some((WorkspaceBlockerKind::ChecksFailing, number));
    }
    None
}

fn pr_cache() -> &'static Mutex<HashMap<String, (Instant, PrSnapshot)>> {
    static CACHE: OnceLock<
        Mutex<HashMap<String, (Instant, PrSnapshot)>>,
    > = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

async fn cached_pr_status(
    state: &WorkspaceCommandState,
    root: &str,
    branch: &str,
) -> Option<PrSnapshot> {
    let key = format!("{root}\u{1f}{branch}");
    if let Some((at, view)) = pr_cache().lock().ok().and_then(|cache| cache.get(&key).cloned()) {
        if at.elapsed() < PR_STATUS_TTL {
            return Some(view);
        }
    }
    let output = workspace_pr_status_with_state(
        state,
        WorkspacePrStatusInput {
            workspace_root: root.to_string(),
            branch: Some(branch.to_string()),
            forge_login: None,
        },
    )
    .await
    .ok()?;
    let view = PrSnapshot {
        number: output.number.map(u64::from),
        head_branch: output.head_branch,
        state: output.state,
        mergeable: output.mergeable,
        merge_state_status: output.merge_state_status,
    };
    if let Ok(mut cache) = pr_cache().lock() {
        cache.insert(key, (Instant::now(), view.clone()));
    }
    Some(view)
}

pub async fn workspace_attention_blockers_with_state(
    state: &WorkspaceCommandState,
) -> Result<WorkspaceAttentionBlockersOutput, String> {
    let workspaces = SqliteWorkspaceRepo::open(&state.db_path)
        .map_err(|error| error.to_string())?
        .list_workspaces()
        .await
        .map_err(|error| error.to_string())?;
    let mut blockers = Vec::new();

    let reviews = SqliteSessionRepo::open(&state.db_path)
        .map_err(|error| error.to_string())?
        .list_delegations_by_status(DelegationStatus::ReviewPending)
        .await
        .map_err(|error| error.to_string())?;
    let mut review_counts = HashMap::<String, u64>::new();
    for delegation in reviews {
        *review_counts.entry(delegation.workspace_id.0).or_default() += 1;
    }

    for workspace in workspaces {
        if matches!(workspace.state, WorkspaceState::Archived | WorkspaceState::Completed) {
            continue;
        }
        if let Some(count) = review_counts.get(&workspace.id.0) {
            blockers.push(WorkspaceBlocker {
                workspace_id: workspace.id.0.clone(),
                kind: WorkspaceBlockerKind::DelegatedEditsReview,
                count: Some(*count),
            });
        }
        let root = workspace
            .worktree_path
            .clone()
            .unwrap_or_else(|| workspace.root_path.clone());
        if !Path::new(&root).is_dir() {
            continue;
        }
        let status_root = root.clone();
        let Ok(Ok(status)) =
            tokio::task::spawn_blocking(move || workspace_git_status_inner(&status_root)).await
        else {
            continue;
        };
        let pr = match status.current_branch.as_deref() {
            Some(branch) if status.conflict_count == 0 => {
                cached_pr_status(state, &root, branch).await
            }
            _ => None,
        };
        if let Some((kind, count)) = classify_git_blockers(
            status.conflict_count,
            status.merge_in_progress,
            status.current_branch.as_deref(),
            pr.as_ref(),
        ) {
            blockers.push(WorkspaceBlocker {
                workspace_id: workspace.id.0.clone(),
                kind,
                count,
            });
        }
    }
    Ok(WorkspaceAttentionBlockersOutput { blockers })
}

#[tauri::command]
pub async fn workspace_attention_blockers(
    state: State<'_, WorkspaceCommandState>,
) -> Result<WorkspaceAttentionBlockersOutput, String> {
    workspace_attention_blockers_with_state(&state).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pr(state: &str, mergeable: Option<&str>, merge_state: Option<&str>) -> PrSnapshot {
        PrSnapshot {
            number: Some(12),
            head_branch: Some("feature".to_string()),
            state: Some(state.to_string()),
            mergeable: mergeable.map(str::to_string),
            merge_state_status: merge_state.map(str::to_string),
        }
    }

    #[test]
    fn local_conflicts_block_before_anything_else() {
        assert_eq!(
            classify_git_blockers(2, true, Some("feature"), Some(&pr("OPEN", None, Some("UNSTABLE")))),
            Some((WorkspaceBlockerKind::Conflicts, None))
        );
    }

    #[test]
    fn open_prs_block_on_conflicts_or_failing_checks_only() {
        let classify = |pr: PrSnapshot| classify_git_blockers(0, false, Some("feature"), Some(&pr));
        assert_eq!(
            classify(pr("OPEN", Some("CONFLICTING"), Some("DIRTY"))),
            Some((WorkspaceBlockerKind::PrConflicts, Some(12)))
        );
        assert_eq!(
            classify(pr("OPEN", Some("MERGEABLE"), Some("UNSTABLE"))),
            Some((WorkspaceBlockerKind::ChecksFailing, Some(12)))
        );
        // Waiting for a required review is not a failure.
        assert_eq!(classify(pr("OPEN", Some("MERGEABLE"), Some("BLOCKED"))), None);
        assert_eq!(classify(pr("OPEN", Some("MERGEABLE"), Some("CLEAN"))), None);
        assert_eq!(classify(pr("MERGED", Some("CONFLICTING"), Some("DIRTY"))), None);
    }

    #[test]
    fn a_pr_from_another_branch_is_ignored() {
        let mut other = pr("OPEN", Some("CONFLICTING"), Some("DIRTY"));
        other.head_branch = Some("older/merged".to_string());
        assert_eq!(classify_git_blockers(0, false, Some("feature"), Some(&other)), None);
        assert_eq!(classify_git_blockers(0, false, Some("feature"), None), None);
    }
}

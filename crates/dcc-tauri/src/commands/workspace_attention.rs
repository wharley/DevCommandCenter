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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
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
    /// When this app first saw the blocker; null when it was already there at
    /// startup, so its real start is unknown.
    pub since: Option<String>,
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

/// When each blocker first showed up, so "Needs you" can order tasks by when
/// they came back to the person.
#[derive(Default)]
struct BlockerFirstSeen {
    primed: bool,
    seen: HashMap<(String, WorkspaceBlockerKind), Option<String>>,
}

impl BlockerFirstSeen {
    /// Stamps the blockers found in this pass and forgets the ones that went
    /// away, so a blocker that comes back is stamped again. Blockers present
    /// on the first pass get no time: stamping them "now" would put blockers
    /// older than the app above fresh results.
    fn stamp(&mut self, blockers: &mut [WorkspaceBlocker], now: &str) {
        let mut next = HashMap::new();
        for blocker in blockers.iter_mut() {
            let key = (blocker.workspace_id.clone(), blocker.kind);
            let since = match self.seen.get(&key) {
                Some(since) => since.clone(),
                None if self.primed => Some(now.to_string()),
                None => None,
            };
            blocker.since = since.clone();
            next.insert(key, since);
        }
        self.seen = next;
        self.primed = true;
    }
}

fn blocker_first_seen() -> &'static Mutex<BlockerFirstSeen> {
    static FIRST_SEEN: OnceLock<Mutex<BlockerFirstSeen>> = OnceLock::new();
    FIRST_SEEN.get_or_init(|| Mutex::new(BlockerFirstSeen::default()))
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
                since: None,
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
                since: None,
            });
        }
    }
    if let Ok(mut first_seen) = blocker_first_seen().lock() {
        first_seen.stamp(&mut blockers, &chrono::Utc::now().to_rfc3339());
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

    fn blocker(workspace: &str, kind: WorkspaceBlockerKind) -> WorkspaceBlocker {
        WorkspaceBlocker {
            workspace_id: workspace.to_string(),
            kind,
            count: None,
            since: None,
        }
    }

    #[test]
    fn first_seen_leaves_startup_blockers_unstamped_and_stamps_new_ones() {
        let mut first_seen = BlockerFirstSeen::default();
        let mut startup = vec![blocker("a", WorkspaceBlockerKind::Conflicts)];
        first_seen.stamp(&mut startup, "2026-10-06T09:00:00Z");
        assert_eq!(startup[0].since, None);

        let mut later = vec![
            blocker("a", WorkspaceBlockerKind::Conflicts),
            blocker("b", WorkspaceBlockerKind::ChecksFailing),
        ];
        first_seen.stamp(&mut later, "2026-10-06T09:01:00Z");
        assert_eq!(later[0].since, None);
        assert_eq!(later[1].since.as_deref(), Some("2026-10-06T09:01:00Z"));

        let mut still = vec![blocker("b", WorkspaceBlockerKind::ChecksFailing)];
        first_seen.stamp(&mut still, "2026-10-06T09:02:00Z");
        assert_eq!(still[0].since.as_deref(), Some("2026-10-06T09:01:00Z"));
    }

    #[test]
    fn first_seen_stamps_a_blocker_again_when_it_comes_back() {
        let mut first_seen = BlockerFirstSeen::default();
        first_seen.stamp(&mut [], "2026-10-06T09:00:00Z");
        let mut checks = vec![blocker("a", WorkspaceBlockerKind::ChecksFailing)];
        first_seen.stamp(&mut checks, "2026-10-06T09:01:00Z");
        first_seen.stamp(&mut [], "2026-10-06T09:02:00Z");
        let mut again = vec![blocker("a", WorkspaceBlockerKind::ChecksFailing)];
        first_seen.stamp(&mut again, "2026-10-06T09:03:00Z");
        assert_eq!(again[0].since.as_deref(), Some("2026-10-06T09:03:00Z"));
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

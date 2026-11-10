//! Git and pull-request state of every open task, refreshed on the desktop's
//! rail cadence (git 30s, PR and branch diff 60s), and the blockers "Needs
//! you" derives from it. Blocking: call `refresh` off the UI thread.

use std::collections::HashMap;
use std::path::Path;

use dcc_core::domain::attention::{
    BlockerFirstSeen, PrSnapshot, WorkspaceBlocker, WorkspaceBlockerKind, classify_git_blockers,
};
use dcc_core::domain::workspace_git::{WorkspaceGitStatusOutput, WorkspacePrStatusOutput};
use dcc_infra::workspace_git::{
    read_rail_git_status, workspace_git_branch_diff, workspace_pr_status,
};

const GIT_EVERY_MS: i64 = 30_000;
const PR_EVERY_MS: i64 = 60_000;
const DIFF_EVERY_MS: i64 = 60_000;

/// What the rail knows about one task's checkout.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GitSnapshot {
    pub git: Option<WorkspaceGitStatusOutput>,
    pub pr: Option<WorkspacePrStatusOutput>,
    /// Files committed on the branch against its base, once asked for.
    pub committed_vs_base: Option<usize>,
    pub git_at: Option<i64>,
    pub pr_at: Option<i64>,
    pub diff_at: Option<i64>,
}

impl GitSnapshot {
    /// No local change and nothing to push.
    pub fn is_clean(&self) -> bool {
        self.git.as_ref().is_some_and(|git| {
            git.staged.is_empty() && git.unstaged.is_empty() && git.ahead_of_remote_count == 0
        })
    }

    pub fn current_branch(&self) -> Option<&str> {
        self.git
            .as_ref()
            .and_then(|git| git.current_branch.as_deref())
    }
}

/// One open task to watch.
#[derive(Clone, Debug)]
pub struct Watched {
    pub id: String,
    /// The worktree, or the project root for a task without one.
    pub root: String,
    pub branch: String,
    /// The desktop asks for the branch diff only for a task whose agent
    /// finished, with a clean checkout and no PR: "ready to open a PR?".
    pub finished_turn: bool,
}

fn due(at: Option<i64>, every: i64, now: i64) -> bool {
    at.is_none_or(|at| now - at >= every)
}

/// How many checkouts are read at once (each may wait on the forge CLI).
const PARALLEL: usize = 8;

/// Polls a future whose work all blocks, so it finishes in one poll. The
/// forge code runs its own `block_on` inside, which `futures::block_on`
/// here would nest (and panic).
pub(crate) fn run_blocking<F: std::future::Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    loop {
        if let std::task::Poll::Ready(output) = future.as_mut().poll(&mut context) {
            return output;
        }
        std::thread::yield_now();
    }
}

fn refresh_one(db_path: &Path, task: &Watched, mut snapshot: GitSnapshot, now: i64) -> GitSnapshot {
    if due(snapshot.git_at, GIT_EVERY_MS, now) {
        if let Ok(git) = read_rail_git_status(&task.root) {
            snapshot.git = Some(git);
        }
        snapshot.git_at = Some(now);
    }
    if due(snapshot.pr_at, PR_EVERY_MS, now) {
        let branch = snapshot
            .current_branch()
            .unwrap_or(&task.branch)
            .to_string();
        if let Ok(pr) = run_blocking(workspace_pr_status(db_path, &task.root, Some(branch), None)) {
            snapshot.pr = Some(pr);
        }
        snapshot.pr_at = Some(now);
    }
    let wants_diff = task.finished_turn
        && snapshot.is_clean()
        && snapshot.pr.as_ref().is_some_and(|pr| pr.number.is_none());
    if wants_diff && due(snapshot.diff_at, DIFF_EVERY_MS, now) {
        if let Ok(diff) = run_blocking(workspace_git_branch_diff(db_path, &task.root)) {
            snapshot.committed_vs_base = Some(diff.changes.len());
        }
        snapshot.diff_at = Some(now);
    }
    snapshot
}

/// Refreshes what is stale, several checkouts at a time, and drops tasks no
/// longer watched. Blocking.
pub fn refresh(
    db_path: &Path,
    watched: &[Watched],
    mut cache: HashMap<String, GitSnapshot>,
    now: i64,
) -> HashMap<String, GitSnapshot> {
    cache.retain(|id, _| watched.iter().any(|task| &task.id == id));
    let present: Vec<&Watched> = watched
        .iter()
        .filter(|task| Path::new(&task.root).is_dir())
        .collect();
    for batch in present.chunks(PARALLEL) {
        let results: Vec<(String, GitSnapshot)> = std::thread::scope(|scope| {
            let handles: Vec<_> = batch
                .iter()
                .map(|task| {
                    let snapshot = cache.get(&task.id).cloned().unwrap_or_default();
                    scope
                        .spawn(move || (task.id.clone(), refresh_one(db_path, task, snapshot, now)))
                })
                .collect();
            handles
                .into_iter()
                .filter_map(|handle| handle.join().ok())
                .collect()
        });
        cache.extend(results);
    }
    cache
}

/// The blockers "Needs you" shows, as `workspace_attention_blockers` builds
/// them: delegated edits to review, then the checkout's git/PR state.
pub fn blockers(
    open_task_ids: &[String],
    cache: &HashMap<String, GitSnapshot>,
    review_pending: &HashMap<String, usize>,
    first_seen: &mut BlockerFirstSeen,
    now_iso: &str,
) -> Vec<WorkspaceBlocker> {
    let mut found = Vec::new();
    for id in open_task_ids {
        if let Some(count) = review_pending.get(id).filter(|count| **count > 0) {
            found.push(WorkspaceBlocker {
                workspace_id: id.clone(),
                kind: WorkspaceBlockerKind::DelegatedEditsReview,
                count: Some(*count as u64),
                since: None,
            });
        }
        let Some(snapshot) = cache.get(id) else {
            continue;
        };
        let Some(git) = &snapshot.git else { continue };
        // Like the desktop, the PR only counts once local conflicts are gone.
        let pr = snapshot
            .pr
            .as_ref()
            .filter(|_| git.conflict_count == 0 && git.current_branch.is_some())
            .map(|pr| PrSnapshot {
                checks_failing: pr.checks_failing,
                number: pr.number.map(u64::from),
                head_branch: pr.head_branch.clone(),
                state: pr.state.clone(),
                mergeable: pr.mergeable.clone(),
                merge_state_status: pr.merge_state_status.clone(),
            });
        if let Some((kind, count)) = classify_git_blockers(
            git.conflict_count,
            git.merge_in_progress,
            git.current_branch.as_deref(),
            pr.as_ref(),
        ) {
            found.push(WorkspaceBlocker {
                workspace_id: id.clone(),
                kind,
                count,
                since: None,
            });
        }
    }
    first_seen.stamp(&mut found, now_iso);
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(conflicts: u32, pr_merge_state: Option<&str>) -> GitSnapshot {
        GitSnapshot {
            git: Some(WorkspaceGitStatusOutput {
                staged: vec![],
                unstaged: vec![],
                staged_fingerprint: String::new(),
                current_branch: Some("dcc/task".into()),
                ahead_of_remote_count: 0,
                behind_of_remote_count: 0,
                conflict_count: conflicts,
                merge_in_progress: false,
                has_remote: true,
            }),
            pr: pr_merge_state.map(|state| WorkspacePrStatusOutput {
                checks_failing: None,
                provider: Some("github".into()),
                host: None,
                number: Some(9),
                title: None,
                url: None,
                head_branch: Some("dcc/task".into()),
                base_branch: None,
                state: Some("OPEN".into()),
                is_draft: false,
                mergeable: Some("MERGEABLE".into()),
                merge_state_status: Some(state.into()),
            }),
            ..GitSnapshot::default()
        }
    }

    #[test]
    fn blockers_come_from_git_pr_and_delegations() {
        let ids: Vec<String> = ["conflicts", "checks", "review", "clean"]
            .map(String::from)
            .to_vec();
        let cache = HashMap::from([
            ("conflicts".to_string(), snapshot(2, Some("UNSTABLE"))),
            ("checks".to_string(), snapshot(0, Some("UNSTABLE"))),
            ("clean".to_string(), snapshot(0, Some("CLEAN"))),
        ]);
        let reviews = HashMap::from([("review".to_string(), 2)]);
        let mut first_seen = BlockerFirstSeen::default();
        let found = blockers(
            &ids,
            &cache,
            &reviews,
            &mut first_seen,
            "2026-10-23T12:00:00Z",
        );
        let kinds: Vec<(&str, WorkspaceBlockerKind, Option<u64>)> = found
            .iter()
            .map(|blocker| (blocker.workspace_id.as_str(), blocker.kind, blocker.count))
            .collect();
        assert_eq!(
            kinds,
            [
                ("conflicts", WorkspaceBlockerKind::Conflicts, None),
                ("checks", WorkspaceBlockerKind::ChecksFailing, Some(9)),
                (
                    "review",
                    WorkspaceBlockerKind::DelegatedEditsReview,
                    Some(2)
                ),
            ]
        );
        assert!(
            found.iter().all(|blocker| blocker.since.is_none()),
            "first pass is unstamped"
        );
    }

    #[test]
    fn untouched_entries_are_dropped_with_their_task() {
        let cache = HashMap::from([("gone".to_string(), GitSnapshot::default())]);
        let refreshed = refresh(Path::new("/nonexistent.sqlite"), &[], cache, 0);
        assert!(refreshed.is_empty());
    }
}

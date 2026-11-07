//! What the task's delivery button offers (`resolveCommitMode` in the
//! desktop's `WorkspaceCommitButton.logic.ts`): local git state first
//! (a merge in progress, conflicts), then the pull request's state, then the
//! working tree, then the branch name as a last hint.

use super::workspace::WorkspaceState;
use super::workspace_git::{WorkspaceGitStatusOutput, WorkspacePrStatusOutput};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CommitMode {
    CreatePr,
    OpenPr,
    CommitAndPush,
    Push,
    Fix,
    ResolveConflicts,
    CompleteMerge,
    Merge,
    Merged,
    Closed,
}

impl CommitMode {
    /// The id the desktop's i18n keys use (`commit.modes.<id>.<status>`).
    pub fn id(self) -> &'static str {
        match self {
            Self::CreatePr => "create-pr",
            Self::OpenPr => "open-pr",
            Self::CommitAndPush => "commit-and-push",
            Self::Push => "push",
            Self::Fix => "fix",
            Self::ResolveConflicts => "resolve-conflicts",
            Self::CompleteMerge => "complete-merge",
            Self::Merge => "merge",
            Self::Merged => "merged",
            Self::Closed => "closed",
        }
    }
}

/// `CommitButtonStatus`
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommitButtonStatus {
    Idle,
    Busy,
    Done,
    Error,
}

impl CommitButtonStatus {
    fn id(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Busy => "busy",
            Self::Done => "done",
            Self::Error => "error",
        }
    }
}

/// `commitTranslationKey`: the button's label under the `common` namespace.
pub fn commit_translation_key(mode: CommitMode, status: CommitButtonStatus) -> String {
    match mode {
        CommitMode::Merged => "commit.merged".to_string(),
        CommitMode::Closed => "commit.closed".to_string(),
        mode => format!("commit.modes.{}.{}", mode.id(), status.id()),
    }
}

fn has_changes(git: Option<&WorkspaceGitStatusOutput>) -> bool {
    git.is_some_and(|git| !git.staged.is_empty() || !git.unstaged.is_empty())
}

fn ahead(git: Option<&WorkspaceGitStatusOutput>) -> u32 {
    git.map_or(0, |git| git.ahead_of_remote_count)
}

/// `resolveCommitMode`. `conflict_resolution_ready`: every conflict of the
/// merge in progress has a marker-free result (`isMergeConflictResolutionReady`).
pub fn resolve_commit_mode(
    branch: &str,
    git: Option<&WorkspaceGitStatusOutput>,
    pr: Option<&WorkspacePrStatusOutput>,
    conflict_resolution_ready: bool,
) -> CommitMode {
    let pr_head = pr.and_then(|pr| pr.head_branch.as_deref());
    let belongs = pr_head.is_none_or(|head| head == branch);
    let pr_state = pr
        .filter(|_| belongs)
        .and_then(|pr| pr.state.as_deref())
        .map(str::to_lowercase);

    if let Some(git) = git.filter(|git| git.merge_in_progress) {
        return if git.conflict_count == 0 || conflict_resolution_ready {
            CommitMode::CompleteMerge
        } else {
            CommitMode::ResolveConflicts
        };
    }
    if git.is_some_and(|git| git.conflict_count > 0) {
        return CommitMode::ResolveConflicts;
    }
    match pr_state.as_deref() {
        Some("merged") => return CommitMode::Merged,
        Some("closed") => return CommitMode::OpenPr,
        Some("open") => {
            if has_changes(git) {
                return CommitMode::CommitAndPush;
            }
            if ahead(git) > 0 {
                return CommitMode::Push;
            }
            // The forge reports CONFLICTING until a local resolution is
            // pushed: local state above takes precedence.
            let pr = pr.expect("an open state comes from a pull request");
            if pr.mergeable.as_deref() == Some("CONFLICTING") {
                return CommitMode::ResolveConflicts;
            }
            let merge_state = pr.merge_state_status.as_deref().map(str::to_uppercase);
            // UNSTABLE also covers a check still running: only a failure
            // needs a fix, when the checks are known.
            let checks_running =
                merge_state.as_deref() == Some("UNSTABLE") && pr.checks_failing == Some(false);
            if matches!(
                merge_state.as_deref(),
                Some("DIRTY" | "BLOCKED" | "UNSTABLE")
            ) && !checks_running
            {
                return CommitMode::Fix;
            }
            return CommitMode::Merge;
        }
        _ => {}
    }
    if has_changes(git) {
        return CommitMode::CommitAndPush;
    }
    if ahead(git) > 0 {
        return CommitMode::Push;
    }
    let normalized = branch.to_lowercase();
    if normalized.contains("closed") {
        CommitMode::Closed
    } else if normalized.contains("merged") {
        CommitMode::Merged
    } else if normalized.contains("conflict") {
        CommitMode::ResolveConflicts
    } else if normalized.contains("merge") {
        CommitMode::Merge
    } else if normalized.contains("fix") {
        CommitMode::Fix
    } else if normalized.contains("pr/") {
        CommitMode::OpenPr
    } else if normalized.contains("push") {
        CommitMode::Push
    } else {
        CommitMode::CreatePr
    }
}

/// `shouldAutoCompleteMergedWorkspace`: a merged pull request completes the
/// task that has its own worktree, once.
pub fn should_auto_complete_merged(
    mode: CommitMode,
    workspace_state: &WorkspaceState,
    own_worktree: bool,
) -> bool {
    if matches!(
        workspace_state,
        WorkspaceState::Completed | WorkspaceState::Archived
    ) {
        return false;
    }
    mode == CommitMode::Merged && own_worktree
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::workspace_git::WorkspaceGitChangeEntry;

    fn entry() -> WorkspaceGitChangeEntry {
        WorkspaceGitChangeEntry {
            path: "a".into(),
            name: "a".into(),
            absolute_path: "/a".into(),
            status: "M".into(),
            insertions: 1,
            deletions: 0,
        }
    }

    fn git(
        staged: usize,
        unstaged: usize,
        ahead: u32,
        conflicts: u32,
        merging: bool,
    ) -> WorkspaceGitStatusOutput {
        WorkspaceGitStatusOutput {
            staged: (0..staged).map(|_| entry()).collect(),
            unstaged: (0..unstaged).map(|_| entry()).collect(),
            staged_fingerprint: String::new(),
            current_branch: Some("feature/statuslane".into()),
            ahead_of_remote_count: ahead,
            behind_of_remote_count: 0,
            conflict_count: conflicts,
            merge_in_progress: merging,
            has_remote: true,
        }
    }

    fn pr(
        state: &str,
        mergeable: Option<&str>,
        merge_state: Option<&str>,
    ) -> WorkspacePrStatusOutput {
        WorkspacePrStatusOutput {
            checks_failing: None,
            provider: Some("github".into()),
            host: None,
            number: Some(1),
            title: None,
            url: None,
            head_branch: None,
            base_branch: None,
            state: Some(state.into()),
            is_draft: false,
            mergeable: mergeable.map(Into::into),
            merge_state_status: merge_state.map(Into::into),
        }
    }

    const BRANCH: &str = "feature/statuslane";

    fn mode(
        git: Option<&WorkspaceGitStatusOutput>,
        pr: Option<&WorkspacePrStatusOutput>,
    ) -> CommitMode {
        resolve_commit_mode(BRANCH, git, pr, false)
    }

    #[test]
    fn branch_names_are_the_last_hint() {
        assert_eq!(
            resolve_commit_mode(BRANCH, None, None, false),
            CommitMode::CreatePr
        );
        for (branch, expected) in [
            ("fix/statuslane", CommitMode::Fix),
            ("release/push", CommitMode::Push),
            ("hotfix/merge", CommitMode::Merge),
            ("pr/statuslane", CommitMode::OpenPr),
        ] {
            assert_eq!(
                resolve_commit_mode(branch, None, None, false),
                expected,
                "{branch}"
            );
        }
    }

    #[test]
    fn the_pull_request_beats_the_branch_name() {
        let clean = git(0, 0, 0, 0, false);
        assert_eq!(
            mode(
                Some(&clean),
                Some(&pr("open", Some("MERGEABLE"), Some("CLEAN")))
            ),
            CommitMode::Merge
        );
        assert_eq!(
            mode(
                Some(&clean),
                Some(&pr("open", Some("CONFLICTING"), Some("BLOCKED")))
            ),
            CommitMode::ResolveConflicts
        );
        assert_eq!(
            mode(None, Some(&pr("merged", None, None))),
            CommitMode::Merged
        );
        assert_eq!(
            mode(None, Some(&pr("closed", None, None))),
            CommitMode::OpenPr
        );
    }

    #[test]
    fn local_state_comes_before_the_forge() {
        let conflicting = pr("open", Some("CONFLICTING"), Some("DIRTY"));
        assert_eq!(
            mode(Some(&git(1, 0, 0, 0, false)), Some(&conflicting)),
            CommitMode::CommitAndPush
        );
        assert_eq!(
            mode(Some(&git(0, 0, 1, 0, false)), Some(&conflicting)),
            CommitMode::Push
        );
        assert_eq!(
            mode(Some(&git(1, 1, 0, 1, false)), Some(&conflicting)),
            CommitMode::ResolveConflicts
        );
        let clean = pr("open", Some("MERGEABLE"), Some("CLEAN"));
        assert_eq!(
            mode(Some(&git(1, 0, 0, 0, false)), Some(&clean)),
            CommitMode::CommitAndPush
        );
        assert_eq!(
            mode(Some(&git(0, 0, 2, 0, false)), Some(&clean)),
            CommitMode::Push
        );
    }

    #[test]
    fn a_check_still_running_does_not_ask_for_a_fix() {
        let unstable = |failing| WorkspacePrStatusOutput {
            checks_failing: failing,
            ..pr("open", Some("MERGEABLE"), Some("UNSTABLE"))
        };
        let clean_git = git(0, 0, 0, 0, false);
        assert_eq!(
            mode(Some(&clean_git), Some(&unstable(Some(false)))),
            CommitMode::Merge
        );
        assert_eq!(
            mode(Some(&clean_git), Some(&unstable(Some(true)))),
            CommitMode::Fix
        );
        assert_eq!(
            mode(Some(&clean_git), Some(&unstable(None))),
            CommitMode::Fix
        );
    }

    #[test]
    fn a_merge_in_progress_completes_once_resolved() {
        let conflicting = pr("open", Some("CONFLICTING"), None);
        assert_eq!(
            mode(Some(&git(1, 0, 0, 0, true)), Some(&conflicting)),
            CommitMode::CompleteMerge
        );
        let unresolved = git(0, 1, 0, 3, true);
        assert_eq!(
            mode(Some(&unresolved), Some(&conflicting)),
            CommitMode::ResolveConflicts
        );
        // Marker-free agent edits still unmerged in the index.
        assert_eq!(
            resolve_commit_mode(BRANCH, Some(&unresolved), Some(&conflicting), true),
            CommitMode::CompleteMerge
        );
    }

    #[test]
    fn labels_and_auto_completion() {
        assert_eq!(
            commit_translation_key(CommitMode::CommitAndPush, CommitButtonStatus::Busy),
            "commit.modes.commit-and-push.busy"
        );
        assert_eq!(
            commit_translation_key(CommitMode::Merged, CommitButtonStatus::Idle),
            "commit.merged"
        );
        let ready = WorkspaceState::Ready;
        assert!(should_auto_complete_merged(
            CommitMode::Merged,
            &ready,
            true
        ));
        assert!(!should_auto_complete_merged(
            CommitMode::Merged,
            &ready,
            false
        ));
        assert!(!should_auto_complete_merged(
            CommitMode::Merged,
            &WorkspaceState::Completed,
            true
        ));
        assert!(!should_auto_complete_merged(
            CommitMode::Merge,
            &ready,
            true
        ));
    }
}

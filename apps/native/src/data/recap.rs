//! What a task row says about its git and pull request, one to one with the
//! desktop's `resolveCommitMode` (`WorkspaceCommitButton.logic.ts`), the part
//! of `buildWorkspaceRecap` the rail feeds (no review, pipeline or policy
//! inputs) and `buildWorkspaceRailMeta` / `railNextActionKey`.

use std::collections::HashSet;

use dcc_core::domain::workspace_git::{WorkspaceGitStatusOutput, WorkspacePrStatusOutput};

pub use dcc_core::domain::commit_mode::CommitMode;

/// `resolveCommitMode` as the rail reads it: without the conflict state
/// (the Rust status has no `conflictResolutionReady`).
pub fn resolve_commit_mode(
    branch: &str,
    git: Option<&WorkspaceGitStatusOutput>,
    pr: Option<&WorkspacePrStatusOutput>,
) -> CommitMode {
    dcc_core::domain::commit_mode::resolve_commit_mode(branch, git, pr, false)
}

/// `WorkspaceRecapTone`: graded by value, only `Attention` spends chroma.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecapTone {
    Neutral,
    Working,
    Attention,
    Ready,
    Done,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Recap {
    /// Under `inspector.recap.messages` / `sidebar.nextAction`.
    pub message_key: &'static str,
    pub count: i64,
    pub additions: i64,
    pub deletions: i64,
    /// "PR #12", or just "PR" without a number.
    pub pr: String,
    pub request_label: &'static str,
    pub tone: RecapTone,
}

pub struct RecapInput<'a> {
    pub branch: &'a str,
    /// An agent turn in flight (waiting on the person does not count).
    pub turn_running: bool,
    pub has_activity: bool,
    pub git: Option<&'a WorkspaceGitStatusOutput>,
    pub pr: Option<&'a WorkspacePrStatusOutput>,
    pub committed_vs_base: usize,
    pub pending_delegations: usize,
}

/// Distinct changed files and the uncommitted +/− of a status.
pub fn diff_totals(git: Option<&WorkspaceGitStatusOutput>) -> (usize, i64, i64) {
    let Some(git) = git else { return (0, 0, 0) };
    let entries = git.staged.iter().chain(&git.unstaged);
    let files: HashSet<&str> = entries.clone().map(|entry| entry.path.as_str()).collect();
    let additions = entries
        .clone()
        .map(|entry| i64::from(entry.insertions))
        .sum();
    let deletions = entries.map(|entry| i64::from(entry.deletions)).sum();
    (files.len(), additions, deletions)
}

fn request_label(pr: Option<&WorkspacePrStatusOutput>) -> &'static str {
    if pr.and_then(|pr| pr.provider.as_deref()) == Some("gitlab") {
        "MR"
    } else {
        "PR"
    }
}

/// `buildWorkspaceRailRecap`: `None` when there is nothing to say, including
/// the "all clean" of a brand-new idle task.
pub fn build_recap(input: &RecapInput) -> Option<Recap> {
    if !input.has_activity
        && input.git.is_none()
        && input.pr.is_none()
        && input.pending_delegations == 0
    {
        return None;
    }
    let branch = input
        .git
        .and_then(|git| git.current_branch.as_deref())
        .unwrap_or(input.branch);
    let mode = resolve_commit_mode(branch, input.git, input.pr);
    let (files, additions, deletions) = diff_totals(input.git);
    let conflicts = input.git.map_or(0, |git| i64::from(git.conflict_count));
    let label = request_label(input.pr);
    let number = input.pr.and_then(|pr| pr.number);
    let pr_state = input
        .pr
        .and_then(|pr| pr.state.as_deref())
        .map(str::to_lowercase);
    let recap = |message_key, count, tone| Recap {
        message_key,
        count,
        additions,
        deletions,
        pr: number.map_or_else(|| label.to_string(), |number| format!("{label} #{number}")),
        request_label: label,
        tone,
    };

    let recap = if mode == CommitMode::CompleteMerge {
        recap("conflictResolutionReady", conflicts, RecapTone::Ready)
    } else if mode == CommitMode::ResolveConflicts || conflicts > 0 {
        recap("conflicts", conflicts.max(1), RecapTone::Attention)
    } else if mode == CommitMode::Merged {
        recap("merged", 0, RecapTone::Done)
    } else if mode == CommitMode::Closed
        || (mode == CommitMode::OpenPr && pr_state.as_deref() == Some("closed"))
    {
        recap("closed", 0, RecapTone::Neutral)
    } else if input.turn_running {
        let key = if files > 0 { "working" } else { "workingClean" };
        recap(key, files as i64, RecapTone::Working)
    } else if input.pending_delegations > 0 {
        recap(
            "delegationResults",
            input.pending_delegations as i64,
            RecapTone::Attention,
        )
    } else {
        match mode {
            CommitMode::CommitAndPush => recap("changes", files as i64, RecapTone::Neutral),
            CommitMode::Push => recap(
                "ahead",
                i64::from(input.git.map_or(0, |git| git.ahead_of_remote_count)),
                RecapTone::Neutral,
            ),
            CommitMode::Fix => recap("checksFailing", 0, RecapTone::Attention),
            CommitMode::Merge => recap("mergeReady", 0, RecapTone::Ready),
            CommitMode::OpenPr => recap("prOpen", 0, RecapTone::Neutral),
            CommitMode::CreatePr if input.committed_vs_base > 0 => recap(
                "readyForPr",
                input.committed_vs_base as i64,
                RecapTone::Ready,
            ),
            _ => recap("clean", 0, RecapTone::Neutral),
        }
    };
    if !input.has_activity && recap.message_key == "clean" {
        return None;
    }
    Some(recap)
}

/// `railNextActionKey`: the row stays quiet where the state or meta line
/// already says it.
pub fn next_action_key(message_key: &str) -> Option<String> {
    (!matches!(message_key, "working" | "workingClean" | "prOpen" | "clean"))
        .then(|| format!("sidebar.nextAction.{message_key}"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrState {
    Open,
    Draft,
    Merged,
    Closed,
}

/// `WorkspaceRailMeta`: the PR number and the uncommitted diff.
#[derive(Clone, Debug, PartialEq)]
pub struct RailMeta {
    pub pr_number: Option<u32>,
    pub pr_label: &'static str,
    pub pr_state: Option<PrState>,
    pub additions: i64,
    pub deletions: i64,
}

/// `buildWorkspaceRailMeta`: only a PR opened from this exact branch belongs
/// to the task (a fresh worktree may sit on a commit an older PR points at).
pub fn build_meta(
    branch: &str,
    git: Option<&WorkspaceGitStatusOutput>,
    pr: Option<&WorkspacePrStatusOutput>,
) -> Option<RailMeta> {
    let (_, additions, deletions) = diff_totals(git);
    let head = pr
        .and_then(|pr| pr.head_branch.as_deref())
        .map(str::trim)
        .unwrap_or("");
    let owns =
        pr.is_some_and(|pr| pr.number.is_some()) && (head.is_empty() || head == branch.trim());
    let number = if owns {
        pr.and_then(|pr| pr.number)
    } else {
        None
    };
    if number.is_none() && additions == 0 && deletions == 0 {
        return None;
    }
    let state =
        number.and(pr).and_then(
            |pr| match pr.state.as_deref().map(str::to_lowercase).as_deref() {
                Some("merged") => Some(PrState::Merged),
                Some("closed") => Some(PrState::Closed),
                _ if pr.is_draft => Some(PrState::Draft),
                Some("open" | "opened") => Some(PrState::Open),
                _ => None,
            },
        );
    Some(RailMeta {
        pr_number: number,
        pr_label: request_label(pr),
        pr_state: state,
        additions,
        deletions,
    })
}

#[cfg(test)]
mod tests;

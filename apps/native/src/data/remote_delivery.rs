//! Where a task on another machine is on its way to production, and the
//! one step that comes next. The same rules as a local task's: the machine's
//! summary and its pull request go through `resolve_commit_mode` and the
//! delivery rail; nothing is decided twice.

use dcc_core::domain::commit_mode::{CommitMode, checks_running, resolve_commit_mode};
use dcc_core::domain::workspace_git::{
    WorkspaceGitChangeEntry, WorkspaceGitStatusOutput, WorkspacePrStatusOutput,
};

use super::delivery_rail::{Rail, RailInput, rail};
use super::machines::{RemoteChange, RemoteChanges};

/// The step the column's button runs on the machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemoteStep {
    CommitAndPush,
    Push,
    CreateRequest,
    Merge,
}

impl RemoteStep {
    /// `codeColumn.remote.steps.*` and the delivery action on the machine.
    pub fn key(self) -> &'static str {
        match self {
            RemoteStep::CommitAndPush => "commit",
            RemoteStep::Push => "push",
            RemoteStep::CreateRequest => "request",
            RemoteStep::Merge => "merge",
        }
    }
}

fn entry(change: &RemoteChange) -> WorkspaceGitChangeEntry {
    WorkspaceGitChangeEntry {
        path: change.path.clone(),
        name: change
            .path
            .rsplit('/')
            .next()
            .unwrap_or_default()
            .to_string(),
        absolute_path: String::new(),
        status: change.status.clone(),
        insertions: change.insertions,
        deletions: change.deletions,
    }
}

/// The machine's summary as the git status the rules read.
fn status(changes: &RemoteChanges) -> WorkspaceGitStatusOutput {
    let (staged, unstaged): (Vec<&RemoteChange>, Vec<&RemoteChange>) =
        changes.changes.iter().partition(|change| change.staged);
    WorkspaceGitStatusOutput {
        staged: staged.into_iter().map(entry).collect(),
        unstaged: unstaged.into_iter().map(entry).collect(),
        staged_fingerprint: String::new(),
        current_branch: changes.branch.clone(),
        ahead_of_remote_count: changes.ahead_of_remote_count,
        behind_of_remote_count: changes.behind_of_remote_count,
        conflict_count: changes.conflict_count,
        merge_in_progress: changes.merge_in_progress,
        has_remote: true,
    }
}

pub fn mode(changes: &RemoteChanges, pr: Option<&WorkspacePrStatusOutput>) -> CommitMode {
    let branch = changes.branch.clone().unwrap_or_default();
    resolve_commit_mode(&branch, Some(&status(changes)), pr, false)
}

pub fn rail_of(changes: &RemoteChanges, pr: Option<&WorkspacePrStatusOutput>) -> Rail {
    rail(
        mode(changes, pr),
        RailInput {
            changes: changes.changes.len(),
            ahead: changes.ahead_of_remote_count,
            branch_files: Some(changes.branch_changes.len()),
            pr,
            review: None,
        },
    )
}

/// The step to offer; none when the next move is not one of these (a
/// conflict, failing checks, a merged or closed pull request, checks still
/// running, or nothing on the branch yet).
pub fn next_step(
    changes: &RemoteChanges,
    pr: Option<&WorkspacePrStatusOutput>,
) -> Option<RemoteStep> {
    match mode(changes, pr) {
        CommitMode::CommitAndPush => Some(RemoteStep::CommitAndPush),
        CommitMode::Push => Some(RemoteStep::Push),
        CommitMode::CreatePr if !changes.branch_changes.is_empty() => {
            Some(RemoteStep::CreateRequest)
        }
        CommitMode::Merge if !pr.is_some_and(checks_running) => Some(RemoteStep::Merge),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::delivery_rail::Standing;

    fn change(path: &str) -> RemoteChange {
        RemoteChange {
            path: path.into(),
            status: "M".into(),
            insertions: 1,
            deletions: 0,
            staged: false,
        }
    }

    fn pr(merge_state: &str, failing: Option<bool>) -> WorkspacePrStatusOutput {
        WorkspacePrStatusOutput {
            provider: Some("github".into()),
            host: None,
            number: Some(7),
            title: None,
            url: None,
            head_branch: None,
            base_branch: None,
            state: Some("OPEN".into()),
            is_draft: false,
            mergeable: Some("MERGEABLE".into()),
            merge_state_status: Some(merge_state.into()),
            checks_failing: failing,
        }
    }

    #[test]
    fn a_remote_task_walks_the_same_steps_as_a_local_one() {
        let branch = Some("dcc/checkout".to_string());
        let dirty = RemoteChanges {
            branch: branch.clone(),
            changes: vec![change("a.ts")],
            ..RemoteChanges::default()
        };
        assert_eq!(next_step(&dirty, None), Some(RemoteStep::CommitAndPush));
        assert_eq!(rail_of(&dirty, None).standing, Standing::ToCommit);

        let unpushed = RemoteChanges {
            branch: branch.clone(),
            ahead_of_remote_count: 2,
            branch_changes: vec![change("a.ts")],
            ..RemoteChanges::default()
        };
        assert_eq!(next_step(&unpushed, None), Some(RemoteStep::Push));

        let pushed = RemoteChanges {
            branch: branch.clone(),
            branch_changes: vec![change("a.ts")],
            ..RemoteChanges::default()
        };
        assert_eq!(next_step(&pushed, None), Some(RemoteStep::CreateRequest));
        // Nothing on the branch: no pull request to offer.
        let empty = RemoteChanges {
            branch: branch.clone(),
            ..RemoteChanges::default()
        };
        assert_eq!(next_step(&empty, None), None);

        let clean = pr("CLEAN", Some(false));
        assert_eq!(next_step(&pushed, Some(&clean)), Some(RemoteStep::Merge));
        assert_eq!(
            rail_of(&pushed, Some(&clean)).standing,
            Standing::ReadyToMerge
        );
        // Checks still running, or failing: no merge button.
        let running = pr("UNSTABLE", Some(false));
        assert_eq!(next_step(&pushed, Some(&running)), None);
        let failing = pr("UNSTABLE", Some(true));
        assert_eq!(next_step(&pushed, Some(&failing)), None);
        assert_eq!(
            rail_of(&pushed, Some(&failing)).standing,
            Standing::ChecksFailed
        );
    }
}

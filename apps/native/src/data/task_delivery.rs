//! Where a task stands in its delivery, as the room's change card says it
//! ("sem commit", "PR #12 aberto") and the step the card offers, taken
//! from the same rules the task's dock follows (`resolve_commit_mode`,
//! `resolve_dock_actions`) over the status the rail already reads. Nothing
//! here runs a step: the card only opens the task's own flow.

use dcc_core::domain::execution_dock::{
    DockAction, DockActionContext, DockActionKind, DockChangeSummary, DockRunMode,
    resolve_dock_actions,
};
use dcc_core::domain::workspace_git::{WorkspaceGitStatusOutput, WorkspacePrStatusOutput};

use super::recap::{CommitMode, resolve_commit_mode};
use super::task_changes::ChangedFile;

/// What the card says, in a few words.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeliveryState {
    Nothing,
    NotCommitted,
    NotPushed,
    /// Committed without a remote to push to.
    CommittedLocal,
    /// On the remote, no pull request yet.
    NoRequest,
    RequestOpen(u32),
    RequestMerged(u32),
    RequestClosed(u32),
}

impl DeliveryState {
    pub fn i18n_key(&self) -> &'static str {
        match self {
            Self::Nothing => "agents.report.delivery.nothing",
            Self::NotCommitted => "agents.report.delivery.notCommitted",
            Self::NotPushed => "agents.report.delivery.notPushed",
            Self::CommittedLocal => "agents.report.delivery.local",
            Self::NoRequest => "agents.report.delivery.noRequest",
            Self::RequestOpen(_) => "agents.report.delivery.requestOpen",
            Self::RequestMerged(_) => "agents.report.delivery.requestMerged",
            Self::RequestClosed(_) => "agents.report.delivery.requestClosed",
        }
    }

    pub fn number(&self) -> Option<u32> {
        match self {
            Self::RequestOpen(number)
            | Self::RequestMerged(number)
            | Self::RequestClosed(number) => Some(*number),
            _ => None,
        }
    }
}

/// The step the card's button opens in the task. Commit and the pull
/// request open the dock's dialogs; a push only takes the person there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeliveryStep {
    Commit { push: bool },
    CreateRequest,
    Push,
}

/// The dock's step as the card's: only the ones the room offers.
pub fn step_of(action: &DockAction) -> Option<DeliveryStep> {
    if action.disabled {
        return None;
    }
    match (action.kind, action.mode) {
        (DockActionKind::CreateRequest, _) => Some(DeliveryStep::CreateRequest),
        (DockActionKind::Execute, Some(DockRunMode::Commit)) => {
            Some(DeliveryStep::Commit { push: false })
        }
        (DockActionKind::Execute, Some(DockRunMode::Mode(CommitMode::CommitAndPush))) => {
            Some(DeliveryStep::Commit { push: true })
        }
        (DockActionKind::Execute, Some(DockRunMode::Mode(CommitMode::Push))) => {
            Some(DeliveryStep::Push)
        }
        // Merge stays in Pull Requests; the rest needs the task's screen.
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequestLink {
    pub number: u32,
    pub url: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CardDelivery {
    pub state: DeliveryState,
    /// The dock's next step, labelled as the dock labels it.
    pub step: Option<DockAction>,
    pub request: Option<RequestLink>,
    /// "PR", or "MR" on GitLab.
    pub request_label: &'static str,
}

pub struct DeliveryInput<'a> {
    /// The task's branch, when git has not said it.
    pub branch: &'a str,
    pub git: &'a WorkspaceGitStatusOutput,
    pub pr: Option<&'a WorkspacePrStatusOutput>,
    /// The branch's commits against its base, once read.
    pub committed: Option<&'a [ChangedFile]>,
}

fn request_label(pr: Option<&WorkspacePrStatusOutput>) -> &'static str {
    if pr.and_then(|pr| pr.provider.as_deref()) == Some("gitlab") {
        "MR"
    } else {
        "PR"
    }
}

/// `dock_summary` over the rail's status and the card's read.
fn summary(input: &DeliveryInput) -> DockChangeSummary {
    let (files, additions, deletions) = super::recap::diff_totals(Some(input.git));
    let committed = input.committed.unwrap_or_default();
    DockChangeSummary {
        files: files as u32,
        additions: additions as u32,
        deletions: deletions as u32,
        branch_files: committed.len() as u32,
        branch_additions: committed.iter().map(|file| file.additions as u32).sum(),
        branch_deletions: committed.iter().map(|file| file.deletions as u32).sum(),
        ahead_of_remote_count: input.git.ahead_of_remote_count,
        behind_of_remote_count: input.git.behind_of_remote_count,
        has_remote: Some(input.git.has_remote),
        pull_request_state: input.pr.and_then(|pr| pr.state.clone()),
        pull_request_number: input.pr.and_then(|pr| pr.number),
    }
}

pub fn card_delivery(input: &DeliveryInput) -> CardDelivery {
    let branch = input.git.current_branch.as_deref().unwrap_or(input.branch);
    let summary = summary(input);
    // Only a pull request opened from this branch is the task's.
    let owned = input.pr.filter(|pr| {
        pr.number.is_some()
            && pr
                .head_branch
                .as_deref()
                .map(str::trim)
                .is_none_or(|head| head.is_empty() || head == branch.trim())
    });
    let number = owned.and_then(|pr| pr.number);
    let pr_state = owned
        .and_then(|pr| pr.state.as_deref())
        .map(str::to_lowercase);
    let state = if summary.files > 0 {
        DeliveryState::NotCommitted
    } else if summary.ahead_of_remote_count > 0 && input.git.has_remote {
        DeliveryState::NotPushed
    } else {
        match (pr_state.as_deref(), number) {
            (Some("merged"), Some(number)) => DeliveryState::RequestMerged(number),
            (Some("closed"), Some(number)) => DeliveryState::RequestClosed(number),
            (Some(_), Some(number)) => DeliveryState::RequestOpen(number),
            _ if summary.branch_files > 0 && !input.git.has_remote => DeliveryState::CommittedLocal,
            _ if summary.branch_files > 0 => DeliveryState::NoRequest,
            _ => DeliveryState::Nothing,
        }
    };
    let mode = resolve_commit_mode(branch, Some(input.git), input.pr);
    // Until the branch is read the dock waits too.
    let loading = input.committed.is_none();
    let context = DockActionContext::from_summary(Some(mode), loading, false, Some(&summary));
    let step = resolve_dock_actions(&context)
        .into_iter()
        .next()
        .filter(|action| step_of(action).is_some());
    let request = owned.and_then(|pr| {
        Some(RequestLink {
            number: pr.number?,
            url: pr.url.clone().filter(|url| !url.is_empty())?,
        })
    });
    CardDelivery {
        state,
        step,
        request,
        request_label: request_label(input.pr),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dcc_core::domain::execution_dock::DockActionId;
    use dcc_core::domain::workspace_git::WorkspaceGitChangeEntry;

    fn git(changed: bool, ahead: u32, remote: bool) -> WorkspaceGitStatusOutput {
        WorkspaceGitStatusOutput {
            staged: vec![],
            unstaged: changed
                .then(|| WorkspaceGitChangeEntry {
                    path: "src/a.rs".into(),
                    name: "a.rs".into(),
                    absolute_path: "/w/src/a.rs".into(),
                    status: "M".into(),
                    insertions: 3,
                    deletions: 1,
                })
                .into_iter()
                .collect(),
            staged_fingerprint: String::new(),
            current_branch: Some("dcc/task".into()),
            ahead_of_remote_count: ahead,
            behind_of_remote_count: 0,
            conflict_count: 0,
            merge_in_progress: false,
            has_remote: remote,
        }
    }

    fn pr(state: &str) -> WorkspacePrStatusOutput {
        WorkspacePrStatusOutput {
            checks_failing: None,
            provider: Some("github".into()),
            host: None,
            number: Some(1032),
            title: None,
            url: Some("https://github.com/acme/app/pull/1032".into()),
            head_branch: Some("dcc/task".into()),
            base_branch: Some("main".into()),
            state: Some(state.into()),
            is_draft: false,
            mergeable: Some("MERGEABLE".into()),
            merge_state_status: Some("CLEAN".into()),
        }
    }

    fn committed() -> Vec<ChangedFile> {
        vec![ChangedFile {
            path: "src/a.rs".into(),
            additions: 10,
            deletions: 2,
        }]
    }

    fn card(
        git: &WorkspaceGitStatusOutput,
        pr: Option<&WorkspacePrStatusOutput>,
        committed: Option<&[ChangedFile]>,
    ) -> CardDelivery {
        card_delivery(&DeliveryInput {
            branch: "dcc/task",
            git,
            pr,
            committed,
        })
    }

    #[test]
    fn the_state_and_the_step_follow_the_dock() {
        let done = committed();
        let changed = git(true, 0, true);
        let open = card(&changed, None, Some(&done));
        assert_eq!(open.state, DeliveryState::NotCommitted);
        assert_eq!(
            open.step.as_ref().and_then(step_of),
            Some(DeliveryStep::Commit { push: true })
        );

        let ahead = git(false, 2, true);
        let pushing = card(&ahead, None, Some(&done));
        assert_eq!(pushing.state, DeliveryState::NotPushed);
        assert_eq!(
            pushing.step.as_ref().and_then(step_of),
            Some(DeliveryStep::Push)
        );

        let clean = git(false, 0, true);
        let none = Some(WorkspacePrStatusOutput {
            number: None,
            state: None,
            url: None,
            ..pr("open")
        });
        let ready = card(&clean, none.as_ref(), Some(&done));
        assert_eq!(ready.state, DeliveryState::NoRequest);
        assert_eq!(
            ready.step.as_ref().and_then(step_of),
            Some(DeliveryStep::CreateRequest)
        );
        assert_eq!(ready.request, None);

        let merged_pr = pr("MERGED");
        let merged = card(&clean, Some(&merged_pr), Some(&done));
        assert_eq!(merged.state, DeliveryState::RequestMerged(1032));
        assert_eq!(merged.step, None);

        let local = git(true, 0, false);
        let commit_only = card(&local, None, Some(&[][..]));
        assert_eq!(
            commit_only.step.as_ref().and_then(step_of),
            Some(DeliveryStep::Commit { push: false })
        );
        assert_eq!(
            card(&git(false, 0, false), None, Some(&done)).state,
            DeliveryState::CommittedLocal
        );
        assert_eq!(
            card(&clean, None, Some(&[][..])).state,
            DeliveryState::Nothing
        );
    }

    #[test]
    fn an_open_request_is_linked_and_never_merged_from_the_room() {
        let clean = git(false, 0, true);
        let open_pr = pr("OPEN");
        let done = committed();
        let open = card(&clean, Some(&open_pr), Some(&done));
        assert_eq!(open.state, DeliveryState::RequestOpen(1032));
        assert_eq!(open.step, None, "the dock's next step is the merge");
        assert_eq!(
            open.request,
            Some(RequestLink {
                number: 1032,
                url: "https://github.com/acme/app/pull/1032".into()
            })
        );
        // New work on an open pull request: commit and push, and the link.
        let changed = git(true, 0, true);
        let more = card(&changed, Some(&open_pr), Some(&done));
        assert_eq!(more.state, DeliveryState::NotCommitted);
        assert_eq!(
            more.step.as_ref().map(|action| action.id),
            Some(DockActionId::Primary)
        );
        assert!(more.request.is_some());
        // Another branch's pull request is not the task's.
        let other = WorkspacePrStatusOutput {
            head_branch: Some("someone/else".into()),
            ..pr("OPEN")
        };
        assert_eq!(card(&clean, Some(&other), Some(&done)).request, None);
    }

    #[test]
    fn nothing_is_offered_until_the_branch_is_read() {
        let changed = git(true, 0, true);
        let reading = card(&changed, None, None);
        assert_eq!(reading.state, DeliveryState::NotCommitted);
        assert_eq!(reading.step, None);
    }
}

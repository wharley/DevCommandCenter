//! The delivery rail's steps: where a task's work is on its way to
//! production — commit, push, pull request, checks, review, merge. Each
//! step is settled from git and the forge; the first one not done is the
//! current one. The next action stays `commit_mode`'s: this only says where
//! things are.

use dcc_core::domain::commit_mode::{CommitMode, checks_running};
use dcc_core::domain::workspace_git::WorkspacePrStatusOutput;

use crate::data::forge::ReviewState;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Step {
    Commit,
    Push,
    Request,
    Checks,
    Review,
    Merge,
}

impl Step {
    pub const ALL: [Step; 6] = [
        Step::Commit,
        Step::Push,
        Step::Request,
        Step::Checks,
        Step::Review,
        Step::Merge,
    ];

    /// The i18n key's last segment.
    pub fn key(self) -> &'static str {
        match self {
            Step::Commit => "commit",
            Step::Push => "push",
            Step::Request => "request",
            Step::Checks => "checks",
            Step::Review => "review",
            Step::Merge => "merge",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepState {
    Done,
    /// Where things are now.
    Current,
    /// Stopped here until someone acts.
    Failed,
    Todo,
}

/// What the line under the rail says (`commit.rail.status.*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Standing {
    /// Nothing on the branch yet.
    Idle,
    ToCommit,
    ToPush,
    ToRequest,
    Draft,
    ChecksRunning,
    ChecksFailed,
    ChangesRequested,
    AwaitingReview,
    Conflicts,
    Blocked,
    ReadyToMerge,
    Merged,
    Closed,
}

impl Standing {
    pub fn key(self) -> &'static str {
        match self {
            Standing::Idle => "idle",
            Standing::ToCommit => "toCommit",
            Standing::ToPush => "toPush",
            Standing::ToRequest => "toRequest",
            Standing::Draft => "draft",
            Standing::ChecksRunning => "checksRunning",
            Standing::ChecksFailed => "checksFailed",
            Standing::ChangesRequested => "changesRequested",
            Standing::AwaitingReview => "awaitingReview",
            Standing::Conflicts => "conflicts",
            Standing::Blocked => "blocked",
            Standing::ReadyToMerge => "readyToMerge",
            Standing::Merged => "merged",
            Standing::Closed => "closed",
        }
    }
}

/// What the rail is settled from.
#[derive(Clone, Copy, Default)]
pub struct RailInput<'a> {
    /// Files changed in the checkout.
    pub changes: usize,
    /// Commits not yet on the remote.
    pub ahead: u32,
    /// Files the branch changes against its base, once read.
    pub branch_files: Option<usize>,
    pub pr: Option<&'a WorkspacePrStatusOutput>,
    pub review: Option<&'a ReviewState>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rail {
    pub steps: [(Step, StepState); 6],
    pub standing: Standing,
    /// Approvals received and required, when the forge says both.
    pub approvals: Option<(u32, u32)>,
}

fn upper(text: Option<&str>) -> Option<String> {
    text.map(str::to_uppercase)
}

/// A step on its own: done, failed, or still to happen.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Own {
    Done,
    Failed,
    Open,
}

pub fn rail(mode: CommitMode, input: RailInput) -> Rail {
    let pr_state = input
        .pr
        .and_then(|pr| pr.state.as_deref())
        .map(str::to_lowercase);
    let approvals = input.review.and_then(|review| {
        review
            .approvals_required
            .filter(|required| *required > 0 && review.approvals_available)
            .map(|required| (review.approvals_received, required))
    });
    let all = |state: StepState| Step::ALL.map(|step| (step, state));
    if mode == CommitMode::Merged || pr_state.as_deref() == Some("merged") {
        return Rail {
            steps: all(StepState::Done),
            standing: Standing::Merged,
            approvals,
        };
    }
    let open = input.pr.filter(|_| pr_state.as_deref() == Some("open"));
    let closed = pr_state.as_deref() == Some("closed") || mode == CommitMode::Closed;
    let local = input.changes > 0;
    let unpushed = input.ahead > 0;
    let on_branch = input.branch_files.is_some_and(|files| files > 0);

    let commit = if local {
        Own::Open
    } else if unpushed || on_branch || input.pr.is_some() {
        Own::Done
    } else {
        Own::Open
    };
    let push = if local || unpushed {
        Own::Open
    } else if on_branch || input.pr.is_some() {
        Own::Done
    } else {
        Own::Open
    };
    let request = match (open, closed) {
        (Some(_), _) => Own::Done,
        (None, true) => Own::Failed,
        (None, false) => Own::Open,
    };
    let merge_state = open.and_then(|pr| upper(pr.merge_state_status.as_deref()));
    let running = open.is_some_and(checks_running);
    let checks = match open {
        None => Own::Open,
        Some(pr) if pr.checks_failing == Some(true) => Own::Failed,
        Some(_) if running => Own::Open,
        Some(pr) => match merge_state.as_deref() {
            Some("CLEAN" | "HAS_HOOKS") => Own::Done,
            // The forge named the checks and none failed.
            _ if pr.checks_failing == Some(false) => Own::Done,
            _ => Own::Open,
        },
    };
    let review_state = input
        .review
        .and_then(|review| upper(review.review_state.as_deref()));
    let changes_requested = review_state.as_deref() == Some("CHANGES_REQUESTED");
    let approved = review_state.as_deref() == Some("APPROVED")
        || approvals.is_some_and(|(received, required)| received >= required)
        || input
            .review
            .is_some_and(|review| review.approvals_left == Some(0));
    let review = match open {
        None => Own::Open,
        Some(_) if changes_requested => Own::Failed,
        Some(_) if approved => Own::Done,
        // Nothing asked for and the forge lets it through.
        Some(_) if merge_state.as_deref() == Some("CLEAN") => Own::Done,
        Some(_) => Own::Open,
    };
    let conflicts = mode == CommitMode::ResolveConflicts
        || mode == CommitMode::CompleteMerge
        || open.is_some_and(|pr| upper(pr.mergeable.as_deref()).as_deref() == Some("CONFLICTING"));
    let merge = if conflicts { Own::Failed } else { Own::Open };

    let own = [commit, push, request, checks, review, merge];
    let mut current_taken = false;
    let steps = std::array::from_fn(|index| {
        let state = match own[index] {
            Own::Done => StepState::Done,
            Own::Failed => StepState::Failed,
            Own::Open if !current_taken => {
                current_taken = true;
                StepState::Current
            }
            Own::Open => StepState::Todo,
        };
        (Step::ALL[index], state)
    });

    let draft = open.is_some_and(|pr| pr.is_draft);
    let standing = if conflicts {
        Standing::Conflicts
    } else if closed && open.is_none() {
        Standing::Closed
    } else if local {
        Standing::ToCommit
    } else if unpushed {
        Standing::ToPush
    } else if open.is_none() {
        if on_branch {
            Standing::ToRequest
        } else {
            Standing::Idle
        }
    } else if checks == Own::Failed {
        Standing::ChecksFailed
    } else if running {
        Standing::ChecksRunning
    } else if changes_requested {
        Standing::ChangesRequested
    } else if draft {
        Standing::Draft
    } else if review == Own::Open {
        Standing::AwaitingReview
    } else if checks == Own::Open || mode == CommitMode::Fix {
        Standing::Blocked
    } else {
        Standing::ReadyToMerge
    };
    Rail {
        steps,
        standing,
        approvals,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pr(state: &str, merge_state: &str, failing: Option<bool>) -> WorkspacePrStatusOutput {
        WorkspacePrStatusOutput {
            provider: Some("github".into()),
            host: None,
            number: Some(12),
            title: None,
            url: None,
            head_branch: None,
            base_branch: None,
            state: Some(state.into()),
            is_draft: false,
            mergeable: Some("MERGEABLE".into()),
            merge_state_status: Some(merge_state.into()),
            checks_failing: failing,
        }
    }

    fn states(rail: &Rail) -> Vec<StepState> {
        rail.steps.iter().map(|(_, state)| *state).collect()
    }

    use StepState::{Current, Done, Failed, Todo};

    #[test]
    fn local_work_walks_from_commit_to_the_pull_request() {
        let nothing = rail(CommitMode::CreatePr, RailInput::default());
        assert_eq!(nothing.standing, Standing::Idle);
        assert_eq!(states(&nothing)[0], Current);

        let dirty = rail(
            CommitMode::CommitAndPush,
            RailInput {
                changes: 3,
                ..Default::default()
            },
        );
        assert_eq!(dirty.standing, Standing::ToCommit);
        assert_eq!(states(&dirty), [Current, Todo, Todo, Todo, Todo, Todo]);

        let unpushed = rail(
            CommitMode::Push,
            RailInput {
                ahead: 2,
                ..Default::default()
            },
        );
        assert_eq!(unpushed.standing, Standing::ToPush);
        assert_eq!(states(&unpushed), [Done, Current, Todo, Todo, Todo, Todo]);

        let pushed = rail(
            CommitMode::CreatePr,
            RailInput {
                branch_files: Some(4),
                ..Default::default()
            },
        );
        assert_eq!(pushed.standing, Standing::ToRequest);
        assert_eq!(states(&pushed), [Done, Done, Current, Todo, Todo, Todo]);
    }

    #[test]
    fn an_open_pull_request_stops_at_its_checks_then_its_review() {
        let running = pr("OPEN", "UNSTABLE", Some(false));
        let rail_running = rail(
            CommitMode::Merge,
            RailInput {
                pr: Some(&running),
                ..Default::default()
            },
        );
        assert_eq!(rail_running.standing, Standing::ChecksRunning);
        assert_eq!(
            states(&rail_running),
            [Done, Done, Done, Current, Todo, Todo]
        );

        let failing = pr("OPEN", "UNSTABLE", Some(true));
        let rail_failing = rail(
            CommitMode::Fix,
            RailInput {
                pr: Some(&failing),
                ..Default::default()
            },
        );
        assert_eq!(rail_failing.standing, Standing::ChecksFailed);
        assert_eq!(states(&rail_failing)[3], Failed);

        let blocked = pr("OPEN", "BLOCKED", Some(false));
        let review = ReviewState {
            approvals_available: true,
            approvals_required: Some(2),
            approvals_received: 1,
            approvals_left: Some(1),
            ..Default::default()
        };
        let waiting = rail(
            CommitMode::Fix,
            RailInput {
                pr: Some(&blocked),
                review: Some(&review),
                ..Default::default()
            },
        );
        assert_eq!(waiting.standing, Standing::AwaitingReview);
        assert_eq!(waiting.approvals, Some((1, 2)));
        assert_eq!(states(&waiting), [Done, Done, Done, Done, Current, Todo]);
    }

    #[test]
    fn a_clean_pull_request_is_ready_and_a_merged_one_is_all_done() {
        let clean = pr("OPEN", "CLEAN", Some(false));
        let ready = rail(
            CommitMode::Merge,
            RailInput {
                pr: Some(&clean),
                ..Default::default()
            },
        );
        assert_eq!(ready.standing, Standing::ReadyToMerge);
        assert_eq!(states(&ready), [Done, Done, Done, Done, Done, Current]);

        let merged = pr("MERGED", "CLEAN", None);
        let done = rail(
            CommitMode::Merged,
            RailInput {
                pr: Some(&merged),
                ..Default::default()
            },
        );
        assert_eq!(done.standing, Standing::Merged);
        assert!(states(&done).iter().all(|state| *state == Done));
    }

    #[test]
    fn conflicts_and_new_local_work_show_on_an_open_pull_request() {
        let mut conflicting = pr("OPEN", "DIRTY", None);
        conflicting.mergeable = Some("CONFLICTING".into());
        let stopped = rail(
            CommitMode::ResolveConflicts,
            RailInput {
                pr: Some(&conflicting),
                ..Default::default()
            },
        );
        assert_eq!(stopped.standing, Standing::Conflicts);
        assert_eq!(states(&stopped)[5], Failed);

        let clean = pr("OPEN", "CLEAN", Some(false));
        let more = rail(
            CommitMode::CommitAndPush,
            RailInput {
                changes: 1,
                pr: Some(&clean),
                ..Default::default()
            },
        );
        assert_eq!(more.standing, Standing::ToCommit);
        assert_eq!(states(&more)[..3], [Current, Todo, Done]);
    }
}

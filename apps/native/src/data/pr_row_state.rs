//! What a pull request's row says at a glance, in one icon: the thing most
//! in its way to production — a draft, failing checks, checks running, the
//! person's own review waiting on the author — else that it is approved or
//! its checks passed.

use crate::data::pr_hub::Item;
use crate::theme::StateTone;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowState {
    Draft,
    ChecksFailed,
    ChecksRunning,
    ChangesRequested,
    Commented,
    Approved,
    ChecksPassed,
    Open,
}

impl RowState {
    /// `pullRequests.rowState.*`
    pub fn key(self) -> &'static str {
        match self {
            RowState::Draft => "draft",
            RowState::ChecksFailed => "checksFailed",
            RowState::ChecksRunning => "checksRunning",
            RowState::ChangesRequested => "changesRequested",
            RowState::Commented => "commented",
            RowState::Approved => "approved",
            RowState::ChecksPassed => "checksPassed",
            RowState::Open => "open",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            RowState::Draft => "git-pull-request",
            RowState::ChecksFailed => "circle-alert",
            RowState::ChecksRunning => "clock-3",
            RowState::ChangesRequested | RowState::Commented => "message-circle",
            RowState::Approved | RowState::ChecksPassed => "circle-check",
            RowState::Open => "git-pull-request",
        }
    }

    pub fn tone(self) -> StateTone {
        match self {
            RowState::Draft | RowState::Open => StateTone::Quiet,
            RowState::ChecksFailed => StateTone::Failed,
            RowState::ChecksRunning => StateTone::NeedsYou,
            RowState::ChangesRequested | RowState::Commented => StateTone::Waiting,
            RowState::Approved | RowState::ChecksPassed => StateTone::Done,
        }
    }
}

/// `my_review`: the person's latest review of it (GitHub's state), if any.
pub fn row_state(item: &Item, my_review: Option<&str>) -> RowState {
    if item.is_draft {
        return RowState::Draft;
    }
    match item.checks_state.as_str() {
        "failure" => return RowState::ChecksFailed,
        "pending" => return RowState::ChecksRunning,
        _ => {}
    }
    match my_review {
        Some("CHANGES_REQUESTED") => return RowState::ChangesRequested,
        Some("COMMENTED") => return RowState::Commented,
        Some("APPROVED") => return RowState::Approved,
        _ => {}
    }
    if item.review_decision.as_deref() == Some("approved") {
        RowState::Approved
    } else if item.checks_state == "success" {
        RowState::ChecksPassed
    } else {
        RowState::Open
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(checks: &str, draft: bool) -> Item {
        Item {
            checks_state: checks.into(),
            is_draft: draft,
            ..Item::default()
        }
    }

    #[test]
    fn what_is_in_the_way_comes_before_what_is_done() {
        assert_eq!(row_state(&item("failure", true), None), RowState::Draft);
        assert_eq!(
            row_state(&item("failure", false), Some("APPROVED")),
            RowState::ChecksFailed
        );
        assert_eq!(
            row_state(&item("pending", false), Some("APPROVED")),
            RowState::ChecksRunning
        );
        assert_eq!(
            row_state(&item("success", false), Some("CHANGES_REQUESTED")),
            RowState::ChangesRequested
        );
        assert_eq!(
            row_state(&item("success", false), Some("APPROVED")),
            RowState::Approved
        );
        assert_eq!(
            row_state(&item("success", false), None),
            RowState::ChecksPassed
        );
        assert_eq!(row_state(&item("", false), None), RowState::Open);
    }
}

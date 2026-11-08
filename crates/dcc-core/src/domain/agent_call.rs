//! Calling a resident agent in the open task (`review-offer.ts`,
//! `session-agent-controls.tsx`): which agents have a button, whether the
//! task has anything to review, why a call waits.

use super::workspace_git::WorkspaceGitStatusOutput;

/// `hasReviewableChanges`: local changes or commits not pushed yet;
/// `None` while the status is still read.
pub fn has_reviewable_changes(status: Option<&WorkspaceGitStatusOutput>) -> Option<bool> {
    status.map(|status| {
        status.staged.len() + status.unstaged.len() > 0 || status.ahead_of_remote_count > 0
    })
}

/// `callAgentBlockKey`: a running turn blocks every agent; the reviewer
/// needs something to review.
pub fn call_block_key(
    preset: Option<&str>,
    busy: bool,
    has_changes: Option<bool>,
) -> Option<&'static str> {
    if busy {
        return Some("agents.call.busy");
    }
    (preset == Some("reviewer") && has_changes == Some(false))
        .then_some("agents.call.nothingToReview")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_reviewer_needs_something_to_review() {
        assert_eq!(
            call_block_key(Some("reviewer"), false, Some(false)),
            Some("agents.call.nothingToReview")
        );
        assert_eq!(call_block_key(Some("reviewer"), false, None), None);
        assert_eq!(call_block_key(None, false, Some(false)), None);
        assert_eq!(
            call_block_key(None, true, Some(true)),
            Some("agents.call.busy")
        );
        assert_eq!(has_reviewable_changes(None), None);
    }
}

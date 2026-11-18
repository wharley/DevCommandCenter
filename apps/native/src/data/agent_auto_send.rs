//! What an agent may send without asking the person first (Compozy's
//! `auto_accept`: an explicit grant for one kind of action, with a ceiling
//! a day): the review the Reviewer writes of a pull request, per project,
//! the reply an agent proposes through a connector, per room, and the
//! delivery of a task it opened (commit, push, pull request), per project. Off
//! everywhere until the person turns it on, on the agent's page; "Pausar
//! todos" holds it too. The grants and their rules live with the triggers
//! (`dcc_tauri::agent_triggers::auto_send`); here, what the screen says.
//!
//! The merge is never here: it stays the person's.

use super::pr_review::{ReviewEvent, can_submit};
use super::pr_review_verdict::Verdict;
use crate::i18n::{Args, Translate};

pub use dcc_tauri::agent_triggers::auto_send::{
    AutoSend, GENERAL_ROOM, MAX_DELIVER_NOTE, REPLIES_A_DAY, ReviewSend,
};

/// A review DCC sent for the agent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SentReview {
    pub event: ReviewEvent,
    pub comments: u32,
    /// What the forge did not take, in its words.
    pub warning: Option<String>,
}

/// Why a review that was to go by itself did not: said on its card, so
/// "reviewed" with nothing on the pull request is never left unexplained.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NotSent {
    /// The agent's answer could not be read as a review.
    Unreadable(String),
    /// No comment and no text to send.
    Nothing,
    /// The forge refused it, in its words.
    Refused(String),
}

impl NotSent {
    pub fn words(&self, cx: &gpui::App) -> String {
        let key = |name: &str| format!("agents.autoSend.notSent.{name}");
        match self {
            Self::Nothing => cx.t(&key("nothing")).to_string(),
            Self::Unreadable(why) => cx
                .t_with(
                    &key("unreadable"),
                    &Args::default().with("error", why.clone()),
                )
                .to_string(),
            Self::Refused(why) => cx
                .t_with(&key("refused"), &Args::default().with("error", why.clone()))
                .to_string(),
        }
    }
}

impl SentReview {
    /// "aprovou · 3 comentários".
    pub fn words(&self, cx: &gpui::App) -> String {
        let decision = cx.t(&format!("agents.autoSend.sent.{}", self.event.key()));
        let key = match self.comments {
            0 => return decision.to_string(),
            1 => "agents.autoSend.sent.oneComment",
            _ => "agents.autoSend.sent.comments",
        };
        let comments = cx.t_with(
            key,
            &Args::default().with("count", self.comments.to_string()),
        );
        format!("{decision} · {comments}")
    }
}

/// What to send of a finished review, if anything: the comments alone when
/// only commenting is granted, when the review does not decide ("not
/// decided yet", or no verdict: what it asks is for the author to read) or
/// when the forge refuses the decision here (the person's own pull request,
/// a draft); and nothing the forge would not take (no comment and no text;
/// asking for changes without saying why).
pub fn plan(
    send: ReviewSend,
    verdict: Option<Verdict>,
    decision_blocked: bool,
    comments: usize,
    body: &str,
) -> Option<ReviewEvent> {
    let event = match (send, verdict) {
        (ReviewSend::Ask, _) => return None,
        (ReviewSend::Decide, Some(verdict)) if !decision_blocked => verdict.event(),
        _ => ReviewEvent::Comment,
    };
    can_submit(event, comments, body, false).then_some(event)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_goes_follows_the_grant_the_verdict_and_the_forge() {
        use ReviewEvent::{Approve, Comment, RequestChanges};
        let plan =
            |send, verdict, blocked, comments, body| plan(send, verdict, blocked, comments, body);
        let approve = Some(Verdict::Approve);
        let changes = Some(Verdict::RequestChanges);
        // Asking sends nothing; no verdict and "not decided yet" go as
        // comments, for the author to read.
        assert_eq!(plan(ReviewSend::Ask, approve, false, 2, "ok"), None);
        assert_eq!(
            plan(ReviewSend::Decide, None, false, 2, "ok"),
            Some(Comment)
        );
        assert_eq!(
            plan(ReviewSend::Decide, Some(Verdict::Undecided), false, 2, "ok"),
            Some(Comment)
        );
        assert_eq!(
            plan(
                ReviewSend::Comment,
                Some(Verdict::Undecided),
                false,
                0,
                "a doubt"
            ),
            Some(Comment)
        );
        // The decision goes only when granted and the forge takes it.
        assert_eq!(
            plan(ReviewSend::Decide, approve, false, 0, ""),
            Some(Approve)
        );
        assert_eq!(
            plan(ReviewSend::Decide, changes, false, 1, "why"),
            Some(RequestChanges)
        );
        assert_eq!(
            plan(ReviewSend::Comment, approve, false, 1, ""),
            Some(Comment)
        );
        assert_eq!(
            plan(ReviewSend::Decide, approve, true, 0, "fine"),
            Some(Comment)
        );
        // Nothing to say, or changes asked without saying why: it waits.
        assert_eq!(plan(ReviewSend::Comment, approve, false, 0, " "), None);
        assert_eq!(plan(ReviewSend::Decide, changes, false, 1, ""), None);
    }
}

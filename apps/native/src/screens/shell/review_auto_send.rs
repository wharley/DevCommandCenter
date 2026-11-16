//! Sending the Reviewer's review of a pull request without asking, where
//! the person granted it for that project (`data::agent_auto_send`): right
//! after the review is written, in the same background run, DCC reads the
//! answer as the hub does, drops what an earlier thread already says and
//! submits it through the hub's own route — the one "Enviar revisão"
//! calls. A review that does not decide goes as comments. What cannot go
//! (nothing to say) waits in Pull Requests as before, and so does a review
//! the forge refused.

use gpui::App;

use crate::data::agent_auto_send::{ReviewSend, SentReview, plan};
use crate::data::desktop_api::DesktopApi;
use crate::data::pr_hub::{Detail, Item};
use crate::data::pr_review::{ReviewEvent, event_blocked};
use crate::data::pr_review_agent::parse_review;
use crate::data::pr_review_follow_up::drop_repeats;
use crate::data::pr_review_jobs::PrReviewJobs;
use crate::i18n::{Args, Translate};
use crate::ui::Toaster;

/// A review that ran, and what became of sending it: `None` when nothing
/// was to be sent.
pub(super) struct ReviewOutcome {
    pub answer: Result<String, String>,
    pub sent: Option<Result<SentReview, String>>,
}

/// Submits the review in `response`, when `send` grants it and there is
/// something the forge takes. Off the UI thread.
pub(super) fn send_review(
    api: &DesktopApi,
    item: &Item,
    detail: &Detail,
    response: &str,
    send: ReviewSend,
) -> Option<Result<SentReview, String>> {
    if send == ReviewSend::Ask {
        return None;
    }
    let mut review = parse_review(response, detail).ok()?;
    if let Some(login) = &item.forge_login {
        drop_repeats(&mut review, detail, login);
    }
    let decision_blocked = review.verdict.is_some_and(|verdict| {
        event_blocked(verdict.event(), item, detail.review_capabilities.as_ref()).is_some()
    });
    let summary = review.summary.trim();
    let event = plan(
        send,
        review.verdict,
        decision_blocked,
        review.comments.len(),
        summary,
    )?;
    let body = Some(summary).filter(|body| !body.is_empty());
    let sent = api
        .pr_hub_submit_review(item, body, event, &review.comments)
        .map_err(|error| error.to_string())
        .and_then(|result| {
            if !result.submitted {
                return Err(result
                    .warning
                    .unwrap_or_else(|| "the forge did not take the review".to_string()));
            }
            Ok(SentReview {
                // A decision the forge left out went as a comment.
                event: if result.decision_submitted {
                    event
                } else {
                    ReviewEvent::Comment
                },
                comments: result.submitted_comment_count,
                warning: result.warning,
            })
        });
    Some(sent)
}

/// The review of `pr_id` finished: its job says it was sent, or the person
/// hears why it was not and finds the drafts in Pull Requests. True when it
/// was sent.
pub(super) fn landed(
    cx: &mut App,
    pr_id: &str,
    agent: &str,
    label: &str,
    sent: Option<Result<SentReview, String>>,
) -> bool {
    let args = Args::default()
        .with("agent", agent.to_string())
        .with("task", label.to_string());
    match sent {
        Some(Ok(sent)) => {
            let title = cx.t_with("agents.room.card.reviewSent", &args);
            crate::ui::attention::notify_away(&title, &sent.words(cx));
            PrReviewJobs::sent(cx, pr_id, sent);
            true
        }
        Some(Err(error)) => {
            let text = cx.t_with("agents.autoSend.failed", &args.with("error", error));
            Toaster::error(text, cx);
            false
        }
        None => false,
    }
}

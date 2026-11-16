//! Sending the Reviewer's review of a pull request without asking, where
//! the person granted it for that project (`data::agent_auto_send`): right
//! after the review is written, in the same background run, DCC reads the
//! answer as the hub does, drops what an earlier thread already says and
//! submits it through the hub's own route — the one "Enviar revisão"
//! calls. A review that does not decide goes as comments. What cannot go
//! (nothing to say) waits in Pull Requests as before, and so does a review
//! the forge refused.
//!
//! A review an agent asked for from its conversation (a link a source
//! brought it) is then told to that agent there, so it answers whoever
//! asked; one the Reviewer's trigger started has nobody waiting.

use chrono::Utc;
use gpui::{App, Context};

use super::Shell;
use crate::data::agent_auto_send::{ReviewSend, SentReview, plan};
use crate::data::agent_triggers::AgentTriggers;
use crate::data::agents::AgentRecord;
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

/// What the agent that asked for a review is told once DCC published it:
/// whoever asked it through a connector can now be answered.
pub(super) fn sent_prompt(label: &str, url: &str, sent: &SentReview) -> String {
    let decision = match sent.event {
        ReviewEvent::Approve => "approved",
        ReviewEvent::RequestChanges => "changes requested",
        ReviewEvent::Comment => "commented, with no decision",
    };
    format!(
        "[DCC] Review sent: {label} — {decision}, {} comment(s).\n\nThis is DCC, not a message from the person. The review of {url} you asked for is written and DCC published it on the pull request in the person's name: {decision}, {} comment(s). If someone asked for this review through a connector (the message a source brought you), propose the reply there with dcc_propose_reply: one or two sentences in their language, in the same thread, saying it was reviewed and how it stands, with the pull request's link; no more than DCC told you here. If nobody is waiting for an answer, answer exactly NOTHING_TO_REPORT and nothing else. Do not ask for the review again, do not open a task and do not pass this on.",
        sent.comments, sent.comments
    )
}

impl Shell {
    /// Tells `agent` something DCC did for it, in the room's conversation
    /// it asked from (its review went, its pull request opened), unless
    /// something holds it (the quiet hours, its brake).
    pub(super) fn tell_agent(
        &mut self,
        agent: &AgentRecord,
        room: Option<&str>,
        prompt: String,
        cx: &mut Context<Self>,
    ) {
        let Some(triggers) = cx.try_global::<AgentTriggers>() else {
            return;
        };
        if triggers
            .quiet_check(Utc::now())
            .and_then(|()| triggers.brake_check(&agent.id))
            .is_err()
        {
            return;
        }
        let Some(send) = self.home_send(agent, room, prompt, cx) else {
            return;
        };
        let name = agent.name.clone();
        cx.background_executor()
            .spawn(async move {
                let sent = DesktopApi::load()
                    .ok_or_else(|| "DCC is not running".to_string())
                    .and_then(|api| super::agent_wake::send_home(&api, send));
                if let Err(error) = sent {
                    eprintln!("[dcc-native] review sent, {name} not told: {error}");
                }
            })
            .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_agent_is_told_what_went_and_asked_to_answer_who_asked() {
        let prompt = sent_prompt(
            "app#1089",
            "https://github.com/acme/app/pull/1089",
            &SentReview {
                event: ReviewEvent::Approve,
                comments: 3,
                warning: None,
            },
        );
        assert!(prompt.starts_with("[DCC] Review sent: app#1089 — approved, 3 comment(s)."));
        assert!(prompt.contains("dcc_propose_reply"));
        assert!(prompt.contains("NOTHING_TO_REPORT"));
        assert!(prompt.contains("https://github.com/acme/app/pull/1089"));
    }
}

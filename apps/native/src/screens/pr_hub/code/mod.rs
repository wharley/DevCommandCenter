//! The "Código" tab (`PullRequestCodeReview`): the changed files on the
//! left, the chosen one's patch with the conversations on its lines, a
//! comment drafted on any line, and the review sent at the bottom — a
//! comment, an approval or a request for changes, with the drafts. Its state
//! starts over on each pull request, as the desktop keys it by id.

mod agent;
#[cfg(test)]
mod agent_tests;
mod footer;
mod panel;
mod rows;
mod runs_on;
mod since;
#[cfg(test)]
mod tests;
mod thread;
mod view;

use std::time::Instant;

use gpui::{AppContext, Context, Entity, Focusable, SharedString, Subscription, Window};

use super::{DETAIL_FRESH, PrHub, message};
use crate::data::pr_hub::Detail;
use crate::data::pr_review::{DraftComment, InlineComment, ReviewEvent, draft_key};
use crate::i18n::Translate;
use crate::ui::{TextArea, TextAreaEvent};

/// The review's fields (comment, reply, review, the Reviewer's instruction)
/// scroll inside past this height.
const FIELD_MAX_HEIGHT: f32 = 200.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Summary,
    Code,
}

pub(crate) struct CodeReview {
    /// The pull request this state belongs to.
    pub for_id: Option<String>,
    pub path: Option<String>,
    pub drafts: Vec<DraftComment>,
    /// The line whose comment is being written (its draft key).
    pub editing: Option<String>,
    pub event: ReviewEvent,
    pub submitting: bool,
    pub submitted: bool,
    pub warning: Option<String>,
    pub submit_error: Option<String>,
    /// A reply or a resolve that failed.
    pub thread_error: Option<String>,
    /// The inline comment being answered.
    pub replying_to: Option<String>,
    pub replying: bool,
    /// The code pushed sideways, in pixels.
    pub scroll_x: f32,
    pub draft: Entity<TextArea>,
    pub reply: Entity<TextArea>,
    pub body: Entity<TextArea>,
    /// The review's text as last seen, to tell typing from clearing.
    pub body_seen: String,
    /// The reviewer's panel ("Revisar com o …").
    pub agent_panel_open: bool,
    /// The person's optional guidance for the reviewer.
    pub instruction: Entity<TextArea>,
    /// The line ("inline:key") or reply ("reply:id") being prepared.
    pub agent_pending: Option<String>,
    pub agent_error: Option<String>,
    /// How many drafts the finished review left, until "Ok".
    pub agent_result: Option<usize>,
    /// The reviewer's verdict on the review it wrote, shown by the event.
    pub verdict: Option<crate::data::pr_review_verdict::Verdict>,
    /// The finished review already applied on this visit.
    pub applied: Option<u64>,
}

impl CodeReview {
    pub fn new(window: &mut Window, cx: &mut Context<PrHub>) -> (Self, Vec<Subscription>) {
        let reply_placeholder = cx.t("pullRequests.code.replyPlaceholder");
        let body_placeholder = cx.t("pullRequests.code.reviewPlaceholders.comment");
        // Past `FIELD_MAX_HEIGHT` each field scrolls inside, so a long text
        // (the Reviewer's review) never pushes its send button off the window.
        let field =
            |placeholder: Option<SharedString>, window: &mut Window, cx: &mut Context<PrHub>| {
                cx.new(|cx| {
                    let area = TextArea::new(window, cx).with_max_height(FIELD_MAX_HEIGHT);
                    match placeholder {
                        Some(placeholder) => area.with_placeholder(placeholder),
                        None => area,
                    }
                })
            };
        let draft = field(None, window, cx);
        let reply = field(Some(reply_placeholder), window, cx);
        let body = field(Some(body_placeholder), window, cx);
        let instruction_placeholder = cx.t("pullRequests.code.agentInstructionPlaceholder");
        let instruction = field(Some(instruction_placeholder), window, cx);
        // Enter is a new line in each; Escape drops the line comment.
        let newline = |area: Entity<TextArea>, cx: &mut Context<PrHub>| {
            area.update(cx, |area, cx| area.insert("\n", cx))
        };
        let subscriptions = vec![
            cx.subscribe(&draft, move |hub, area, event, cx| match event {
                TextAreaEvent::Submitted => newline(area, cx),
                TextAreaEvent::Cancelled => hub.cancel_line_comment(cx),
                TextAreaEvent::Changed => cx.notify(),
                _ => {}
            }),
            cx.subscribe(&instruction, move |_, area, event, cx| match event {
                TextAreaEvent::Submitted => newline(area, cx),
                TextAreaEvent::Changed => cx.notify(),
                _ => {}
            }),
            cx.subscribe(&reply, move |_, area, event, cx| match event {
                TextAreaEvent::Submitted => newline(area, cx),
                TextAreaEvent::Changed => cx.notify(),
                _ => {}
            }),
            cx.subscribe(&body, move |hub, area, event, cx| match event {
                TextAreaEvent::Submitted => newline(area, cx),
                // What the person typed, not the field cleared after sending.
                TextAreaEvent::Changed => {
                    let text = area.read(cx).text().to_string();
                    if text != hub.review.body_seen {
                        hub.review.body_seen = text;
                        hub.review.submitted = false;
                        hub.review.warning = None;
                    }
                    cx.notify();
                }
                _ => {}
            }),
        ];
        let review = Self {
            for_id: None,
            path: None,
            drafts: Vec::new(),
            editing: None,
            event: ReviewEvent::Comment,
            submitting: false,
            submitted: false,
            warning: None,
            submit_error: None,
            thread_error: None,
            replying_to: None,
            replying: false,
            scroll_x: 0.0,
            draft,
            reply,
            body,
            body_seen: String::new(),
            agent_panel_open: false,
            instruction,
            agent_pending: None,
            agent_error: None,
            agent_result: None,
            verdict: None,
            applied: None,
        };
        (review, subscriptions)
    }
}

impl PrHub {
    /// The code tab's detail, with the patches, when it was read.
    pub(crate) fn code_read(&self, id: &str) -> Option<&Result<Detail, String>> {
        self.code_detail
            .as_ref()
            .filter(|(read, _, _)| read == id)
            .and_then(|(_, _, result)| result.as_ref())
    }

    /// A new pull request starts the code tab over.
    pub(crate) fn sync_review(&mut self, cx: &mut Context<Self>) {
        if self.review.for_id == self.selected {
            return;
        }
        let review = &mut self.review;
        review.for_id = self.selected.clone();
        review.path = None;
        review.drafts.clear();
        review.editing = None;
        review.event = ReviewEvent::Comment;
        review.verdict = None;
        review.submitting = false;
        review.submitted = false;
        review.warning = None;
        review.submit_error = None;
        review.thread_error = None;
        review.replying_to = None;
        review.replying = false;
        review.scroll_x = 0.0;
        review.body_seen.clear();
        for area in [
            review.draft.clone(),
            review.reply.clone(),
            review.body.clone(),
        ] {
            area.update(cx, |area, cx| area.set_text("", cx));
        }
        self.set_body_placeholder(cx);
    }

    fn set_body_placeholder(&mut self, cx: &mut Context<Self>) {
        let placeholder = cx.t(&format!(
            "pullRequests.code.reviewPlaceholders.{}",
            self.review.event.key()
        ));
        self.review
            .body
            .update(cx, |area, cx| area.set_placeholder(placeholder, cx));
    }

    pub(crate) fn set_tab(&mut self, tab: Tab, cx: &mut Context<Self>) {
        self.tab = tab;
        self.ensure_code_detail(cx);
        self.apply_review_job(cx);
        cx.notify();
    }

    /// `codeDetailQuery`: read only while the code tab shows, again after 20s.
    pub(crate) fn ensure_code_detail(&mut self, cx: &mut Context<Self>) {
        if self.tab != Tab::Code {
            return;
        }
        let Some(item) = self.selected_item(cx) else {
            return;
        };
        let fresh = self
            .code_detail
            .as_ref()
            .is_some_and(|(id, at, _)| *id == item.id && at.elapsed() < DETAIL_FRESH);
        let Some(api) = self.api.clone().filter(|_| !fresh) else {
            return;
        };
        let id = item.id.clone();
        // What was read stays on screen while it is read again.
        let kept = self
            .code_detail
            .take()
            .filter(|(read, _, _)| *read == id)
            .and_then(|(_, _, result)| result);
        self.code_detail = Some((id.clone(), Instant::now(), kept));
        cx.spawn(async move |this, cx| {
            let read = cx
                .background_executor()
                .spawn(async move { api.pr_hub_detail(&item, true) })
                .await;
            let _ = this.update(cx, |hub, cx| {
                if let Some((current, _, result)) = hub.code_detail.as_mut()
                    && *current == id
                {
                    *result = Some(read.map_err(|error| message(error, cx)));
                    hub.apply_review_job(cx);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn reread_code(&mut self, cx: &mut Context<Self>) {
        if let Some((_, at, _)) = self.code_detail.as_mut() {
            *at = Instant::now() - DETAIL_FRESH;
        }
        self.ensure_code_detail(cx);
    }

    pub(crate) fn choose_file(&mut self, path: String, cx: &mut Context<Self>) {
        self.review.path = Some(path);
        self.review.scroll_x = 0.0;
        cx.notify();
    }

    pub(crate) fn set_event(&mut self, event: ReviewEvent, cx: &mut Context<Self>) {
        self.review.event = event;
        self.review.submitted = false;
        self.review.warning = None;
        self.set_body_placeholder(cx);
        cx.notify();
    }

    /// `startComment`
    pub(crate) fn start_line_comment(
        &mut self,
        path: &str,
        line: u32,
        side: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.review.editing = Some(draft_key(path, line, side));
        let draft = self.review.draft.clone();
        draft.update(cx, |area, cx| area.set_text("", cx));
        let placeholder = cx.t_with(
            "pullRequests.code.inlinePlaceholder",
            &crate::i18n::Args::default().with("path", path),
        );
        draft.update(cx, |area, cx| area.set_placeholder(placeholder, cx));
        window.focus(&draft.focus_handle(cx), cx);
        cx.notify();
    }

    pub(crate) fn cancel_line_comment(&mut self, cx: &mut Context<Self>) {
        self.review.editing = None;
        let draft = self.review.draft.clone();
        draft.update(cx, |area, cx| area.set_text("", cx));
        cx.notify();
    }

    /// `saveComment`: one draft per line, the newest kept.
    pub(crate) fn save_line_comment(
        &mut self,
        path: &str,
        line: u32,
        side: &'static str,
        cx: &mut Context<Self>,
    ) {
        let body = self.review.draft.read(cx).text().trim().to_string();
        if body.is_empty() {
            return;
        }
        let next = DraftComment {
            path: path.to_string(),
            body,
            line,
            side,
        };
        let key = next.key();
        self.review.drafts.retain(|draft| draft.key() != key);
        self.review.drafts.push(next);
        self.review.submitted = false;
        self.cancel_line_comment(cx);
    }

    pub(crate) fn remove_draft(&mut self, key: &str, cx: &mut Context<Self>) {
        self.review.drafts.retain(|draft| draft.key() != key);
        cx.notify();
    }

    pub(crate) fn start_reply(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        self.review.replying_to = Some(id);
        let reply = self.review.reply.clone();
        reply.update(cx, |area, cx| area.set_text("", cx));
        window.focus(&reply.focus_handle(cx), cx);
        cx.notify();
    }

    pub(crate) fn cancel_reply(&mut self, cx: &mut Context<Self>) {
        self.review.replying_to = None;
        let reply = self.review.reply.clone();
        reply.update(cx, |area, cx| area.set_text("", cx));
        cx.notify();
    }

    /// `replyMutation`: on the conversation's first comment.
    pub(crate) fn send_reply(&mut self, comment: InlineComment, cx: &mut Context<Self>) {
        let body = self.review.reply.read(cx).text().to_string();
        let (Some(api), Some(item)) = (self.api.clone(), self.selected_item(cx)) else {
            return;
        };
        if body.trim().is_empty() || self.review.replying {
            return;
        }
        let thread = comment.thread_id.clone().unwrap_or_default();
        let root = self
            .code_read(&item.id)
            .and_then(|read| read.as_ref().ok())
            .and_then(|detail| {
                detail
                    .inline_comments
                    .iter()
                    .find(|entry| entry.thread_id == comment.thread_id)
                    .map(|entry| entry.id.clone())
            })
            .unwrap_or(comment.id.clone());
        self.review.replying = true;
        self.review.thread_error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let sent = cx
                .background_executor()
                .spawn(async move { api.pr_hub_reply_thread(&item, &root, &thread, &body) })
                .await;
            let _ = this.update(cx, |hub, cx| {
                hub.review.replying = false;
                match sent {
                    Ok(()) => {
                        hub.cancel_reply(cx);
                        hub.reread_code(cx);
                    }
                    Err(error) => hub.review.thread_error = Some(message(error, cx)),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// `resolveMutation`: resolves an open conversation, reopens a resolved one.
    pub(crate) fn toggle_resolved(&mut self, comment: InlineComment, cx: &mut Context<Self>) {
        let (Some(api), Some(item)) = (self.api.clone(), self.selected_item(cx)) else {
            return;
        };
        let thread = comment.thread_id.clone().unwrap_or_default();
        let resolved = comment.resolved != Some(true);
        self.review.thread_error = None;
        cx.spawn(async move |this, cx| {
            let done = cx
                .background_executor()
                .spawn(async move { api.pr_hub_resolve_thread(&item, &thread, resolved) })
                .await;
            let _ = this.update(cx, |hub, cx| {
                match done {
                    Ok(()) => hub.reread_code(cx),
                    Err(error) => hub.review.thread_error = Some(message(error, cx)),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// `submitMutation`: what the forge took leaves the screen; the rest stays.
    pub(crate) fn submit_review(&mut self, cx: &mut Context<Self>) {
        let (Some(api), Some(item)) = (self.api.clone(), self.selected_item(cx)) else {
            return;
        };
        let body = self.review.body.read(cx).text().trim().to_string();
        let event = self.review.event;
        let drafts = self.review.drafts.clone();
        let item_id = item.id.clone();
        if !crate::data::pr_review::can_submit(event, drafts.len(), &body, self.review.submitting) {
            return;
        }
        self.review.submitting = true;
        self.review.submit_error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let sent = cx
                .background_executor()
                .spawn(async move {
                    let body = Some(body.as_str()).filter(|body| !body.is_empty());
                    api.pr_hub_submit_review(&item, body, event, &drafts)
                })
                .await;
            let _ = this.update(cx, |hub, cx| {
                hub.review.submitting = false;
                match sent {
                    Ok(result) => {
                        let taken =
                            (result.submitted_comment_count as usize).min(hub.review.drafts.len());
                        hub.review.drafts.drain(..taken);
                        if result.body_submitted {
                            hub.review.body_seen.clear();
                            let body = hub.review.body.clone();
                            body.update(cx, |area, cx| area.set_text("", cx));
                        }
                        if result.decision_submitted {
                            hub.review.event = ReviewEvent::Comment;
                            hub.set_body_placeholder(cx);
                        }
                        // After the body cleared, which marks it unsent.
                        hub.review.submitted = result.submitted;
                        if result.submitted {
                            crate::data::pr_review_jobs::PrReviewJobs::dismiss(cx, &item_id);
                        }
                        hub.review.warning = result.warning;
                        hub.detail = None;
                        hub.ensure_detail(cx);
                        hub.reread_code(cx);
                        hub.refresh(cx);
                    }
                    Err(error) => hub.review.submit_error = Some(message(error, cx)),
                }
                cx.notify();
            });
        })
        .detach();
    }
}

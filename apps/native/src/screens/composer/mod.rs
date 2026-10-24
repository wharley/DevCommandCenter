//! Screen 3: the composer (`WorkspaceComposer`) under the conversation. The
//! prompt goes to the desktop, which runs the agent (`data::desktop_api`):
//! Enter sends, the stop button interrupts, and a draft is kept per
//! conversation while you move between tasks.

mod drafts;
mod surface;
#[cfg(test)]
mod tests;

use std::collections::HashMap;

use gpui::{App, Context, Entity, EventEmitter, Focusable, Subscription, Window, prelude::*};

use crate::data::desktop_api::{ApiError, DesktopApi};
use crate::i18n::Translate;
use crate::ui::{TextArea, TextAreaEvent, Toaster};

/// Past this the prompt scrolls inside the field (`max-h-[240px]`).
const INPUT_MAX_HEIGHT: f32 = 240.0;

/// What the conversation tells the composer about the open session.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ComposerSession {
    pub session_id: Option<String>,
    pub provider_id: Option<String>,
    pub model: Option<String>,
    pub running: bool,
    pub aborted: bool,
}

/// Something the conversation should do after the composer acted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComposerEvent {
    /// A turn was sent or stopped: read the history again now.
    Refresh,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Busy {
    Sending,
    Stopping,
}

pub struct Composer {
    pub(crate) input: Entity<TextArea>,
    pub(crate) session: ComposerSession,
    /// Drafts by conversation, also kept on disk.
    drafts: HashMap<String, String>,
    pub(crate) busy: Option<Busy>,
    /// Where the turns go instead of the desktop's own endpoint (tests).
    pub(crate) endpoint: Option<DesktopApi>,
    _input_events: Subscription,
}

impl EventEmitter<ComposerEvent> for Composer {}

impl Composer {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let placeholder = cx.t("composer.placeholder.default");
        let input = cx.new(|cx| {
            TextArea::new(window, cx)
                .with_placeholder(placeholder)
                .with_max_height(INPUT_MAX_HEIGHT)
        });
        let input_events = cx.subscribe_in(
            &input,
            window,
            |this, _, event: &TextAreaEvent, window, cx| match event {
                TextAreaEvent::Submitted => this.send(window, cx),
                TextAreaEvent::Changed => this.remember_draft(cx),
                _ => {}
            },
        );
        Self {
            input,
            session: ComposerSession::default(),
            drafts: drafts::load(),
            busy: None,
            endpoint: None,
            _input_events: input_events,
        }
    }

    /// Follows the conversation: switching sessions swaps the draft.
    pub fn set_session(&mut self, session: ComposerSession, cx: &mut Context<Self>) {
        if session == self.session {
            return;
        }
        let switched = session.session_id != self.session.session_id;
        self.session = session;
        if switched {
            let draft = self
                .session
                .session_id
                .as_ref()
                .and_then(|id| self.drafts.get(id))
                .cloned()
                .unwrap_or_default();
            self.input
                .update(cx, |input, cx| input.set_text(&draft, cx));
        }
        let disabled = self.session.session_id.is_none();
        self.input
            .update(cx, |input, cx| input.set_disabled(disabled, cx));
        cx.notify();
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        let handle = self.input.read(cx).focus_handle(cx);
        window.focus(&handle, cx);
    }

    /// Puts text in the field (the continuation prompt), replacing the draft.
    pub fn replace_draft(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |input, cx| input.set_text(text, cx));
        self.focus(window, cx);
    }

    fn remember_draft(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.session.session_id.clone() else {
            return;
        };
        let text = self.input.read(cx).text().to_string();
        let changed = if text.trim().is_empty() {
            self.drafts.remove(&session).is_some()
        } else {
            self.drafts.insert(session, text.clone()) != Some(text)
        };
        if changed {
            drafts::save_later(self.drafts.clone(), cx);
        }
        cx.notify();
    }

    pub(crate) fn can_send(&self, cx: &App) -> bool {
        self.session.session_id.is_some()
            && !self.session.running
            && self.busy.is_none()
            && !self.input.read(cx).text().trim().is_empty()
    }

    pub(crate) fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_send(cx) {
            return;
        }
        let prompt = self.input.read(cx).text().trim().to_string();
        self.submit(prompt, None, true, window, cx);
    }

    /// "Tentar de novo": the interrupted turn's prompt again, linked to it.
    pub fn retry(
        &mut self,
        prompt: String,
        turn_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.session.session_id.is_none() || self.busy.is_some() {
            return;
        }
        self.submit(prompt, Some(turn_id), false, window, cx);
    }

    /// "Continuar": reopens the session when it was stopped and leaves the
    /// continuation prompt in the field for the person to send.
    pub fn continue_interrupted(
        &mut self,
        original: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let preamble = cx.t("conversation.message.continuePrompt").to_string();
        let label = cx.t("conversation.message.originalPrompt").to_string();
        let text = continuation_prompt(original.as_deref(), &preamble, &label);
        if self.session.aborted
            && let Some(session) = self.session.session_id.clone()
        {
            let endpoint = self.endpoint.clone();
            let failure = cx.t("conversation.message.resumeFailed");
            cx.spawn(async move |this, cx| {
                let result = cx
                    .background_executor()
                    .spawn(async move { api(endpoint)?.resume(&session) })
                    .await;
                let _ = this.update(cx, |_, cx| {
                    if let Err(error) = result {
                        report(error, failure, cx);
                    }
                    cx.emit(ComposerEvent::Refresh);
                });
            })
            .detach();
        }
        self.replace_draft(&text, window, cx);
    }

    fn submit(
        &mut self,
        prompt: String,
        retry_of: Option<String>,
        from_field: bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.session.session_id.clone() else {
            return;
        };
        let resume_first = self.session.aborted && retry_of.is_some();
        let endpoint = self.endpoint.clone();
        let failure = if retry_of.is_some() {
            cx.t("conversation.message.retryFailed")
        } else {
            cx.t("native.sendFailed")
        };
        self.busy = Some(Busy::Sending);
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let api = api(endpoint)?;
                    if resume_first {
                        api.resume(&session)?;
                    }
                    api.send_turn(&session, &prompt, retry_of.as_deref())
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.busy = None;
                match result {
                    Ok(()) => {
                        if from_field {
                            this.clear_field(cx);
                        }
                        // The turn is on its way: the stop button shows until
                        // the history says otherwise.
                        this.session.running = true;
                    }
                    Err(error) => report(error, failure, cx),
                }
                cx.emit(ComposerEvent::Refresh);
                cx.notify();
            });
        })
        .detach();
    }

    fn clear_field(&mut self, cx: &mut Context<Self>) {
        self.input.update(cx, |input, cx| input.set_text("", cx));
        if let Some(session) = &self.session.session_id
            && self.drafts.remove(session).is_some()
        {
            drafts::save_later(self.drafts.clone(), cx);
        }
    }

    pub(crate) fn stop(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.session.session_id.clone() else {
            return;
        };
        if self.busy.is_some() {
            return;
        }
        let endpoint = self.endpoint.clone();
        let failure = cx.t("native.stopFailed");
        self.busy = Some(Busy::Stopping);
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api(endpoint)?.abort(&session) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.busy = None;
                if let Err(error) = result {
                    report(error, failure, cx);
                }
                cx.emit(ComposerEvent::Refresh);
                cx.notify();
            });
        })
        .detach();
    }
}

fn api(endpoint: Option<DesktopApi>) -> Result<DesktopApi, ApiError> {
    endpoint
        .or_else(DesktopApi::load)
        .ok_or(ApiError::DesktopClosed)
}

fn report(error: ApiError, failure: gpui::SharedString, cx: &mut App) {
    match error {
        ApiError::DesktopClosed => Toaster::error(cx.t("native.desktopClosed"), cx),
        error => Toaster::error_with(failure, error.to_string(), cx),
    }
}

/// `buildSafeContinuationPrompt`
pub fn continuation_prompt(original: Option<&str>, preamble: &str, label: &str) -> String {
    match original
        .map(str::trim)
        .filter(|original| !original.is_empty())
    {
        Some(original) => format!("{}\n\n{}:\n{original}", preamble.trim(), label.trim()),
        None => preamble.trim().to_string(),
    }
}

impl Focusable for Composer {
    fn focus_handle(&self, cx: &App) -> gpui::FocusHandle {
        self.input.read(cx).focus_handle(cx)
    }
}

impl Render for Composer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        surface::render(self, window, cx)
    }
}

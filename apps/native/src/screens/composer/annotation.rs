//! A snippet sent from the diff or a file (`handleSubmitAnnotation`): a
//! turn of its own with the composer's model, effort and approval (never
//! in plan mode), into this conversation — after the running turn when
//! one runs — or into a new conversation of the task ("Nova sessão",
//! `forceNewSession`), which then comes forward.

use gpui::Context;

use super::{Busy, Composer, ComposerEvent, api, report};
use crate::data::desktop_api::TurnOptions;
use crate::i18n::Translate;

impl Composer {
    /// False when nothing can take it now (no conversation, or busy).
    pub fn send_annotation(
        &mut self,
        prompt: String,
        new_session: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.busy.is_some() || prompt.trim().is_empty() {
            return false;
        }
        let options = TurnOptions {
            plan_mode: false,
            fast_mode: false,
            ..self.turn_options(&prompt)
        };
        if new_session {
            return self.start_annotation_session(prompt, options, cx);
        }
        if self.session.session_id.is_none() {
            if self.session.start.is_none() {
                return false;
            }
            self.dispatch(prompt, None, options, false, cx);
            return true;
        }
        if self.session.running {
            self.queue_annotation(prompt, options, cx);
        } else {
            self.dispatch(prompt, None, options, false, cx);
        }
        true
    }

    fn queue_annotation(&mut self, prompt: String, options: TurnOptions, cx: &mut Context<Self>) {
        let Some(session) = self.session.session_id.clone() else {
            return;
        };
        let options = TurnOptions {
            provider_id: None,
            model: None,
            ..options
        };
        let endpoint = self.endpoint.clone();
        let failure = cx.t("composer.followUp.queueFailed");
        self.busy = Some(Busy::Queueing);
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api(endpoint)?.queue_turn(&session, &prompt, &options) })
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

    fn start_annotation_session(
        &mut self,
        prompt: String,
        options: TurnOptions,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(start) = self.session.new_thread.clone() else {
            return false;
        };
        let Some(provider) = self.selected_provider_id() else {
            return false;
        };
        let model = self.selected_model_id();
        // The session starts on the pick: the turn needs none of its own.
        let options = TurnOptions {
            provider_id: None,
            model: None,
            ..options
        };
        let endpoint = self.endpoint.clone();
        let failure = cx.t("native.sendFailed");
        self.busy = Some(Busy::Sending);
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let api = api(endpoint)?;
                    let session = api.start_thread(
                        &start,
                        &provider,
                        model.as_deref(),
                        start.name.as_deref(),
                    )?;
                    api.send_turn(&session, &prompt, None, &options)?;
                    Ok(session)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.busy = None;
                match result {
                    Ok(session) => cx.emit(ComposerEvent::OpenSession(session)),
                    Err(error) => {
                        report(error, failure, cx);
                        cx.emit(ComposerEvent::Refresh);
                    }
                }
                cx.notify();
            });
        })
        .detach();
        true
    }
}

#[cfg(test)]
#[path = "annotation_tests.rs"]
mod tests;

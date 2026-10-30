//! A task's first prompt (`handleSubmitPrompt` with no session): starts its
//! conversation on the composer's provider and model, sends the turn, and
//! names an automatically named task after the prompt (`deriveTaskTitle`,
//! never a model).

use dcc_core::domain::task_title::derive_task_title;
use gpui::Context;

use super::{Busy, Composer, ComposerEvent, api, report};
use crate::data::desktop_api::TurnOptions;
use crate::i18n::Translate;

impl Composer {
    pub(super) fn first_send(
        &mut self,
        prompt: String,
        options: TurnOptions,
        from_field: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(start) = self.session.start.clone() else {
            return;
        };
        // No provider can start: the send button is off (`hasProvider`).
        let Some(pick) = self.new_task_pick() else {
            return;
        };
        let title = start
            .auto_named
            .then(|| derive_task_title(&prompt, &cx.t("sidebar.newWorkspace")));
        let thread_title = title.clone().or_else(|| start.name.clone());
        // The session starts on the pick: the turn needs no provider of its own.
        let options = TurnOptions {
            provider_id: None,
            model: None,
            ..options
        };
        let endpoint = self.endpoint.clone();
        let failure = cx.t("native.sendFailed");
        let sent = from_field.then(|| self.take_field(cx));
        let key = self.session.draft_key();
        self.busy = Some(Busy::Sending);
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let api = api(endpoint)?;
                    let session = api.start_thread(
                        &start,
                        &pick.provider_id,
                        Some(&pick.model_id),
                        thread_title.as_deref(),
                    )?;
                    api.send_turn(&session, &prompt, None, &options)?;
                    // Naming is extra: it never undoes an accepted first turn.
                    if let Some(title) = &title
                        && let Err(error) =
                            api.apply_task_title(&session, &start.workspace_id, title)
                    {
                        eprintln!("[dcc-native] automatic task title failed: {error}");
                    }
                    Ok(session)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.busy = None;
                match result {
                    Ok(_) => {
                        crate::data::ux_metrics::record("first_prompt");
                        if this.still_on(&key) {
                            this.session.running = true;
                        }
                    }
                    Err(error) => {
                        if let Some(sent) = sent {
                            this.restore_field(sent, cx);
                        }
                        report(error, failure, cx);
                    }
                }
                cx.emit(ComposerEvent::Refresh);
                cx.notify();
            });
        })
        .detach();
    }
}

//! "Editar a partir daqui" on a prompt: asks the backend what a rewind
//! would do, lets the person confirm (and choose whether files are
//! restored), then the conversation ends before that message and the message
//! returns to the composer. A provider that cannot cut its own conversation
//! only gets the message back in the composer (the app has no fork).

mod dialog;

use gpui::{Context, Task, Window};

use super::ConversationView;
use crate::data::desktop_api::DesktopApi;
use crate::data::rewind::{self, Outcome, Plan, Prepared, Stopped};
use crate::i18n::{Args, Translate};
use crate::ui::Toaster;

#[derive(Default)]
pub(crate) struct RewindState {
    preparing: bool,
    /// The plan the dialog confirms.
    target: Option<Plan>,
    working: bool,
    /// Restoring stopped: shown instead of the choices.
    stopped: Option<Stopped>,
    /// The message for the composer at the next frame (it needs the window).
    prompt: Option<String>,
    _task: Option<Task<()>>,
}

fn api() -> Option<DesktopApi> {
    if cfg!(test) { None } else { DesktopApi::load() }
}

impl ConversationView {
    /// `useEditFromHere().begin`: a prompt without a durable turn is only
    /// copied to the composer.
    pub(super) fn edit_from_here(
        &mut self,
        turn_id: Option<String>,
        prompt: String,
        cx: &mut Context<Self>,
    ) {
        let session = self.data.as_ref().and_then(|data| data.session_id.clone());
        let (Some(session), Some(turn)) = (session, turn_id) else {
            self.put_prompt(prompt, cx);
            return;
        };
        if self.rewind.preparing {
            return;
        }
        let Some(api) = api() else {
            Toaster::error(cx.t("native.desktopClosed"), cx);
            return;
        };
        self.rewind.preparing = true;
        self.rewind._task = Some(cx.spawn(async move |this, cx| {
            let prepared = cx
                .background_executor()
                .spawn(async move { api.prepare_rewind(&session, &turn) })
                .await;
            let _ = this.update(cx, |view, cx| {
                view.rewind.preparing = false;
                view.prepared(prepared, prompt, cx);
            });
        }));
    }

    fn prepared(
        &mut self,
        prepared: Result<Prepared, crate::data::desktop_api::ApiError>,
        prompt: String,
        cx: &mut Context<Self>,
    ) {
        match prepared {
            Ok(Prepared::Ready(plan)) => {
                self.rewind.stopped = None;
                self.rewind.target = Some(plan);
            }
            Ok(Prepared::NewThread { reason, prompt }) => {
                let reason = cx.t(&format!(
                    "conversation.editFromHere.newThreadReasons.{reason}"
                ));
                Toaster::info(
                    cx.t_with(
                        "conversation.editFromHere.unsupported",
                        &Args::default().with("reason", reason),
                    ),
                    cx,
                );
                self.put_prompt(prompt, cx);
            }
            Ok(Prepared::Blocked(
                dcc_core::application::ConversationRewindBlock::AnchorNotFound,
            )) => self.put_prompt(prompt, cx),
            Ok(Prepared::Blocked(block)) => Toaster::error(
                cx.t(&format!(
                    "conversation.editFromHere.blocked.{}",
                    rewind::block_key(block)
                )),
                cx,
            ),
            Err(error) => {
                let detail = error.text(cx);
                Toaster::error_with(cx.t("conversation.editFromHere.failed"), detail, cx)
            }
        }
        cx.notify();
    }

    /// `confirm(restoreFiles)`: files first, only when chosen; the
    /// conversation is cut only after they are.
    fn confirm_rewind(&mut self, restore_files: bool, cx: &mut Context<Self>) {
        let Some(plan) = self.rewind.target.clone().filter(|_| !self.rewind.working) else {
            return;
        };
        let Some(api) = api() else {
            Toaster::error(cx.t("native.desktopClosed"), cx);
            return;
        };
        self.rewind.working = true;
        cx.notify();
        self.rewind._task = Some(cx.spawn(async move |this, cx| {
            let outcome = cx
                .background_executor()
                .spawn(async move { api.execute_rewind(&plan, restore_files) })
                .await;
            let _ = this.update(cx, |view, cx| {
                view.rewind.working = false;
                view.rewound(outcome, cx);
                view.refresh_now(cx);
            });
        }));
    }

    fn rewound(
        &mut self,
        outcome: Result<Outcome, crate::data::desktop_api::ApiError>,
        cx: &mut Context<Self>,
    ) {
        match outcome {
            Ok(Outcome::Rewound {
                prompt,
                restored_turns,
            }) => {
                self.rewind.target = None;
                self.put_prompt(prompt, cx);
                Toaster::success(
                    if restored_turns > 0 {
                        cx.t_with(
                            "conversation.editFromHere.doneRestored",
                            &Args::default().with_count(restored_turns as i64),
                        )
                    } else {
                        cx.t("conversation.editFromHere.done")
                    },
                    cx,
                );
            }
            Ok(Outcome::FilesStopped(stopped)) => self.rewind.stopped = Some(stopped),
            Ok(Outcome::Refused) => {
                self.rewind.target = None;
                Toaster::error(cx.t("conversation.editFromHere.refused"), cx);
            }
            Err(error) => {
                self.rewind.target = None;
                let detail = error.text(cx);
                Toaster::error_with(cx.t("conversation.editFromHere.failed"), detail, cx);
            }
        }
        cx.notify();
    }

    fn cancel_rewind(&mut self, cx: &mut Context<Self>) {
        if self.rewind.working {
            return;
        }
        self.rewind.target = None;
        self.rewind.stopped = None;
        cx.notify();
    }

    fn put_prompt(&mut self, prompt: String, cx: &mut Context<Self>) {
        self.rewind.prompt = Some(prompt);
        cx.notify();
    }

    /// The message replaces the draft, attachments as chips again.
    pub(super) fn take_rewound_prompt(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(prompt) = self.rewind.prompt.take() {
            let composer = self.composer.clone();
            composer.update(cx, |composer, cx| {
                composer.replace_draft(&prompt, window, cx)
            });
        }
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

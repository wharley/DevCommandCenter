//! Follow-ups while the agent works: "Enfileirar para depois" (also Enter
//! while a turn runs), "Orientar o agente agora" for providers that take
//! guidance mid-turn, and the "Próximas mensagens" tray over the prompt
//! (move, edit, remove, "Enviar próxima" once nothing runs). The queue is
//! read from the session's events; the desktop sends each one in turn.

use dcc_core::domain::session::QueuedTurn;
use dcc_core::ports::STEER_WINDOW_CLOSED;
use gpui::{AnyElement, Context, FontWeight, SharedString, Window, div, prelude::*, px};

use super::{Busy, Composer, ComposerEvent, api, report};
use crate::data::desktop_api::{ApiError, TurnOptions};
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, Toaster};

/// Where a follow-up goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FollowUp {
    Queue,
    Steer,
}

/// A tray action on its way: the queued turn it is about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum QueueAction {
    Edit(String),
    Remove(String),
    Move(String),
    DispatchNext,
}

impl Composer {
    /// `canSteerActiveTurn`: a running turn on a provider that takes it.
    pub(crate) fn can_steer(&self) -> bool {
        self.session.running
            && self
                .provider()
                .is_some_and(|provider| provider.capabilities.supports_steering)
    }

    /// Queues or steers what is in the field. Guidance that arrives as the
    /// turn wraps up becomes the next message instead of being lost.
    pub(crate) fn follow_up(&mut self, kind: FollowUp, cx: &mut Context<Self>) {
        let Some(session) = self.session.session_id.clone() else {
            return;
        };
        let prompt = self.prompt(cx).trim().to_string();
        if prompt.is_empty() || self.busy.is_some() || !self.session.running {
            return;
        }
        if kind == FollowUp::Steer && !self.can_steer() {
            return;
        }
        let options = TurnOptions {
            provider_id: None,
            model: None,
            ..self.turn_options(&prompt)
        };
        let endpoint = self.endpoint.clone();
        let failure = match kind {
            FollowUp::Queue => cx.t("composer.followUp.queueFailed"),
            FollowUp::Steer => cx.t("composer.followUp.steerFailed"),
        };
        self.busy = Some(match kind {
            FollowUp::Queue => Busy::Queueing,
            FollowUp::Steer => Busy::Steering,
        });
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let api = api(endpoint)?;
                    if kind == FollowUp::Steer {
                        match api.steer_turn(&session, &prompt) {
                            Err(ApiError::Refused { code, .. }) if code == STEER_WINDOW_CLOSED => {}
                            other => return other.map(|()| false),
                        }
                    }
                    api.queue_turn(&session, &prompt, &options)
                        .map(|()| kind == FollowUp::Steer)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.busy = None;
                match result {
                    Ok(fell_back) => {
                        this.clear_field(cx);
                        if fell_back {
                            Toaster::info(cx.t("composer.followUp.steerQueued"), cx);
                        }
                    }
                    Err(error) => report(error, failure, cx),
                }
                cx.emit(ComposerEvent::Refresh);
                cx.notify();
            });
        })
        .detach();
    }

    /// One tray action; the desktop answers with the queue as it now is.
    fn run_queue(
        &mut self,
        action: QueueAction,
        job: impl FnOnce(
            &crate::data::desktop_api::DesktopApi,
        ) -> Result<Option<Vec<QueuedTurn>>, ApiError>
        + Send
        + 'static,
        then: impl FnOnce(&mut Self, &mut Window, &mut Context<Self>) + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.queue_busy.is_some() {
            return;
        }
        let failure = if action == QueueAction::DispatchNext {
            cx.t("composer.followUp.dispatchFailed")
        } else {
            cx.t("composer.followUp.queueActionFailed")
        };
        self.queue_busy = Some(action);
        let endpoint = self.endpoint.clone();
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { job(&api(endpoint)?) })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.queue_busy = None;
                match result {
                    Ok(queue) => {
                        if let Some(queue) = queue {
                            this.session.queue = std::sync::Arc::new(queue);
                        }
                        then(this, window, cx);
                    }
                    Err(error) => report(error, failure, cx),
                }
                cx.emit(ComposerEvent::Refresh);
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn remove_queued(
        &mut self,
        id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.session.session_id.clone() else {
            return;
        };
        let target = id.clone();
        self.run_queue(
            QueueAction::Remove(id),
            move |api| api.remove_queued_turn(&session, &target).map(Some),
            |_, _, _| {},
            window,
            cx,
        );
    }

    /// "Editar mensagem": out of the queue and back into the field.
    pub(crate) fn edit_queued(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.session.session_id.clone() else {
            return;
        };
        let Some(prompt) = self
            .session
            .queue
            .iter()
            .find(|turn| turn.id == id)
            .map(|turn| turn.prompt.clone())
        else {
            return;
        };
        let target = id.clone();
        self.run_queue(
            QueueAction::Edit(id),
            move |api| api.remove_queued_turn(&session, &target).map(Some),
            move |this, window, cx| this.replace_draft(&prompt, window, cx),
            window,
            cx,
        );
    }

    /// ↑ / ↓ on a queued turn.
    pub(crate) fn move_queued(
        &mut self,
        index: usize,
        down: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.session.session_id.clone() else {
            return;
        };
        let target = if down {
            index + 1
        } else {
            index.wrapping_sub(1)
        };
        if target >= self.session.queue.len() || index >= self.session.queue.len() {
            return;
        }
        let mut ids: Vec<String> = self
            .session
            .queue
            .iter()
            .map(|turn| turn.id.clone())
            .collect();
        ids.swap(index, target);
        let moving = self.session.queue[index].id.clone();
        self.run_queue(
            QueueAction::Move(moving),
            move |api| api.reorder_queue(&session, &ids).map(Some),
            |_, _, _| {},
            window,
            cx,
        );
    }

    pub(crate) fn dispatch_next(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.session.session_id.clone() else {
            return;
        };
        if self.session.running || self.session.queue.is_empty() {
            return;
        }
        self.run_queue(
            QueueAction::DispatchNext,
            move |api| api.dispatch_next_queued_turn(&session).map(|()| None),
            |_, _, _| {},
            window,
            cx,
        );
    }
}

/// `size-6` ghost icon buttons (their `aria-label` has no visible text).
fn row_button(id: SharedString, icon: &'static str, disabled: bool) -> Button {
    Button::new(id)
        .variant(ButtonVariant::Ghost)
        .size(ButtonSize::IconXs)
        .disabled(disabled)
        .child(Icon::new(icon).size(12.0))
}

/// The "Próximas mensagens" tray (`mb-2 rounded-xl border bg-background/30`).
pub(super) fn queue_tray(composer: &Composer, cx: &mut Context<Composer>) -> Option<AnyElement> {
    let queue = composer.session.queue.clone();
    if queue.is_empty() {
        return None;
    }
    let colors = cx.theme().colors.clone();
    let busy = composer.queue_busy.is_some();
    let last = queue.len() - 1;
    let header =
        div()
            .mb(px(6.0))
            .flex()
            .items_center()
            .gap(px(6.0))
            .text_size(px(11.0))
            .font_weight(FontWeight::MEDIUM)
            .text_color(colors.muted_foreground)
            .child(Icon::new("list-plus").size(12.0).stroke(2.0))
            .child(cx.t_with(
                "composer.followUp.queueTitle",
                &Args::count(queue.len() as i64),
            ))
            .when(!composer.session.running, |header| {
                header.child(
                    div().ml_auto().child(
                        Button::new("queue-send-next")
                            .variant(ButtonVariant::Ghost)
                            .size(ButtonSize::Sm)
                            .disabled(busy)
                            .on_click(cx.listener(|composer, _, window, cx| {
                                composer.dispatch_next(window, cx)
                            }))
                            .child(
                                div()
                                    .flex()
                                    .h(px(24.0))
                                    .items_center()
                                    .gap(px(4.0))
                                    .text_size(px(11.0))
                                    .child(Icon::new("play-filled").size(12.0))
                                    .child(cx.t("composer.followUp.sendNext")),
                            ),
                    ),
                )
            });
    let rows: Vec<AnyElement> = queue
        .iter()
        .enumerate()
        .map(|(index, turn)| {
            let edit = turn.id.clone();
            let remove = turn.id.clone();
            let id = &turn.id;
            div()
                .flex()
                .min_w_0()
                .items_center()
                .gap(px(4.0))
                .rounded(px(8.0))
                .bg(colors.muted.opacity(0.35))
                .px(px(8.0))
                .py(px(6.0))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_size(px(12.0))
                        .text_color(colors.foreground.opacity(0.85))
                        .child(SharedString::from(turn.prompt.replace('\n', " "))),
                )
                .child(
                    row_button(
                        SharedString::from(format!("queue-up-{id}")),
                        "arrow-up",
                        index == 0 || busy,
                    )
                    .on_click(cx.listener(move |composer, _, window, cx| {
                        composer.move_queued(index, false, window, cx)
                    })),
                )
                .child(
                    row_button(
                        SharedString::from(format!("queue-down-{id}")),
                        "arrow-down",
                        index == last || busy,
                    )
                    .on_click(cx.listener(move |composer, _, window, cx| {
                        composer.move_queued(index, true, window, cx)
                    })),
                )
                .child(
                    row_button(
                        SharedString::from(format!("queue-edit-{id}")),
                        "pencil",
                        busy,
                    )
                    .on_click(cx.listener(move |composer, _, window, cx| {
                        composer.edit_queued(edit.clone(), window, cx)
                    })),
                )
                .child(
                    row_button(SharedString::from(format!("queue-remove-{id}")), "x", busy)
                        .on_click(cx.listener(move |composer, _, window, cx| {
                            composer.remove_queued(remove.clone(), window, cx)
                        })),
                )
                .into_any_element()
        })
        .collect();
    Some(
        div()
            .debug_selector(|| "queue-tray".into())
            .mb(px(8.0))
            .rounded(px(12.0))
            .border_1()
            .border_color(colors.border.opacity(0.45))
            .bg(colors.background.opacity(0.3))
            .px(px(10.0))
            .py(px(8.0))
            .child(header)
            .child(
                div()
                    .id("queue-rows")
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .max_h(px(144.0))
                    .overflow_y_scroll()
                    .children(rows),
            )
            .into_any_element(),
    )
}

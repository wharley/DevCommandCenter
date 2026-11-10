//! The change card's footer: where the task stands in a few words and,
//! for an open task no turn runs in, "Ver mudanças", the dock's next step
//! and "Abrir PR". A click here never commits nor pushes: the step opens
//! the task, where its own dock shows the commit's or the pull request's
//! dialog for the person to review (`RoomEvent::DeliverTask`); a push, which
//! the dock runs at once, only takes the person there. Merging stays in
//! Pull Requests.

use gpui::{AnyElement, Context, SharedString, div, prelude::*, px};

use super::room::{RoomEvent, TeamRoom};
use crate::data::task_changes::ChangesRead;
use crate::data::task_delivery::{
    CardDelivery, DeliveryInput, DeliveryStep, card_delivery, step_of,
};
use crate::data::task_reports::{ReportState, TaskReport};
use crate::data::tasks::TaskStatus;
use crate::i18n::{Args, Translate};
use crate::screens::conversation::header::delivery::action_label;
use crate::theme::ActiveTheme;
use crate::ui::{Button, ButtonSize, ButtonVariant};

/// What a click on the card's step does: never the step itself.
pub(crate) fn step_event(task_id: &str, step: DeliveryStep) -> RoomEvent {
    match step {
        DeliveryStep::Commit { .. } | DeliveryStep::CreateRequest => RoomEvent::DeliverTask {
            task_id: task_id.to_string(),
            step,
        },
        // The dock pushes at once: the person clicks it there.
        DeliveryStep::Push => RoomEvent::OpenTaskChanges(task_id.to_string()),
    }
}

/// What the footer shows of a task.
enum Footer {
    /// Completed or archived: its state, nothing to click.
    Closed(&'static str),
    Open {
        delivery: Option<CardDelivery>,
        /// A turn runs: the state only.
        busy: bool,
    },
}

impl TeamRoom {
    fn footer_of(
        &self,
        task: &TaskReport,
        task_id: &str,
        read: Option<&ChangesRead>,
        cx: &Context<Self>,
    ) -> Option<Footer> {
        let sidebar = self.sidebar.read(cx);
        let checkout = sidebar.task_checkout(task_id)?;
        match checkout.status {
            TaskStatus::Completed => {
                return Some(Footer::Closed("agents.report.delivery.completed"));
            }
            TaskStatus::Archived => return Some(Footer::Closed("agents.report.delivery.archived")),
            _ => {}
        }
        let delivery = checkout.snapshot.and_then(|snapshot| {
            Some(card_delivery(&DeliveryInput {
                branch: checkout.branch,
                git: snapshot.git.as_ref()?,
                pr: snapshot.pr.as_ref(),
                committed: read
                    .filter(|read| read.from_checkout)
                    .map(|read| read.committed.as_slice()),
            }))
        });
        Some(Footer::Open {
            delivery,
            busy: checkout.turn_open || task.state == ReportState::Working,
        })
    }

    pub(super) fn delivery_footer(
        &self,
        index: usize,
        row: usize,
        task: &TaskReport,
        task_id: &str,
        read: Option<&ChangesRead>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let colors = cx.theme().colors.clone();
        let (words, delivery, busy): (Option<SharedString>, Option<CardDelivery>, bool) =
            match self.footer_of(task, task_id, read, cx)? {
                Footer::Closed(key) => (Some(cx.t(key)), None, true),
                Footer::Open { delivery, busy } => {
                    let words = delivery.as_ref().map(|delivery| {
                        let mut args = Args::default().with("requestLabel", delivery.request_label);
                        if let Some(number) = delivery.state.number() {
                            args = args.with("number", number.to_string());
                        }
                        cx.t_with(delivery.state.i18n_key(), &args)
                    });
                    (words, delivery, busy)
                }
            };
        let id = |name: &str| SharedString::from(format!("room-report-{name}-{index}-{row}"));
        let mut buttons: Vec<AnyElement> = Vec::new();
        if !busy {
            let open = task_id.to_string();
            buttons.push(
                Button::new(id("changes"))
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::Xs)
                    .text_size(11.5)
                    .muted()
                    .on_click(cx.listener(move |_, _, _, cx| {
                        cx.emit(RoomEvent::OpenTaskChanges(open.clone()))
                    }))
                    .child(cx.t("agents.report.actions.changes"))
                    .into_any_element(),
            );
            if let Some(delivery) = &delivery {
                if let Some(request) = &delivery.request {
                    let url = request.url.clone();
                    let args = Args::default().with("requestLabel", delivery.request_label);
                    buttons.push(
                        Button::new(id("request"))
                            .variant(ButtonVariant::Ghost)
                            .size(ButtonSize::Xs)
                            .text_size(11.5)
                            .muted()
                            .on_click(cx.listener(move |_, _, _, cx| {
                                cx.emit(RoomEvent::OpenPullRequest(url.clone()))
                            }))
                            .child(cx.t_with("agents.report.actions.openRequest", &args))
                            .into_any_element(),
                    );
                }
                if let Some((action, step)) = delivery
                    .step
                    .and_then(|action| Some((action, step_of(&action)?)))
                {
                    let open = task_id.to_string();
                    buttons.push(
                        Button::new(id("deliver"))
                            .variant(ButtonVariant::Outline)
                            .size(ButtonSize::Xs)
                            .text_size(11.5)
                            .on_click(
                                cx.listener(move |_, _, _, cx| cx.emit(step_event(&open, step))),
                            )
                            .child(action_label(&action, delivery.request_label, cx))
                            .into_any_element(),
                    );
                }
            }
        }
        if words.is_none() && buttons.is_empty() {
            return None;
        }
        let selector = format!("room-report-delivery-{index}-{row}");
        Some(
            div()
                .debug_selector(move || selector.clone())
                .flex()
                .items_center()
                .gap(px(6.0))
                .min_h(px(24.0))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_size(px(11.0))
                        .text_color(colors.muted_foreground)
                        .children(words),
                )
                .children(buttons)
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_push_only_opens_the_task_and_the_rest_asks_for_the_dialog() {
        assert!(matches!(
            step_event("w", DeliveryStep::Push),
            RoomEvent::OpenTaskChanges(task) if task == "w"
        ));
        for step in [
            DeliveryStep::Commit { push: true },
            DeliveryStep::Commit { push: false },
            DeliveryStep::CreateRequest,
        ] {
            assert!(matches!(
                step_event("w", step),
                RoomEvent::DeliverTask { task_id, step: asked } if task_id == "w" && asked == step
            ));
        }
    }
}

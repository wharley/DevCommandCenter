//! The delivery of a task on another machine: the same rail a local task
//! has, where things stand, and the next step's button, which runs on the
//! machine with its git and its forge login after the person confirms. The
//! commit takes the machine's own message and the pull request its default
//! title; editing them here comes later.

use gpui::{AnyElement, Context, FontWeight, SharedString, Window, div, prelude::*, px};

use super::CodeColumn;
use crate::data::delivery_rail::{Standing, Step, StepState};
use crate::data::machines::RemoteChanges;
use crate::data::remote_delivery::{RemoteStep, next_step, rail_of};
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::ui::delivery_rail::{RailStep, RailTone, delivery_rail};
use crate::ui::{Button, ButtonSize, ButtonVariant, Dialog, Toaster, dialog_footer, spinner};

/// "PR" until the machine says which forge it is.
const REQUEST_LABEL: &str = "PR";

fn read(column: &CodeColumn) -> Option<&RemoteChanges> {
    column.remote.changes.as_ref()?.as_ref().ok()
}

impl CodeColumn {
    fn ask_remote_step(&mut self, step: RemoteStep, cx: &mut Context<Self>) {
        if self.remote.delivering {
            return;
        }
        self.remote.confirm = Some(step);
        cx.notify();
    }

    fn cancel_remote_step(&mut self, cx: &mut Context<Self>) {
        self.remote.confirm = None;
        cx.notify();
    }

    /// The dialog's "yes", for the column's own tests (the dialog is drawn
    /// by the shell's overlay).
    #[cfg(test)]
    pub(super) fn run_remote_step_for_tests(&mut self, cx: &mut Context<Self>) {
        self.run_remote_step(cx);
    }

    /// Runs the confirmed step on the machine, then reads its state again.
    fn run_remote_step(&mut self, cx: &mut Context<Self>) {
        let Some(step) = self.remote.confirm.take() else {
            return;
        };
        let Some((api, task)) = self.remote_api() else {
            return;
        };
        let token = read(self)
            .map(|changes| changes.changes_token.clone())
            .unwrap_or_default();
        self.remote.delivering = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let asked = task.clone();
            let done = cx
                .background_executor()
                .spawn(async move { api.remote_deliver(&asked, step, &token) })
                .await;
            let _ = this.update(cx, |column, cx| {
                if column.remote.task.as_deref() != Some(task.as_str()) {
                    return;
                }
                column.remote.delivering = false;
                match done {
                    Ok(()) => Toaster::success(
                        cx.t(&format!("codeColumn.remote.done.{}", step.key())),
                        cx,
                    ),
                    Err(error) => Toaster::error_with(
                        cx.t(&format!("codeColumn.remote.failed.{}", step.key())),
                        super::remote::reason(error),
                        cx,
                    ),
                }
                column.read_remote_changes(cx);
                cx.notify();
            });
        })
        .detach();
    }
}

/// The rail, where things stand and the next step, over the list.
pub(super) fn delivery_strip(
    column: &CodeColumn,
    cx: &mut Context<CodeColumn>,
) -> Option<AnyElement> {
    let changes = read(column)?;
    let colors = cx.theme().colors.clone();
    let pr = column.remote.pr.as_ref();
    let rail = rail_of(changes, pr);
    let url = pr.and_then(|pr| pr.url.clone());
    let steps = rail
        .steps
        .iter()
        .map(|(step, state)| {
            let label: SharedString = match (step, pr.and_then(|pr| pr.number)) {
                (Step::Request, Some(number)) => format!("{REQUEST_LABEL} #{number}").into(),
                (Step::Request, None) => REQUEST_LABEL.into(),
                (step, _) => cx.t(&format!("commit.rail.steps.{}", step.key())),
            };
            let tone = match state {
                StepState::Done => RailTone::Done,
                StepState::Current => RailTone::Current,
                StepState::Failed => RailTone::Failed,
                StepState::Todo => RailTone::Todo,
            };
            let rail_step = RailStep::new(step.key(), label, tone);
            match (step, url.clone()) {
                (Step::Request, Some(url)) => rail_step.on_click(move |_, _, cx| cx.open_url(&url)),
                _ => rail_step,
            }
        })
        .collect();
    let args = match rail.standing {
        Standing::ToCommit => Args::count(changes.changes.len() as i64),
        Standing::ToPush => Args::count(changes.ahead_of_remote_count as i64),
        _ => Args::default().with("label", REQUEST_LABEL),
    };
    let standing = cx.t_with(
        &format!("commit.rail.status.{}", rail.standing.key()),
        &args,
    );
    let busy = column.remote.delivering;
    let button = next_step(changes, pr).map(|step| {
        Button::new("remote-delivery-step")
            .size(ButtonSize::Sm)
            .text_size(12.0)
            .disabled(busy)
            .on_click(cx.listener(move |column, _, _, cx| column.ask_remote_step(step, cx)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .when(busy, |label| {
                        label.child(spinner("remote-delivery-busy", 14.0))
                    })
                    .child(cx.t(&format!("codeColumn.remote.steps.{}", step.key()))),
            )
    });
    Some(
        div()
            .debug_selector(|| "remote-delivery".into())
            .flex_none()
            .border_b_1()
            .border_color(colors.border.opacity(0.5))
            .bg(colors.muted.opacity(0.25))
            .child(
                div()
                    .px(px(8.0))
                    .pt(px(10.0))
                    .child(delivery_rail("remote-rail", steps, cx)),
            )
            .child(
                div()
                    .flex()
                    .min_h(px(28.0))
                    .items_center()
                    .gap(px(8.0))
                    .px(px(12.0))
                    .pt(px(8.0))
                    .pb(px(10.0))
                    .child(
                        div()
                            .debug_selector(|| "remote-standing".into())
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(12.0))
                            .font_weight(FontWeight::MEDIUM)
                            .child(standing),
                    )
                    .children(button),
            )
            .into_any_element(),
    )
}

/// "Commitar e enviar na máquina?": what the step does there, then yes.
pub(crate) fn confirm_dialog(
    column: &CodeColumn,
    _window: &Window,
    cx: &mut Context<CodeColumn>,
) -> Option<AnyElement> {
    let step = column.remote.confirm?;
    let changes = read(column)?;
    let colors = cx.theme().colors.clone();
    let key = |what: &str| format!("codeColumn.remote.confirm.{}.{what}", step.key());
    let args = Args::count(changes.changes.len() as i64)
        .with("message", changes.commit_message.clone())
        .with(
            "title",
            column.remote.request_title.clone().unwrap_or_default(),
        )
        .with("branch", changes.branch.clone().unwrap_or_default())
        .with("base", changes.base_branch.clone());
    let header = div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(
            div()
                .text_size(px(16.0))
                .line_height(px(20.0))
                .font_weight(FontWeight::MEDIUM)
                .child(cx.t(&key("title"))),
        )
        .child(
            div()
                .text_size(px(12.0))
                .line_height(px(19.5))
                .text_color(colors.muted_foreground)
                .child(cx.t_with(&key("description"), &args)),
        );
    let footer = dialog_footer(
        [
            Button::new("remote-delivery-cancel")
                .variant(ButtonVariant::Outline)
                .size(ButtonSize::Default)
                .child(cx.t("inspector.gitConfirmation.cancel"))
                .on_click(cx.listener(|column, _, _, cx| column.cancel_remote_step(cx)))
                .into_any_element(),
            Button::new("remote-delivery-run")
                .variant(ButtonVariant::Default)
                .size(ButtonSize::Default)
                .child(cx.t(&format!("codeColumn.remote.steps.{}", step.key())))
                .on_click(cx.listener(|column, _, _, cx| column.run_remote_step(cx)))
                .into_any_element(),
        ],
        cx,
    );
    let close = cx.entity().downgrade();
    Some(
        Dialog::new("remote-delivery-dialog", move |_: &mut Window, cx| {
            let _ = close.update(cx, |column, cx| column.cancel_remote_step(cx));
        })
        .close_button(false)
        .child(header)
        .child(footer)
        .into_any_element(),
    )
}

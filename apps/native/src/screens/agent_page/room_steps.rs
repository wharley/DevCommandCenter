//! The folded line of work above an answer ("Trabalhou 4 min · leu 6
//! arquivos · rodou 3 comandos", Compozy's and Tucano's fold): counted when
//! the lines are built (`team_room_steps`), its text made once with the row
//! (`room_rows`). A click lists the steps — what each tool did and on what
//! — read from the conversation's events for that turn alone, and another
//! folds it. The open ones are kept here, so the row model holds what it
//! draws and only that row is measured again.

use std::rc::Rc;

use dcc_core::domain::team_room_steps::{Step, TurnSteps};
use gpui::{AnyElement, Context, IntoElement, SharedString, div, prelude::*, px};

use super::room::TeamRoom;
use crate::data::team_room::RoomLine;
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::ui::Icon;

/// How many kinds the folded line names.
const KINDS_SHOWN: usize = 3;
/// How many steps the opened list draws before "+N".
const STEPS_SHOWN: usize = 30;

/// The line as the row keeps it.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct StepsRow {
    pub label: SharedString,
    pub open: bool,
    pub items: Vec<StepItem>,
    /// Steps beyond what the list draws.
    pub more: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct StepItem {
    pub verb: SharedString,
    pub target: SharedString,
    pub failed: bool,
}

/// "42 s", "4 min", "1 h 5 min".
pub(crate) fn duration_text(seconds: i64) -> String {
    match seconds.max(0) {
        s if s < 60 => format!("{s} s"),
        s if s < 3600 => format!("{} min", s / 60),
        s => format!("{} h {} min", s / 3600, s % 3600 / 60),
    }
}

/// The turn id inside a line's key (`session:turn`).
fn turn_of(line: &RoomLine) -> Option<&str> {
    line.key
        .strip_prefix(line.session_id.as_str())
        .and_then(|rest| rest.strip_prefix(':'))
}

fn label(steps: &TurnSteps, seconds: Option<i64>, cx: &gpui::App) -> SharedString {
    let mut parts = vec![match seconds {
        Some(seconds) => cx
            .t_with(
                "agents.room.steps.worked",
                &Args::default().with("duration", duration_text(seconds)),
            )
            .to_string(),
        None => cx.t("agents.room.steps.workedNoDuration").to_string(),
    }];
    for (kind, count) in steps.counts.iter().take(KINDS_SHOWN) {
        parts.push(
            cx.t_with(
                &format!("conversation.activity.turn.parts.{}", kind.key()),
                &Args::count(*count as i64),
            )
            .to_string(),
        );
    }
    if steps.failures > 0 {
        parts.push(
            cx.t_with(
                "agents.room.steps.failed",
                &Args::count(i64::from(steps.failures)),
            )
            .to_string(),
        );
    }
    parts.join(" · ").into()
}

impl TeamRoom {
    /// The folded line of `line`'s row; none when the turn used no tool.
    pub(super) fn steps_row(&self, line: &RoomLine, cx: &gpui::App) -> Option<Rc<StepsRow>> {
        if line.steps.is_empty() {
            return None;
        }
        let seconds = line.receipt.and_then(|receipt| receipt.seconds);
        let mut row = StepsRow {
            label: label(&line.steps, seconds, cx),
            ..StepsRow::default()
        };
        if let Some(steps) = self.steps_open.get(&line.key) {
            row.open = true;
            row.more = steps.len().saturating_sub(STEPS_SHOWN);
            row.items = steps
                .iter()
                .take(STEPS_SHOWN)
                .map(|step| StepItem {
                    verb: cx.t(&format!(
                        "conversation.toolCall.verbs.{}.done",
                        step.kind.key()
                    )),
                    target: step.target.clone().unwrap_or_default().into(),
                    failed: step.failed,
                })
                .collect();
        }
        Some(Rc::new(row))
    }

    /// Opens or folds the steps of `line`, listing them from the events.
    fn toggle_steps(&mut self, line: &RoomLine, cx: &mut Context<Self>) {
        if self.steps_open.remove(&line.key).is_none() {
            let steps: Vec<Step> = turn_of(line)
                .and_then(|turn| {
                    self.feeds
                        .iter()
                        .find(|feed| feed.session_id == line.session_id)
                        .map(|feed| feed.steps_of(turn))
                })
                .unwrap_or_default();
            self.steps_open.insert(line.key.clone(), Rc::new(steps));
        }
        // The change reaches the row model, which measures this row again.
        cx.notify();
    }

    /// The folded line, with its list under it while open.
    pub(super) fn steps_line(
        &self,
        index: usize,
        line: &RoomLine,
        steps: &StepsRow,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let line = line.clone();
        let items: Vec<AnyElement> = steps
            .items
            .iter()
            .enumerate()
            .map(|(n, item)| {
                div()
                    .debug_selector(move || format!("room-step-{index}-{n}"))
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .min_w_0()
                    .text_size(px(11.5))
                    .text_color(if item.failed {
                        colors.destructive
                    } else {
                        colors.muted_foreground
                    })
                    .child(div().flex_none().child(item.verb.clone()))
                    .child(
                        div()
                            .truncate()
                            .text_color(if item.failed {
                                colors.destructive
                            } else {
                                colors.foreground.opacity(0.75)
                            })
                            .child(item.target.clone()),
                    )
                    .when(item.failed, |row| {
                        row.child(
                            Icon::new("circle-alert")
                                .size(11.0)
                                .color(colors.destructive),
                        )
                    })
                    .into_any_element()
            })
            .collect();
        div()
            .flex()
            .flex_col()
            .gap(px(3.0))
            .child(
                div()
                    .id(SharedString::from(format!("room-steps-{index}")))
                    .debug_selector(move || format!("room-steps-{index}"))
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .cursor_pointer()
                    .text_size(px(11.5))
                    .text_color(colors.muted_foreground)
                    .hover(|row| row.text_color(colors.foreground))
                    .on_click(cx.listener(move |room, _, _, cx| room.toggle_steps(&line, cx)))
                    .child(
                        Icon::new(if steps.open {
                            "chevron-down"
                        } else {
                            "chevron-right"
                        })
                        .size(12.0)
                        .color(colors.muted_foreground),
                    )
                    .child(div().truncate().child(steps.label.clone())),
            )
            .when(steps.open, |column| {
                column.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .pl(px(16.0))
                        .children(items)
                        .when(steps.more > 0, |list| {
                            list.child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(colors.muted_foreground)
                                    .child(cx.t_with(
                                        "agents.room.steps.more",
                                        &Args::count(steps.more as i64),
                                    )),
                            )
                        }),
                )
            })
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_duration_reads_in_its_largest_unit() {
        assert_eq!(duration_text(0), "0 s");
        assert_eq!(duration_text(42), "42 s");
        assert_eq!(duration_text(252), "4 min");
        assert_eq!(duration_text(3900), "1 h 5 min");
    }
}

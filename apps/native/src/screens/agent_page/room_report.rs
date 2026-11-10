//! The report under an agent's answer to the tasks it opened (Monocode's
//! completion report, drawn by DCC from what it keeps — no model writes
//! it): the totals on top, then one row per task with whose it is, where it
//! stands, how long it ran and a bar of the lines it added and removed,
//! scaled to the largest of them. A row opens its task; under it, its files
//! and its delivery (`room_change_card`).

use gpui::{AnyElement, Context, FontWeight, Hsla, SharedString, div, prelude::*, px};

use super::room::{RoomEvent, TeamRoom};
use crate::agent::AgentAvatarView;
use crate::data::task_reports::{ReportState, TaskReport, duration_label};
use crate::data::team_room::RoomLine;
use crate::i18n::{Args, Translate};
use crate::project::ProviderIcon;
use crate::theme::ActiveTheme;
use crate::theme::tailwind as tw;
use crate::ui::diff_squares::diff_squares;
use crate::ui::{Icon, tabular_nums};

/// The widest bar, for the task that changed the most lines.
const BAR_WIDTH: f32 = 120.0;

fn state_color(state: ReportState) -> Hsla {
    match state {
        ReportState::Working => tw::SKY_600.hsla(),
        ReportState::TurnEnded => tw::EMERALD_600.hsla(),
        ReportState::Closed => tw::EMERALD_600.hsla(),
        ReportState::Failed => tw::ROSE_600.hsla(),
    }
}

impl TeamRoom {
    /// The report of the tasks `line` answers, if it answers any we know.
    pub(super) fn report_card(
        &self,
        index: usize,
        line: &RoomLine,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let sidebar = self.sidebar.read(cx);
        let tasks: Vec<TaskReport> = line
            .tasks
            .iter()
            .filter_map(|id| sidebar.task_report(id).cloned())
            .collect();
        if tasks.is_empty() {
            return None;
        }
        let agents = sidebar.agents().to_vec();
        let colors = cx.theme().colors.clone();
        let added: u64 = tasks.iter().map(|task| task.additions).sum();
        let removed: u64 = tasks.iter().map(|task| task.deletions).sum();
        let files: usize = tasks.iter().map(|task| task.files).sum();
        let longest = tasks.iter().filter_map(|task| task.seconds).max();
        let widest = tasks
            .iter()
            .map(|task| task.additions + task.deletions)
            .max()
            .unwrap_or(0)
            .max(1);
        let green = tw::EMERALD_600.hsla();
        let red = tw::ROSE_600.hsla();
        let totals = div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .text_size(px(11.5))
            .font_features(tabular_nums())
            .text_color(colors.muted_foreground)
            .child(div().text_color(green).child(format!("+{added}")))
            .child(div().text_color(red).child(format!("−{removed}")))
            .child(diff_squares(added, removed, 7.0, (green, red), cx))
            .child(cx.t_with("agents.report.files", &Args::count(files as i64)))
            .children(longest.map(|seconds| format!("· {}", duration_label(seconds))));
        let head = div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(
                Icon::new("chart-column")
                    .size(14.0)
                    .color(colors.muted_foreground),
            )
            .child(
                div()
                    .flex_1()
                    .text_size(px(12.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(colors.foreground)
                    .child(cx.t_with("agents.report.title", &Args::count(tasks.len() as i64))),
            )
            .child(totals);
        let rows: Vec<AnyElement> = tasks
            .iter()
            .enumerate()
            .map(|(row, task)| {
                let agent = task
                    .agent_id
                    .as_deref()
                    .and_then(|id| agents.iter().find(|agent| agent.id == id));
                let lines = task.additions + task.deletions;
                let width = BAR_WIDTH * lines as f32 / widest as f32;
                let plus = if lines == 0 {
                    0.0
                } else {
                    width * task.additions as f32 / lines as f32
                };
                let state = state_color(task.state);
                let open = task.task_id.clone();
                let who = div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .child(ProviderIcon::new(&task.provider_id).size(12.0))
                    .children(agent.map(|agent| {
                        div()
                            .ml(px(-4.0))
                            .rounded_full()
                            .bg(colors.card)
                            .p(px(1.0))
                            .child(AgentAvatarView::new(
                                SharedString::from(format!("room-report-face-{index}-{row}")),
                                &agent.avatar,
                                None,
                                14.0,
                            ))
                    }));
                let detail = [
                    Some(cx.t(task.state.i18n_key()).to_string()),
                    task.seconds.map(duration_label),
                    (task.files > 0).then(|| {
                        cx.t_with("agents.report.files", &Args::count(task.files as i64))
                            .to_string()
                    }),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" · ");
                let line = div()
                    .id(SharedString::from(format!("room-report-{index}-{row}")))
                    .debug_selector(move || format!("room-report-{index}-{row}"))
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .px(px(8.0))
                    .py(px(6.0))
                    .rounded(px(8.0))
                    .when(open.is_some(), |row| {
                        row.cursor_pointer()
                            .hover(|row| row.bg(colors.accent.opacity(0.5)))
                    })
                    .on_click(cx.listener(move |_, _, _, cx| {
                        if let Some(task) = open.clone() {
                            cx.emit(RoomEvent::OpenTask(task));
                        }
                    }))
                    .child(who)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(12.5))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(colors.foreground)
                                    .child(task.title.clone()),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .text_size(px(11.0))
                                    .text_color(colors.muted_foreground)
                                    .child(div().flex_none().size(px(6.0)).rounded_full().bg(state))
                                    .child(div().truncate().child(detail)),
                            ),
                    )
                    // The lines it changed, as a bar against the largest.
                    .child(
                        div()
                            .flex()
                            .flex_none()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .flex()
                                    .w(px(BAR_WIDTH))
                                    .h(px(6.0))
                                    .rounded_full()
                                    .bg(colors.muted.opacity(0.6))
                                    .overflow_hidden()
                                    .child(div().h_full().w(px(plus)).bg(green))
                                    .child(div().h_full().w(px(width - plus)).bg(red)),
                            )
                            .child(
                                div()
                                    .w(px(72.0))
                                    .flex()
                                    .justify_end()
                                    .gap(px(4.0))
                                    .text_size(px(11.0))
                                    .font_features(tabular_nums())
                                    .child(
                                        div()
                                            .text_color(green)
                                            .child(format!("+{}", task.additions)),
                                    )
                                    .child(
                                        div().text_color(red).child(format!("−{}", task.deletions)),
                                    ),
                            ),
                    );
                // Its files and where its delivery stands (`room_change_card`).
                div()
                    .flex()
                    .flex_col()
                    .child(line)
                    .children(self.change_block(index, row, task, cx))
                    .into_any_element()
            })
            .collect();
        Some(
            div()
                .debug_selector(move || format!("room-report-{index}"))
                .mt(px(6.0))
                .flex()
                .flex_col()
                .gap(px(6.0))
                .p(px(12.0))
                .rounded(px(12.0))
                .border_1()
                .border_color(colors.border.opacity(0.7))
                .bg(colors.card)
                .child(head)
                .child(div().flex().flex_col().children(rows))
                .into_any_element(),
        )
    }
}

//! The day's recap as a card in the team room (`DailyRecapCard`): one line
//! "Resumo desde seg 09:00", a dot while unread; opening it reads it and
//! shows what blocks the person now, then what moved since the previous
//! recap — tasks with finished or stopped turns, completed and created ones
//! — read from what DCC recorded, with no model. A row opens its task.

use std::collections::HashMap;

use gpui::{AnyElement, Context, FontWeight, SharedString, div, prelude::*, px};

use super::room::{RoomEvent, TeamRoom};
use crate::data::time::weekday_time;
use crate::i18n::{Args, I18n, Locale, Translate};
use crate::screens::sidebar::reason_label;
use crate::theme::tailwind as tw;
use crate::theme::{ActiveTheme, Appearance};
use crate::ui::{Icon, pulse_spinner};
use dcc_infra::db::ActivityRecap;

/// The recap read for a window: `None` while reading.
#[derive(Default)]
pub(crate) struct RecapRead {
    /// `from|to` of the window it was read for.
    pub(crate) key: Option<String>,
    pub(crate) result: Option<Result<ActivityRecap, String>>,
}

struct Row {
    workspace_id: String,
    title: String,
    detail: String,
    attention: bool,
}

impl TeamRoom {
    /// The window's recap, read once per window off the UI thread.
    fn ensure_recap(&mut self, from: &str, to: &str, cx: &mut Context<Self>) {
        let key = format!("{from}|{to}");
        if self.recap.key.as_deref() == Some(key.as_str()) {
            return;
        }
        self.recap = RecapRead {
            key: Some(key.clone()),
            result: None,
        };
        let (path, from, to) = (self.db_path.clone(), from.to_string(), to.to_string());
        cx.spawn(async move |this, cx| {
            let read = cx
                .background_executor()
                .spawn(async move {
                    dcc_infra::db::SqliteSessionRepo::open_read_only(&path)
                        .and_then(|sessions| sessions.activity_recap(&from, &to))
                        .map_err(|error| error.to_string())
                })
                .await;
            let _ = this.update(cx, |room, cx| {
                if room.recap.key.as_deref() == Some(key.as_str()) {
                    room.recap.result = Some(read);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Opens or closes the recap; opening it reads it.
    pub(crate) fn toggle_recap(&mut self, cx: &mut Context<Self>) {
        self.recap_open = !self.recap_open;
        if self.recap_open {
            self.sidebar
                .update(cx, |sidebar, cx| sidebar.mark_recap_seen(cx));
        }
        cx.notify();
    }

    fn recap_rows(&self, rows: Vec<Row>, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let colors = cx.theme().colors.clone();
        let dark = cx.theme().appearance == Appearance::Dark;
        let amber = if dark { tw::AMBER_300 } else { tw::AMBER_700 }.hsla();
        rows.into_iter()
            .map(|row| {
                let id = row.workspace_id.clone();
                div()
                    .id(SharedString::from(format!(
                        "recap-row-{}",
                        row.workspace_id
                    )))
                    .debug_selector({
                        let id = row.workspace_id.clone();
                        move || format!("recap-row-{id}")
                    })
                    .flex()
                    .flex_col()
                    .w_full()
                    .px(px(10.0))
                    .py(px(6.0))
                    .rounded(px(8.0))
                    .cursor_pointer()
                    .hover(|row| row.bg(colors.accent.opacity(0.5)))
                    .on_click(
                        cx.listener(move |_, _, _, cx| cx.emit(RoomEvent::OpenTask(id.clone()))),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(12.5))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(colors.foreground)
                            .child(row.title),
                    )
                    .when(!row.detail.is_empty(), |item| {
                        item.child(
                            div()
                                .truncate()
                                .text_size(px(11.0))
                                .text_color(if row.attention {
                                    amber
                                } else {
                                    colors.muted_foreground
                                })
                                .child(row.detail),
                        )
                    })
                    .into_any_element()
            })
            .collect()
    }

    fn recap_section(
        &self,
        title: SharedString,
        rows: Vec<Row>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        (!rows.is_empty()).then(|| {
            let muted = cx.theme().colors.muted_foreground;
            div()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(
                    div()
                        .px(px(10.0))
                        .text_size(px(10.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(muted)
                        .child(title.to_uppercase()),
                )
                .children(self.recap_rows(rows, cx))
                .into_any_element()
        })
    }

    /// The recap's card, while there is a window.
    pub(super) fn recap_card(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let window = self.sidebar.read(cx).recap_window()?;
        let colors = cx.theme().colors.clone();
        let english = cx.global::<I18n>().locale() == Locale::En;
        let unread = self.sidebar.read(cx).recap_unread();
        let from = weekday_time(&window.from, english);
        let head = div()
            .id("room-recap")
            .debug_selector(|| "room-recap".into())
            .flex()
            .items_center()
            .gap(px(10.0))
            .px(px(12.0))
            .py(px(8.0))
            .cursor_pointer()
            .hover(|row| row.bg(colors.accent.opacity(0.4)))
            .on_click(cx.listener(|room, _, _, cx| room.toggle_recap(cx)))
            .child(
                Icon::new("newspaper")
                    .size(18.0)
                    .color(colors.muted_foreground),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(12.5))
                    .font_weight(if unread {
                        FontWeight::MEDIUM
                    } else {
                        FontWeight::NORMAL
                    })
                    .text_color(colors.foreground)
                    .child(cx.t_with(
                        "agents.recap.title",
                        &Args::default().with("from", from.clone()),
                    )),
            )
            .children(unread.then(|| {
                div()
                    .flex_none()
                    .size(px(7.0))
                    .rounded_full()
                    .bg(tw::SKY_500.hsla())
            }))
            .child(
                Icon::new(if self.recap_open {
                    "chevron-up"
                } else {
                    "chevron-down"
                })
                .size(14.0)
                .color(colors.muted_foreground),
            );
        let body = self.recap_open.then(|| {
            self.ensure_recap(&window.from, &window.to, cx);
            self.recap_body(&from, english, cx)
        });
        Some(
            div()
                .flex()
                .flex_col()
                .rounded(px(12.0))
                .border_1()
                .border_color(colors.border.opacity(0.7))
                .bg(colors.card)
                .overflow_hidden()
                .child(head)
                .children(body)
                .into_any_element(),
        )
    }

    fn recap_body(&mut self, from: &str, english: bool, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let sidebar = self.sidebar.read(cx);
        let names: HashMap<String, String> = sidebar
            .tasks()
            .iter()
            .map(|task| (task.id.clone(), task.name.clone()))
            .collect();
        let labels: HashMap<String, String> = sidebar
            .projects()
            .iter()
            .map(|project| (project.project_id.clone(), project.display_name.clone()))
            .collect();
        let projects: HashMap<String, String> = sidebar
            .tasks()
            .iter()
            .filter_map(|task| {
                labels
                    .get(&task.project_id)
                    .map(|label| (task.id.clone(), label.clone()))
            })
            .collect();
        let blocked_items = sidebar.recap_blocked();
        let removed = cx.t("agents.recap.removedTask").to_string();
        let title_of = |id: &str| names.get(id).cloned().unwrap_or_else(|| removed.clone());
        let project_of = |id: &str| projects.get(id).cloned();
        let blocked: Vec<Row> = blocked_items
            .into_iter()
            .map(|item| {
                let detail: Vec<String> = [
                    Some(reason_label(item.reason, item.count, cx).to_string()),
                    item.returned_at.as_deref().map(|at| {
                        cx.t_with(
                            "agents.recap.since",
                            &Args::default().with("time", weekday_time(at, english)),
                        )
                        .to_string()
                    }),
                    project_of(&item.workspace_id),
                ]
                .into_iter()
                .flatten()
                .collect();
                Row {
                    title: title_of(&item.workspace_id),
                    workspace_id: item.workspace_id,
                    detail: detail.join(" · "),
                    attention: true,
                }
            })
            .collect();
        let mut sections: Vec<AnyElement> = Vec::new();
        sections.extend(self.recap_section(cx.t("agents.recap.blocked"), blocked, cx));
        let state: Option<AnyElement> = match &self.recap.result {
            None => Some(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .px(px(10.0))
                    .text_size(px(12.0))
                    .text_color(colors.muted_foreground)
                    .child(pulse_spinner("recap-loading", 14.0))
                    .child(cx.t("agents.recap.loading"))
                    .into_any_element(),
            ),
            Some(Err(_)) => Some(
                div()
                    .px(px(10.0))
                    .text_size(px(12.0))
                    .text_color(colors.destructive)
                    .child(cx.t("agents.recap.failed"))
                    .into_any_element(),
            ),
            Some(Ok(recap)) => {
                let recap = recap.clone();
                let moved: Vec<Row> = recap
                    .tasks
                    .iter()
                    .map(|task| {
                        let detail: Vec<String> = [
                            (task.completed_turns > 0).then(|| {
                                cx.t_with(
                                    "agents.recap.turns",
                                    &Args::count(task.completed_turns as i64),
                                )
                                .to_string()
                            }),
                            (task.aborted_turns > 0).then(|| {
                                cx.t_with(
                                    "agents.recap.aborted",
                                    &Args::count(task.aborted_turns as i64),
                                )
                                .to_string()
                            }),
                            project_of(&task.workspace_id),
                            Some(weekday_time(&task.last_activity_at, english)),
                        ]
                        .into_iter()
                        .flatten()
                        .collect();
                        Row {
                            workspace_id: task.workspace_id.clone(),
                            title: title_of(&task.workspace_id),
                            detail: detail.join(" · "),
                            attention: false,
                        }
                    })
                    .collect();
                let list = |ids: &[String]| -> Vec<Row> {
                    ids.iter()
                        .map(|id| Row {
                            workspace_id: id.clone(),
                            title: title_of(id),
                            detail: project_of(id).unwrap_or_default(),
                            attention: false,
                        })
                        .collect()
                };
                let completed = list(&recap.completed_workspace_ids);
                let created = list(&recap.created_workspace_ids);
                if moved.is_empty() && completed.is_empty() && created.is_empty() {
                    Some(
                        div()
                            .debug_selector(|| "recap-empty".into())
                            .px(px(10.0))
                            .text_size(px(12.0))
                            .text_color(colors.muted_foreground)
                            .child(cx.t_with(
                                "agents.recap.empty",
                                &Args::default().with("from", from.to_string()),
                            ))
                            .into_any_element(),
                    )
                } else {
                    sections.extend(self.recap_section(cx.t("agents.recap.moved"), moved, cx));
                    sections.extend(self.recap_section(
                        cx.t("agents.recap.completed"),
                        completed,
                        cx,
                    ));
                    sections.extend(self.recap_section(cx.t("agents.recap.created"), created, cx));
                    None
                }
            }
        };
        div()
            .debug_selector(|| "recap-card".into())
            .flex()
            .flex_col()
            .gap(px(12.0))
            .px(px(4.0))
            .pt(px(4.0))
            .pb(px(10.0))
            .border_t_1()
            .border_color(colors.border.opacity(0.6))
            .children(sections)
            .children(state)
            .child(
                div()
                    .px(px(10.0))
                    .text_size(px(11.0))
                    .line_height(px(17.0))
                    .text_color(colors.muted_foreground)
                    .child(cx.t("agents.recap.howItWorks")),
            )
            .into_any_element()
    }
}

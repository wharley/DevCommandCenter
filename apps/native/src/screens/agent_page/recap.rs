//! The Reporter's page (`DailyRecapCard`): what blocks the person now, then
//! what moved since the previous recap — tasks with finished or stopped
//! turns, completed and created ones — read from what DCC recorded, with
//! no model. Seeing it marks the recap as read; a row opens its task.

use gpui::{AnyElement, Context, FontWeight, SharedString, div, prelude::*, px};

use super::{AgentPage, AgentPageEvent};
use crate::data::time::weekday_time;
use crate::i18n::{Args, I18n, Locale, Translate};
use crate::screens::sidebar::reason_label;
use crate::theme::tailwind as tw;
use crate::theme::{ActiveTheme, Appearance};
use crate::ui::spinner;
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

impl AgentPage {
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
            let _ = this.update(cx, |page, cx| {
                if page.recap.key.as_deref() == Some(key.as_str()) {
                    page.recap.result = Some(read);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn recap_list(&self, rows: Vec<Row>, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let dark = cx.theme().appearance == Appearance::Dark;
        let amber = if dark { tw::AMBER_300 } else { tw::AMBER_700 }.hsla();
        let items: Vec<AnyElement> =
            rows.into_iter()
                .enumerate()
                .map(|(index, row)| {
                    let id = row.workspace_id.clone();
                    div()
                        .id(SharedString::from(format!(
                            "recap-row-{}-{index}",
                            row.workspace_id
                        )))
                        .debug_selector({
                            let id = row.workspace_id.clone();
                            move || format!("recap-row-{id}")
                        })
                        .flex()
                        .flex_col()
                        .w_full()
                        .px(px(16.0))
                        .py(px(12.0))
                        .cursor_pointer()
                        .hover(|row| row.bg(colors.accent.opacity(0.5)))
                        .when(index > 0, |item| {
                            item.border_t_1().border_color(colors.border.opacity(0.6))
                        })
                        .on_click(cx.listener(move |_, _, _, cx| {
                            cx.emit(AgentPageEvent::OpenTask(id.clone()))
                        }))
                        .child(
                            div()
                                .truncate()
                                .text_size(px(13.0))
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
                .collect();
        Self::card(cx)
            .overflow_hidden()
            .children(items)
            .into_any_element()
    }

    fn recap_section(
        &self,
        title: SharedString,
        rows: Vec<Row>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        (!rows.is_empty()).then(|| {
            div()
                .child(Self::section_title(title, cx))
                .child(self.recap_list(rows, cx))
                .into_any_element()
        })
    }

    pub(super) fn recap_card(
        &mut self,
        data: &crate::screens::sidebar::AgentPageData,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let english = cx.global::<I18n>().locale() == Locale::En;
        let sidebar = self.sidebar.clone();
        let window = sidebar.read(cx).recap_window();
        // Seeing this page reads the recap.
        if sidebar.read(cx).recap_unread() {
            cx.defer(move |cx| sidebar.update(cx, |sidebar, cx| sidebar.mark_recap_seen(cx)));
        }
        if let Some(window) = &window {
            self.ensure_recap(&window.from, &window.to, cx);
        }
        let from = window
            .as_ref()
            .map(|window| weekday_time(&window.from, english))
            .unwrap_or_default();
        let project_of = |id: &str, cx: &gpui::App| {
            self.sidebar
                .read(cx)
                .task_project(id)
                .and_then(|project| data.project_labels.get(&project).cloned())
        };
        let title_of = |id: &str, cx: &gpui::App| {
            data.task_names
                .get(id)
                .cloned()
                .unwrap_or_else(|| cx.t("agents.recap.removedTask").to_string())
        };
        let blocked: Vec<Row> = self
            .sidebar
            .read(cx)
            .recap_blocked()
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
                    project_of(&item.workspace_id, cx),
                ]
                .into_iter()
                .flatten()
                .collect();
                Row {
                    title: title_of(&item.workspace_id, cx),
                    workspace_id: item.workspace_id,
                    detail: detail.join(" · "),
                    attention: true,
                }
            })
            .collect();
        let state: AnyElement = match &self.recap.result {
            None => div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .text_size(px(13.0))
                .text_color(colors.muted_foreground)
                .child(spinner("recap-loading", 16.0))
                .child(cx.t("agents.recap.loading"))
                .into_any_element(),
            Some(Err(_)) => div()
                .text_size(px(13.0))
                .text_color(colors.destructive)
                .child(cx.t("agents.recap.failed"))
                .into_any_element(),
            Some(Ok(recap)) => {
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
                            project_of(&task.workspace_id, cx),
                            Some(weekday_time(&task.last_activity_at, english)),
                        ]
                        .into_iter()
                        .flatten()
                        .collect();
                        Row {
                            workspace_id: task.workspace_id.clone(),
                            title: title_of(&task.workspace_id, cx),
                            detail: detail.join(" · "),
                            attention: false,
                        }
                    })
                    .collect();
                let list = |ids: &[String], cx: &gpui::App| -> Vec<Row> {
                    ids.iter()
                        .map(|id| Row {
                            workspace_id: id.clone(),
                            title: title_of(id, cx),
                            detail: project_of(id, cx).unwrap_or_default(),
                            attention: false,
                        })
                        .collect()
                };
                let completed = list(&recap.completed_workspace_ids, cx);
                let created = list(&recap.created_workspace_ids, cx);
                if moved.is_empty() && completed.is_empty() && created.is_empty() {
                    div()
                        .debug_selector(|| "recap-empty".into())
                        .rounded(px(18.0))
                        .border_1()
                        .border_dashed()
                        .border_color(colors.border.opacity(0.7))
                        .px(px(20.0))
                        .py(px(24.0))
                        .text_size(px(13.0))
                        .text_color(colors.muted_foreground)
                        .child(cx.t_with(
                            "agents.recap.empty",
                            &Args::default().with("from", from.clone()),
                        ))
                        .into_any_element()
                } else {
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(24.0))
                        .children(self.recap_section(cx.t("agents.recap.moved"), moved, cx))
                        .children(self.recap_section(cx.t("agents.recap.completed"), completed, cx))
                        .children(self.recap_section(cx.t("agents.recap.created"), created, cx))
                        .into_any_element()
                }
            }
        };
        div()
            .debug_selector(|| "recap-card".into())
            .flex()
            .flex_col()
            .gap(px(24.0))
            .child(
                div()
                    .child(
                        div()
                            .text_size(px(15.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(
                                cx.t_with(
                                    "agents.recap.title",
                                    &Args::default().with("from", from),
                                ),
                            ),
                    )
                    .child(
                        div()
                            .mt(px(4.0))
                            .text_size(px(12.0))
                            .line_height(px(19.5))
                            .text_color(colors.muted_foreground)
                            .child(cx.t("agents.recap.howItWorks")),
                    ),
            )
            .children(self.recap_section(cx.t("agents.recap.blocked"), blocked, cx))
            .child(state)
            .into_any_element()
    }
}

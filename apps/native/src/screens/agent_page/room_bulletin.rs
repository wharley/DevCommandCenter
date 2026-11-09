//! The weekly bulletin in the team room (`dcc_core::domain::agent_bulletin`):
//! one card, "Boletim da semana · 2–8 nov", a dot while that week is
//! unread; opened, each agent that worked shows its answers, tasks, stopped
//! turns, tokens, the cost the providers reported and the triggers that
//! woke it, next to the week before. Read once per week off the UI thread
//! when the card opens, with no model.

use chrono::{Datelike, Duration, Local};
use dcc_core::domain::agent_bulletin::{AgentWeek, Week, trend, week_before};
use gpui::{AnyElement, Context, FontWeight, SharedString, div, prelude::*, px};

use super::editor_budget::tokens_short;
use super::room::TeamRoom;
use crate::agent::AgentAvatarView;
use crate::data::agent_triggers::AgentTriggers;
use crate::data::agents::AgentRecord;
use crate::i18n::{Args, I18n, Locale, Translate};
use crate::theme::ActiveTheme;
use crate::theme::tailwind as tw;
use crate::ui::{Icon, spinner};

/// The two weeks as read, for the week they were read for.
#[derive(Clone, Debug, Default)]
pub(crate) struct BulletinRead {
    pub(crate) key: Option<String>,
    pub(crate) this: Option<Vec<AgentWeek>>,
    pub(crate) before: Vec<AgentWeek>,
}

const MONTHS_PT: [&str; 12] = [
    "jan", "fev", "mar", "abr", "mai", "jun", "jul", "ago", "set", "out", "nov", "dez",
];
const MONTHS_EN: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// `2–8 nov` / `Nov 2–8`; across months, `27 out–2 nov`.
pub(crate) fn week_label(week: &Week, english: bool) -> String {
    let (start, end) = (week.monday, week.monday + Duration::days(6));
    let months = if english { MONTHS_EN } else { MONTHS_PT };
    let month = |at: chrono::DateTime<Local>| months[at.month0() as usize];
    match (english, start.month() == end.month()) {
        (false, true) => format!("{}–{} {}", start.day(), end.day(), month(end)),
        (false, false) => format!(
            "{} {}–{} {}",
            start.day(),
            month(start),
            end.day(),
            month(end)
        ),
        (true, true) => format!("{} {}–{}", month(start), start.day(), end.day()),
        (true, false) => format!(
            "{} {}–{} {}",
            month(start),
            start.day(),
            month(end),
            end.day()
        ),
    }
}

fn runs_between(agent_id: &str, week: &Week, cx: &gpui::App) -> usize {
    cx.try_global::<AgentTriggers>().map_or(0, |triggers| {
        triggers
            .runs()
            .iter()
            .filter(|run| run.agent_id == agent_id && run.at.as_str() >= week.from.as_str())
            .filter(|run| run.at.as_str() < week.to.as_str())
            .count()
    })
}

impl TeamRoom {
    fn ensure_bulletin(&mut self, cx: &mut Context<Self>) {
        let now = Local::now();
        let week = week_before(now, 1);
        if self.bulletin.key.as_deref() == Some(week.key.as_str()) {
            return;
        }
        self.bulletin = BulletinRead {
            key: Some(week.key.clone()),
            ..BulletinRead::default()
        };
        let path = self.db_path.clone();
        let before = week_before(now, 2);
        cx.spawn(async move |room, cx| {
            let read = cx
                .background_executor()
                .spawn(async move {
                    let repo = dcc_infra::db::SqliteSessionRepo::open_read_only(&path).ok()?;
                    Some((
                        repo.agents_week(&week.from, &week.to).ok()?,
                        repo.agents_week(&before.from, &before.to)
                            .unwrap_or_default(),
                    ))
                })
                .await;
            let _ = room.update(cx, |room, cx| {
                let (this, before) = read.unwrap_or_default();
                room.bulletin.this = Some(this);
                room.bulletin.before = before;
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn toggle_bulletin(&mut self, cx: &mut Context<Self>) {
        self.bulletin_open = !self.bulletin_open;
        if self.bulletin_open {
            self.sidebar
                .update(cx, |sidebar, cx| sidebar.mark_bulletin_seen(cx));
        }
        cx.notify();
    }

    fn bulletin_row(
        &self,
        index: usize,
        agent: &AgentRecord,
        week: &AgentWeek,
        before: Option<&AgentWeek>,
        runs: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let mut parts: Vec<String> = vec![
            cx.t_with("agents.bulletin.answers", &Args::count(week.answers as i64))
                .to_string(),
            cx.t_with("agents.bulletin.tasks", &Args::count(week.tasks as i64))
                .to_string(),
        ];
        if week.stopped > 0 {
            parts.push(
                cx.t_with("agents.bulletin.stopped", &Args::count(week.stopped as i64))
                    .to_string(),
            );
        }
        parts.push(
            cx.t_with(
                "agents.spend.tokens",
                &Args::default().with("tokens", tokens_short(week.tokens)),
            )
            .to_string(),
        );
        if let Some(cost) = week.cost_usd {
            parts.push(format!("US$ {cost:.2}"));
        }
        if runs > 0 {
            parts.push(
                cx.t_with("agents.bulletin.triggers", &Args::count(runs as i64))
                    .to_string(),
            );
        }
        let base = before.cloned().unwrap_or_default();
        let change = |now: u64, then: u64| {
            trend(now, then).map(|percent| {
                if percent >= 0 {
                    format!("↑{percent}%")
                } else {
                    format!("↓{}%", -percent)
                }
            })
        };
        let compare: Vec<String> = [
            change(week.answers as u64, base.answers as u64).map(|change| {
                cx.t_with(
                    "agents.bulletin.answersTrend",
                    &Args::default().with("change", change),
                )
                .to_string()
            }),
            change(week.tokens, base.tokens).map(|change| {
                cx.t_with(
                    "agents.bulletin.tokensTrend",
                    &Args::default().with("change", change),
                )
                .to_string()
            }),
        ]
        .into_iter()
        .flatten()
        .collect();
        div()
            .debug_selector(move || format!("room-bulletin-agent-{index}"))
            .flex()
            .items_start()
            .gap(px(10.0))
            .px(px(10.0))
            .py(px(6.0))
            .child(AgentAvatarView::new(
                SharedString::from(format!("room-bulletin-face-{index}")),
                &agent.avatar,
                None,
                22.0,
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(12.5))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(colors.foreground)
                            .child(agent.name.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(colors.muted_foreground)
                            .child(parts.join(" · ")),
                    )
                    .when(!compare.is_empty(), |text| {
                        text.child(
                            div()
                                .text_size(px(11.0))
                                .text_color(colors.muted_foreground)
                                .child(compare.join(" · ")),
                        )
                    }),
            )
            .into_any_element()
    }

    /// The bulletin's card, collapsed until the person opens it.
    pub(super) fn bulletin_card(
        &mut self,
        agents: &[AgentRecord],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let english = cx.global::<I18n>().locale() == Locale::En;
        let now = Local::now();
        let week = week_before(now, 1);
        let unread = self.sidebar.read(cx).bulletin_unread();
        let head = div()
            .id("room-bulletin")
            .debug_selector(|| "room-bulletin".into())
            .flex()
            .items_center()
            .gap(px(10.0))
            .px(px(12.0))
            .py(px(8.0))
            .cursor_pointer()
            .hover(|row| row.bg(colors.accent.opacity(0.4)))
            .on_click(cx.listener(|room, _, _, cx| room.toggle_bulletin(cx)))
            .child(
                Icon::new("chart-column")
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
                        "agents.bulletin.title",
                        &Args::default().with("week", week_label(&week, english)),
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
                Icon::new(if self.bulletin_open {
                    "chevron-up"
                } else {
                    "chevron-down"
                })
                .size(14.0)
                .color(colors.muted_foreground),
            );
        let body = self.bulletin_open.then(|| {
            self.ensure_bulletin(cx);
            let content: AnyElement = match self.bulletin.this.clone() {
                None => div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .px(px(10.0))
                    .text_size(px(12.0))
                    .text_color(colors.muted_foreground)
                    .child(spinner("bulletin-loading", 14.0))
                    .child(cx.t("agents.bulletin.loading"))
                    .into_any_element(),
                Some(weeks) if weeks.is_empty() => div()
                    .debug_selector(|| "room-bulletin-empty".into())
                    .px(px(10.0))
                    .text_size(px(12.0))
                    .text_color(colors.muted_foreground)
                    .child(cx.t("agents.bulletin.empty"))
                    .into_any_element(),
                Some(weeks) => {
                    let before = self.bulletin.before.clone();
                    let rows: Vec<AnyElement> = agents
                        .iter()
                        .enumerate()
                        .filter_map(|(index, agent)| {
                            let this = weeks.iter().find(|week| week.agent_id == agent.id)?;
                            let base = before.iter().find(|week| week.agent_id == agent.id);
                            let runs = runs_between(&agent.id, &week, cx);
                            Some(self.bulletin_row(index, agent, this, base, runs, cx))
                        })
                        .collect();
                    div().flex().flex_col().children(rows).into_any_element()
                }
            };
            div()
                .debug_selector(|| "room-bulletin-body".into())
                .flex()
                .flex_col()
                .gap(px(8.0))
                .px(px(4.0))
                .pt(px(4.0))
                .pb(px(10.0))
                .border_t_1()
                .border_color(colors.border.opacity(0.6))
                .child(content)
                .child(
                    div()
                        .px(px(10.0))
                        .text_size(px(11.0))
                        .line_height(px(17.0))
                        .text_color(colors.muted_foreground)
                        .child(cx.t("agents.bulletin.howItWorks")),
                )
        });
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
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn the_week_reads_like_a_calendar() {
        let wednesday = Local.with_ymd_and_hms(2026, 11, 11, 15, 0, 0).unwrap();
        let week = week_before(wednesday, 1);
        assert_eq!(week_label(&week, false), "2–8 nov");
        assert_eq!(week_label(&week, true), "Nov 2–8");
        let across = week_before(wednesday, 2);
        assert_eq!(week_label(&across, false), "26 out–1 nov");
        assert_eq!(week_label(&across, true), "Oct 26–Nov 1");
    }
}

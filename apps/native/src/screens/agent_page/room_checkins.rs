//! The check-in's cards in the team room (`data::check_ins`): a pull
//! request asking for the person's review, an issue assigned to them, a
//! task with no sign of life — found with no model. A card opens where the
//! thing is in DCC (the pull request in Pull Requests, the task's
//! conversation; an issue, which DCC has no screen for, on GitHub), a pull
//! request's card keeps "GitHub ↗" beside it, and is read from then on.

use gpui::{AnyElement, BorrowAppContext, Context, FontWeight, SharedString, div, prelude::*, px};

use super::room::{RoomEvent, TeamRoom};
use crate::data::check_ins::{CheckIn, CheckInKind, CheckIns};
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::theme::tailwind as tw;
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon};

impl TeamRoom {
    pub(crate) fn check_ins(&self, cx: &gpui::App) -> Vec<CheckIn> {
        cx.try_global::<CheckIns>()
            .map(|checks| checks.cards().to_vec())
            .unwrap_or_default()
    }

    fn mark_check_in_read(&mut self, card: &CheckIn, cx: &mut Context<Self>) {
        if cx.has_global::<CheckIns>() {
            cx.update_global::<CheckIns, _>(|checks, _| checks.mark_read(&card.key));
            // The count beside "Agentes".
            self.sidebar.update(cx, |_, cx| cx.notify());
        }
    }

    fn open_check_in(&mut self, card: &CheckIn, cx: &mut Context<Self>) {
        self.mark_check_in_read(card, cx);
        match (&card.workspace_id, &card.session_id, &card.url) {
            (Some(task), Some(session), _) => cx.emit(RoomEvent::OpenSession {
                task_id: task.clone(),
                session_id: session.clone(),
            }),
            // Pull Requests finds it by its link, under "Todos".
            (_, _, Some(url)) if card.kind == CheckInKind::ReviewRequested => {
                cx.emit(RoomEvent::OpenPullRequest(url.clone()))
            }
            (_, _, Some(url)) => cx.open_url(url),
            _ => {}
        }
        cx.notify();
    }

    /// "GitHub ↗" on a pull request's card: the page itself, in the browser.
    fn open_check_in_on_github(&mut self, card: &CheckIn, cx: &mut Context<Self>) {
        self.mark_check_in_read(card, cx);
        if let Some(url) = &card.url {
            cx.open_url(url);
        }
        cx.notify();
    }

    fn check_in_github_button(
        &self,
        index: usize,
        card: &CheckIn,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if card.kind != CheckInKind::ReviewRequested || card.url.is_none() {
            return None;
        }
        let opened = card.clone();
        Some(
            Button::new(SharedString::from(format!("room-check-in-github-{index}")))
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Xs)
                .on_click(cx.listener(move |room, _, _, cx| {
                    cx.stop_propagation();
                    room.open_check_in_on_github(&opened, cx);
                }))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .child(cx.t("agents.room.checkIn.github"))
                        .child(Icon::new("external-link").size(12.0)),
                )
                .into_any_element(),
        )
    }

    pub(super) fn check_in_row(
        &self,
        index: usize,
        card: &CheckIn,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let args = Args::default().with("title", card.title.clone()).with(
            "minutes",
            crate::data::check_ins::STALLED_MINUTES.to_string(),
        );
        let (icon, key) = match card.kind {
            CheckInKind::ReviewRequested => {
                ("git-pull-request", "agents.room.checkIn.reviewRequested")
            }
            CheckInKind::IssueAssigned => ("circle-dot", "agents.room.checkIn.issueAssigned"),
            CheckInKind::TaskStalled => ("timer", "agents.room.checkIn.taskStalled"),
        };
        let when = crate::data::time::medium_date_time(
            &card.at,
            cx.try_global::<crate::i18n::I18n>()
                .is_some_and(|i18n| i18n.locale() == crate::i18n::Locale::En),
        );
        let opened = card.clone();
        div()
            .id(SharedString::from(format!("room-check-in-{index}")))
            .debug_selector(move || format!("room-check-in-{index}"))
            .flex()
            .items_center()
            .gap(px(10.0))
            .px(px(12.0))
            .py(px(8.0))
            .rounded(px(12.0))
            .border_1()
            .border_color(colors.border.opacity(0.7))
            .bg(colors.card)
            .cursor_pointer()
            .hover(|row| row.bg(colors.accent.opacity(0.4)))
            .on_click(cx.listener(move |room, _, _, cx| room.open_check_in(&opened, cx)))
            .child(Icon::new(icon).size(16.0).color(colors.muted_foreground))
            .child(
                div()
                    .flex_none()
                    .px(px(6.0))
                    .rounded(px(4.0))
                    .bg(colors.accent)
                    .text_size(px(10.0))
                    .text_color(colors.muted_foreground)
                    .child(cx.t("agents.room.checkIn.tag")),
            )
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
                            .font_weight(if card.unread {
                                FontWeight::MEDIUM
                            } else {
                                FontWeight::NORMAL
                            })
                            .text_color(colors.foreground)
                            .child(cx.t_with(key, &args)),
                    )
                    .when(!card.place.is_empty(), |text| {
                        text.child(
                            div()
                                .truncate()
                                .text_size(px(11.0))
                                .text_color(colors.muted_foreground)
                                .child(card.place.clone()),
                        )
                    }),
            )
            .children(self.check_in_github_button(index, card, cx))
            .child(
                div()
                    .flex_none()
                    .text_size(px(11.0))
                    .text_color(colors.muted_foreground)
                    .child(when),
            )
            .children(card.unread.then(|| {
                div()
                    .flex_none()
                    .size(px(7.0))
                    .rounded_full()
                    .bg(tw::SKY_500.hsla())
            }))
            .into_any_element()
    }

    /// "Pausar agentes" / "Retomar agentes": no trigger wakes anyone while
    /// paused; the check-in's cards still come.
    pub(super) fn pause_button(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let paused = cx
            .try_global::<crate::data::agent_triggers::AgentTriggers>()?
            .paused();
        Some(
            Button::new("room-pause")
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Xs)
                .on_click(cx.listener(move |_, _, _, cx| {
                    cx.update_global::<crate::data::agent_triggers::AgentTriggers, _>(
                        |triggers, _| triggers.set_paused(!paused),
                    );
                    cx.notify();
                }))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .child(Icon::new(if paused { "play" } else { "pause" }).size(12.0))
                        .child(cx.t(if paused {
                            "agents.room.resume"
                        } else {
                            "agents.room.pause"
                        })),
                )
                .into_any_element(),
        )
    }
}

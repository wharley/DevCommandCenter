//! The inbox in the team room (`data::room_inbox`): a card per thing an
//! agent did on its own or finished in a task, among the room's lines —
//! "Gatilho · CI falhou" when a trigger woke it, a dot while unread. A card
//! opens where the result is (the task's conversation, the pull request's
//! code tab) and is read from then on; "Marcar tudo como lido" reads all.

use gpui::{AnyElement, Context, FontWeight, SharedString, div, prelude::*, px};

use super::room::{RoomEvent, TeamRoom};
use crate::agent::AgentAvatarView;
use crate::data::agent_triggers::AgentTriggers;
use crate::data::agents::AgentRecord;
use crate::data::room_inbox::{CardKind, InboxCard};
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::theme::tailwind as tw;
use crate::ui::{Button, ButtonSize, ButtonVariant, pulse_spinner};

impl TeamRoom {
    /// The inbox's cards now, with what woke each agent.
    pub(crate) fn cards(&self, cx: &gpui::App) -> Vec<InboxCard> {
        let runs = cx
            .try_global::<AgentTriggers>()
            .map(|triggers| triggers.runs().to_vec())
            .unwrap_or_default();
        self.sidebar.read(cx).inbox(&runs)
    }

    pub(crate) fn open_card(&mut self, card: &InboxCard, cx: &mut Context<Self>) {
        self.sidebar
            .update(cx, |sidebar, cx| sidebar.mark_card_read(card, cx));
        match &card.kind {
            CardKind::TaskResult {
                workspace_id,
                session_id,
                ..
            }
            | CardKind::TaskWaiting {
                workspace_id,
                session_id,
            } => cx.emit(RoomEvent::OpenSession {
                task_id: workspace_id.clone(),
                session_id: session_id.clone(),
            }),
            CardKind::PrReview { pr_id, .. } => cx.emit(RoomEvent::OpenPullRequest(pr_id.clone())),
        }
    }

    /// "Marcar tudo como lido": the Geral room marks everything, as before;
    /// a project's room only its own cards and check-ins.
    pub(crate) fn mark_all_read(&mut self, cx: &mut Context<Self>) {
        if self.is_geral() {
            self.sidebar
                .update(cx, |sidebar, cx| sidebar.mark_inbox_read(cx));
            cx.notify();
            return;
        }
        let cards: Vec<InboxCard> = self
            .cards(cx)
            .into_iter()
            .filter(|card| card.unread && self.card_room(card, cx) == self.room)
            .collect();
        let checks: Vec<String> = self
            .check_ins(cx)
            .into_iter()
            .filter(|check| check.unread && self.check_in_room(check) == self.room)
            .map(|check| check.key)
            .collect();
        for card in cards {
            match &card.kind {
                CardKind::PrReview { pr_id, .. } => {
                    crate::data::pr_review_jobs::PrReviewJobs::mark_seen(cx, pr_id)
                }
                _ => self
                    .sidebar
                    .update(cx, |sidebar, cx| sidebar.mark_card_read(&card, cx)),
            }
        }
        if cx.has_global::<crate::data::check_ins::CheckIns>() {
            cx.update_global::<crate::data::check_ins::CheckIns, _>(|all, _| {
                for key in &checks {
                    all.mark_read(key);
                }
            });
        }
        self.sidebar.update(cx, |_, cx| cx.notify());
        cx.notify();
    }

    pub(super) fn card_row(
        &self,
        index: usize,
        card: &InboxCard,
        agent: &AgentRecord,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let args = Args::default()
            .with("agent", agent.name.clone())
            .with("task", card.title.clone());
        let (key, waiting) = match &card.kind {
            CardKind::TaskResult { .. } => ("agents.room.card.done", false),
            CardKind::TaskWaiting { .. } => ("agents.room.card.waiting", true),
            CardKind::PrReview { running: true, .. } => ("agents.room.card.reviewing", false),
            CardKind::PrReview { failed: false, .. } => ("agents.room.card.reviewed", false),
            CardKind::PrReview { failed: true, .. } => ("agents.room.card.reviewFailed", false),
        };
        let working = matches!(card.kind, CardKind::PrReview { running: true, .. }).then(|| {
            pulse_spinner(
                SharedString::from(format!("room-card-working-{index}")),
                14.0,
            )
        });
        let text = cx.t_with(key, &args);
        let reason = match &card.kind {
            CardKind::PrReview {
                reason: Some(reason),
                ..
            } => Some(super::room_failure::reason_line(reason, cx)),
            _ => None,
        };
        let trigger = card.trigger.map(|trigger| {
            div()
                .flex_none()
                .px(px(6.0))
                .rounded(px(4.0))
                .bg(colors.accent)
                .text_size(px(10.0))
                .text_color(colors.muted_foreground)
                .child(cx.t(&format!("agents.room.card.trigger.{}", trigger.id())))
        });
        let when = crate::data::time::medium_date_time(
            &card.at,
            cx.try_global::<crate::i18n::I18n>()
                .is_some_and(|i18n| i18n.locale() == crate::i18n::Locale::En),
        );
        let dot = card.unread.then(|| {
            div()
                .flex_none()
                .size(px(7.0))
                .rounded_full()
                .bg(if waiting {
                    tw::AMBER_500.hsla()
                } else {
                    tw::SKY_500.hsla()
                })
        });
        let opened = card.clone();
        div()
            .id(SharedString::from(format!("room-card-{index}")))
            .debug_selector(move || format!("room-card-{index}"))
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
            .on_click(cx.listener(move |room, _, _, cx| room.open_card(&opened, cx)))
            .child(AgentAvatarView::new(
                SharedString::from(format!("room-card-face-{index}")),
                &agent.avatar,
                None,
                22.0,
            ))
            .children(trigger)
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
                            .child(text),
                    )
                    .children(reason.map(|reason| {
                        div()
                            .debug_selector(move || format!("room-card-reason-{index}"))
                            .text_size(px(11.0))
                            .text_color(colors.muted_foreground)
                            .child(reason)
                    })),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(px(11.0))
                    .text_color(colors.muted_foreground)
                    .child(when),
            )
            .children(working)
            .children(dot)
            .into_any_element()
    }

    /// "3 novas · Marcar tudo como lido", while any card is unread.
    pub(super) fn unread_bar(&self, unread: usize, cx: &mut Context<Self>) -> Option<AnyElement> {
        (unread > 0).then(|| {
            let colors = cx.theme().colors.clone();
            div()
                .debug_selector(|| "room-unread".into())
                .flex()
                .items_center()
                .gap(px(8.0))
                .text_size(px(11.0))
                .text_color(colors.muted_foreground)
                .child(cx.t_with(
                    "agents.room.unread",
                    &Args::default().with("count", unread.to_string()),
                ))
                .child(
                    Button::new("room-mark-read")
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Xs)
                        .on_click(cx.listener(|room, _, _, cx| room.mark_all_read(cx)))
                        .child(cx.t("agents.room.markRead")),
                )
                .into_any_element()
        })
    }
}

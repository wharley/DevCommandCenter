//! The reviewer's pull request reviews on its page ("Revisões de Pull
//! Requests"): the title, `repository#number`, and how it went; a click
//! opens the hub on that pull request's code tab.

use gpui::{AnyElement, Context, FontWeight, SharedString, div, prelude::*, px};

use super::{AgentPage, AgentPageEvent};
use crate::agent::AgentAvatarView;
use crate::data::agents::AgentState;
use crate::data::pr_review_jobs::JobStatus;
use crate::i18n::Translate;
use crate::screens::sidebar::AgentPageData;
use crate::theme::tailwind as tw;
use crate::theme::{ActiveTheme, Appearance, fonts};

impl AgentPage {
    pub(super) fn pr_reviews(
        &self,
        data: &AgentPageData,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if data.pr_reviews.is_empty() {
            return None;
        }
        let colors = cx.theme().colors.clone();
        let dark = cx.theme().appearance == Appearance::Dark;
        let rows: Vec<AnyElement> = data
            .pr_reviews
            .iter()
            .enumerate()
            .map(|(index, job)| {
                let unread = job.unread();
                let (text, color, weight) = match job.status {
                    JobStatus::Running => (
                        cx.t("agents.state.working"),
                        colors.muted_foreground,
                        FontWeight::NORMAL,
                    ),
                    JobStatus::Failed => (
                        cx.t("agents.state.failed"),
                        colors.destructive,
                        FontWeight::NORMAL,
                    ),
                    JobStatus::Done if unread => (
                        cx.t("agents.state.done"),
                        if dark {
                            tw::EMERALD_300
                        } else {
                            tw::EMERALD_700
                        }
                        .hsla(),
                        FontWeight::MEDIUM,
                    ),
                    JobStatus::Done => (
                        cx.t("agents.state.finished"),
                        colors.muted_foreground,
                        FontWeight::NORMAL,
                    ),
                };
                let state = match job.status {
                    JobStatus::Running => Some(AgentState::Working),
                    _ if unread => Some(AgentState::Done),
                    _ => None,
                };
                let pr_id = job.pr_id.clone();
                div()
                    .id(SharedString::from(format!("agent-pr-review-{index}")))
                    .debug_selector(move || format!("agent-pr-review-{index}"))
                    .flex()
                    .w_full()
                    .items_center()
                    .gap(px(12.0))
                    .px(px(16.0))
                    .py(px(12.0))
                    .cursor_pointer()
                    .hover(|row| row.bg(colors.accent.opacity(0.5)))
                    .when(index > 0, |row| {
                        row.border_t_1().border_color(colors.border.opacity(0.6))
                    })
                    .on_click(cx.listener(move |_, _, _, cx| {
                        cx.emit(AgentPageEvent::OpenPullRequest(pr_id.clone()))
                    }))
                    .child(AgentAvatarView::new(
                        SharedString::from(format!("agent-pr-review-avatar-{index}")),
                        &data.agent.avatar,
                        state,
                        28.0,
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(13.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(colors.foreground)
                                    .child(job.title.clone()),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .font_family(fonts::MONO)
                                    .text_size(px(11.0))
                                    .text_color(colors.muted_foreground)
                                    .child(job.label.clone()),
                            ),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_size(px(11.0))
                            .font_weight(weight)
                            .text_color(color)
                            .child(text),
                    )
                    .into_any_element()
            })
            .collect();
        Some(
            div()
                .child(Self::section_title(cx.t("agents.page.pullRequests"), cx))
                .child(Self::card(cx).overflow_hidden().children(rows))
                .into_any_element(),
        )
    }
}

//! "Enviar sem perguntar" on an agent's page (`data::agent_auto_send`):
//! what it may publish by itself. The Reviewer's review of a pull request —
//! ask, comment only, or comment and decide — holds in the projects picked;
//! any agent's reply through a connector — ask or send — holds in the rooms
//! picked, Geral among them. Everything starts at asking.

use dcc_core::domain::agent::REVIEWER_PRESET;
use gpui::{AnyElement, BorrowAppContext, Context, FontWeight, SharedString, div, prelude::*, px};

use super::AgentPage;
use crate::data::agent_auto_send::{GENERAL_ROOM, REPLIES_A_DAY, ReviewSend};
use crate::data::agent_triggers::AgentTriggers;
use crate::i18n::{Args, Translate};
use crate::screens::sidebar::AgentPageData;
use crate::theme::ActiveTheme;
use crate::ui::{Icon, ToggleGroup};

/// Which grant a chip switches.
#[derive(Clone, Copy)]
enum Grant {
    Review,
    Reply,
}

impl Grant {
    fn id(self) -> &'static str {
        match self {
            Self::Review => "review",
            Self::Reply => "reply",
        }
    }
}

impl AgentPage {
    /// A project or room the grant holds in, switched by a click.
    fn auto_send_chip(
        &self,
        grant: Grant,
        index: usize,
        agent_id: &str,
        place: (String, SharedString),
        on: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let (place_id, name) = place;
        let agent = agent_id.to_string();
        let selector = format!("agent-auto-send-{}-place-{index}", grant.id());
        div()
            .id(SharedString::from(selector.clone()))
            .debug_selector(move || selector.clone())
            .px(px(10.0))
            .h(px(26.0))
            .flex()
            .items_center()
            .rounded(px(13.0))
            .border_1()
            .text_size(px(12.0))
            .cursor_pointer()
            .map(|chip| {
                if on {
                    chip.border_color(colors.ring)
                        .bg(colors.accent)
                        .text_color(colors.foreground)
                } else {
                    chip.border_color(colors.border.opacity(0.7))
                        .text_color(colors.muted_foreground)
                        .hover(|chip| chip.bg(colors.accent.opacity(0.5)))
                }
            })
            .on_click(cx.listener(move |_, _, _, cx| {
                cx.update_global::<AgentTriggers, _>(|triggers, _| match grant {
                    Grant::Review => triggers.set_review_send_in(&agent, &place_id, !on),
                    Grant::Reply => triggers.set_reply_send_in(&agent, &place_id, !on),
                });
                cx.notify();
            }))
            .child(name)
            .into_any_element()
    }

    /// One grant: its name and choices on a line, what the choice does,
    /// and — once it is more than asking — where it holds.
    #[allow(clippy::too_many_arguments)]
    fn auto_send_row(
        &self,
        grant: Grant,
        agent_id: &str,
        choices: Vec<SharedString>,
        picked: usize,
        hint: SharedString,
        places: Vec<(String, SharedString, bool)>,
        on_pick: impl Fn(&mut AgentTriggers, &str, usize) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let id = grant.id();
        let page = cx.entity().downgrade();
        let agent = agent_id.to_string();
        let group = ToggleGroup::new(
            SharedString::from(format!("agent-auto-send-{id}")),
            choices,
            Some(picked),
        )
        .on_select(move |index, _, cx| {
            if cx.has_global::<AgentTriggers>() {
                cx.update_global::<AgentTriggers, _>(|triggers, _| {
                    on_pick(triggers, &agent, index)
                });
            }
            let _ = page.update(cx, |_, cx| cx.notify());
        });
        let muted = |text: SharedString| {
            div()
                .text_size(px(12.0))
                .line_height(px(19.5))
                .text_color(colors.muted_foreground)
                .child(text)
        };
        let granted = picked > 0;
        let nowhere = granted && !places.iter().any(|place| place.2);
        let chips: Vec<AnyElement> = places
            .into_iter()
            .enumerate()
            .map(|(index, (place, name, on))| {
                self.auto_send_chip(grant, index, agent_id, (place, name), on, cx)
            })
            .collect();
        let (places_key, nowhere_key) = match grant {
            Grant::Review => (
                "agents.autoSend.review.projects",
                "agents.autoSend.review.noProjects",
            ),
            Grant::Reply => (
                "agents.autoSend.reply.rooms",
                "agents.autoSend.reply.noRooms",
            ),
        };
        let where_it_holds = granted.then(|| {
            div()
                .debug_selector(move || format!("agent-auto-send-{id}-places"))
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(muted(cx.t(places_key)))
                .child(div().flex().flex_wrap().gap(px(6.0)).children(chips))
                .children(nowhere.then(|| {
                    div()
                        .debug_selector(move || format!("agent-auto-send-{id}-nowhere"))
                        .text_size(px(11.5))
                        .text_color(colors.muted_foreground)
                        .child(cx.t(nowhere_key))
                }))
        });
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap(px(10.0))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(colors.foreground)
                            .child(cx.t(&format!("agents.autoSend.{id}.title"))),
                    )
                    .child(group),
            )
            .child(muted(hint))
            .children(where_it_holds)
            .into_any_element()
    }

    pub(super) fn auto_send_card(
        &self,
        data: &AgentPageData,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let triggers = cx.try_global::<AgentTriggers>()?;
        let reviewer = data.agent.preset.as_deref() == Some(REVIEWER_PRESET);
        let agent_id = data.agent.id.clone();
        let review = triggers.review_send_choice(&agent_id);
        let reply = triggers.reply_send_on(&agent_id);
        // The projects it works in, by name.
        let mut projects: Vec<(String, String)> = data
            .project_labels
            .iter()
            .filter(|(id, _)| data.agent.works_in(id))
            .map(|(id, name)| (id.clone(), name.clone()))
            .collect();
        projects.sort_by(|left, right| left.1.to_lowercase().cmp(&right.1.to_lowercase()));
        let review_places: Vec<(String, SharedString, bool)> = projects
            .iter()
            .map(|(id, name)| {
                (
                    id.clone(),
                    name.clone().into(),
                    triggers.review_send_in(&agent_id, id),
                )
            })
            .collect();
        // Geral first, then each project's room.
        let reply_places: Vec<(String, SharedString, bool)> = std::iter::once((
            GENERAL_ROOM.to_string(),
            cx.t("agents.autoSend.reply.general"),
        ))
        .chain(
            projects
                .iter()
                .map(|(id, name)| (id.clone(), name.clone().into())),
        )
        .map(|(id, name)| {
            let on = triggers.reply_send_in(&agent_id, &id);
            (id, name, on)
        })
        .collect();
        let colors = cx.theme().colors.clone();
        let review_row = reviewer.then(|| {
            let choices = ReviewSend::ALL
                .iter()
                .map(|send| cx.t(&format!("agents.autoSend.review.{}", send.key())))
                .collect();
            let picked = ReviewSend::ALL
                .iter()
                .position(|send| *send == review)
                .unwrap_or_default();
            let hint = cx.t(&format!("agents.autoSend.review.{}Hint", review.key()));
            self.auto_send_row(
                Grant::Review,
                &agent_id,
                choices,
                picked,
                hint,
                review_places,
                |triggers, agent, index| triggers.set_review_send(agent, ReviewSend::ALL[index]),
                cx,
            )
        });
        let reply_row = {
            let choices = vec![
                cx.t("agents.autoSend.reply.ask"),
                cx.t("agents.autoSend.reply.send"),
            ];
            let hint = if reply {
                cx.t_with(
                    "agents.autoSend.reply.sendHint",
                    &Args::default().with("count", REPLIES_A_DAY.to_string()),
                )
            } else {
                cx.t("agents.autoSend.reply.askHint")
            };
            self.auto_send_row(
                Grant::Reply,
                &agent_id,
                choices,
                usize::from(reply),
                hint,
                reply_places,
                |triggers, agent, index| triggers.set_reply_send(agent, index == 1),
                cx,
            )
        };
        Some(
            Self::card(cx)
                .debug_selector(|| "agent-auto-send".into())
                .flex()
                .flex_col()
                .gap(px(16.0))
                .p(px(20.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .text_size(px(14.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(Icon::new("send").size(16.0).color(colors.muted_foreground))
                        .child(cx.t("agents.autoSend.title")),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .line_height(px(19.5))
                        .text_color(colors.muted_foreground)
                        .child(cx.t("agents.autoSend.hint")),
                )
                .children(review_row)
                .child(reply_row)
                .into_any_element(),
        )
    }
}

//! Sources in the team room (`dcc_core::domain::agent_source`): a source's
//! news shows as one line ("Fonte · Slack — menções a mim"), its answer
//! stays quiet when nothing needed the person, and a source an agent
//! proposed is a card with "Criar" / "Descartar".

use dcc_core::domain::agent_source::{AgentSource, source_request};
use gpui::{AnyElement, BorrowAppContext, Context, FontWeight, SharedString, div, prelude::*, px};

use super::room::TeamRoom;
use crate::agent::AgentAvatarView;
use crate::data::agent_sources::Sources;
use crate::data::agents::AgentRecord;
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, Toaster};

impl TeamRoom {
    /// A source's news, as one line.
    pub(super) fn source_line(
        &self,
        index: usize,
        text: &str,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let (watch, connector) = source_request(text)?;
        let colors = cx.theme().colors.clone();
        Some(
            div()
                .debug_selector(move || format!("room-source-{index}"))
                .flex()
                .items_center()
                .gap(px(6.0))
                .text_size(px(11.5))
                .text_color(colors.muted_foreground)
                .child(Icon::new("radar").size(12.0).color(colors.muted_foreground))
                .child(
                    div().truncate().child(
                        cx.t_with(
                            "agents.sources.line",
                            &Args::default()
                                .with("connector", connector.to_string())
                                .with("watch", watch.to_string()),
                        ),
                    ),
                )
                .into_any_element(),
        )
    }

    fn answer_source(&mut self, source: &AgentSource, create: bool, cx: &mut Context<Self>) {
        let Some(api) = self.api.clone() else {
            Toaster::error(cx.t("native.desktopClosed"), cx);
            return;
        };
        // Gone from the room at once; the clock reads the table again.
        if cx.has_global::<Sources>() {
            let id = source.id.clone();
            cx.update_global::<Sources, _>(|sources, _| {
                if create {
                    for found in sources.list.iter_mut().filter(|found| found.id == id) {
                        found.proposed = false;
                    }
                } else {
                    sources.list.retain(|found| found.id != id);
                }
            });
        }
        let source = source.clone();
        cx.spawn(async move |room, cx| {
            let done = cx
                .background_executor()
                .spawn(async move {
                    if create {
                        api.switch_source(&source, true)
                    } else {
                        api.delete_source(&source)
                    }
                })
                .await;
            if let Err(error) = done {
                let _ = room.update(cx, |_, cx| Toaster::error(error.to_string(), cx));
            }
        })
        .detach();
    }

    /// The sources agents proposed, a card each.
    pub(super) fn source_cards(
        &self,
        agents: &[AgentRecord],
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let colors = cx.theme().colors.clone();
        let proposed: Vec<AgentSource> = cx
            .try_global::<Sources>()
            .map(|sources| sources.proposed().cloned().collect())
            .unwrap_or_default();
        let mut rows = Vec::new();
        for (index, source) in proposed.iter().enumerate() {
            let Some(agent) = agents.iter().find(|agent| agent.id == source.agent_id) else {
                continue;
            };
            let (create, discard) = (source.clone(), source.clone());
            rows.push(
                div()
                    .debug_selector(move || format!("room-source-proposal-{index}"))
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .px(px(12.0))
                    .py(px(8.0))
                    .rounded(px(12.0))
                    .border_1()
                    .border_color(colors.border.opacity(0.7))
                    .bg(colors.card)
                    .child(AgentAvatarView::new(
                        SharedString::from(format!("room-source-face-{index}")),
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
                            .child(
                                div()
                                    .text_size(px(12.5))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(colors.foreground)
                                    .child(cx.t_with(
                                        "agents.sources.proposal",
                                        &Args::default().with("agent", agent.name.clone()),
                                    )),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(11.5))
                                    .text_color(colors.muted_foreground)
                                    .child(
                                        cx.t_with(
                                            "agents.sources.what",
                                            &Args::default()
                                                .with("connector", source.connector.clone())
                                                .with("watch", source.watch.clone())
                                                .with("every", source.every_minutes.to_string()),
                                        ),
                                    ),
                            ),
                    )
                    .child(
                        Button::new(SharedString::from(format!("room-source-create-{index}")))
                            .variant(ButtonVariant::Outline)
                            .size(ButtonSize::Xs)
                            .on_click(cx.listener(move |room, _, _, cx| {
                                room.answer_source(&create, true, cx)
                            }))
                            .child(cx.t("agents.sources.create")),
                    )
                    .child(
                        Button::new(SharedString::from(format!("room-source-discard-{index}")))
                            .variant(ButtonVariant::Ghost)
                            .size(ButtonSize::Xs)
                            .on_click(cx.listener(move |room, _, _, cx| {
                                room.answer_source(&discard, false, cx)
                            }))
                            .child(cx.t("agents.sources.discard")),
                    )
                    .into_any_element(),
            );
        }
        rows
    }
}

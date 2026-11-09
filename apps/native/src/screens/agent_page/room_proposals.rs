//! An agent another agent proposed (`dcc_propose_agent`), as a card in the
//! team room: who proposes it, the new agent's mascot, name and what it
//! does, where it works and the first request to try it, with "Criar",
//! "Editar antes" and "Descartar". "Criar" makes it and opens its
//! conversation with that request in the field — sent only when the person
//! sends it; "Editar antes" opens the editor filled with it.

use dcc_core::domain::agent_proposal::AgentProposal;
use gpui::{AnyElement, Context, FontWeight, SharedString, div, prelude::*, px};

use super::editor_projects::project_options;
use super::room::{RoomEvent, TeamRoom};
use crate::agent::AgentAvatarView;
use crate::data::agents::AgentRecord;
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::ui::{Button, ButtonSize, ButtonVariant, Toaster};

/// How much of the role the card shows.
const ROLE_PREVIEW_CHARS: usize = 220;

fn preview(text: &str) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() <= ROLE_PREVIEW_CHARS {
        return text;
    }
    let cut: String = text.chars().take(ROLE_PREVIEW_CHARS).collect();
    format!("{}…", cut.trim_end())
}

impl TeamRoom {
    /// The proposals still waiting, not answered in this room just now.
    fn waiting_proposals(&self, cx: &gpui::App) -> Vec<AgentProposal> {
        self.sidebar
            .read(cx)
            .agent_proposals()
            .iter()
            .filter(|proposal| !self.answered_proposals.contains(&proposal.id))
            .filter(|proposal| self.shows(&proposal.agent_id))
            .cloned()
            .collect()
    }

    /// "Criar" (`create`) or "Descartar".
    fn answer_agent_proposal(
        &mut self,
        proposal: AgentProposal,
        create: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(api) = self.api.clone() else {
            Toaster::error(cx.t("native.desktopClosed"), cx);
            return;
        };
        self.answered_proposals.push(proposal.id.clone());
        cx.notify();
        cx.spawn(async move |room, cx| {
            let sent = proposal.clone();
            let done = cx
                .background_executor()
                .spawn(async move {
                    if create {
                        api.accept_agent_proposal(&sent).map(Some)
                    } else {
                        api.dismiss_agent_proposal(&sent).map(|()| None)
                    }
                })
                .await;
            let _ = room.update(cx, |room, cx| {
                match done {
                    Ok(Some(agent_id)) => cx.emit(RoomEvent::AgentCreated {
                        agent_id,
                        first_request: proposal.draft.kickoff_prompt.clone(),
                    }),
                    Ok(None) => {}
                    Err(error) => {
                        room.answered_proposals.retain(|id| *id != proposal.id);
                        Toaster::error(error.to_string(), cx);
                    }
                }
                if !cfg!(test) {
                    room.sidebar
                        .update(cx, |sidebar, cx| sidebar.reload_now(cx));
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn proposal_cards(
        &self,
        agents: &[AgentRecord],
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let colors = cx.theme().colors.clone();
        let projects = project_options(self.sidebar.read(cx).projects());
        let mut rows = Vec::new();
        for (index, proposal) in self.waiting_proposals(cx).into_iter().enumerate() {
            let Some(proposer) = agents.iter().find(|agent| agent.id == proposal.agent_id) else {
                continue;
            };
            let draft = &proposal.draft;
            let names: Vec<&str> = draft
                .projects
                .iter()
                .filter_map(|id| {
                    projects
                        .iter()
                        .find(|(project, _)| project == id)
                        .map(|(_, name)| name.as_str())
                })
                .collect();
            let where_ = if names.is_empty() {
                cx.t("agents.proposal.everyProject")
            } else {
                cx.t_with(
                    "agents.proposal.projects",
                    &Args::default().with("projects", names.join(", ")),
                )
            };
            let detail = |text: SharedString| {
                div()
                    .text_size(px(11.5))
                    .line_height(px(17.0))
                    .text_color(colors.muted_foreground)
                    .child(text)
            };
            let (create, edit, discard) = (proposal.clone(), proposal.clone(), proposal.clone());
            rows.push(
                div()
                    .debug_selector(move || format!("room-agent-proposal-{index}"))
                    .flex()
                    .flex_col()
                    .gap(px(10.0))
                    .px(px(14.0))
                    .py(px(12.0))
                    .rounded(px(14.0))
                    .border_1()
                    .border_color(colors.border.opacity(0.7))
                    .bg(colors.card)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(AgentAvatarView::new(
                                SharedString::from(format!("room-proposal-by-{index}")),
                                &proposer.avatar,
                                None,
                                18.0,
                            ))
                            .child(
                                div()
                                    .text_size(px(11.5))
                                    .text_color(colors.muted_foreground)
                                    .child(cx.t_with(
                                        "agents.proposal.by",
                                        &Args::default().with("agent", proposer.name.clone()),
                                    )),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(12.0))
                            .child(AgentAvatarView::new(
                                SharedString::from(format!("room-proposal-face-{index}")),
                                &draft.avatar,
                                None,
                                40.0,
                            ))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap(px(4.0))
                                    .child(
                                        div()
                                            .text_size(px(14.0))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(colors.foreground)
                                            .child(draft.name.clone()),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(12.5))
                                            .line_height(px(19.0))
                                            .text_color(colors.foreground)
                                            .child(preview(&draft.role)),
                                    )
                                    .child(detail(where_))
                                    .child(detail(
                                        cx.t_with(
                                            "agents.proposal.first",
                                            &Args::default()
                                                .with("request", preview(&draft.kickoff_prompt)),
                                        ),
                                    )),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(8.0))
                            .child(
                                Button::new(SharedString::from(format!(
                                    "room-proposal-discard-{index}"
                                )))
                                .variant(ButtonVariant::Ghost)
                                .size(ButtonSize::Sm)
                                .on_click(cx.listener(move |room, _, _, cx| {
                                    room.answer_agent_proposal(discard.clone(), false, cx)
                                }))
                                .child(cx.t("agents.proposal.discard")),
                            )
                            .child(
                                Button::new(SharedString::from(format!(
                                    "room-proposal-edit-{index}"
                                )))
                                .variant(ButtonVariant::Outline)
                                .size(ButtonSize::Sm)
                                .on_click(cx.listener(move |_, _, _, cx| {
                                    cx.emit(RoomEvent::EditProposal(edit.clone()))
                                }))
                                .child(cx.t("agents.proposal.edit")),
                            )
                            .child(
                                Button::new(SharedString::from(format!(
                                    "room-proposal-create-{index}"
                                )))
                                .size(ButtonSize::Sm)
                                .on_click(cx.listener(move |room, _, _, cx| {
                                    room.answer_agent_proposal(create.clone(), true, cx)
                                }))
                                .child(cx.t("agents.proposal.create")),
                            ),
                    )
                    .into_any_element(),
            );
        }
        rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_role_is_cut_on_the_card() {
        assert_eq!(preview("Teste o\n checkout."), "Teste o checkout.");
        let long = "palavra ".repeat(60);
        let cut = preview(&long);
        assert!(cut.ends_with('…') && cut.chars().count() <= ROLE_PREVIEW_CHARS + 1);
    }
}

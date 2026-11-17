//! A change an agent proposed to what exists (`dcc_propose_change`), as a
//! card in the team room: who proposes it, what it is about, each thing
//! that changes with its before and its after, why, and "Aplicar" or
//! "Descartar". Nothing changes before the person applies it; with more
//! than one waiting, "Aplicar todos" answers them together.

use dcc_core::domain::agent_change::{AgentChange, Change};
use gpui::{AnyElement, Context, FontWeight, SharedString, div, prelude::*, px};

use super::editor_projects::project_options;
use super::room::TeamRoom;
use crate::agent::AgentAvatarView;
use crate::data::agents::AgentRecord;
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::ui::{Button, ButtonSize, ButtonVariant, Toaster};

/// One thing that changes: its label's key under `agents.change`, and what
/// it is before and after.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Line {
    pub label: &'static str,
    pub before: String,
    pub after: String,
}

/// What the card lists for `change`. `project`: a project's name by id;
/// `word`: a text of `agents.change` by key.
pub(super) fn lines(
    change: &Change,
    project: &dyn Fn(&str) -> String,
    word: &dyn Fn(&str) -> String,
) -> Vec<Line> {
    let room = |room: &Option<String>| match room {
        Some(id) => project(id),
        None => word("general"),
    };
    let listed = |ids: &[String]| {
        if ids.is_empty() {
            word("all")
        } else {
            ids.iter()
                .map(|id| project(id))
                .collect::<Vec<_>>()
                .join(", ")
        }
    };
    let mut lines = Vec::new();
    match change {
        Change::Source {
            room: moved,
            every_minutes,
            enabled,
            ..
        } => {
            if let Some((before, after)) = moved {
                lines.push(Line {
                    label: "room",
                    before: room(before),
                    after: room(after),
                });
            }
            if let Some((before, after)) = every_minutes {
                lines.push(Line {
                    label: "every",
                    before: format!("{before} min"),
                    after: format!("{after} min"),
                });
            }
            if let Some((before, after)) = enabled {
                let state = |on: bool| word(if on { "on" } else { "off" });
                lines.push(Line {
                    label: "reads",
                    before: state(*before),
                    after: state(*after),
                });
            }
        }
        Change::Projects { before, after, .. } => lines.push(Line {
            label: "projects",
            before: listed(before),
            after: listed(after),
        }),
    }
    lines.retain(|line| line.before != line.after);
    lines
}

impl TeamRoom {
    /// The changes still waiting, not answered in this room just now.
    fn waiting_adjusts(&self, cx: &gpui::App) -> Vec<AgentChange> {
        self.sidebar
            .read(cx)
            .agent_changes()
            .iter()
            .filter(|change| !self.answered_proposals.contains(&change.id))
            .filter(|change| self.shows(&change.agent_id))
            .cloned()
            .collect()
    }

    /// "Aplicar" (`apply`) or "Descartar", for one or for all.
    fn answer_adjusts(&mut self, changes: Vec<AgentChange>, apply: bool, cx: &mut Context<Self>) {
        let Some(api) = self.api.clone() else {
            Toaster::error(cx.t("native.desktopClosed"), cx);
            return;
        };
        for change in &changes {
            self.answered_proposals.push(change.id.clone());
        }
        cx.notify();
        cx.spawn(async move |room, cx| {
            let sent = changes.clone();
            let failed = cx
                .background_executor()
                .spawn(async move {
                    let mut failed = Vec::new();
                    for change in &sent {
                        let done = if apply {
                            api.accept_agent_change(change)
                        } else {
                            api.dismiss_agent_change(change)
                        };
                        if let Err(error) = done {
                            failed.push((change.id.clone(), error.to_string()));
                        }
                    }
                    failed
                })
                .await;
            let _ = room.update(cx, |room, cx| {
                for (id, error) in &failed {
                    room.answered_proposals.retain(|answered| answered != id);
                    Toaster::error(error.clone(), cx);
                }
                if apply && failed.len() < changes.len() {
                    Toaster::success(cx.t("agents.change.applied"), cx);
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

    pub(super) fn adjust_cards(
        &self,
        agents: &[AgentRecord],
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let colors = cx.theme().colors.clone();
        let projects = project_options(self.sidebar.read(cx).projects());
        let project = |id: &str| {
            projects
                .iter()
                .find(|(project, _)| project == id)
                .map_or_else(|| id.to_string(), |(_, name)| name.clone())
        };
        let waiting = self.waiting_adjusts(cx);
        let mut rows = Vec::new();
        if waiting.len() > 1 {
            let all = waiting.clone();
            rows.push(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(colors.muted_foreground)
                            .child(cx.t_with(
                                "agents.change.several",
                                &Args::default().with("count", waiting.len().to_string()),
                            )),
                    )
                    .child(
                        Button::new("room-change-apply-all")
                            .size(ButtonSize::Sm)
                            .on_click(cx.listener(move |room, _, _, cx| {
                                room.answer_adjusts(all.clone(), true, cx)
                            }))
                            .child(cx.t("agents.change.applyAll")),
                    )
                    .into_any_element(),
            );
        }
        for (index, waiting) in waiting.into_iter().enumerate() {
            let Some(proposer) = agents.iter().find(|agent| agent.id == waiting.agent_id) else {
                continue;
            };
            let about = agents
                .iter()
                .find(|agent| agent.id == waiting.change.about());
            let about_name = about.map_or_else(String::new, |agent| agent.name.clone());
            let title = match &waiting.change {
                Change::Source {
                    connector, watch, ..
                } => cx.t_with(
                    "agents.change.source",
                    &Args::default()
                        .with("agent", about_name.clone())
                        .with("connector", connector.clone())
                        .with("watch", watch.clone()),
                ),
                Change::Projects { .. } => cx.t_with(
                    "agents.change.projectsOf",
                    &Args::default().with("agent", about_name.clone()),
                ),
            };
            let word = |key: &str| cx.t(&format!("agents.change.{key}")).to_string();
            let listed = lines(&waiting.change, &project, &word);
            // More projects for an agent that starts tasks by itself: it
            // will start them there too, and the card says so.
            let starts_there = matches!(&waiting.change, Change::Projects { after, .. } if !after.is_empty())
                && about.is_some_and(|agent| agent.auto_start_tasks);
            let (apply, discard) = (waiting.clone(), waiting.clone());
            let mut card = div()
                .debug_selector(move || format!("room-agent-change-{index}"))
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
                            SharedString::from(format!("room-change-by-{index}")),
                            &proposer.avatar,
                            None,
                            18.0,
                        ))
                        .child(
                            div()
                                .text_size(px(11.5))
                                .text_color(colors.muted_foreground)
                                .child(cx.t_with(
                                    "agents.change.by",
                                    &Args::default().with("agent", proposer.name.clone()),
                                )),
                        ),
                )
                .child(
                    div()
                        .text_size(px(13.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(colors.foreground)
                        .child(title),
                );
            for line in listed {
                card = card.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(6.0))
                        .text_size(px(12.5))
                        .line_height(px(19.0))
                        .child(
                            div()
                                .text_color(colors.muted_foreground)
                                .child(cx.t(&format!("agents.change.{}", line.label))),
                        )
                        .child(
                            div()
                                .text_color(colors.muted_foreground)
                                .line_through()
                                .child(line.before),
                        )
                        .child(div().text_color(colors.muted_foreground).child("→"))
                        .child(
                            div()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(colors.foreground)
                                .child(line.after),
                        ),
                );
            }
            let note = |text: SharedString| {
                div()
                    .text_size(px(11.5))
                    .line_height(px(17.0))
                    .text_color(colors.muted_foreground)
                    .child(text)
            };
            if !waiting.reason.is_empty() {
                card = card.child(note(waiting.reason.clone().into()));
            }
            if starts_there {
                card = card.child(note(cx.t("agents.change.startsThere")));
            }
            rows.push(
                card.child(
                    div()
                        .flex()
                        .justify_end()
                        .gap(px(8.0))
                        .child(
                            Button::new(SharedString::from(format!("room-change-discard-{index}")))
                                .variant(ButtonVariant::Ghost)
                                .size(ButtonSize::Sm)
                                .on_click(cx.listener(move |room, _, _, cx| {
                                    room.answer_adjusts(vec![discard.clone()], false, cx)
                                }))
                                .child(cx.t("agents.change.discard")),
                        )
                        .child(
                            Button::new(SharedString::from(format!("room-change-apply-{index}")))
                                .size(ButtonSize::Sm)
                                .on_click(cx.listener(move |room, _, _, cx| {
                                    room.answer_adjusts(vec![apply.clone()], true, cx)
                                }))
                                .child(cx.t("agents.change.apply")),
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
    fn the_card_lists_only_what_changes_with_its_before_and_after() {
        let project = |id: &str| match id {
            "app" => "Checkout".to_string(),
            other => other.to_string(),
        };
        let word = |key: &str| key.to_string();
        let moved = Change::Source {
            source_id: "s".into(),
            agent_id: "sup".into(),
            connector: "Slack".into(),
            watch: "menções".into(),
            room: Some((None, Some("app".into()))),
            every_minutes: Some((15, 15)),
            enabled: Some((true, false)),
        };
        let listed = lines(&moved, &project, &word);
        let shown: Vec<(&str, &str, &str)> = listed
            .iter()
            .map(|line| (line.label, line.before.as_str(), line.after.as_str()))
            .collect();
        // The interval is the same: it is not on the card.
        assert_eq!(
            shown,
            [("room", "general", "Checkout"), ("reads", "on", "off")]
        );
        let projects = Change::Projects {
            agent_id: "dev".into(),
            before: Vec::new(),
            after: vec!["app".into(), "zapp".into()],
        };
        let listed = lines(&projects, &project, &word);
        assert_eq!(listed[0].before, "all");
        assert_eq!(listed[0].after, "Checkout, zapp");
    }
}

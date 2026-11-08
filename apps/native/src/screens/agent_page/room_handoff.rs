//! Passing the turn in the team room: an agent ends its answer with a line
//! that names a teammate ("@Pesquisador pode comparar as filas?",
//! `dcc_core::domain::team_room::handoff`); under that answer the room asks
//! the person "Passar a vez" or "Ignorar". Only the person's click sends it,
//! to the teammate's own conversation, with what the first agent said, so
//! nothing chains on its own. The passed message shows as one line.

use dcc_core::domain::team_room::{Teammate, handoff, handoff_prompt};
use gpui::{AnyElement, Context, FontWeight, SharedString, div, prelude::*, px};

use super::room::TeamRoom;
use crate::data::agents::AgentRecord;
use crate::data::room_handoffs::Handled;
use crate::data::team_room::RoomLine;
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::ui::{Button, ButtonSize, ButtonVariant};

impl TeamRoom {
    /// The teammate `line` (an agent's answer) suggests, and its request.
    pub(crate) fn suggested(
        &self,
        line: &RoomLine,
        agents: &[AgentRecord],
    ) -> Option<(AgentRecord, String)> {
        let team: Vec<Teammate<'_>> = agents
            .iter()
            .map(|agent| Teammate {
                id: &agent.id,
                name: &agent.name,
                preset: agent.preset.as_deref(),
            })
            .collect();
        let (mate, request) = handoff(&line.text, &team, &line.agent_id)?;
        let agent = agents.iter().find(|agent| agent.id == mate.id)?.clone();
        Some((agent, request))
    }

    /// "Passar a vez": the request goes to the teammate with what `from` said.
    pub(crate) fn pass_turn(&mut self, line: &RoomLine, from: &str, cx: &mut Context<Self>) {
        let agents = self.sidebar.read(cx).agents().to_vec();
        let Some((to, request)) = self.suggested(line, &agents) else {
            return;
        };
        let prompt = handoff_prompt(from, &to.name, &request, &line.text);
        if self.deliver(to, prompt, None, cx) {
            self.handoffs.set(&line.key, Handled::Passed);
            cx.notify();
        }
    }

    pub(crate) fn ignore_handoff(&mut self, key: &str, cx: &mut Context<Self>) {
        self.handoffs.set(key, Handled::Ignored);
        cx.notify();
    }

    /// Under an answer that suggests a teammate: the question, or what the
    /// person did with it.
    pub(super) fn handoff_bar(
        &self,
        index: usize,
        line: &RoomLine,
        author: &AgentRecord,
        agents: &[AgentRecord],
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let (to, request) = self.suggested(line, agents)?;
        let colors = cx.theme().colors.clone();
        let args = Args::default()
            .with("agent", author.name.clone())
            .with("teammate", to.name.clone());
        let bar = div()
            .debug_selector(move || format!("room-handoff-{index}"))
            .ml(px(38.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .px(px(12.0))
            .py(px(8.0))
            .rounded(px(12.0))
            .border_1()
            .border_color(colors.border.opacity(0.7))
            .text_size(px(12.0))
            .text_color(colors.muted_foreground);
        match self.handoffs.get(&line.key) {
            Some(Handled::Ignored) => None,
            Some(Handled::Passed) => Some(
                bar.child(cx.t_with("agents.room.handoff.passed", &args))
                    .into_any_element(),
            ),
            None => {
                let (pass_line, ignore_key) = (line.clone(), line.key.clone());
                let from = author.name.clone();
                Some(
                    bar.child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(colors.foreground)
                                    .child(cx.t_with("agents.room.handoff.suggests", &args)),
                            )
                            .child(div().truncate().child(request)),
                    )
                    .child(
                        Button::new(SharedString::from(format!("room-handoff-ignore-{index}")))
                            .variant(ButtonVariant::Ghost)
                            .size(ButtonSize::Sm)
                            .on_click(cx.listener(move |room, _, _, cx| {
                                room.ignore_handoff(&ignore_key, cx)
                            }))
                            .child(cx.t("agents.room.handoff.ignore")),
                    )
                    .child(
                        Button::new(SharedString::from(format!("room-handoff-pass-{index}")))
                            .size(ButtonSize::Sm)
                            .disabled(self.sending)
                            .on_click(cx.listener(move |room, _, _, cx| {
                                room.pass_turn(&pass_line, &from, cx)
                            }))
                            .child(cx.t("agents.room.handoff.pass")),
                    )
                    .into_any_element(),
                )
            }
        }
    }

    /// A message the person passed, as its one line ("↪ Revisor → …").
    pub(super) fn passed_line(
        &self,
        index: usize,
        heading: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        div()
            .debug_selector(move || format!("room-passed-{index}"))
            .flex()
            .justify_center()
            .child(
                div()
                    .max_w(px(560.0))
                    .truncate()
                    .px(px(10.0))
                    .py(px(4.0))
                    .rounded(px(10.0))
                    .bg(colors.accent.opacity(0.6))
                    .text_size(px(11.5))
                    .text_color(colors.muted_foreground)
                    .child(heading.to_string()),
            )
            .into_any_element()
    }
}

//! 👍 / 👎 under each answer in the team room (Monocode): a click marks it,
//! another takes it back, and the bulletin counts them. A 👎 opens "Guardar
//! como regra?": the person writes what the agent should do instead, and it
//! goes to the agent's memory as the person's note — no model writes it.

use dcc_core::domain::agent_reaction::Reaction;
use gpui::{AnyElement, AppContext, Context, SharedString, Window, div, prelude::*, px};

use super::room::TeamRoom;
use crate::data::team_room::RoomLine;
use crate::i18n::Translate;
use crate::theme::ActiveTheme;
use crate::theme::tailwind as tw;
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, TextInput, Toaster};

/// The longest rule kept from the field.
const MAX_RULE_CHARS: usize = 500;

/// "Guardar como regra?" while open: the answer, its agent and the field.
pub(crate) struct RuleDraft {
    pub line_key: String,
    pub agent_id: String,
    pub input: gpui::Entity<TextInput>,
}

impl TeamRoom {
    /// The person's reaction to `line` now.
    pub(crate) fn reaction(&self, line: &RoomLine) -> Option<Reaction> {
        if let Some(changed) = self.reacted.get(&line.key) {
            return *changed;
        }
        self.feeds
            .iter()
            .find(|feed| feed.session_id == line.session_id)
            .and_then(|feed| feed.reactions.get(&line.key).copied())
    }

    fn react(
        &mut self,
        line: &RoomLine,
        reaction: Reaction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(api) = self.api.clone() else {
            Toaster::error(cx.t("native.desktopClosed"), cx);
            return;
        };
        // The same one again takes it back.
        let chosen = (self.reaction(line) != Some(reaction)).then_some(reaction);
        self.reacted.insert(line.key.clone(), chosen);
        self.rule = None;
        if chosen == Some(Reaction::Down) {
            let input = cx.new(|cx| {
                TextInput::new(window, cx)
                    .with_placeholder(cx.t("agents.reactions.rulePlaceholder"))
                    .with_max_len(MAX_RULE_CHARS)
            });
            let handle = gpui::Focusable::focus_handle(input.read(cx), cx);
            window.focus(&handle, cx);
            self.rule = Some(RuleDraft {
                line_key: line.key.clone(),
                agent_id: line.agent_id.clone(),
                input,
            });
        }
        cx.notify();
        let (agent, session, key) = (
            line.agent_id.clone(),
            line.session_id.clone(),
            line.key.clone(),
        );
        cx.spawn(async move |room, cx| {
            let sent = cx
                .background_executor()
                .spawn(async move { api.set_agent_reaction(&agent, &session, &key, chosen) })
                .await;
            if let Err(error) = sent {
                let _ = room.update(cx, |_, cx| Toaster::error(error.to_string(), cx));
            }
        })
        .detach();
    }

    /// "Guardar": the rule, as the person's note in the agent's memory.
    fn keep_rule(&mut self, cx: &mut Context<Self>) {
        let Some(draft) = self.rule.as_ref() else {
            return;
        };
        let body = draft.input.read(cx).text().trim().to_string();
        if body.is_empty() {
            return;
        }
        let Some(api) = self.api.clone() else {
            Toaster::error(cx.t("native.desktopClosed"), cx);
            return;
        };
        let agent = draft.agent_id.clone();
        self.rule = None;
        cx.notify();
        cx.spawn(async move |room, cx| {
            let kept = cx
                .background_executor()
                .spawn(async move { api.add_memory_note(&agent, &body, None) })
                .await;
            let _ = room.update(cx, |_, cx| match kept {
                Ok(_) => Toaster::success(cx.t("agents.reactions.kept"), cx),
                Err(error) => Toaster::error(error.to_string(), cx),
            });
        })
        .detach();
    }

    /// 👍 and 👎 beside "Abrir conversa".
    pub(super) fn reaction_buttons(
        &self,
        index: usize,
        line: &RoomLine,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let current = self.reaction(line);
        let button = |reaction: Reaction, icon: &'static str, cx: &mut Context<Self>| {
            let picked = current == Some(reaction);
            let line = line.clone();
            div()
                .id(SharedString::from(format!(
                    "room-react-{}-{index}",
                    reaction.key()
                )))
                .debug_selector(move || format!("room-react-{}-{index}", reaction.key()))
                .flex()
                .items_center()
                .justify_center()
                .size(px(22.0))
                .rounded(px(6.0))
                .cursor_pointer()
                .hover(|button| button.bg(colors.accent))
                .on_click(
                    cx.listener(move |room, _, window, cx| room.react(&line, reaction, window, cx)),
                )
                .child(Icon::new(icon).size(13.0).color(if picked {
                    match reaction {
                        Reaction::Up => tw::EMERALD_600.hsla(),
                        Reaction::Down => tw::AMBER_600.hsla(),
                    }
                } else {
                    colors.muted_foreground
                }))
        };
        div()
            .flex()
            .items_center()
            .gap(px(2.0))
            .child(button(Reaction::Up, "thumbs-up", cx))
            .child(button(Reaction::Down, "thumbs-down", cx))
            .into_any_element()
    }

    /// "Guardar como regra?" under the answer just marked 👎.
    pub(super) fn rule_field(&self, line: &RoomLine, cx: &mut Context<Self>) -> Option<AnyElement> {
        let draft = self
            .rule
            .as_ref()
            .filter(|draft| draft.line_key == line.key)?;
        let colors = cx.theme().colors.clone();
        Some(
            div()
                .debug_selector(|| "room-rule".into())
                .mt(px(6.0))
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .flex_none()
                        .text_size(px(12.0))
                        .text_color(colors.muted_foreground)
                        .child(cx.t("agents.reactions.ruleAsk")),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .h(px(30.0))
                        .px(px(10.0))
                        .flex()
                        .items_center()
                        .rounded(px(8.0))
                        .border_1()
                        .border_color(colors.input)
                        .bg(crate::ui::input_fill(cx.theme()))
                        .text_size(px(12.5))
                        .child(draft.input.clone()),
                )
                .child(
                    Button::new("room-rule-keep")
                        .variant(ButtonVariant::Outline)
                        .size(ButtonSize::Sm)
                        .on_click(cx.listener(|room, _, _, cx| room.keep_rule(cx)))
                        .child(cx.t("agents.reactions.keep")),
                )
                .child(
                    Button::new("room-rule-skip")
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Sm)
                        .on_click(cx.listener(|room, _, _, cx| {
                            room.rule = None;
                            cx.notify();
                        }))
                        .child(cx.t("agents.reactions.skip")),
                )
                .into_any_element(),
        )
    }
}

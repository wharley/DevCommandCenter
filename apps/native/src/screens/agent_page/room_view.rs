//! How the team room draws: the conversation (the person's messages to the
//! right with who took them, each answer with its agent's mascot and
//! receipt), who is at work or waiting, and the field with the team's names
//! to address one and who would answer now.

use gpui::{
    AnyElement, Context, ExternalPaths, FontWeight, IntoElement, Render, SharedString, Window, div,
    list, prelude::*, px,
};

use super::room::TeamRoom;
use crate::agent::AgentAvatarView;
use crate::data::agents::AgentRecord;
use crate::data::team_room::{Receipt, RoomLine};
use crate::i18n::{Args, I18n, Locale, Translate};
use crate::markdown::{MarkdownStyle, render_markdown};
use crate::theme::ActiveTheme;
use crate::ui::{Button, ButtonSize, pulse_spinner};

/// The face's side, and so the column the words line up in.
const FACE: f32 = 32.0;

pub(super) fn english(cx: &gpui::App) -> bool {
    cx.try_global::<I18n>()
        .is_some_and(|i18n| i18n.locale() == Locale::En)
}

impl TeamRoom {
    /// "→ Revisor · pelo papel".
    pub(super) fn routed(
        &self,
        agent: &str,
        text: &str,
        name: &str,
        cx: &gpui::App,
    ) -> SharedString {
        let to = cx.t_with(
            "agents.room.to",
            &Args::default().with("agent", name.to_string()),
        );
        match self.reasons.get(&(agent.to_string(), text.to_string())) {
            Some(reason) => format!("{to} · {}", cx.t(reason.i18n_key())).into(),
            None => to,
        }
    }

    fn receipt_text(receipt: Receipt, cx: &gpui::App) -> Option<String> {
        let mut parts = Vec::new();
        if let Some(seconds) = receipt.seconds {
            parts.push(if seconds < 60 {
                format!("{seconds} s")
            } else {
                format!("{} min {} s", seconds / 60, seconds % 60)
            });
        }
        if let Some(tokens) = receipt.tokens.filter(|tokens| *tokens > 0) {
            parts.push(
                cx.t_with(
                    "agents.room.tokens",
                    &Args::default().with("count", crate::data::goal::format_tokens(tokens)),
                )
                .to_string(),
            );
        }
        (!parts.is_empty()).then(|| parts.join(" · "))
    }

    /// An answer as a flat row: the agent's face, bold name and the time on
    /// one line, the words under; the next answers of a group (`first` is
    /// false) sit under the face with the words alone.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn agent_line(
        &self,
        index: usize,
        first: bool,
        line: &RoomLine,
        agent: &AgentRecord,
        model: &super::room_rows::RoomModel,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let blocks = self
            .markdown
            .borrow_mut()
            .get(&line.key, &line.text, &model.mentions);
        let when = first.then(|| super::room_flow::clock(&line.at, english(cx)));
        let receipt = line
            .receipt
            .and_then(|receipt| Self::receipt_text(receipt, cx));
        let agent_id = agent.id.clone();
        div()
            .debug_selector(move || format!("room-answer-{index}"))
            .flex()
            .items_start()
            .gap(px(10.0))
            .child(match first {
                true => div()
                    .debug_selector(move || format!("room-face-{index}"))
                    .flex_none()
                    .child(AgentAvatarView::new(
                        SharedString::from(format!("room-face-{index}")),
                        &agent.avatar,
                        None,
                        FACE,
                    ))
                    .into_any_element(),
                // The face's column, empty: the words keep their line.
                false => div().w(px(FACE)).flex_none().into_any_element(),
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .children(when.map(|when| {
                        div()
                            .flex()
                            .items_baseline()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(colors.foreground)
                                    .child(agent.name.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(colors.muted_foreground)
                                    .child(when),
                            )
                    }))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(colors.foreground)
                            .child(render_markdown(
                                &blocks,
                                MarkdownStyle {
                                    size: 13.0,
                                    ..MarkdownStyle::reply()
                                },
                                &format!("room-{}", line.key),
                                window,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .text_size(px(11.0))
                            .text_color(colors.muted_foreground)
                            .children(receipt)
                            .child(
                                div()
                                    .id(SharedString::from(format!("room-open-{index}")))
                                    .debug_selector(move || format!("room-open-{index}"))
                                    .cursor_pointer()
                                    .hover(|link| link.text_color(colors.foreground))
                                    .on_click(cx.listener(move |room, _, _, cx| {
                                        room.open_conversation(&agent_id, cx)
                                    }))
                                    .child(cx.t("agents.room.open")),
                            )
                            .child(self.reaction_buttons(index, line, cx)),
                    )
                    .children(self.report_card(index, line, cx))
                    .children(self.rule_field(line, cx)),
            )
            .into_any_element()
    }

    fn composer(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let drop_border = crate::theme::color::mix_oklab(colors.accent_calm, 0.7, colors.border);
        let text = self.input.read(cx).text().trim().to_string();
        let next = (!text.is_empty())
            .then(|| self.pick(&text, cx))
            .flatten()
            .map(|(agent, reason)| {
                cx.t_with(
                    "agents.room.next",
                    &Args::default()
                        .with("agent", agent.name.clone())
                        .with("reason", cx.t(reason.i18n_key()).to_string()),
                )
            })
            .unwrap_or_else(|| cx.t("agents.room.hint"));
        let blank = text.is_empty();
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .px(px(24.0))
            .py(px(16.0))
            .border_t_1()
            .border_color(colors.border.opacity(0.7))
            .child(
                div()
                    .flex()
                    .items_end()
                    .gap(px(8.0))
                    .child(
                        div()
                            .id("room-input")
                            .debug_selector(|| "room-input".into())
                            .relative()
                            .on_drop(cx.listener(|room, paths: &ExternalPaths, _, cx| {
                                room.add_paths(paths.paths(), cx)
                            }))
                            .drag_over::<ExternalPaths>(move |style, _, _, _| {
                                style.border_color(drop_border)
                            })
                            .flex_1()
                            .min_w_0()
                            .min_h(px(40.0))
                            .px(px(12.0))
                            .py(px(10.0))
                            .rounded(px(12.0))
                            .border_1()
                            .border_color(colors.input)
                            .bg(crate::ui::input_fill(cx.theme()))
                            .text_size(px(13.0))
                            .child(self.input.clone())
                            .children(self.mention_popup(cx)),
                    )
                    .child(
                        Button::new("room-send")
                            .size(ButtonSize::Default)
                            .disabled(blank || self.sending)
                            .on_click(cx.listener(|room, _, _, cx| room.send(cx)))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .when(self.sending, |label| {
                                        label.child(pulse_spinner("room-sending", 14.0))
                                    })
                                    .child(cx.t("agents.room.send")),
                            ),
                    ),
            )
            .child(
                div()
                    .debug_selector(|| "room-next".into())
                    .text_size(px(11.0))
                    .text_color(colors.muted_foreground)
                    .child(next),
            )
            .into_any_element()
    }
}

impl Render for TeamRoom {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors.clone();
        // The rows are read again only when the room's data moved, not on a
        // frame the shared clock alone asked for (`room_list`).
        let scope = self.scope();
        if self.list.needs_build(&scope) {
            let model = self.build_rows(cx);
            {
                let mut markdown = self.markdown.borrow_mut();
                markdown.roster(&model.mentions);
                markdown.retain(model.answers());
            }
            self.list.take(model);
        }
        let whole = self.focus.is_none();
        let unread_bar = self.unread_bar(self.list.model().unread, cx);
        let header = whole.then(|| {
            div()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .px(px(24.0))
                .pt(px(20.0))
                .pb(px(12.0))
                .border_b_1()
                .border_color(colors.border.opacity(0.7))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .text_size(px(16.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(colors.foreground)
                                .debug_selector(|| "room-title".into())
                                .child(match self.room_name(cx) {
                                    Some(project) => cx.t_with(
                                        "agents.room.projectTitle",
                                        &Args::default().with("project", project),
                                    ),
                                    None => cx.t("agents.room.title"),
                                }),
                        )
                        .children(self.pause_button(cx)),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(colors.muted_foreground)
                        .child(match self.room_name(cx) {
                            Some(project) => cx.t_with(
                                "agents.room.projectSubtitle",
                                &Args::default().with("project", project),
                            ),
                            None => cx.t("agents.room.subtitle"),
                        }),
                )
                .children(unread_bar)
        });
        let room = cx.entity();
        div()
            .debug_selector(|| "team-room".into())
            .flex()
            .flex_col()
            .size_full()
            .min_h_0()
            .bg(colors.background)
            // On one agent, its page draws the head.
            .children(header)
            .child(
                div().id("room-lines").flex_1().min_h_0().child(
                    list(self.list.state.clone(), move |index, window, cx| {
                        room.update(cx, |room, cx| room.render_item(index, window, cx))
                    })
                    .size_full(),
                ),
            )
            .child(self.composer(cx))
    }
}

//! How the team room draws: the conversation (the person's messages to the
//! right with who took them, each answer with its agent's mascot and
//! receipt), who is at work or waiting, and the field with the team's names
//! to address one and who would answer now.

use dcc_core::domain::team_room::handoff_heading;
use gpui::{
    AnyElement, Context, FontWeight, IntoElement, Render, SharedString, Window, div, prelude::*, px,
};

use super::room::{Pending, TeamRoom};
use crate::agent::{AgentAvatarView, Mood, MoodAvatarView};
use crate::data::agents::{AgentRecord, AgentState};
use crate::data::team_room::{Receipt, RoomLine, Speaker};
use crate::i18n::{Args, I18n, Locale, Translate};
use crate::markdown::{MarkdownStyle, parse, render_markdown};
use crate::theme::ActiveTheme;
use crate::ui::{Button, ButtonSize, spinner};

fn english(cx: &gpui::App) -> bool {
    cx.try_global::<I18n>()
        .is_some_and(|i18n| i18n.locale() == Locale::En)
}

impl TeamRoom {
    fn agent<'a>(agents: &'a [AgentRecord], id: &str) -> Option<&'a AgentRecord> {
        agents.iter().find(|agent| agent.id == id)
    }

    /// "→ Revisor · pelo papel".
    fn routed(&self, agent: &str, text: &str, name: &str, cx: &gpui::App) -> SharedString {
        let to = cx.t_with(
            "agents.room.to",
            &Args::default().with("agent", name.to_string()),
        );
        match self.reasons.get(&(agent.to_string(), text.to_string())) {
            Some(reason) => format!("{to} · {}", cx.t(reason.i18n_key())).into(),
            None => to,
        }
    }

    fn person_line(
        &self,
        key: String,
        text: String,
        note: SharedString,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        div()
            .debug_selector(move || key.clone())
            .flex()
            .flex_col()
            .items_end()
            .gap(px(4.0))
            .child(
                div()
                    .max_w(px(520.0))
                    .px(px(12.0))
                    .py(px(8.0))
                    .rounded(px(14.0))
                    .bg(colors.accent)
                    .text_size(px(13.0))
                    .line_height(px(20.0))
                    .text_color(colors.foreground)
                    .child(text),
            )
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(colors.muted_foreground)
                    .child(note),
            )
            .into_any_element()
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

    fn agent_line(
        &self,
        index: usize,
        line: &RoomLine,
        agent: &AgentRecord,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let blocks = self
            .markdown
            .borrow_mut()
            .entry(format!("{}:{}", line.key, line.text.len()))
            .or_insert_with(|| std::rc::Rc::new(parse(&line.text)))
            .clone();
        let when = crate::data::time::medium_date_time(&line.at, english(cx));
        let receipt = line
            .receipt
            .and_then(|receipt| Self::receipt_text(receipt, cx));
        let agent_id = agent.id.clone();
        div()
            .debug_selector(move || format!("room-answer-{index}"))
            .flex()
            .items_start()
            .gap(px(10.0))
            .child(AgentAvatarView::new(
                SharedString::from(format!("room-face-{index}")),
                &agent.avatar,
                None,
                28.0,
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
                            .flex()
                            .items_baseline()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(colors.foreground)
                                    .child(agent.name.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(colors.muted_foreground)
                                    .child(when),
                            ),
                    )
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
                            ),
                    ),
            )
            .into_any_element()
    }

    /// "Revisor está trabalhando…" / "Revisor precisa de você".
    fn busy_line(
        &self,
        agent: &AgentRecord,
        state: AgentState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let needs_you = state == AgentState::NeedsYou;
        let text = cx.t_with(
            if needs_you {
                "agents.room.needsYou"
            } else {
                "agents.room.working"
            },
            &Args::default().with("agent", agent.name.clone()),
        );
        let agent_id = agent.id.clone();
        let id = agent.id.clone();
        div()
            .debug_selector(move || format!("room-busy-{id}"))
            .flex()
            .items_center()
            .gap(px(10.0))
            .child(MoodAvatarView::new(
                SharedString::from(format!("room-busy-face-{}", agent.id)),
                &agent.avatar,
                if needs_you {
                    Mood::NeedsYou
                } else {
                    Mood::Working
                },
                28.0,
            ))
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(colors.muted_foreground)
                    .child(text),
            )
            .when(needs_you, |row| {
                row.child(
                    div()
                        .id(SharedString::from(format!("room-busy-open-{agent_id}")))
                        .text_size(px(12.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(colors.foreground)
                        .cursor_pointer()
                        .on_click(
                            cx.listener(move |room, _, _, cx| {
                                room.open_conversation(&agent_id, cx)
                            }),
                        )
                        .child(cx.t("agents.room.open")),
                )
            })
            .into_any_element()
    }

    fn composer(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
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
                            .debug_selector(|| "room-input".into())
                            .relative()
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
                                        label.child(spinner("room-sending", 14.0))
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors.clone();
        let agents: Vec<AgentRecord> = self.sidebar.read(cx).agents().to_vec();
        let lines: Vec<RoomLine> = self.lines().into_iter().cloned().collect();
        let cards = self.cards(cx);
        let checks = self.check_ins(cx);
        let unread = cards.iter().filter(|card| card.unread).count()
            + checks.iter().filter(|card| card.unread).count();
        // The room's lines, the inbox's cards and the check-in's, in time order.
        let mut timeline: Vec<(&str, Option<usize>, Option<usize>)> = lines
            .iter()
            .enumerate()
            .map(|(index, line)| (line.at.as_str(), Some(index), None))
            .chain(
                cards
                    .iter()
                    .enumerate()
                    .map(|(index, card)| (card.at.as_str(), None, Some(index))),
            )
            .chain(
                checks
                    .iter()
                    .enumerate()
                    .map(|(index, card)| (card.at.as_str(), None, Some(cards.len() + index))),
            )
            .collect();
        timeline.sort_by(|left, right| left.0.cmp(right.0));
        let mut rows: Vec<AnyElement> = Vec::new();
        for (_, line, card) in timeline {
            if let Some(index) = card.and_then(|index| index.checked_sub(cards.len())) {
                rows.push(self.check_in_row(index, &checks[index], cx));
                continue;
            }
            if let Some(index) = card {
                let card = &cards[index];
                if let Some(agent) = Self::agent(&agents, &card.agent_id) {
                    rows.push(self.card_row(index, card, agent, cx));
                }
                continue;
            }
            let Some(index) = line else {
                continue;
            };
            let line = &lines[index];
            let Some(agent) = Self::agent(&agents, &line.agent_id) else {
                continue;
            };
            match line.speaker {
                Speaker::Person => match handoff_heading(&line.text) {
                    Some(heading) => rows.push(self.passed_line(index, heading, cx)),
                    None => {
                        let note = self.routed(&agent.id, &line.text, &agent.name, cx);
                        rows.push(self.person_line(
                            format!("room-said-{index}"),
                            line.text.clone(),
                            note,
                            cx,
                        ));
                    }
                },
                Speaker::Agent => {
                    rows.push(self.agent_line(index, line, agent, window, cx));
                    rows.extend(self.handoff_bar(index, line, agent, &agents, cx));
                }
            }
        }
        // What DCC itself has for the person now: the day's recap, the space.
        rows.extend(self.recap_card(cx));
        rows.extend(self.cleanup_card(cx));
        let unread_bar = self.unread_bar(unread, cx);
        let pending: Vec<Pending> = self.pending.clone();
        for (index, sent) in pending.iter().enumerate() {
            let name = Self::agent(&agents, &sent.agent_id)
                .map(|agent| agent.name.clone())
                .unwrap_or_default();
            let note: SharedString = format!(
                "{} · {}",
                self.routed(&sent.agent_id, &sent.text, &name, cx),
                cx.t("agents.room.sending")
            )
            .into();
            rows.push(self.person_line(
                format!("room-pending-{index}"),
                sent.text.clone(),
                note,
                cx,
            ));
        }
        for (agent, state) in self.busy(cx) {
            rows.push(self.busy_line(&agent, state, cx));
        }
        if rows.len() != self.shown {
            self.shown = rows.len();
            self.scroll.scroll_to_bottom();
        }
        let empty = rows.is_empty().then(|| {
            div()
                .debug_selector(|| "room-empty".into())
                .text_size(px(13.0))
                .line_height(px(20.0))
                .text_color(colors.muted_foreground)
                .child(cx.t("agents.room.empty"))
        });
        div()
            .debug_selector(|| "team-room".into())
            .flex()
            .flex_col()
            .size_full()
            .min_h_0()
            .bg(colors.background)
            .child(
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
                                    .child(cx.t("agents.room.title")),
                            )
                            .children(self.pause_button(cx)),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(colors.muted_foreground)
                            .child(cx.t("agents.room.subtitle")),
                    )
                    .children(unread_bar),
            )
            .child(
                div()
                    .id("room-lines")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .child(
                        div()
                            .mx_auto()
                            .max_w(px(760.0))
                            .flex()
                            .flex_col()
                            .gap(px(18.0))
                            .px(px(24.0))
                            .py(px(20.0))
                            .children(empty)
                            .children(rows),
                    ),
            )
            .child(self.composer(cx))
    }
}

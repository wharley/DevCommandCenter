//! "Responder" on an agent's answer: the next message goes to that agent,
//! whoever the room would have picked, and opens with the part of the
//! answer it replies to as a quote — so in a room where several agents
//! speak it is plain which answer is meant, to the agent and to whoever
//! reads the room later. A chip over the field says what is being replied
//! to; × (or sending) clears it, and so does another room or agent.

use gpui::{AnyElement, Context, Focusable, SharedString, Window, div, prelude::*, px};

use super::room::TeamRoom;
use crate::data::agents::AgentRecord;
use crate::data::team_room::RoomLine;
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::ui::Icon;

/// How much of the answer is quoted.
const QUOTE_CHARS: usize = 240;

/// The answer the next message replies to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReplyTo {
    pub(crate) key: String,
    pub(crate) agent_id: String,
    pub(crate) agent_name: String,
    pub(crate) quote: String,
    /// The room and agent it was picked in.
    place: (Option<String>, Option<String>),
}

/// The start of `answer` as one line: what the quote holds.
pub(crate) fn quote_of(answer: &str) -> String {
    let flat = answer
        .lines()
        .map(|line| line.trim().trim_start_matches(['#', '>', '-', '*']).trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if flat.chars().count() <= QUOTE_CHARS {
        return flat;
    }
    let cut: String = flat.chars().take(QUOTE_CHARS).collect();
    format!("{}…", cut.trim_end())
}

/// The message as it is sent: the quote, then the person's words.
pub(crate) fn quoted_message(quote: &str, text: &str) -> String {
    format!("> {quote}\n\n{text}")
}

impl TeamRoom {
    /// The next message replies to `line`, an answer of `agent`.
    pub(crate) fn reply_to(
        &mut self,
        line: &RoomLine,
        agent: &AgentRecord,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.reply = Some(ReplyTo {
            key: line.key.clone(),
            agent_id: agent.id.clone(),
            agent_name: agent.name.clone(),
            quote: quote_of(&line.text),
            place: (self.room.clone(), self.focus.clone()),
        });
        let field = self.input.read(cx).focus_handle(cx);
        window.focus(&field, cx);
        self.live.quiet = true;
        cx.notify();
    }

    pub(crate) fn clear_reply(&mut self, cx: &mut Context<Self>) {
        if self.reply.take().is_some() {
            self.live.quiet = true;
            cx.notify();
        }
    }

    /// The reply in effect here: one picked in another room or on another
    /// agent is dropped.
    pub(crate) fn current_reply(&mut self) -> Option<ReplyTo> {
        let here = (self.room.clone(), self.focus.clone());
        if self.reply.as_ref().is_some_and(|reply| reply.place != here) {
            self.reply = None;
        }
        self.reply.clone()
    }

    /// "Responder", among an answer's actions.
    pub(super) fn reply_button(
        &self,
        index: usize,
        line: &RoomLine,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let line = line.clone();
        div()
            .id(SharedString::from(format!("room-reply-{index}")))
            .debug_selector(move || format!("room-reply-{index}"))
            .cursor_pointer()
            .hover(|link| link.text_color(colors.foreground))
            .on_click(cx.listener(move |room, _, window, cx| {
                let agent = super::agent_card::agent_of(&room.sidebar, &line.agent_id, cx);
                if let Some(agent) = agent {
                    room.reply_to(&line, &agent, window, cx)
                }
            }))
            .child(cx.t("agents.room.reply.action"))
            .into_any_element()
    }

    /// The chip over the field, while a reply is being written.
    pub(super) fn reply_chip(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let reply = self.current_reply()?;
        let colors = cx.theme().colors.clone();
        let to = cx.t_with(
            "agents.room.reply.to",
            &Args::default().with("agent", reply.agent_name.clone()),
        );
        Some(
            div()
                .debug_selector(|| "room-reply-chip".into())
                .flex()
                .items_center()
                .gap(px(8.0))
                .px(px(14.0))
                .pt(px(10.0))
                .text_size(px(12.0))
                .text_color(colors.muted_foreground)
                .child(Icon::new("reply").size(13.0))
                .child(div().flex_none().text_color(colors.foreground).child(to))
                .child(div().flex_1().min_w_0().truncate().child(reply.quote))
                .child(
                    div()
                        .id("room-reply-clear")
                        .debug_selector(|| "room-reply-clear".into())
                        .flex_none()
                        .cursor_pointer()
                        .hover(|close| close.text_color(colors.foreground))
                        .on_click(cx.listener(|room, _, _, cx| room.clear_reply(cx)))
                        .child(Icon::new("x").size(13.0)),
                )
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use gpui::{Modifiers, TestAppContext};

    use super::super::room_list_tests::room_with_lines;
    use super::*;
    use crate::data::agents::AgentState;
    use crate::data::team_room::Speaker;

    #[test]
    fn a_quote_is_the_answers_start_on_one_line() {
        assert_eq!(
            quote_of("## Resumo\n\n- Abri a tarefa\n- O PR está pronto"),
            "Resumo Abri a tarefa O PR está pronto"
        );
        let long = quote_of(&"palavra ".repeat(100));
        assert_eq!(long.chars().count(), QUOTE_CHARS);
        assert!(long.ends_with('…'));
        assert_eq!(
            quoted_message("Abri a tarefa", "pode mesclar"),
            "> Abri a tarefa\n\npode mesclar"
        );
    }

    #[gpui::test]
    fn replying_to_an_answer_shows_what_is_replied_to(cx: &mut TestAppContext) {
        let line = |index: usize, speaker: Speaker, text: &str| RoomLine {
            key: format!("l-{index}"),
            agent_id: "dev".into(),
            session_id: "s-geral".into(),
            speaker,
            text: text.into(),
            at: format!("2026-11-10T10:0{index}:00Z"),
            receipt: None,
            tasks: Vec::new(),
            steps: Default::default(),
        };
        let (page, room, cx) = room_with_lines(
            cx,
            vec![
                line(0, Speaker::Person, "corrige o login"),
                line(1, Speaker::Agent, "Abri a tarefa e o PR está pronto."),
            ],
            AgentState::Idle,
        );
        assert!(cx.debug_bounds("room-reply-chip").is_none());
        let reply = cx.debug_bounds("room-reply-1").unwrap();
        cx.simulate_click(reply.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(cx.debug_bounds("room-reply-chip").is_some());
        room.read_with(cx, |room, _| {
            let reply = room.reply.as_ref().unwrap();
            assert_eq!(reply.agent_id, "dev");
            assert_eq!(reply.quote, "Abri a tarefa e o PR está pronto.");
        });
        // The field has the keys: what is typed is the reply.
        cx.simulate_input("pode mesclar");
        cx.run_until_parked();
        assert_eq!(
            room.read_with(cx, |room, cx| room.input.read(cx).text().to_string()),
            "pode mesclar"
        );
        // × drops it.
        let clear = cx.debug_bounds("room-reply-clear").unwrap();
        cx.simulate_click(clear.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(cx.debug_bounds("room-reply-chip").is_none());

        // One picked here does not follow to another agent's conversation.
        let reply = cx.debug_bounds("room-reply-1").unwrap();
        cx.simulate_click(reply.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(room.read_with(cx, |room, _| room.reply.is_some()));
        page.update(cx, |page, cx| page.show_conversation("dev".into(), cx));
        cx.run_until_parked();
        assert!(room.update(cx, |room, _| room.current_reply()).is_none());
    }
}

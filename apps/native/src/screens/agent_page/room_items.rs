//! Each row of the room's list drawn when it shows (`room_list`): a line
//! (the person's, a turn passed, a routine's or a source's, an answer with
//! its handoff), a card, a check-in, the day's recap, the week; and under
//! them all the live part, drawn fresh each frame: DCC's own cards for now,
//! the messages on their way, who is at work, the approvals, who is busy.

use std::rc::Rc;

use dcc_core::domain::team_room::handoff_heading;
use gpui::{AnyElement, Context, IntoElement, SharedString, Window, div, prelude::*, px};

use super::room::{Pending, TeamRoom};
use super::room_rows::{RoomModel, Row};
use super::room_said::Said;
use super::room_steps::StepsRow;
use crate::data::agent_mentions::MentionNames;
use crate::data::agents::{AgentRecord, AgentState};
use crate::data::team_room::{RoomLine, Speaker};
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;

/// Between rows, as the column's `gap` was.
const GAP: f32 = 18.0;
/// Above the first row and under the last.
const EDGE: f32 = 20.0;

/// Between an answer and the next of its group.
const TIGHT: f32 = 4.0;

/// `content` in the column's width; nothing at all when it draws nothing.
/// `tight` when it joins the row above (an answer in a group).
fn item(content: Vec<AnyElement>, first: bool, last: bool, tight: bool) -> AnyElement {
    if content.is_empty() {
        return div().when(last, |end| end.h(px(EDGE))).into_any_element();
    }
    div()
        .w_full()
        .flex()
        .justify_center()
        .child(
            div()
                .w_full()
                .max_w(px(760.0))
                .flex()
                .flex_col()
                .gap(px(GAP))
                .px(px(24.0))
                .pt(px(if first {
                    EDGE
                } else if tight {
                    TIGHT
                } else {
                    GAP
                }))
                .when(last, |end| end.pb(px(EDGE)))
                .children(content),
        )
        .into_any_element()
}

impl TeamRoom {
    /// The list's item `index`: a row, or the live part after the last.
    pub(super) fn render_item(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        #[cfg(test)]
        self.list.rows_built.set(self.list.rows_built.get() + 1);
        let model = self.list.model();
        let content = match model.rows.get(index) {
            Some(row) => self.row(row, &model, window, cx),
            None => self.tail(&model, window, cx),
        };
        let tight = matches!(model.rows.get(index), Some(Row::Line { first: false, .. }));
        item(content, index == 0, index >= model.rows.len(), tight)
    }

    fn row(
        &mut self,
        row: &Row,
        model: &RoomModel,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        match row {
            Row::CheckIn { index, check } => vec![self.check_in_row(*index, check, cx)],
            Row::Card { index, card } => model
                .agent(&card.agent_id)
                .map(|agent| self.card_row(*index, card, agent, cx))
                .into_iter()
                .collect(),
            Row::Recap => self.recap_card(cx).into_iter().collect(),
            Row::Bulletin => vec![self.bulletin_card(&model.agents, cx)],
            Row::Day { day } => vec![self.day_row(*day, cx)],
            Row::Line {
                index,
                line,
                first,
                said,
                steps,
            } => match model.agent(&line.agent_id) {
                Some(agent) => self.line_rows(
                    (*index, *first, said.as_ref(), steps.as_ref()),
                    line,
                    agent,
                    model,
                    window,
                    cx,
                ),
                None => Vec::new(),
            },
        }
    }

    fn line_rows(
        &self,
        (index, first, said, steps): (usize, bool, Option<&Rc<Said>>, Option<&Rc<StepsRow>>),
        line: &RoomLine,
        agent: &AgentRecord,
        model: &RoomModel,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        if line.speaker == Speaker::Agent {
            let mut rows =
                vec![self.agent_line(index, (first, steps), line, agent, model, window, cx)];
            rows.extend(self.handoff_bar(index, line, agent, &model.agents, cx));
            return rows;
        }
        if let Some(heading) = handoff_heading(&line.text) {
            let automatic = dcc_core::domain::agent_chain::passed_answer(&line.text).is_some();
            return vec![self.passed_line(index, heading, automatic, cx)];
        }
        if line
            .text
            .starts_with(dcc_core::domain::agent_routine::ROUTINE_MARK)
        {
            return self
                .routine_line(index, &line.text, cx)
                .into_iter()
                .collect();
        }
        if line
            .text
            .starts_with(dcc_core::domain::agent_source::SOURCE_MARK)
        {
            return self
                .source_line(index, &line.text, cx)
                .into_iter()
                .collect();
        }
        let note = if self.focus.is_none() {
            self.routed(&agent.id, &line.text, &agent.name, cx)
        } else {
            SharedString::default()
        };
        let said = said
            .cloned()
            .unwrap_or_else(|| Rc::new(Said::new(&line.text, &model.mentions)));
        vec![self.person_line(format!("room-said-{index}"), &said, note, cx)]
    }

    /// What holds now, after everything: read fresh on each frame it shows.
    fn tail(
        &mut self,
        model: &RoomModel,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let agents = &model.agents;
        let whole = self.focus.is_none();
        let geral = self.is_geral();
        let shown: Vec<AgentRecord> = agents
            .iter()
            .filter(|agent| self.shows(&agent.id))
            .cloned()
            .collect();
        let mut rows: Vec<AnyElement> = Vec::new();
        // What DCC itself has for the person now: the space.
        if whole && geral {
            rows.extend(self.cleanup_card(cx));
        }
        if geral {
            rows.extend(self.limit_rows(&shown, cx));
            rows.extend(self.routine_cards(&shown, cx));
            rows.extend(self.proposal_cards(agents, cx));
        }
        // A source's proposal shows in the room it was asked in.
        rows.extend(self.source_cards(&shown, cx));
        // A reply waits under the answers, in the room it was asked in.
        rows.extend(self.reply_cards(&shown, cx));
        rows.extend(self.pending_rows(agents, &model.mentions, whole, cx));
        // Tasks opened here that still run.
        rows.extend(self.running_rows(agents, cx));
        // Agents at work in this project's tasks, however they got there.
        rows.extend(self.at_work_rows(agents, cx));
        // A trigger held for the day, said here.
        rows.extend(self.held_rows(agents, cx));
        // The room's own conversations at work, as they go
        // (`room_live_turn`), and the last turn that stopped.
        let (live, working) = self.live_turn_rows(agents, &model.mentions, window, cx);
        rows.extend(live);
        rows.extend(self.stopped_rows(agents, &working, cx));
        // An approval or a question is answered in the dock above the field;
        // its agent needs no "precisa de você".
        let asking: Vec<String> = self
            .approvals(agents)
            .into_iter()
            .map(|approval| approval.agent.id)
            .chain(
                self.questions(agents)
                    .into_iter()
                    .map(|question| question.agent.id),
            )
            .collect();
        for (agent, state) in self
            .busy(cx)
            .into_iter()
            .filter(|(agent, _)| self.shows(&agent.id))
            // An agent shown at work live needs no "está trabalhando…".
            .filter(|(agent, state)| match state {
                AgentState::NeedsYou => !asking.contains(&agent.id),
                AgentState::Working => !working.contains(&agent.id),
                _ => true,
            })
        {
            rows.push(self.busy_line(&agent, state, cx));
        }
        if rows.is_empty() && model.rows.is_empty() {
            rows.push(self.empty_line(cx));
        }
        rows
    }

    /// The person's messages on their way.
    fn pending_rows(
        &self,
        agents: &[AgentRecord],
        names: &MentionNames,
        whole: bool,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let pending: Vec<Pending> = self
            .pending
            .iter()
            .filter(|sent| self.shows(&sent.agent_id))
            .cloned()
            .collect();
        pending
            .into_iter()
            .enumerate()
            .map(|(index, sent)| {
                let name = agents
                    .iter()
                    .find(|agent| agent.id == sent.agent_id)
                    .map(|agent| agent.name.clone())
                    .unwrap_or_default();
                let state = self.pending_note(&sent, &name, cx);
                let note: SharedString = if whole {
                    format!(
                        "{} · {state}",
                        self.routed(&sent.agent_id, &sent.text, &name, cx),
                    )
                    .into()
                } else {
                    state
                };
                let said = Said::new(&sent.text, names);
                let bubble = self.person_line(format!("room-pending-{index}"), &said, note, cx);
                self.with_unqueue(index, &sent, bubble, cx)
            })
            .collect()
    }

    fn empty_line(&self, cx: &mut Context<Self>) -> AnyElement {
        let text = match self.focused(cx) {
            Some(agent) => cx.t_with(
                "agents.room.emptyAgent",
                &Args::default().with("agent", agent.name),
            ),
            None => cx.t("agents.room.empty"),
        };
        div()
            .debug_selector(|| "room-empty".into())
            .text_size(px(13.0))
            .line_height(px(20.0))
            .text_color(cx.theme().colors.muted_foreground)
            .child(text)
            .into_any_element()
    }
}

//! What the room's conversation is made of, as data: the lines, the cards
//! and check-ins in their project's room, DCC's own cards at their time
//! (`room_timeline`). Read again only when the room's data moves
//! (`room_list`); each row is drawn when it shows (`room_items`).

use dcc_core::domain::agent_routine::is_quiet;
use gpui::Context;

use super::room::TeamRoom;
use super::room_timeline::Slot;
use crate::data::agents::AgentRecord;
use crate::data::check_ins::CheckIn;
use crate::data::room_inbox::InboxCard;
use crate::data::team_room::{RoomLine, Speaker};

/// What the rows were built for: the room, the agent shown alone, and the
/// projects that have a room (where each card goes).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Scope {
    pub room: Option<String>,
    pub focus: Option<String>,
    pub rooms: Vec<String>,
}

/// One row of the conversation. `index` is its place among its kind, as
/// the tests know it (`room-said-3`, `room-card-0`).
#[derive(Clone, Debug)]
pub(crate) enum Row {
    CheckIn { index: usize, check: CheckIn },
    Card { index: usize, card: InboxCard },
    Recap,
    Bulletin,
    Line { index: usize, line: RoomLine },
}

impl Row {
    /// Whether `other` draws the same, wherever it sits: a row that moved
    /// down keeps its height.
    pub(crate) fn same(&self, other: &Row) -> bool {
        match (self, other) {
            (Row::CheckIn { check: a, .. }, Row::CheckIn { check: b, .. }) => a == b,
            (Row::Card { card: a, .. }, Row::Card { card: b, .. }) => a == b,
            (Row::Recap, Row::Recap) | (Row::Bulletin, Row::Bulletin) => true,
            (Row::Line { line: a, .. }, Row::Line { line: b, .. }) => a == b,
            _ => false,
        }
    }
}

#[derive(Default)]
pub(crate) struct RoomModel {
    pub scope: Scope,
    pub agents: Vec<AgentRecord>,
    pub rows: Vec<Row>,
    /// Unread cards and check-ins, for "Marcar tudo como lido".
    pub unread: usize,
}

impl RoomModel {
    pub(crate) fn agent(&self, id: &str) -> Option<&AgentRecord> {
        self.agents.iter().find(|agent| agent.id == id)
    }

    /// The answers' lines, for the markdown kept by line.
    pub(crate) fn answers(&self) -> impl Iterator<Item = (&str, &str)> {
        self.rows.iter().filter_map(|row| match row {
            Row::Line { line, .. } if line.speaker == Speaker::Agent => {
                Some((line.key.as_str(), line.text.as_str()))
            }
            _ => None,
        })
    }
}

impl TeamRoom {
    pub(crate) fn scope(&self) -> Scope {
        Scope {
            room: self.room.clone(),
            focus: self.focus.clone(),
            rooms: self.rooms.clone(),
        }
    }

    /// The room's rows now.
    pub(super) fn build_rows(&self, cx: &mut Context<Self>) -> RoomModel {
        let agents: Vec<AgentRecord> = self.sidebar.read(cx).agents().to_vec();
        let mut lines: Vec<Option<RoomLine>> = self
            .lines()
            .into_iter()
            .filter(|line| self.shows(&line.agent_id))
            .map(|line| Some(line.clone()))
            .collect();
        // Each card in its project's room, else in Geral; the rest of DCC's
        // own (the day, the week, limits, proposals) stays in Geral.
        let geral = self.is_geral();
        let mut cards = self.cards(cx);
        cards.retain(|card| {
            self.shows(&card.agent_id)
                && self.card_room(card, cx) == self.room
                && !self.asked_in(card, &self.room, cx)
        });
        // On one agent, only what is its own; the room's cards stay there.
        let whole = self.focus.is_none();
        let checks: Vec<CheckIn> = if whole {
            self.check_ins(cx)
                .into_iter()
                .filter(|check| self.check_in_room(check) == self.room)
                .collect()
        } else {
            Vec::new()
        };
        let unread = cards.iter().filter(|card| card.unread).count()
            + checks.iter().filter(|card| card.unread).count();
        // DCC's own cards sit at their time (`room_timeline`).
        let recap_at = (whole && geral)
            .then(|| self.sidebar.read(cx).recap_window())
            .flatten()
            .map(|window| window.to);
        let bulletin_at = (whole && geral)
            .then(|| dcc_core::domain::agent_bulletin::week_before(chrono::Local::now(), 1).to);
        let timeline = super::room_timeline::order(
            lines.iter().flatten().map(|line| line.at.as_str()),
            cards.iter().map(|card| card.at.as_str()),
            checks.iter().map(|card| card.at.as_str()),
            recap_at.as_deref(),
            bulletin_at.as_deref(),
        );
        let known = |id: &str| agents.iter().any(|agent| agent.id == id);
        let mut cards: Vec<Option<InboxCard>> = cards.into_iter().map(Some).collect();
        let mut checks: Vec<Option<CheckIn>> = checks.into_iter().map(Some).collect();
        let mut rows = Vec::with_capacity(timeline.len());
        for slot in timeline {
            match slot {
                Slot::CheckIn(index) => {
                    if let Some(check) = checks[index].take() {
                        rows.push(Row::CheckIn { index, check });
                    }
                }
                Slot::Card(index) => {
                    if let Some(card) = cards[index].take().filter(|card| known(&card.agent_id)) {
                        rows.push(Row::Card { index, card });
                    }
                }
                Slot::Recap => rows.push(Row::Recap),
                Slot::Bulletin => rows.push(Row::Bulletin),
                Slot::Line(index) => {
                    let Some(line) = lines[index].take() else {
                        continue;
                    };
                    // A routine with nothing to report stays quiet.
                    let quiet = line.speaker == Speaker::Agent && is_quiet(&line.text);
                    if known(&line.agent_id) && !quiet {
                        rows.push(Row::Line { index, line });
                    }
                }
            }
        }
        RoomModel {
            scope: self.scope(),
            agents,
            rows,
            unread,
        }
    }
}

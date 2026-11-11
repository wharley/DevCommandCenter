//! The team room's lines as the desktop reads them
//! (`dcc_core::domain::team_room_lines`): all the agents together in time
//! order. Each conversation is read from where it was last read, so a
//! refresh costs only its new events. Blocking: call `refresh` off the UI
//! thread.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use dcc_core::domain::agent_reaction::Reaction;
use dcc_core::domain::session::{SessionEventRecord, SessionId};
pub use dcc_core::domain::team_room_lines::{Receipt, RoomLine, Speaker, last_answer, lines_of};
use dcc_core::domain::team_room_live::{TurnNow, turn_now};
use dcc_core::domain::timeline::activity::total_turn_tokens;
use dcc_core::domain::timeline::project_session;
use dcc_core::ports::UsageRepo;
use dcc_infra::db::SqliteSessionRepo;
use futures::executor::block_on;

/// One agent's conversation as the room has read it.
#[derive(Clone, Debug, Default)]
pub struct HomeFeed {
    pub agent_id: String,
    pub session_id: String,
    /// The conversation's `updated_at` when it was read.
    pub read_at: String,
    /// Shared, so a poll off the UI thread takes them without a copy
    /// (`team_room_poll`).
    pub(crate) events: Arc<Vec<SessionEventRecord>>,
    pub lines: Vec<RoomLine>,
    /// The person's 👍 / 👎 by answer (`agent_reaction`).
    pub reactions: HashMap<String, Reaction>,
    /// The last turn now: at work, stopped, or neither (`team_room_live`).
    pub now: Arc<TurnNow>,
}

impl HomeFeed {
    pub fn new(agent_id: &str, session_id: &str) -> Self {
        Self {
            agent_id: agent_id.to_string(),
            session_id: session_id.to_string(),
            ..Self::default()
        }
    }

    /// A conversation as read, for tests.
    #[cfg(test)]
    pub fn with_events(agent_id: &str, session_id: &str, events: Vec<SessionEventRecord>) -> Self {
        Self {
            events: Arc::new(events),
            ..Self::new(agent_id, session_id)
        }
    }

    /// A conversation as read, its lines and its last turn projected from
    /// `events` as a refresh would (no tokens), for tests.
    #[cfg(test)]
    pub fn projected(agent_id: &str, session_id: &str, events: Vec<SessionEventRecord>) -> Self {
        let mut feed = Self::with_events(agent_id, session_id, events);
        feed.project(&HashMap::new());
        feed
    }

    /// The last event read; a read with more is the newer one.
    pub fn last_sequence(&self) -> u64 {
        self.events.last().map_or(0, |event| event.sequence)
    }

    fn project(&mut self, tokens: &HashMap<String, u64>) {
        let messages = project_session(&self.events, &self.session_id);
        self.lines = lines_of(&self.agent_id, &self.session_id, &messages, tokens);
        self.now = Arc::new(turn_now(&messages));
    }

    /// The approval the conversation waits on, if any (`room_approvals`).
    pub fn pending_approval(&self) -> Option<super::room_approvals::PendingApproval> {
        super::room_approvals::pending_approval(&self.events)
    }

    /// The question the conversation waits on, if any (`room_questions`).
    pub fn pending_question(&self) -> Option<super::room_questions::PendingQuestion> {
        super::room_questions::pending_question(&self.events)
    }

    /// The tool calls of `turn`, listed when its steps are opened: the
    /// conversation is projected once, on that click.
    pub fn steps_of(&self, turn: &str) -> Vec<dcc_core::domain::team_room_steps::Step> {
        let messages = project_session(&self.events, &self.session_id);
        dcc_core::domain::team_room_steps::steps_of(&messages, turn)
    }

    /// Reads the events after the ones it has and its turns' tokens.
    pub fn refresh(mut self, db: &Path, updated_at: &str) -> anyhow::Result<Self> {
        let sessions = SqliteSessionRepo::open_read_only(db)?;
        let fresh =
            super::conversation::events_after(&sessions, &self.session_id, self.last_sequence())?;
        Arc::make_mut(&mut self.events).extend(fresh);
        self.read_all(&sessions)?;
        self.read_at = updated_at.to_string();
        Ok(self)
    }

    /// The lines, the last turn, the tokens and the reactions from the
    /// events read.
    pub(crate) fn read_all(&mut self, sessions: &SqliteSessionRepo) -> anyhow::Result<()> {
        let id = SessionId(self.session_id.clone());
        let tokens: HashMap<String, u64> = block_on(sessions.list_session_turn_usage(&id))?
            .iter()
            .filter_map(|turn| {
                total_turn_tokens(&turn.models).map(|totals| (turn.turn_id.clone(), totals.total))
            })
            .collect();
        self.project(&tokens);
        // A database older than the reactions has none yet.
        self.reactions = sessions
            .session_reactions(&self.session_id)
            .unwrap_or_default()
            .into_iter()
            .collect();
        Ok(())
    }
}

/// Every conversation's lines in time order.
pub fn merged(feeds: &[HomeFeed]) -> Vec<&RoomLine> {
    let mut lines: Vec<&RoomLine> = feeds.iter().flat_map(|feed| &feed.lines).collect();
    lines.sort_by(|left, right| left.at.cmp(&right.at));
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(agent: &str, text: &str, at: &str) -> RoomLine {
        RoomLine {
            key: text.into(),
            agent_id: agent.into(),
            session_id: format!("s-{agent}"),
            speaker: Speaker::Agent,
            text: text.into(),
            at: at.into(),
            receipt: None,
            tasks: Vec::new(),
            steps: Default::default(),
        }
    }

    #[test]
    fn two_conversations_merge_by_time() {
        let feeds = vec![
            HomeFeed {
                lines: vec![
                    line("rev", "revisa o PR", "2026-11-07T10:00:00Z"),
                    line("rev", "Tudo certo.", "2026-11-07T10:00:42Z"),
                ],
                ..HomeFeed::default()
            },
            HomeFeed {
                lines: vec![line("res", "achei", "2026-11-07T10:00:30Z")],
                ..HomeFeed::default()
            },
        ];
        let order: Vec<&str> = merged(&feeds)
            .iter()
            .map(|line| line.text.as_str())
            .collect();
        assert_eq!(order, ["revisa o PR", "achei", "Tudo certo."]);
    }
}

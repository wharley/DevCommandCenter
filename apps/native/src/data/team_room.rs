//! The team room's lines as the desktop reads them
//! (`dcc_core::domain::team_room_lines`): all the agents together in time
//! order. Each conversation is read from where it was last read, so a
//! refresh costs only its new events. Blocking: call `refresh` off the UI
//! thread.

use std::collections::HashMap;
use std::path::Path;

use dcc_core::domain::session::{SessionEventRecord, SessionId};
pub use dcc_core::domain::team_room_lines::{Receipt, RoomLine, Speaker, last_answer, lines_of};
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
    events: Vec<SessionEventRecord>,
    pub lines: Vec<RoomLine>,
}

impl HomeFeed {
    pub fn new(agent_id: &str, session_id: &str) -> Self {
        Self {
            agent_id: agent_id.to_string(),
            session_id: session_id.to_string(),
            ..Self::default()
        }
    }

    /// Reads the events after the ones it has and its turns' tokens.
    pub fn refresh(mut self, db: &Path, updated_at: &str) -> anyhow::Result<Self> {
        let sessions = SqliteSessionRepo::open_read_only(db)?;
        let after = self.events.last().map_or(0, |event| event.sequence);
        let fresh = super::conversation::events_after(&sessions, &self.session_id, after)?;
        self.events.extend(fresh);
        let id = SessionId(self.session_id.clone());
        let tokens: HashMap<String, u64> = block_on(sessions.list_session_turn_usage(&id))?
            .iter()
            .filter_map(|turn| {
                total_turn_tokens(&turn.models).map(|totals| (turn.turn_id.clone(), totals.total))
            })
            .collect();
        let messages = project_session(&self.events, &self.session_id);
        self.lines = lines_of(&self.agent_id, &self.session_id, &messages, &tokens);
        self.read_at = updated_at.to_string();
        Ok(self)
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

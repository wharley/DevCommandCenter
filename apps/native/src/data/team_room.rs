//! The team room's lines (`dcc_core::domain::team_room`): what the person
//! said to each agent in its own conversation and the agent's answer, all
//! the agents together in time order, each answer with its receipt (how
//! long it took, how many tokens). Each conversation is read from where it
//! was last read, so a refresh costs only its new events. Blocking: call
//! `refresh` off the UI thread.

use std::collections::HashMap;
use std::path::Path;

use dcc_core::domain::session::{AssistantMessagePhase, SessionEventRecord, SessionId};
use dcc_core::domain::timeline::activity::total_turn_tokens;
use dcc_core::domain::timeline::{Role, TimelineMessage, project_session};
use dcc_core::ports::UsageRepo;
use dcc_infra::db::SqliteSessionRepo;
use futures::executor::block_on;

/// Who said a line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Speaker {
    Person,
    Agent,
}

/// How an answer went.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Receipt {
    pub seconds: Option<i64>,
    pub tokens: Option<u64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RoomLine {
    pub key: String,
    pub agent_id: String,
    pub session_id: String,
    pub speaker: Speaker,
    pub text: String,
    /// When it was said (RFC 3339).
    pub at: String,
    pub receipt: Option<Receipt>,
}

fn seconds_between(start: &str, end: &str) -> Option<i64> {
    let start = chrono::DateTime::parse_from_rfc3339(start).ok()?;
    let end = chrono::DateTime::parse_from_rfc3339(end).ok()?;
    Some((end - start).num_seconds().max(0))
}

/// One conversation's lines: the person's messages (not DCC's own turns)
/// and, per finished turn, the agent's last answer.
pub fn lines_of(
    agent_id: &str,
    session_id: &str,
    messages: &[TimelineMessage],
    tokens: &HashMap<String, u64>,
) -> Vec<RoomLine> {
    let mut lines: Vec<RoomLine> = Vec::new();
    // The last answer of each turn, by its turn.
    let mut answer_of: HashMap<&str, usize> = HashMap::new();
    for message in messages {
        let at = message
            .created_at
            .clone()
            .or_else(|| message.turn_ended_at.clone())
            .unwrap_or_default();
        match message.role {
            Role::User if message.hand_back.is_none() && !message.content.trim().is_empty() => {
                lines.push(RoomLine {
                    key: format!("{session_id}:{}", message.id),
                    agent_id: agent_id.to_string(),
                    session_id: session_id.to_string(),
                    speaker: Speaker::Person,
                    text: message.content.trim().to_string(),
                    at,
                    receipt: None,
                });
            }
            Role::Assistant
                if message.assistant_phase != Some(AssistantMessagePhase::Commentary)
                    && !message.streaming
                    && message.turn_settled
                    && !message.content.trim().is_empty() =>
            {
                let Some(turn) = message.turn_id.as_deref() else {
                    continue;
                };
                let receipt = Receipt {
                    seconds: message
                        .turn_started_at
                        .as_deref()
                        .zip(message.turn_ended_at.as_deref())
                        .and_then(|(start, end)| seconds_between(start, end)),
                    tokens: tokens.get(turn).copied(),
                };
                let line = RoomLine {
                    key: format!("{session_id}:{turn}"),
                    agent_id: agent_id.to_string(),
                    session_id: session_id.to_string(),
                    speaker: Speaker::Agent,
                    text: message.content.trim().to_string(),
                    at: message.turn_ended_at.clone().unwrap_or(at),
                    receipt: Some(receipt),
                };
                match answer_of.get(turn) {
                    Some(&index) => lines[index] = line,
                    None => {
                        answer_of.insert(turn, lines.len());
                        lines.push(line);
                    }
                }
            }
            _ => {}
        }
    }
    lines
}

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

/// The agent that answered last and how many minutes ago.
pub fn last_answer(
    lines: &[&RoomLine],
    now: chrono::DateTime<chrono::Utc>,
) -> Option<(String, i64)> {
    let line = lines
        .iter()
        .rev()
        .find(|line| line.speaker == Speaker::Agent)?;
    let at = chrono::DateTime::parse_from_rfc3339(&line.at).ok()?;
    Some((
        line.agent_id.clone(),
        (now - at.with_timezone(&chrono::Utc)).num_minutes(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(id: &str, role: Role, turn: &str, content: &str, at: &str) -> TimelineMessage {
        let mut message = TimelineMessage {
            id: String::new(),
            role: Role::User,
            turn_id: None,
            turn_settled: false,
            turn_started_at: None,
            turn_ended_at: None,
            assistant_phase: None,
            content: String::new(),
            label: String::new(),
            model: None,
            streaming: false,
            created_at: None,
            status: None,
            annotations: Vec::new(),
            plan_mode: false,
            answers_plan_mode: false,
            evidence: None,
            retry_of_turn_id: None,
            action: None,
            delegation: None,
            delegation_turn_id: None,
            hand_back: None,
        };
        message.id = id.into();
        message.role = role;
        message.turn_id = Some(turn.into());
        message.turn_settled = true;
        message.content = content.into();
        message.created_at = Some(at.into());
        message.turn_started_at = Some("2026-11-07T10:00:00Z".into());
        message.turn_ended_at = Some("2026-11-07T10:00:42Z".into());
        message
    }

    #[test]
    fn a_conversation_reads_as_the_persons_lines_and_the_last_answer_of_each_turn() {
        let mut thinking = message(
            "m-2",
            Role::Assistant,
            "t-1",
            "olhando…",
            "2026-11-07T10:00:10Z",
        );
        thinking.assistant_phase = Some(AssistantMessagePhase::Commentary);
        let messages = vec![
            message(
                "m-1",
                Role::User,
                "t-1",
                "revisa o PR",
                "2026-11-07T10:00:00Z",
            ),
            thinking,
            message(
                "m-3",
                Role::Assistant,
                "t-1",
                "Primeira parte",
                "2026-11-07T10:00:20Z",
            ),
            message(
                "m-4",
                Role::Assistant,
                "t-1",
                "Tudo certo.",
                "2026-11-07T10:00:40Z",
            ),
        ];
        let tokens = HashMap::from([("t-1".to_string(), 1200)]);
        let lines = lines_of("rev", "s-1", &messages, &tokens);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].speaker, Speaker::Person);
        assert_eq!(lines[1].text, "Tudo certo.");
        assert_eq!(
            lines[1].receipt,
            Some(Receipt {
                seconds: Some(42),
                tokens: Some(1200)
            })
        );
        // Two conversations merge by time; the last answer keeps the subject.
        let other = HomeFeed {
            lines: vec![RoomLine {
                key: "x".into(),
                agent_id: "res".into(),
                session_id: "s-2".into(),
                speaker: Speaker::Agent,
                text: "achei".into(),
                at: "2026-11-07T10:00:30Z".into(),
                receipt: None,
            }],
            ..HomeFeed::default()
        };
        let feeds = vec![
            HomeFeed {
                lines,
                ..HomeFeed::default()
            },
            other,
        ];
        let room = merged(&feeds);
        let order: Vec<&str> = room.iter().map(|line| line.text.as_str()).collect();
        assert_eq!(order, ["revisa o PR", "achei", "Tudo certo."]);
        let now = chrono::DateTime::parse_from_rfc3339("2026-11-07T10:05:42Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        assert_eq!(last_answer(&room, now), Some(("rev".to_string(), 5)));
    }
}

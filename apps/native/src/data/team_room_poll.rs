//! The room's live read of a conversation at work (`room_live_poll`): what
//! a running turn added since it was read, cheap enough for every beat. The
//! conversation's events go off the UI thread shared, not copied; while only
//! the open turn moved, that turn alone is projected again
//! (`turn_now_of_events`) and the lines, tokens and reactions are left as
//! they were. Blocking: call `poll` off the UI thread.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use dcc_core::domain::agent_reaction::Reaction;
use dcc_core::domain::session::{SessionEventKind, SessionEventRecord};
use dcc_core::domain::team_room_live::{TurnNow, moves_only_the_turn, turn_now_of_events};
use dcc_infra::db::SqliteSessionRepo;

use super::team_room::{HomeFeed, RoomLine};

/// What a poll found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Polled {
    /// Nothing new.
    Same,
    /// Only the open turn's own work moved: the lines are as they were.
    Turn,
    /// Something else came (the turn ended, a message, an approval): the
    /// lines were read again.
    Lines,
}

/// A running conversation as a poll takes it.
pub struct LiveProbe {
    agent_id: String,
    session_id: String,
    events: Arc<Vec<SessionEventRecord>>,
    /// A task's own conversation followed from the room: only its open turn
    /// is wanted, never its lines, so only the last turn's events are kept.
    turn_only: bool,
}

/// What a poll read, for the feed it came from (`HomeFeed::take_poll`).
pub struct LivePoll {
    session_id: String,
    /// The last event the feed had when it was taken.
    base: u64,
    events: Arc<Vec<SessionEventRecord>>,
    now: Arc<TurnNow>,
    lines: Option<(Vec<RoomLine>, HashMap<String, Reaction>)>,
    pub polled: Polled,
}

impl HomeFeed {
    pub fn probe(&self) -> LiveProbe {
        LiveProbe {
            agent_id: self.agent_id.clone(),
            session_id: self.session_id.clone(),
            events: self.events.clone(),
            turn_only: false,
        }
    }

    /// A probe for a task's conversation whose open turn the room shows
    /// (`room_task_live`): its polls never read the lines.
    pub fn turn_probe(&self) -> LiveProbe {
        LiveProbe {
            turn_only: true,
            ..self.probe()
        }
    }

    /// Takes what a poll read, unless the feed was read again meanwhile (a
    /// refresh has it all): what moved, `Same` when nothing did here.
    pub fn take_poll(&mut self, poll: LivePoll) -> Polled {
        if poll.polled == Polled::Same
            || poll.session_id != self.session_id
            || poll.base != self.last_sequence()
        {
            return Polled::Same;
        }
        self.events = poll.events;
        self.now = poll.now;
        if let Some((lines, reactions)) = poll.lines {
            self.lines = lines;
            self.reactions = reactions;
        }
        poll.polled
    }
}

impl LivePoll {
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// What a poll of `feed` would bring if the conversation then read as
    /// `next`, for tests.
    #[cfg(test)]
    pub fn for_tests(feed: &HomeFeed, next: HomeFeed, polled: Polled) -> Self {
        Self {
            session_id: feed.session_id.clone(),
            base: feed.last_sequence(),
            events: next.events,
            now: next.now,
            lines: (polled == Polled::Lines).then_some((next.lines, next.reactions)),
            polled,
        }
    }
}

impl LiveProbe {
    fn base(&self) -> u64 {
        self.events.last().map_or(0, |event| event.sequence)
    }

    /// Reads the events after the ones it has. A database that cannot be
    /// read finds nothing new.
    pub fn poll(mut self, db: &Path) -> LivePoll {
        let base = self.base();
        let same = |probe: LiveProbe| LivePoll {
            session_id: probe.session_id,
            base,
            events: probe.events,
            now: Arc::default(),
            lines: None,
            polled: Polled::Same,
        };
        let Ok(sessions) = SqliteSessionRepo::open_read_only(db) else {
            return same(self);
        };
        let fresh = match super::conversation::events_after(&sessions, &self.session_id, base) {
            Ok(fresh) if !fresh.is_empty() => fresh,
            _ => return same(self),
        };
        let whole = fresh.iter().any(|event| !moves_only_the_turn(&event.kind));
        // Copied here, off the UI thread, only when the room still shares it.
        Arc::make_mut(&mut self.events).extend(fresh);
        if self.turn_only {
            // What came before the last turn is never drawn: dropped, so a
            // long task keeps only its open turn here.
            let start = self
                .events
                .iter()
                .rposition(|event| matches!(event.kind, SessionEventKind::TurnStarted { .. }))
                .unwrap_or(0);
            if start > 0 {
                Arc::make_mut(&mut self.events).drain(..start);
            }
        } else if whole {
            let mut feed = HomeFeed {
                events: self.events.clone(),
                ..HomeFeed::new(&self.agent_id, &self.session_id)
            };
            if feed.read_all(&sessions).is_ok() {
                return LivePoll {
                    session_id: self.session_id,
                    base,
                    events: feed.events,
                    now: feed.now,
                    lines: Some((feed.lines, feed.reactions)),
                    polled: Polled::Lines,
                };
            }
        }
        let now = Arc::new(turn_now_of_events(&self.events, &self.session_id));
        LivePoll {
            session_id: self.session_id,
            base,
            events: self.events,
            now,
            lines: None,
            polled: Polled::Turn,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dcc_core::domain::session::SessionId;

    fn event(sequence: u64, json: serde_json::Value) -> SessionEventRecord {
        SessionEventRecord {
            event_id: format!("e-{sequence}"),
            session_id: SessionId("s".into()),
            sequence,
            occurred_at: format!("2026-11-10T10:00:{sequence:02}Z"),
            kind: serde_json::from_value::<SessionEventKind>(json).unwrap(),
        }
    }

    #[test]
    fn a_poll_taken_after_a_refresh_is_dropped() {
        let started = event(
            1,
            serde_json::json!({
                "type": "turn_started", "turnId": "t1", "prompt": "oi",
                "planMode": false, "model": null, "evidence": null,
                "retryOfTurnId": null, "approvalPolicy": "ask"
            }),
        );
        let mut feed = HomeFeed::projected("dev", "s", vec![started]);
        assert!(feed.now.live().is_some());
        let poll = |base: u64, polled: Polled| LivePoll {
            session_id: "s".into(),
            base,
            events: Arc::new(Vec::new()),
            now: Arc::default(),
            lines: None,
            polled,
        };
        assert_eq!(feed.take_poll(poll(0, Polled::Turn)), Polled::Same);
        assert!(feed.now.live().is_some(), "read for fewer events");
        assert_eq!(feed.take_poll(poll(1, Polled::Same)), Polled::Same);
        assert_eq!(feed.take_poll(poll(1, Polled::Turn)), Polled::Turn);
        assert_eq!(*feed.now, TurnNow::Idle);
    }

    #[test]
    fn a_database_that_cannot_be_read_finds_nothing() {
        let feed = HomeFeed::new("dev", "s");
        let read = feed.probe().poll(Path::new("/nonexistent/dcc.sqlite"));
        assert_eq!(read.polled, Polled::Same);
    }
}

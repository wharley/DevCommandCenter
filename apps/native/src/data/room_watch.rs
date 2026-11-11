//! Which resident agents' room conversations moved between two sidebar
//! reads in a way the person may want to hear about (`room_notices`): one
//! that now waits on them, one whose turn just ended. The first read only
//! records what is already there; nothing is read while agents are paused or
//! the notices are off. What a moved conversation holds is read afterwards,
//! off the UI thread (`dcc_core::domain::room_notice`); here it is only
//! told once per request or turn, and at most once per agent and room every
//! `WINDOW`, the newest held back until the window passes.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use dcc_core::domain::agent_conversation::home_project_id;
use dcc_core::domain::room_notice::RoomNotice;

use super::agents::{AgentRecord, AgentState};

/// One notice per agent and room within this.
pub const WINDOW: Duration = Duration::from_secs(30);

/// A room conversation as one read saw it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Seen {
    state: Option<AgentState>,
    completed_at: Option<String>,
    updated_at: String,
}

impl Seen {
    fn open(&self) -> bool {
        matches!(
            self.state,
            Some(AgentState::Working) | Some(AgentState::NeedsYou)
        )
    }
}

/// A conversation to read for a notice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Probe {
    pub agent_id: String,
    pub agent_name: String,
    pub session_id: String,
    /// The room's project; none: Geral.
    pub room: Option<String>,
    /// It waits on the person: read its request, not how it ended.
    pub waiting: bool,
}

/// A notice ready to deliver.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    pub probe: Probe,
    pub notice: RoomNotice,
}

type RoomKey = (String, Option<String>);

#[derive(Default)]
pub struct RoomWatch {
    /// None until the first read.
    seen: Option<HashMap<String, Seen>>,
    /// The request or turn last told, by conversation.
    told: HashMap<String, String>,
    sent_at: HashMap<RoomKey, Instant>,
    held: HashMap<RoomKey, Notice>,
}

/// What changed from `before` to `now` worth reading: `Some(true)` it waits
/// on the person anew, `Some(false)` its turn ended.
fn moved(before: &Seen, now: &Seen) -> Option<bool> {
    if now.state == Some(AgentState::NeedsYou) {
        let anew = before.state != now.state || before.updated_at != now.updated_at;
        return anew.then_some(true);
    }
    let finished = now.completed_at.is_some() && now.completed_at != before.completed_at;
    (!now.open() && (before.open() || finished)).then_some(false)
}

impl RoomWatch {
    /// Records the agents' room conversations as read now and returns the
    /// ones to read; nothing on the first read or while `muted`.
    pub fn observe(&mut self, agents: &[AgentRecord], muted: bool) -> Vec<Probe> {
        let mut now: HashMap<String, Seen> = HashMap::new();
        let mut probes = Vec::new();
        for agent in agents {
            let home = home_project_id(&agent.id);
            for session in agent.sessions.iter().filter(|s| s.project_id == home) {
                let seen = Seen {
                    state: Some(session.state),
                    completed_at: session.completed_at.clone(),
                    updated_at: session.updated_at.clone(),
                };
                if let Some(before) = &self.seen {
                    let before = before.get(&session.session_id).cloned().unwrap_or_default();
                    if let Some(waiting) = moved(&before, &seen).filter(|_| !muted) {
                        probes.push(Probe {
                            agent_id: agent.id.clone(),
                            agent_name: agent.name.clone(),
                            session_id: session.session_id.clone(),
                            room: session.room.clone(),
                            waiting,
                        });
                    }
                }
                now.insert(session.session_id.clone(), seen);
            }
        }
        // Before the sidebar has read anything there is nothing to record.
        if self.seen.is_some() || !agents.is_empty() {
            self.seen = Some(now);
        }
        probes
    }

    /// The notice to deliver now, if any: once per request or turn, and the
    /// first in `WINDOW` for its agent and room (a later one waits, the
    /// newest winning, for `flush`).
    pub fn offer(&mut self, notice: Notice, now: Instant) -> Option<Notice> {
        let session = &notice.probe.session_id;
        if self.told.get(session).map(String::as_str) == Some(notice.notice.key()) {
            return None;
        }
        self.told
            .insert(session.clone(), notice.notice.key().to_string());
        let key = (notice.probe.agent_id.clone(), notice.probe.room.clone());
        match self.sent_at.get(&key) {
            Some(at) if now.duration_since(*at) < WINDOW => {
                self.held.insert(key, notice);
                None
            }
            _ => {
                self.sent_at.insert(key, now);
                Some(notice)
            }
        }
    }

    /// The held notices whose window passed; a request no longer waited on
    /// (`still_waiting` of its conversation) is dropped.
    pub fn flush(&mut self, now: Instant, still_waiting: impl Fn(&str) -> bool) -> Vec<Notice> {
        let due: Vec<RoomKey> = self
            .held
            .keys()
            .filter(|key| {
                self.sent_at
                    .get(*key)
                    .is_none_or(|at| now.duration_since(*at) >= WINDOW)
            })
            .cloned()
            .collect();
        let mut out = Vec::new();
        for key in due {
            let Some(notice) = self.held.remove(&key) else {
                continue;
            };
            if notice.notice.asks() && !still_waiting(&notice.probe.session_id) {
                continue;
            }
            self.sent_at.insert(key, now);
            out.push(notice);
        }
        out
    }

    /// Whether something is held back.
    pub fn holding(&self) -> bool {
        !self.held.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::agents::AgentSession;

    fn agent(state: AgentState, completed: Option<&str>, at: &str) -> AgentRecord {
        AgentRecord {
            id: "dev".into(),
            name: "Dev".into(),
            preset: None,
            avatar: dcc_core::domain::agent::AgentAvatar {
                shape: "octopus".into(),
                accessory: "none".into(),
                color: "violet".into(),
                arms: 3,
                eyes: "round".into(),
            },
            provider_id: None,
            model: None,
            effort: None,
            kickoff_prompt: String::new(),
            role: String::new(),
            offer_prompt: String::new(),
            extra_instructions: String::new(),
            projects: Vec::new(),
            monthly_token_limit: None,
            tool_access: Default::default(),
            auto_start_tasks: false,
            may_call: Vec::new(),
            account_id: None,
            sessions: vec![AgentSession {
                session_id: "s".into(),
                workspace_id: "w".into(),
                state,
                completed_at: completed.map(str::to_string),
                project_id: home_project_id("dev"),
                title: None,
                updated_at: at.into(),
                room: Some("checkout".into()),
            }],
        }
    }

    fn notice(probe: &Probe, notice: RoomNotice) -> Notice {
        Notice {
            probe: probe.clone(),
            notice,
        }
    }

    fn done(turn: &str) -> RoomNotice {
        RoomNotice::Done {
            turn_id: turn.into(),
            first_line: "Pronto".into(),
        }
    }

    #[test]
    fn the_first_read_only_records() {
        let mut watch = RoomWatch::default();
        assert!(watch.observe(&[], false).is_empty());
        let waiting = agent(AgentState::NeedsYou, None, "1");
        assert!(
            watch
                .observe(std::slice::from_ref(&waiting), false)
                .is_empty()
        );
        assert!(watch.observe(&[waiting], false).is_empty(), "nothing moved");
    }

    #[test]
    fn waiting_and_ending_are_read_but_not_while_paused() {
        let mut watch = RoomWatch::default();
        watch.observe(&[agent(AgentState::Idle, None, "1")], false);
        let probes = watch.observe(&[agent(AgentState::NeedsYou, None, "2")], false);
        assert_eq!(probes.len(), 1);
        assert!(probes[0].waiting);
        assert_eq!(probes[0].room.as_deref(), Some("checkout"));
        // A second request moves the conversation again.
        assert_eq!(
            watch
                .observe(&[agent(AgentState::NeedsYou, None, "3")], false)
                .len(),
            1
        );
        let ended = watch.observe(&[agent(AgentState::Done, Some("4"), "4")], false);
        assert_eq!(ended.len(), 1);
        assert!(!ended[0].waiting);
        // A turn that started and ended between two reads still counts.
        let quick = watch.observe(&[agent(AgentState::Done, Some("5"), "5")], false);
        assert_eq!(quick.len(), 1);
        // Paused: recorded, never read.
        assert!(
            watch
                .observe(&[agent(AgentState::NeedsYou, None, "6")], true)
                .is_empty()
        );
        assert!(
            watch
                .observe(&[agent(AgentState::NeedsYou, None, "6")], false)
                .is_empty(),
            "what moved while paused stays quiet"
        );
    }

    #[test]
    fn one_notice_per_agent_and_room_every_window_the_newest_held() {
        let mut watch = RoomWatch::default();
        watch.observe(&[agent(AgentState::Idle, None, "1")], false);
        let probe = watch.observe(&[agent(AgentState::Working, None, "2")], false);
        assert!(probe.is_empty(), "starting work tells nothing");
        let probe = watch
            .observe(&[agent(AgentState::Done, Some("3"), "3")], false)
            .remove(0);
        let start = Instant::now();
        assert!(watch.offer(notice(&probe, done("t1")), start).is_some());
        assert!(
            watch.offer(notice(&probe, done("t1")), start).is_none(),
            "the same turn is told once"
        );
        let soon = start + Duration::from_secs(5);
        assert!(watch.offer(notice(&probe, done("t2")), soon).is_none());
        assert!(watch.offer(notice(&probe, done("t3")), soon).is_none());
        assert!(watch.holding());
        assert!(watch.flush(soon, |_| true).is_empty());
        let later = start + WINDOW;
        let flushed = watch.flush(later, |_| true);
        assert_eq!(flushed.len(), 1);
        assert_eq!(flushed[0].notice.key(), "t3", "the newest wins");
        assert!(!watch.holding());
    }

    #[test]
    fn a_held_request_answered_meanwhile_is_dropped() {
        let mut watch = RoomWatch::default();
        let probe = Probe {
            agent_id: "dev".into(),
            agent_name: "Dev".into(),
            session_id: "s".into(),
            room: None,
            waiting: true,
        };
        let start = Instant::now();
        assert!(watch.offer(notice(&probe, done("t1")), start).is_some());
        let question = RoomNotice::Question {
            request_id: "q".into(),
            text: "Qual?".into(),
        };
        assert!(watch.offer(notice(&probe, question), start).is_none());
        assert!(watch.flush(start + WINDOW, |_| false).is_empty());
    }
}

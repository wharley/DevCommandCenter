//! What an agent may send without asking the person first (Compozy's
//! `auto_accept`: an explicit grant for one kind of action, with a ceiling
//! a day): the review the Reviewer writes of a pull request, per project,
//! the reply an agent proposes through a connector, per room, and the
//! delivery of a task it opened (commit, push, pull request), per project. Off
//! everywhere until the person turns it on, on the agent's page; "Pausar
//! todos" holds it too. Kept with the triggers (`agent-triggers.json`).
//!
//! The merge is never here: it stays the person's.

use chrono::{DateTime, Local, Utc};
use serde::{Deserialize, Serialize};

use super::AgentTriggers;

/// How far the agent goes with a review by itself.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReviewSend {
    /// Its drafts wait in Pull Requests, as always.
    #[default]
    Ask,
    /// Its comments and summary go; approving or asking for changes stays
    /// the person's.
    Comment,
    /// Its comments go with its decision, where the forge allows one.
    Decide,
}

impl ReviewSend {
    pub const ALL: [ReviewSend; 3] = [Self::Ask, Self::Comment, Self::Decide];

    /// `agents.autoSend.review.<key>`
    pub fn key(self) -> &'static str {
        match self {
            Self::Ask => "ask",
            Self::Comment => "comment",
            Self::Decide => "decide",
        }
    }
}

/// One agent's grants.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AutoSend {
    pub review: ReviewSend,
    /// The projects the review grant holds in; none: nowhere.
    pub review_projects: Vec<String>,
    /// Its proposed replies go without the person pressing "Enviar".
    pub reply: bool,
    /// The rooms that holds in, a project's id each and `GENERAL_ROOM` for
    /// Geral; none: nowhere.
    pub reply_rooms: Vec<String>,
    /// When each reply went by itself today, for the ceiling.
    pub replies_sent: Vec<String>,
    /// A task it opened is committed, pushed and gets its pull request
    /// when it ends a turn with changes.
    pub deliver: bool,
    /// The projects that holds in; none: nowhere.
    pub deliver_projects: Vec<String>,
    /// When delivery was turned on: a task that ended before it is the
    /// person's to send, or turning it on would send every old one.
    pub deliver_since: Option<String>,
    /// What the agent is to do when a pull request it delivered opens, in
    /// the person's words ("avise no #tech marcando o Giovanni"); empty:
    /// it is not told.
    pub deliver_note: String,
    /// Review comments on the person's own pull request go to the task's
    /// own conversation to be fixed, instead of this agent reading them
    /// and only saying what to change (its "review received" trigger).
    pub fix_in_task: bool,
}

impl AutoSend {
    pub(crate) fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// The longest `AutoSend::deliver_note`.
pub const MAX_DELIVER_NOTE: usize = 400;

/// Geral among `AutoSend::reply_rooms`.
pub const GENERAL_ROOM: &str = "";
/// Replies an agent sends by itself in a day (Compozy's ceiling); past it
/// they wait as cards.
pub const REPLIES_A_DAY: usize = 20;

fn is_today(at: &str, now: DateTime<Utc>) -> bool {
    DateTime::parse_from_rfc3339(at).is_ok_and(|at| {
        at.with_timezone(&Local).date_naive() == now.with_timezone(&Local).date_naive()
    })
}

impl AgentTriggers {
    /// What the person set for the agent, whatever the project.
    pub fn review_send_choice(&self, agent_id: &str) -> ReviewSend {
        self.stored
            .auto_send
            .get(agent_id)
            .map(|grants| grants.review)
            .unwrap_or_default()
    }

    pub fn review_send_in(&self, agent_id: &str, project_id: &str) -> bool {
        self.stored
            .auto_send
            .get(agent_id)
            .is_some_and(|grants| grants.review_projects.iter().any(|id| id == project_id))
    }

    /// What the agent may send of a review in `project_id` now.
    pub fn review_send(&self, agent_id: &str, project_id: &str) -> ReviewSend {
        if self.stored.paused || !self.review_send_in(agent_id, project_id) {
            return ReviewSend::Ask;
        }
        self.review_send_choice(agent_id)
    }

    pub fn set_review_send(&mut self, agent_id: &str, send: ReviewSend) {
        let _lock = self.begin();
        self.stored
            .auto_send
            .entry(agent_id.to_string())
            .or_default()
            .review = send;
        self.forget_empty_grants(agent_id);
        self.save();
    }

    pub fn set_review_send_in(&mut self, agent_id: &str, project_id: &str, on: bool) {
        let _lock = self.begin();
        let grants = self
            .stored
            .auto_send
            .entry(agent_id.to_string())
            .or_default();
        grants.review_projects.retain(|id| id != project_id);
        if on {
            grants.review_projects.push(project_id.to_string());
            grants.review_projects.sort();
        }
        self.forget_empty_grants(agent_id);
        self.save();
    }

    pub fn reply_send_on(&self, agent_id: &str) -> bool {
        self.stored
            .auto_send
            .get(agent_id)
            .is_some_and(|grants| grants.reply)
    }

    /// `room`: a project's id, or `GENERAL_ROOM`.
    pub fn reply_send_in(&self, agent_id: &str, room: &str) -> bool {
        self.stored
            .auto_send
            .get(agent_id)
            .is_some_and(|grants| grants.reply_rooms.iter().any(|id| id == room))
    }

    /// Whether a reply the agent proposed in `room` (none: Geral) goes by
    /// itself now.
    pub fn may_send_reply(&self, agent_id: &str, room: Option<&str>, now: DateTime<Utc>) -> bool {
        let Some(grants) = self.stored.auto_send.get(agent_id) else {
            return false;
        };
        let today = grants
            .replies_sent
            .iter()
            .filter(|at| is_today(at, now))
            .count();
        !self.stored.paused
            && grants.reply
            && self.reply_send_in(agent_id, room.unwrap_or(GENERAL_ROOM))
            && today < REPLIES_A_DAY
    }

    /// A reply went by itself; only today's are kept.
    pub fn record_reply_sent(&mut self, agent_id: &str, now: DateTime<Utc>) {
        let _lock = self.begin();
        let grants = self
            .stored
            .auto_send
            .entry(agent_id.to_string())
            .or_default();
        grants.replies_sent.retain(|at| is_today(at, now));
        grants.replies_sent.push(now.to_rfc3339());
        self.save();
    }

    pub fn set_reply_send(&mut self, agent_id: &str, on: bool) {
        let _lock = self.begin();
        self.stored
            .auto_send
            .entry(agent_id.to_string())
            .or_default()
            .reply = on;
        self.forget_empty_grants(agent_id);
        self.save();
    }

    pub fn set_reply_send_in(&mut self, agent_id: &str, room: &str, on: bool) {
        let _lock = self.begin();
        let grants = self
            .stored
            .auto_send
            .entry(agent_id.to_string())
            .or_default();
        grants.reply_rooms.retain(|id| id != room);
        if on {
            grants.reply_rooms.push(room.to_string());
            grants.reply_rooms.sort();
        }
        self.forget_empty_grants(agent_id);
        self.save();
    }

    pub fn deliver_on(&self, agent_id: &str) -> bool {
        self.stored
            .auto_send
            .get(agent_id)
            .is_some_and(|grants| grants.deliver)
    }

    pub fn deliver_in(&self, agent_id: &str, project_id: &str) -> bool {
        self.stored
            .auto_send
            .get(agent_id)
            .is_some_and(|grants| grants.deliver_projects.iter().any(|id| id == project_id))
    }

    /// Whether a task the agent opened in `project_id`, whose turn ended at
    /// `ended_at`, is sent by itself now.
    pub fn may_deliver(&self, agent_id: &str, project_id: &str, ended_at: &str) -> bool {
        let after_grant = || {
            let since = self
                .stored
                .auto_send
                .get(agent_id)?
                .deliver_since
                .as_deref()?;
            let since = DateTime::parse_from_rfc3339(since).ok()?;
            let ended = DateTime::parse_from_rfc3339(ended_at).ok()?;
            Some(ended > since)
        };
        !self.stored.paused
            && self.deliver_on(agent_id)
            && self.deliver_in(agent_id, project_id)
            && after_grant() == Some(true)
    }

    pub fn fix_in_task(&self, agent_id: &str) -> bool {
        !self.stored.paused
            && self
                .stored
                .auto_send
                .get(agent_id)
                .is_some_and(|grants| grants.fix_in_task)
    }

    /// What the person picked, whatever "Pausar todos" says.
    pub fn fix_in_task_choice(&self, agent_id: &str) -> bool {
        self.stored
            .auto_send
            .get(agent_id)
            .is_some_and(|grants| grants.fix_in_task)
    }

    pub fn set_fix_in_task(&mut self, agent_id: &str, on: bool) {
        let _lock = self.begin();
        self.stored
            .auto_send
            .entry(agent_id.to_string())
            .or_default()
            .fix_in_task = on;
        self.forget_empty_grants(agent_id);
        self.save();
    }

    pub fn deliver_note(&self, agent_id: &str) -> String {
        self.stored
            .auto_send
            .get(agent_id)
            .map(|grants| grants.deliver_note.trim().to_string())
            .unwrap_or_default()
    }

    pub fn set_deliver_note(&mut self, agent_id: &str, note: &str) {
        let _lock = self.begin();
        let note: String = note.trim().chars().take(MAX_DELIVER_NOTE).collect();
        if self.deliver_note(agent_id) == note {
            return;
        }
        self.stored
            .auto_send
            .entry(agent_id.to_string())
            .or_default()
            .deliver_note = note;
        self.forget_empty_grants(agent_id);
        self.save();
    }

    pub fn set_deliver(&mut self, agent_id: &str, on: bool) {
        let _lock = self.begin();
        let grants = self
            .stored
            .auto_send
            .entry(agent_id.to_string())
            .or_default();
        grants.deliver = on;
        grants.deliver_since = on.then(|| Utc::now().to_rfc3339());
        self.forget_empty_grants(agent_id);
        self.save();
    }

    pub fn set_deliver_in(&mut self, agent_id: &str, project_id: &str, on: bool) {
        let _lock = self.begin();
        let grants = self
            .stored
            .auto_send
            .entry(agent_id.to_string())
            .or_default();
        grants.deliver_projects.retain(|id| id != project_id);
        if on {
            grants.deliver_projects.push(project_id.to_string());
            grants.deliver_projects.sort();
        }
        self.forget_empty_grants(agent_id);
        self.save();
    }

    fn forget_empty_grants(&mut self, agent_id: &str) {
        if self
            .stored
            .auto_send
            .get(agent_id)
            .is_some_and(AutoSend::is_default)
        {
            self.stored.auto_send.remove(agent_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_goes_without_a_grant_for_that_project() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("agent-triggers.json");
        let mut triggers = AgentTriggers::load(path.clone());
        assert_eq!(triggers.review_send("rev", "app"), ReviewSend::Ask);
        // The mode alone holds nowhere.
        triggers.set_review_send("rev", ReviewSend::Decide);
        assert_eq!(triggers.review_send("rev", "app"), ReviewSend::Ask);
        triggers.set_review_send_in("rev", "app", true);
        assert_eq!(triggers.review_send("rev", "app"), ReviewSend::Decide);
        assert_eq!(triggers.review_send("rev", "site"), ReviewSend::Ask);
        assert_eq!(triggers.review_send("dev", "app"), ReviewSend::Ask);
        // It is kept, and "Pausar todos" holds it.
        let mut read = AgentTriggers::load(path);
        assert_eq!(read.review_send("rev", "app"), ReviewSend::Decide);
        read.stored.paused = true;
        assert_eq!(read.review_send("rev", "app"), ReviewSend::Ask);
        read.stored.paused = false;
        // Back to asking with no project: nothing is left of it.
        read.set_review_send("rev", ReviewSend::Ask);
        read.set_review_send_in("rev", "app", false);
        assert!(read.stored.auto_send.is_empty());
    }

    #[test]
    fn a_reply_goes_only_in_a_granted_room_and_under_the_ceiling() {
        let dir = tempfile::tempdir().unwrap();
        let mut triggers = AgentTriggers::load(dir.path().join("agent-triggers.json"));
        let now = Utc::now();
        assert!(!triggers.may_send_reply("sup", None, now));
        triggers.set_reply_send("sup", true);
        assert!(!triggers.may_send_reply("sup", None, now), "no room yet");
        triggers.set_reply_send_in("sup", GENERAL_ROOM, true);
        assert!(triggers.may_send_reply("sup", None, now));
        assert!(!triggers.may_send_reply("sup", Some("app"), now));
        assert!(!triggers.may_send_reply("dev", None, now));
        triggers.stored.paused = true;
        assert!(!triggers.may_send_reply("sup", None, now));
        triggers.stored.paused = false;
        for _ in 0..REPLIES_A_DAY {
            triggers.record_reply_sent("sup", now);
        }
        assert!(!triggers.may_send_reply("sup", None, now), "at the ceiling");
        // Another day starts from none.
        assert!(triggers.may_send_reply("sup", None, now + chrono::Duration::days(2)));
        triggers.set_reply_send("sup", false);
        assert!(!triggers.may_send_reply("sup", None, now + chrono::Duration::days(2)));
    }

    #[test]
    fn a_task_is_sent_only_where_delivery_is_granted() {
        let dir = tempfile::tempdir().unwrap();
        let mut triggers = AgentTriggers::load(dir.path().join("agent-triggers.json"));
        let later = (Utc::now() + chrono::Duration::minutes(5)).to_rfc3339();
        let before = (Utc::now() - chrono::Duration::minutes(5)).to_rfc3339();
        assert!(!triggers.may_deliver("dev", "app", &later));
        triggers.set_deliver("dev", true);
        assert!(
            !triggers.may_deliver("dev", "app", &later),
            "no project yet"
        );
        triggers.set_deliver_in("dev", "app", true);
        assert!(triggers.may_deliver("dev", "app", &later));
        // A task that ended before it was turned on stays the person's.
        assert!(!triggers.may_deliver("dev", "app", &before));
        assert!(!triggers.may_deliver("dev", "site", &later));
        assert!(!triggers.may_deliver("sup", "app", &later));
        triggers.stored.paused = true;
        assert!(!triggers.may_deliver("dev", "app", &later));
        // Review comments go to the task only where picked, and not paused.
        assert!(!triggers.fix_in_task("dev"));
        triggers.set_fix_in_task("dev", true);
        assert!(triggers.fix_in_task_choice("dev") && !triggers.fix_in_task("dev"));
        triggers.stored.paused = false;
        assert!(triggers.fix_in_task("dev"));
        triggers.stored.paused = true;
        // What to do when the pull request opens, in the person's words.
        assert_eq!(triggers.deliver_note("dev"), "");
        triggers.set_deliver_note("dev", "  avise no #tech  ");
        assert_eq!(triggers.deliver_note("dev"), "avise no #tech");
    }
}

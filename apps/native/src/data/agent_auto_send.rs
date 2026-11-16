//! What an agent may send without asking the person first (Compozy's
//! `auto_accept`: an explicit grant for one kind of action, with a ceiling
//! a day): the review the Reviewer writes of a pull request, per project,
//! the reply an agent proposes through a connector, per room, and the
//! delivery of a task it opened (commit, push, pull request), per project. Off
//! everywhere until the person turns it on, on the agent's page; "Pausar
//! todos" holds it too. Kept with the triggers (`agent-triggers.json`), on
//! this computer.
//!
//! The merge is never here: it stays the person's.

use chrono::{DateTime, Local, Utc};
use serde::{Deserialize, Serialize};

use super::agent_triggers::AgentTriggers;
use super::pr_review::{ReviewEvent, can_submit};
use super::pr_review_verdict::Verdict;
use crate::i18n::{Args, Translate};

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
}

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

/// A review DCC sent for the agent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SentReview {
    pub event: ReviewEvent,
    pub comments: u32,
    /// What the forge did not take, in its words.
    pub warning: Option<String>,
}

impl SentReview {
    /// "aprovou · 3 comentários".
    pub fn words(&self, cx: &gpui::App) -> String {
        let decision = cx.t(&format!("agents.autoSend.sent.{}", self.event.key()));
        let key = match self.comments {
            0 => return decision.to_string(),
            1 => "agents.autoSend.sent.oneComment",
            _ => "agents.autoSend.sent.comments",
        };
        let comments = cx.t_with(
            key,
            &Args::default().with("count", self.comments.to_string()),
        );
        format!("{decision} · {comments}")
    }
}

/// What to send of a finished review, if anything: the comments alone when
/// only commenting is granted, when the review does not decide ("not
/// decided yet", or no verdict: what it asks is for the author to read) or
/// when the forge refuses the decision here (the person's own pull request,
/// a draft); and nothing the forge would not take (no comment and no text;
/// asking for changes without saying why).
pub fn plan(
    send: ReviewSend,
    verdict: Option<Verdict>,
    decision_blocked: bool,
    comments: usize,
    body: &str,
) -> Option<ReviewEvent> {
    let event = match (send, verdict) {
        (ReviewSend::Ask, _) => return None,
        (ReviewSend::Decide, Some(verdict)) if !decision_blocked => verdict.event(),
        _ => ReviewEvent::Comment,
    };
    can_submit(event, comments, body, false).then_some(event)
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
        self.stored
            .auto_send
            .entry(agent_id.to_string())
            .or_default()
            .review = send;
        self.forget_empty_grants(agent_id);
        self.save();
    }

    pub fn set_review_send_in(&mut self, agent_id: &str, project_id: &str, on: bool) {
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
        self.stored
            .auto_send
            .entry(agent_id.to_string())
            .or_default()
            .reply = on;
        self.forget_empty_grants(agent_id);
        self.save();
    }

    pub fn set_reply_send_in(&mut self, agent_id: &str, room: &str, on: bool) {
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

    pub fn set_deliver(&mut self, agent_id: &str, on: bool) {
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
        if self.stored.auto_send.get(agent_id) == Some(&AutoSend::default()) {
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
    }

    #[test]
    fn what_goes_follows_the_grant_the_verdict_and_the_forge() {
        use ReviewEvent::{Approve, Comment, RequestChanges};
        let plan =
            |send, verdict, blocked, comments, body| plan(send, verdict, blocked, comments, body);
        let approve = Some(Verdict::Approve);
        let changes = Some(Verdict::RequestChanges);
        // Asking sends nothing; no verdict and "not decided yet" go as
        // comments, for the author to read.
        assert_eq!(plan(ReviewSend::Ask, approve, false, 2, "ok"), None);
        assert_eq!(
            plan(ReviewSend::Decide, None, false, 2, "ok"),
            Some(Comment)
        );
        assert_eq!(
            plan(ReviewSend::Decide, Some(Verdict::Undecided), false, 2, "ok"),
            Some(Comment)
        );
        assert_eq!(
            plan(
                ReviewSend::Comment,
                Some(Verdict::Undecided),
                false,
                0,
                "a doubt"
            ),
            Some(Comment)
        );
        // The decision goes only when granted and the forge takes it.
        assert_eq!(
            plan(ReviewSend::Decide, approve, false, 0, ""),
            Some(Approve)
        );
        assert_eq!(
            plan(ReviewSend::Decide, changes, false, 1, "why"),
            Some(RequestChanges)
        );
        assert_eq!(
            plan(ReviewSend::Comment, approve, false, 1, ""),
            Some(Comment)
        );
        assert_eq!(
            plan(ReviewSend::Decide, approve, true, 0, "fine"),
            Some(Comment)
        );
        // Nothing to say, or changes asked without saying why: it waits.
        assert_eq!(plan(ReviewSend::Comment, approve, false, 0, " "), None);
        assert_eq!(plan(ReviewSend::Decide, changes, false, 1, ""), None);
    }
}

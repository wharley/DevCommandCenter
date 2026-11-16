//! "Conferir" under an agent's "Enviar sem perguntar": what the person set
//! that will not do what they expect, said before the first real case
//! shows it — an announcement that will wait as a card, replies granted
//! in a room its sources do not use, a choice whose trigger is off. Read
//! from what is stored, with no model; nothing here changes a grant.

use super::agent_auto_send::{GENERAL_ROOM, ReviewSend};
use super::agent_triggers::{AgentTriggers, Trigger};

/// Something set that does not close.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gap {
    /// "Pausar todos" is on: nothing goes by itself.
    Paused,
    /// The announcement is written but replies are not granted: it will
    /// wait as a card.
    AnnounceWaits,
    /// Replies are granted, but not in a room its sources bring news to.
    SourceRoomNotGranted,
    /// "Corrigir na tarefa" is picked but "review received" is off.
    FixWithoutTrigger,
    /// A review grant with nothing that starts a review.
    ReviewNeverStarts,
    /// A grant picked with no project or room: it holds nowhere.
    HoldsNowhere,
}

impl Gap {
    /// `agents.autoSend.check.<key>`
    pub fn key(self) -> &'static str {
        match self {
            Self::Paused => "paused",
            Self::AnnounceWaits => "announceWaits",
            Self::SourceRoomNotGranted => "sourceRoom",
            Self::FixWithoutTrigger => "fixWithoutTrigger",
            Self::ReviewNeverStarts => "reviewNeverStarts",
            Self::HoldsNowhere => "holdsNowhere",
        }
    }
}

impl AgentTriggers {
    /// What does not close in `agent_id`'s grants. `source_rooms`: the room
    /// of each source it watches (none: Geral).
    pub fn auto_send_gaps(&self, agent_id: &str, source_rooms: &[Option<String>]) -> Vec<Gap> {
        let Some(grants) = self.stored.auto_send.get(agent_id) else {
            return Vec::new();
        };
        let review = grants.review != ReviewSend::Ask;
        let any = review || grants.reply || grants.deliver || grants.fix_in_task;
        let mut gaps = Vec::new();
        if any && self.stored.paused {
            gaps.push(Gap::Paused);
        }
        let reply_holds = grants.reply && !grants.reply_rooms.is_empty();
        if grants.deliver && !grants.deliver_note.trim().is_empty() && !reply_holds {
            gaps.push(Gap::AnnounceWaits);
        }
        if reply_holds
            && source_rooms.iter().any(|room| {
                let room = room.as_deref().unwrap_or(GENERAL_ROOM);
                !grants.reply_rooms.iter().any(|granted| granted == room)
            })
        {
            gaps.push(Gap::SourceRoomNotGranted);
        }
        if grants.fix_in_task && !self.is_on(agent_id, Trigger::ReviewReceived) {
            gaps.push(Gap::FixWithoutTrigger);
        }
        if review
            && !grants.review_projects.is_empty()
            && !self.is_on(agent_id, Trigger::ReviewRequested)
            && !self.is_on(agent_id, Trigger::PrUpdated)
            && source_rooms.is_empty()
        {
            gaps.push(Gap::ReviewNeverStarts);
        }
        if (review && grants.review_projects.is_empty())
            || (grants.reply && grants.reply_rooms.is_empty())
            || (grants.deliver && grants.deliver_projects.is_empty())
        {
            gaps.push(Gap::HoldsNowhere);
        }
        gaps
    }

    /// Whether the agent has anything granted, so the check has something
    /// to say.
    pub fn any_auto_send(&self, agent_id: &str) -> bool {
        self.stored.auto_send.get(agent_id).is_some_and(|grants| {
            grants.review != ReviewSend::Ask || grants.reply || grants.deliver || grants.fix_in_task
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn triggers() -> (tempfile::TempDir, AgentTriggers) {
        let dir = tempfile::tempdir().unwrap();
        let triggers = AgentTriggers::load(dir.path().join("agent-triggers.json"));
        (dir, triggers)
    }

    #[test]
    fn an_announcement_without_granted_replies_waits_as_a_card() {
        let (_dir, mut triggers) = triggers();
        assert!(triggers.auto_send_gaps("dev", &[]).is_empty());
        assert!(!triggers.any_auto_send("dev"));
        triggers.set_deliver("dev", true);
        triggers.set_deliver_in("dev", "app", true);
        assert!(triggers.auto_send_gaps("dev", &[]).is_empty());
        triggers.set_deliver_note("dev", "avise no #tech");
        assert_eq!(triggers.auto_send_gaps("dev", &[]), [Gap::AnnounceWaits]);
        // Sending picked with no room is still waiting, and holds nowhere.
        triggers.set_reply_send("dev", true);
        assert_eq!(
            triggers.auto_send_gaps("dev", &[]),
            [Gap::AnnounceWaits, Gap::HoldsNowhere]
        );
        triggers.set_reply_send_in("dev", GENERAL_ROOM, true);
        assert!(triggers.auto_send_gaps("dev", &[]).is_empty());
        assert!(triggers.any_auto_send("dev"));
    }

    #[test]
    fn replies_must_be_granted_where_the_sources_bring_news() {
        let (_dir, mut triggers) = triggers();
        triggers.set_reply_send("sup", true);
        triggers.set_reply_send_in("sup", "app", true);
        // Its source is in Geral, which is not granted.
        assert_eq!(
            triggers.auto_send_gaps("sup", &[None]),
            [Gap::SourceRoomNotGranted]
        );
        assert!(
            triggers
                .auto_send_gaps("sup", &[Some("app".to_string())])
                .is_empty()
        );
        triggers.set_reply_send_in("sup", GENERAL_ROOM, true);
        assert!(triggers.auto_send_gaps("sup", &[None]).is_empty());
    }

    #[test]
    fn a_choice_needs_what_starts_it_and_a_pause_holds_all() {
        let (_dir, mut triggers) = triggers();
        triggers.set_fix_in_task("rev", true);
        assert_eq!(
            triggers.auto_send_gaps("rev", &[]),
            [Gap::FixWithoutTrigger]
        );
        triggers.set("rev", Trigger::ReviewReceived, true);
        assert!(triggers.auto_send_gaps("rev", &[]).is_empty());
        // A review grant with no trigger and no source never starts.
        triggers.set_review_send("rev", ReviewSend::Decide);
        assert_eq!(triggers.auto_send_gaps("rev", &[]), [Gap::HoldsNowhere]);
        triggers.set_review_send_in("rev", "app", true);
        assert_eq!(
            triggers.auto_send_gaps("rev", &[]),
            [Gap::ReviewNeverStarts]
        );
        triggers.set("rev", Trigger::ReviewRequested, true);
        assert!(triggers.auto_send_gaps("rev", &[]).is_empty());
        triggers.stored.paused = true;
        assert_eq!(triggers.auto_send_gaps("rev", &[]), [Gap::Paused]);
    }
}

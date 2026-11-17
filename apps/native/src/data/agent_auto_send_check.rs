//! "Conferir" under an agent's "Enviar sem perguntar": what the person set
//! that will not do what they expect, said before the first real case
//! shows it — an announcement that will wait as a card, replies granted
//! in a room its sources do not use, a choice whose trigger is off. Read
//! from what is stored, with no model; nothing here changes a grant.

use dcc_core::domain::agent_grants_check::{Grants, gaps};

pub use dcc_core::domain::agent_grants_check::Gap;

use super::agent_auto_send::ReviewSend;
use super::agent_triggers::{AgentTriggers, Trigger};

impl AgentTriggers {
    /// What does not close in `agent_id`'s grants. `source_rooms`: the room
    /// of each source it watches (none: Geral). The rules are the ones an
    /// agent reads the team's state with (`agent_grants_check`).
    pub fn auto_send_gaps(&self, agent_id: &str, source_rooms: &[Option<String>]) -> Vec<Gap> {
        let Some(grants) = self.stored.auto_send.get(agent_id) else {
            return Vec::new();
        };
        let grants = Grants {
            paused: self.stored.paused,
            review: grants.review != ReviewSend::Ask,
            review_projects: grants.review_projects.clone(),
            reply: grants.reply,
            reply_rooms: grants.reply_rooms.clone(),
            deliver: grants.deliver,
            deliver_projects: grants.deliver_projects.clone(),
            announces: !grants.deliver_note.trim().is_empty(),
            fix_in_task: grants.fix_in_task,
            triggers: Trigger::ALL
                .into_iter()
                .filter(|trigger| self.is_on(agent_id, *trigger))
                .collect(),
        };
        gaps(&grants, source_rooms)
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
    use crate::data::agent_auto_send::GENERAL_ROOM;

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

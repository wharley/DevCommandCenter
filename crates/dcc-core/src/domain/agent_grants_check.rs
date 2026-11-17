//! What a person set under an agent's "Enviar sem perguntar" that will not
//! do what they expect, said before the first real case shows it: an
//! announcement that will wait as a card, replies granted in a room its
//! sources do not use, a choice whose trigger is off. Pure, with no model:
//! the desktop's "Conferir" and the team's state an agent reads
//! (`dcc_team_state`) say the same things from the same rules.

use serde_json::Value;

use super::agent_trigger::Trigger;

/// Geral among the rooms a reply grant holds in.
pub const GENERAL_ROOM: &str = "";

/// One agent's grants and switches, as the check needs them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Grants {
    /// "Pausar todos".
    pub paused: bool,
    /// Its reviews go by themselves (comment or decide).
    pub review: bool,
    pub review_projects: Vec<String>,
    pub reply: bool,
    /// A project's id each, `GENERAL_ROOM` for Geral.
    pub reply_rooms: Vec<String>,
    pub deliver: bool,
    pub deliver_projects: Vec<String>,
    /// "Ao abrir o PR" is written.
    pub announces: bool,
    pub fix_in_task: bool,
    /// The triggers turned on for it.
    pub triggers: Vec<Trigger>,
}

impl Grants {
    /// Whether anything goes without asking.
    pub fn any(&self) -> bool {
        self.review || self.reply || self.deliver || self.fix_in_task
    }
}

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

    /// The gap as an agent reads it.
    pub fn says(self) -> &'static str {
        match self {
            Self::Paused => "\"Pausar todos\" is on: nothing goes by itself until the person turns it off.",
            Self::AnnounceWaits => "\"Ao abrir o PR\" is written, but its replies are not granted anywhere: the announcement will wait as a card.",
            Self::SourceRoomNotGranted => "Its replies go without asking, but not in a room one of its sources brings news to: replies to that source will wait as cards.",
            Self::FixWithoutTrigger => "\"Corrigir na tarefa\" is picked, but its \"review received\" trigger is off: nothing starts it.",
            Self::ReviewNeverStarts => "Its reviews go without asking, but no trigger and no source starts a review.",
            Self::HoldsNowhere => "Something is granted with no project or room picked: it holds nowhere.",
        }
    }
}

/// What does not close in `grants`. `source_rooms`: the room of each source
/// the agent watches (none: Geral).
pub fn gaps(grants: &Grants, source_rooms: &[Option<String>]) -> Vec<Gap> {
    let mut gaps = Vec::new();
    if grants.any() && grants.paused {
        gaps.push(Gap::Paused);
    }
    let reply_holds = grants.reply && !grants.reply_rooms.is_empty();
    if grants.deliver && grants.announces && !reply_holds {
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
    let on = |trigger| grants.triggers.contains(&trigger);
    if grants.fix_in_task && !on(Trigger::ReviewReceived) {
        gaps.push(Gap::FixWithoutTrigger);
    }
    if grants.review
        && !grants.review_projects.is_empty()
        && !on(Trigger::ReviewRequested)
        && !on(Trigger::PrUpdated)
        && source_rooms.is_empty()
    {
        gaps.push(Gap::ReviewNeverStarts);
    }
    if (grants.review && grants.review_projects.is_empty())
        || (grants.reply && grants.reply_rooms.is_empty())
        || (grants.deliver && grants.deliver_projects.is_empty())
    {
        gaps.push(Gap::HoldsNowhere);
    }
    gaps
}

/// `agent_id`'s grants as the desktop's file keeps them
/// (`agent-triggers.json`); what the file does not say is off.
pub fn stored_grants(file: &Value, agent_id: &str) -> Grants {
    let own = &file["autoSend"][agent_id];
    let list = |value: &Value| -> Vec<String> {
        value
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    };
    let triggers = file["on"][agent_id]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| serde_json::from_value::<Trigger>(item.clone()).ok())
                .collect()
        })
        .unwrap_or_default();
    Grants {
        paused: file["paused"].as_bool().unwrap_or(false),
        review: matches!(own["review"].as_str(), Some("comment" | "decide")),
        review_projects: list(&own["reviewProjects"]),
        reply: own["reply"].as_bool().unwrap_or(false),
        reply_rooms: list(&own["replyRooms"]),
        deliver: own["deliver"].as_bool().unwrap_or(false),
        deliver_projects: list(&own["deliverProjects"]),
        announces: own["deliverNote"]
            .as_str()
            .is_some_and(|note| !note.trim().is_empty()),
        fix_in_task: own["fixInTask"].as_bool().unwrap_or(false),
        triggers,
    }
}

/// How far `agent_id`'s reviews go by themselves, as stored: "ask" when
/// the file does not say.
pub fn stored_review_send(file: &Value, agent_id: &str) -> String {
    file["autoSend"][agent_id]["review"]
        .as_str()
        .unwrap_or("ask")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_stored_file_reads_into_the_grants_the_check_uses() {
        let file = json!({
            "paused": false,
            "on": {"rev": ["reviewRequested", "reviewReceived", "somethingNewer"]},
            "autoSend": {
                "rev": {"review": "decide", "reviewProjects": ["app"], "fixInTask": true},
                "dev": {"deliver": true, "deliverProjects": ["app"], "deliverNote": "avise no #tech"},
                "sup": {"reply": true, "replyRooms": ["app"]},
            }
        });
        let rev = stored_grants(&file, "rev");
        assert!(rev.review && rev.fix_in_task && rev.any());
        assert_eq!(
            rev.triggers,
            [Trigger::ReviewRequested, Trigger::ReviewReceived]
        );
        assert!(gaps(&rev, &[]).is_empty());
        // The announcement is written and replies are not granted.
        assert_eq!(
            gaps(&stored_grants(&file, "dev"), &[]),
            [Gap::AnnounceWaits]
        );
        // Replies granted in the project's room, a source in Geral.
        let sup = stored_grants(&file, "sup");
        assert_eq!(gaps(&sup, &[None]), [Gap::SourceRoomNotGranted]);
        assert!(gaps(&sup, &[Some("app".to_string())]).is_empty());
        // An agent the file does not know has nothing granted.
        let none = stored_grants(&file, "other");
        assert!(!none.any() && gaps(&none, &[None]).is_empty());
        assert!(!stored_grants(&json!(null), "rev").any());
    }
}

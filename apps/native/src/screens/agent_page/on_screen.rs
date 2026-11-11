//! Which room the page shows now, for the room notices (`shell::room_notices`):
//! a room open shows every agent in it; an agent's "Conversa" tab shows that
//! agent alone, in the room the tab is on.

use super::AgentPage;
use super::page_tabs::AgentTab;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoomOnScreen {
    /// The room's project; none: Geral.
    pub room: Option<String>,
    /// The one agent shown (its "Conversa"); none: the whole room.
    pub agent: Option<String>,
}

impl RoomOnScreen {
    /// Whether `agent_id`'s conversation in `room` is in sight.
    pub fn shows(&self, agent_id: &str, room: Option<&str>) -> bool {
        self.room.as_deref() == room && self.agent.as_deref().is_none_or(|agent| agent == agent_id)
    }
}

impl AgentPage {
    /// The room on screen, if the page shows one.
    pub fn room_on_screen(&self, cx: &gpui::App) -> Option<RoomOnScreen> {
        if self.room_open && self.editor.is_none() {
            return Some(RoomOnScreen {
                room: self.room_project.clone(),
                agent: None,
            });
        }
        (self.tab == AgentTab::Conversation && self.editor_is_for_agent(cx)).then(|| RoomOnScreen {
            room: self.agent_room(cx),
            agent: Some(self.agent_id.clone()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::RoomOnScreen;

    #[test]
    fn a_room_shows_every_agent_and_a_conversation_only_its_own() {
        let room = RoomOnScreen {
            room: Some("checkout".into()),
            agent: None,
        };
        assert!(room.shows("dev", Some("checkout")));
        assert!(!room.shows("dev", None));
        let conversation = RoomOnScreen {
            room: None,
            agent: Some("dev".into()),
        };
        assert!(conversation.shows("dev", None));
        assert!(!conversation.shows("qa", None));
    }
}

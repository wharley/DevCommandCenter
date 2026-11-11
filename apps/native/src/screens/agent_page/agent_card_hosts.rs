//! The two places an agent card opens from: the room (its answers, live
//! rows and `@` list) and the agent's page (the team's column).

use gpui::{Context, Entity, Window};

use super::AgentPage;
use super::agent_card::{AgentCards, CardHost, agent_of};
use super::page_tabs::AgentTab;
use super::room::{RoomEvent, TeamRoom};
use crate::screens::composer::Composer;
use crate::screens::sidebar::Sidebar;

impl TeamRoom {
    /// "@Name " where the caret is in the field, focused.
    pub(super) fn address(&mut self, agent_id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(agent) = agent_of(&self.sidebar, agent_id, cx) else {
            return;
        };
        let text = format!("@{} ", agent.name);
        self.input.update(cx, |input, cx| input.insert(&text, cx));
        let handle = gpui::Focusable::focus_handle(self.input.read(cx), cx);
        window.focus(&handle, cx);
    }
}

impl CardHost for TeamRoom {
    fn cards(&mut self) -> &mut AgentCards {
        &mut self.card
    }

    fn cards_ref(&self) -> &AgentCards {
        &self.card
    }

    fn card_sources(&self) -> (Entity<Sidebar>, Entity<Composer>) {
        (self.sidebar.clone(), self.composer.clone())
    }

    fn card_message(&mut self, agent_id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.address(agent_id, window, cx);
    }

    fn card_profile(&mut self, agent_id: &str, cx: &mut Context<Self>) {
        cx.emit(RoomEvent::OpenAgent(agent_id.to_string()));
    }

    /// The card alone moved: the rows are kept, as the live poll does.
    fn card_redraw(&mut self, cx: &mut Context<Self>) {
        self.live.quiet = true;
        cx.notify();
    }
}

impl CardHost for AgentPage {
    fn cards(&mut self) -> &mut AgentCards {
        &mut self.card
    }

    fn cards_ref(&self) -> &AgentCards {
        &self.card
    }

    fn card_sources(&self) -> (Entity<Sidebar>, Entity<Composer>) {
        (self.sidebar.clone(), self.composer.clone())
    }

    /// In the room on screen when there is one; else the Geral room opens
    /// with it.
    fn card_message(&mut self, agent_id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let shown = self.room_open || self.tab == AgentTab::Conversation;
        match self.room.as_ref().map(|(room, _)| room.clone()) {
            Some(room) if shown => {
                room.update(cx, |room, cx| room.address(agent_id, window, cx));
            }
            _ => {
                let Some(agent) = agent_of(&self.sidebar, agent_id, cx) else {
                    return;
                };
                self.open_room(Some(format!("@{} ", agent.name)), cx);
            }
        }
    }

    fn card_profile(&mut self, agent_id: &str, cx: &mut Context<Self>) {
        self.show(agent_id.to_string(), cx);
    }
}

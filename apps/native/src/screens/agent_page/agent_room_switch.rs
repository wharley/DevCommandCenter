//! Which room an agent's "Conversa" tab shows (`docs/PLANO_SALA_POR_PROJETO`,
//! slice 5): the agent talks in Geral and in each project's room, a
//! conversation per room. The tab opens on the room it talked in last; when
//! it has more than one, chips under the tabs ("Geral · Checkout") switch,
//! and "Conversa completa" opens the one shown.

use dcc_core::domain::agent_conversation::home_project_id;
use gpui::{AnyElement, Context, SharedString, div, prelude::*, px};

use super::AgentPage;
use crate::data::agents::AgentRecord;
use crate::i18n::Translate;
use crate::theme::ActiveTheme;

/// The rooms `agent` has a conversation in, Geral first, each once.
pub(crate) fn agent_rooms(agent: &AgentRecord) -> Vec<Option<String>> {
    let home = home_project_id(&agent.id);
    let mut rooms: Vec<Option<String>> = vec![None];
    for session in agent
        .sessions
        .iter()
        .filter(|session| session.project_id == home)
    {
        if !rooms.contains(&session.room) {
            rooms.push(session.room.clone());
        }
    }
    rooms
}

/// The room it talked in last; none: Geral.
pub(crate) fn latest_room(agent: &AgentRecord) -> Option<String> {
    let home = home_project_id(&agent.id);
    agent
        .sessions
        .iter()
        .filter(|session| session.project_id == home)
        .max_by(|left, right| left.updated_at.cmp(&right.updated_at))
        .and_then(|session| session.room.clone())
}

impl AgentPage {
    /// The room the "Conversa" tab shows for the agent on the page.
    pub(crate) fn agent_room(&self, cx: &gpui::App) -> Option<String> {
        if let Some(picked) = &self.agent_room {
            return picked.clone();
        }
        self.sidebar
            .read(cx)
            .agents()
            .iter()
            .find(|agent| agent.id == self.agent_id)
            .and_then(latest_room)
    }

    /// The chips, when the agent talks in more than one room.
    pub(super) fn agent_room_chips(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let sidebar = self.sidebar.read(cx);
        let agent = sidebar
            .agents()
            .iter()
            .find(|agent| agent.id == self.agent_id)?;
        let rooms = agent_rooms(agent);
        if rooms.len() < 2 {
            return None;
        }
        let names: Vec<(Option<String>, SharedString)> = rooms
            .into_iter()
            .map(|room| {
                let name: SharedString = match &room {
                    None => cx.t("agents.rooms.geral"),
                    Some(id) => sidebar
                        .projects()
                        .iter()
                        .find(|project| project.project_id == *id)
                        .map(|project| project.display_name.clone())
                        .unwrap_or_else(|| id.clone())
                        .into(),
                };
                (room, name)
            })
            .collect();
        let current = self.agent_room(cx);
        let colors = cx.theme().colors.clone();
        let chips = names.into_iter().enumerate().map(|(index, (room, name))| {
            let picked = room == current;
            div()
                .id(SharedString::from(format!("agent-room-{index}")))
                .debug_selector(move || format!("agent-room-{index}"))
                .flex_none()
                .px(px(10.0))
                .h(px(26.0))
                .flex()
                .items_center()
                .rounded_full()
                .border_1()
                .border_color(colors.border.opacity(0.8))
                .text_size(px(12.0))
                .cursor_pointer()
                .when(picked, |chip| {
                    chip.bg(colors.accent).text_color(colors.foreground)
                })
                .when(!picked, |chip| {
                    chip.text_color(colors.muted_foreground)
                        .hover(|chip| chip.bg(colors.accent.opacity(0.5)))
                })
                .on_click(cx.listener(move |page, _, _, cx| {
                    page.agent_room = Some(room.clone());
                    cx.notify();
                }))
                .child(name)
        });
        Some(
            div()
                .debug_selector(|| "agent-room-chips".into())
                .flex()
                .flex_wrap()
                .gap(px(6.0))
                .px(px(24.0))
                .py(px(8.0))
                .border_b_1()
                .border_color(colors.border.opacity(0.5))
                .children(chips)
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::agents::{AgentSession, AgentState};

    fn session(id: &str, room: Option<&str>, at: &str) -> AgentSession {
        AgentSession {
            session_id: id.into(),
            workspace_id: "w-home".into(),
            state: AgentState::Idle,
            completed_at: None,
            project_id: "dcc-agent-dev".into(),
            title: None,
            updated_at: at.into(),
            room: room.map(str::to_string),
        }
    }

    #[test]
    fn the_agents_rooms_and_the_last_one_it_talked_in() {
        let mut dev = super::super::editor_role::blank_agent();
        dev.id = "dev".into();
        dev.sessions = vec![
            session("s-geral", None, "2026-11-10T10:00:00Z"),
            session("s-checkout", Some("checkout"), "2026-11-10T12:00:00Z"),
        ];
        assert_eq!(agent_rooms(&dev), [None, Some("checkout".to_string())]);
        assert_eq!(latest_room(&dev).as_deref(), Some("checkout"));
        dev.sessions.pop();
        assert_eq!(agent_rooms(&dev), [None]);
        assert_eq!(latest_room(&dev), None);
    }
}

//! The dot next to a room's name in the team's column: green while
//! something runs there, an amber question mark while something waits on the
//! person (Tucano's live dot and waiting mark). Static: it follows the data,
//! not a clock, so it adds nothing to the mascots' redraws.

use std::collections::HashMap;

use dcc_core::domain::agent_conversation::home_project_id;
use gpui::{AnyElement, SharedString, div, prelude::*, px};

use crate::data::agents::AgentState;
use crate::data::room_presence::{Presence, Where, Work, presence};
use crate::screens::sidebar::Sidebar;
use crate::ui::Icon;

/// The presence of every room with some, from the sidebar's data; `rooms`
/// are the projects with a room.
pub(super) fn rooms_presence(
    sidebar: &Sidebar,
    rooms: &[String],
) -> HashMap<Option<String>, Presence> {
    let live = |state: AgentState| matches!(state, AgentState::Working | AgentState::NeedsYou);
    let sessions = sidebar.agents().iter().flat_map(|agent| {
        let home = home_project_id(&agent.id);
        agent
            .sessions
            .iter()
            .filter(|session| live(session.state))
            .map(move |session| Work {
                // A task's session counts in its project; a conversation in
                // the room it was held in.
                at: if session.project_id == home {
                    Where::Room(session.room.as_deref())
                } else {
                    Where::Project(&session.project_id)
                },
                waiting: session.state == AgentState::NeedsYou,
            })
    });
    let running = sidebar.running_agent_tasks();
    let tasks = running.iter().map(|task| Work {
        at: Where::Project(&task.project_id),
        waiting: false,
    });
    presence(sessions.chain(tasks), rooms)
}

/// The mark for `presence`, with `key` for debugging.
pub(super) fn presence_mark(key: &str, presence: Presence) -> AnyElement {
    let key: SharedString = format!(
        "{key}-{}",
        match presence {
            Presence::Running => "running",
            Presence::Waiting => "waiting",
        }
    )
    .into();
    let debug = key.clone();
    match presence {
        Presence::Running => div()
            .debug_selector(move || debug.to_string())
            .flex_none()
            .size(px(7.0))
            .rounded_full()
            .bg(crate::theme::tailwind::GREEN_500.hsla())
            .into_any_element(),
        Presence::Waiting => div()
            .debug_selector(move || debug.to_string())
            .flex_none()
            .text_color(crate::theme::tailwind::AMBER_600.hsla())
            .child(Icon::new("circle-question-mark").size(14.0))
            .into_any_element(),
    }
}

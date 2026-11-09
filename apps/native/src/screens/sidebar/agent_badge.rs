//! Who ran a task, on its row's detail line: the provider's icon and, when
//! an agent opened the task or works in it (`data::task_agents`), the
//! agent's mascot beside it, the two overlapping by half (Monocode shows
//! whose session it is). The mascot steps with the project's glyph while
//! the task runs — the sidebar's own frame, no animation of its own.

use gpui::{AnyElement, App, IntoElement, SharedString, div, prelude::*, px};

use super::Sidebar;
use crate::agent::AgentAvatarView;
use crate::data::agents::AgentRecord;
use crate::theme::ActiveTheme;

/// The mascot's size beside an 11px provider icon.
const MASCOT: f32 = 13.0;

impl Sidebar {
    /// What the room's report shows of the task delegated as `id`.
    pub(crate) fn task_report(&self, id: &str) -> Option<&crate::data::task_reports::TaskReport> {
        self.state.data.task_reports.get(id)
    }

    /// The agent task `task_id` belongs to, if one does.
    pub(crate) fn task_agent(&self, task_id: &str) -> Option<&AgentRecord> {
        let agent_id = self.state.data.task_agents.get(task_id)?;
        self.agents().iter().find(|agent| &agent.id == agent_id)
    }
}

/// The provider's icon with the agent's mascot half over it; either alone
/// when the other is missing.
pub(super) fn runner(
    provider: Option<impl IntoElement>,
    task_id: &str,
    agent: Option<AgentRecord>,
    step: bool,
    cx: &App,
) -> Option<AnyElement> {
    let Some(agent) = agent else {
        return provider.map(IntoElement::into_any_element);
    };
    let ring = cx.theme().colors.sidebar;
    let has_provider = provider.is_some();
    let task = task_id.to_string();
    Some(
        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .children(provider)
            .child(
                div()
                    .debug_selector(move || format!("task-agent-{task}"))
                    .relative()
                    .when(has_provider, |mascot| mascot.ml(px(-4.0)))
                    .top(px(if step { -1.0 } else { 0.0 }))
                    .rounded_full()
                    .bg(ring)
                    .p(px(1.0))
                    .child(AgentAvatarView::new(
                        SharedString::from(format!("task-agent-face-{task_id}")),
                        &agent.avatar,
                        None,
                        MASCOT,
                    )),
            )
            .into_any_element(),
    )
}

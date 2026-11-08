//! `CallAgentButtons`: one button per resident agent, its mascot at
//! 18px, dimmed with the reason while it cannot be called (a turn runs;
//! the reviewer with nothing to review). Only the agents that work in the
//! task's project show: the team of that project. A view of its own: the
//! shell sets the agents and the task's changes, the conversation whether it
//! runs and its project.

use dcc_core::domain::agent_call::call_block_key;
use gpui::{Context, EventEmitter, Render, SharedString, Window, div, prelude::*};

use crate::agent::AgentAvatarView;
use crate::data::agents::AgentRecord;
use crate::i18n::{Args, Translate};
use crate::ui::{Button, ButtonSize, ButtonVariant, Tooltip};

pub enum CallAgentEvent {
    /// Start (or continue) this agent's session in the open task.
    Call(String),
}

#[derive(Default)]
pub struct CallAgentButtons {
    pub agents: Vec<AgentRecord>,
    /// A turn runs in the task.
    pub busy: bool,
    /// Anything to review; `None` while git is read.
    pub has_changes: Option<bool>,
    /// The open task's project.
    pub project_id: Option<String>,
}

impl EventEmitter<CallAgentEvent> for CallAgentButtons {}

impl CallAgentButtons {
    pub fn set_agents(&mut self, agents: Vec<AgentRecord>, cx: &mut Context<Self>) {
        if self.agents != agents {
            self.agents = agents;
            cx.notify();
        }
    }

    pub fn set_project(&mut self, project_id: Option<String>, cx: &mut Context<Self>) {
        if self.project_id != project_id {
            self.project_id = project_id;
            cx.notify();
        }
    }

    /// The agents that take a button in this task.
    pub fn team(&self) -> impl Iterator<Item = &AgentRecord> {
        self.agents.iter().filter(|agent| {
            self.project_id
                .as_deref()
                .is_none_or(|project| agent.works_in(project))
        })
    }

    pub fn set_busy(&mut self, busy: bool, cx: &mut Context<Self>) {
        if self.busy != busy {
            self.busy = busy;
            cx.notify();
        }
    }

    pub fn set_has_changes(&mut self, has_changes: Option<bool>, cx: &mut Context<Self>) {
        if self.has_changes != has_changes {
            self.has_changes = has_changes;
            cx.notify();
        }
    }

    /// Why this agent waits, as the text its button says.
    pub fn block_key(&self, agent: &AgentRecord) -> Option<&'static str> {
        call_block_key(agent.preset.as_deref(), self.busy, self.has_changes)
    }
}

impl Render for CallAgentButtons {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut row = div().flex().items_center().gap(gpui::px(4.0));
        for (index, agent) in self.team().enumerate() {
            let blocked = self.block_key(agent);
            let label: SharedString = match blocked {
                Some(key) => cx.t(key),
                None => cx.t_with(
                    "agents.call.inTask",
                    &Args::default().with("agent", agent.name.clone()),
                ),
            };
            let id = agent.id.clone();
            let button = Button::new(("call-agent", index))
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::IconSm)
                .on_click(cx.listener(move |buttons, _, _, cx| {
                    let agent = buttons.agents.iter().find(|agent| agent.id == id);
                    if agent.is_some_and(|agent| buttons.block_key(agent).is_none()) {
                        cx.emit(CallAgentEvent::Call(id.clone()));
                    }
                }))
                .child(AgentAvatarView::new(
                    ("call-agent-avatar", index),
                    &agent.avatar,
                    None,
                    18.0,
                ));
            // `aria-disabled`: it stays clickable for its reason, dimmed.
            row = row.child(Tooltip::text(
                ("call-agent-tip", index),
                div()
                    .when(blocked.is_some(), |slot| {
                        slot.opacity(0.45)
                            .cursor(gpui::CursorStyle::OperationNotAllowed)
                    })
                    .child(button),
                label,
            ));
        }
        row
    }
}

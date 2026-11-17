//! `CallAgentButtons`: the resident agents that can be called in the open
//! task. One agent is a button with its mascot; a team is a pile of its
//! mascots, overlapping, that opens a list — each with its name and, while
//! it cannot be called (a turn runs; the reviewer with nothing to review),
//! the reason. A pile, since a team of five took the header's room for
//! something clicked now and then. Only the agents that work in the task's
//! project show: the team of that project. A view of its own: the shell
//! sets the agents and the task's changes, the conversation whether it runs
//! and its project.

use dcc_core::domain::agent_call::call_block_key;
use gpui::{
    AnyElement, Context, EventEmitter, FontWeight, Render, SharedString, Window, div, prelude::*,
    px,
};

use crate::agent::AgentAvatarView;
use crate::data::agents::AgentRecord;
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::ui::popover::{Popover, PopoverAlign};
use crate::ui::{Button, ButtonSize, ButtonVariant, Tooltip};

/// Mascots drawn in the pile; the rest is a count.
const PILE: usize = 3;

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
    /// The team's list is open.
    pub open: bool,
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

impl CallAgentButtons {
    /// What the agent's button or row says: why it waits, or the call.
    fn label(&self, agent: &AgentRecord, cx: &gpui::App) -> SharedString {
        match self.block_key(agent) {
            Some(key) => cx.t(key),
            None => cx.t_with(
                "agents.call.inTask",
                &Args::default().with("agent", agent.name.clone()),
            ),
        }
    }

    fn call(&mut self, agent_id: &str, cx: &mut Context<Self>) {
        let agent = self.agents.iter().find(|agent| agent.id == agent_id);
        if agent.is_some_and(|agent| self.block_key(agent).is_none()) {
            self.open = false;
            cx.emit(CallAgentEvent::Call(agent_id.to_string()));
            cx.notify();
        }
    }

    /// One agent alone: its mascot is the button.
    fn single(&self, agent: &AgentRecord, cx: &mut Context<Self>) -> AnyElement {
        let blocked = self.block_key(agent).is_some();
        let id = agent.id.clone();
        let button = Button::new(("call-agent", 0usize))
            .variant(ButtonVariant::Ghost)
            .size(ButtonSize::IconSm)
            .on_click(cx.listener(move |buttons, _, _, cx| buttons.call(&id, cx)))
            .child(AgentAvatarView::new(
                ("call-agent-avatar", 0usize),
                &agent.avatar,
                None,
                18.0,
            ));
        // `aria-disabled`: it stays clickable for its reason, dimmed.
        Tooltip::text(
            ("call-agent-tip", 0usize),
            div()
                .when(blocked, |slot| {
                    slot.opacity(0.45)
                        .cursor(gpui::CursorStyle::OperationNotAllowed)
                })
                .child(button),
            self.label(agent, cx),
        )
        .into_any_element()
    }

    /// The team's list: each agent with its name, and why it waits.
    fn list(&self, team: &[AgentRecord], cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let mut list = div().flex().flex_col().gap(px(2.0));
        for (index, agent) in team.iter().enumerate() {
            let reason = self.block_key(agent).map(|key| cx.t(key));
            let id = agent.id.clone();
            list = list.child(
                div()
                    .id(("call-agent", index))
                    .debug_selector(move || format!("call-agent-{index}"))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .px(px(8.0))
                    .py(px(6.0))
                    .rounded(px(6.0))
                    .when(reason.is_none(), |row| {
                        row.cursor_pointer()
                            .hover(|row| row.bg(colors.accent.opacity(0.6)))
                    })
                    .when(reason.is_some(), |row| {
                        row.opacity(0.55)
                            .cursor(gpui::CursorStyle::OperationNotAllowed)
                    })
                    .on_click(cx.listener(move |buttons, _, _, cx| buttons.call(&id, cx)))
                    .child(AgentAvatarView::new(
                        ("call-agent-avatar", index),
                        &agent.avatar,
                        None,
                        20.0,
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(px(12.5))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(colors.foreground)
                                    .child(agent.name.clone()),
                            )
                            .children(reason.map(|reason| {
                                div()
                                    .text_size(px(11.0))
                                    .line_height(px(15.0))
                                    .text_color(colors.muted_foreground)
                                    .child(reason)
                            })),
                    ),
            );
        }
        list.into_any_element()
    }
}

impl Render for CallAgentButtons {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let team: Vec<AgentRecord> = self.team().cloned().collect();
        let row = div().flex().items_center();
        match team.as_slice() {
            [] => return row,
            [agent] => return row.child(self.single(agent, cx)),
            _ => {}
        }
        // The pile: the first mascots over one another, then how many more.
        let background = cx.theme().colors.background;
        let muted = cx.theme().colors.muted_foreground;
        let mut pile = div().flex().items_center();
        for (index, agent) in team.iter().take(PILE).enumerate() {
            pile = pile.child(
                div()
                    .when(index > 0, |mascot| mascot.ml(px(-7.0)))
                    .rounded_full()
                    .bg(background)
                    .p(px(1.0))
                    .child(AgentAvatarView::new(
                        ("call-agents-pile", index),
                        &agent.avatar,
                        None,
                        18.0,
                    )),
            );
        }
        if team.len() > PILE {
            pile = pile.child(
                div()
                    .ml(px(3.0))
                    .text_size(px(11.0))
                    .text_color(muted)
                    .child(format!("+{}", team.len() - PILE)),
            );
        }
        let trigger = Tooltip::text(
            "call-agents-tip",
            Button::new("call-agents")
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Sm)
                .child(pile),
            cx.t("agents.call.team"),
        );
        let list = self.list(&team, cx);
        let view = cx.entity().downgrade();
        row.child(
            Popover::new(
                "call-agents-menu",
                self.open,
                trigger,
                move |_, _| list,
                move |open, _, cx| {
                    let _ = view.update(cx, |buttons, cx| {
                        buttons.open = open;
                        cx.notify();
                    });
                },
            )
            .align(PopoverAlign::End)
            .width(260.0)
            .padding(4.0),
        )
    }
}

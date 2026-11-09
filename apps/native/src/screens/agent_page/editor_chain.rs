//! "Pode chamar sozinho" in the agent's editor (`dcc_core::domain::
//! agent_chain`): the teammates this agent may pass the turn to without the
//! person, a chip each with its mascot. The Reviewer is never offered — it
//! joins by the offer at the end of a task's turn or when the person calls
//! it. Nothing picked: every pass waits for the person's click.

use dcc_core::domain::agent::{AgentAvatar, REVIEWER_PRESET};
use gpui::{AnyElement, Context, SharedString, div, prelude::*, px};

use super::editor::{AgentEditor, field};
use crate::agent::AgentAvatarView;
use crate::data::agents::AgentRecord;
use crate::i18n::Translate;
use crate::theme::ActiveTheme;

/// A teammate the editor offers: id, name and mascot.
pub(crate) type Teammate = (String, String, AgentAvatar);

/// The teammates `agent` may be allowed to call: the others, the Reviewer
/// aside.
pub(crate) fn callable(agents: &[AgentRecord], agent_id: &str) -> Vec<Teammate> {
    agents
        .iter()
        .filter(|mate| mate.id != agent_id && mate.preset.as_deref() != Some(REVIEWER_PRESET))
        .map(|mate| (mate.id.clone(), mate.name.clone(), mate.avatar.clone()))
        .collect()
}

impl AgentEditor {
    pub(crate) fn with_teammates(mut self, teammates: Vec<Teammate>) -> Self {
        self.teammates = teammates;
        self
    }

    fn toggle_call(&mut self, id: &str) {
        if let Some(at) = self.may_call.iter().position(|picked| picked == id) {
            self.may_call.remove(at);
        } else {
            self.may_call.push(id.to_string());
        }
    }

    /// None when there is no teammate to pick.
    pub(super) fn chain_field(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.teammates.is_empty() {
            return None;
        }
        let colors = cx.theme().colors.clone();
        let chips: Vec<AnyElement> = self
            .teammates
            .clone()
            .into_iter()
            .enumerate()
            .map(|(index, (id, name, avatar))| {
                let selected = self.may_call.contains(&id);
                div()
                    .id(SharedString::from(format!("agent-editor-call-{index}")))
                    .debug_selector(move || format!("agent-editor-call-{index}"))
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .pl(px(4.0))
                    .pr(px(10.0))
                    .h(px(28.0))
                    .rounded(px(14.0))
                    .border_1()
                    .text_size(px(12.0))
                    .cursor_pointer()
                    .map(|chip| {
                        if selected {
                            chip.border_color(colors.ring)
                                .bg(colors.accent)
                                .text_color(colors.foreground)
                        } else {
                            chip.border_color(colors.border.opacity(0.7))
                                .text_color(colors.muted_foreground)
                                .hover(|chip| chip.bg(colors.accent.opacity(0.5)))
                        }
                    })
                    .on_click(cx.listener(move |editor, _, _, cx| {
                        editor.toggle_call(&id);
                        cx.notify();
                    }))
                    .child(AgentAvatarView::new(
                        SharedString::from(format!("agent-editor-call-face-{index}")),
                        &avatar,
                        None,
                        20.0,
                    ))
                    .child(name)
                    .into_any_element()
            })
            .collect();
        Some(field(
            cx.t("agents.editor.mayCall"),
            Some(cx.t("agents.editor.mayCallHint")),
            div()
                .flex()
                .flex_wrap()
                .gap(px(6.0))
                .children(chips)
                .into_any_element(),
            cx,
        ))
    }
}

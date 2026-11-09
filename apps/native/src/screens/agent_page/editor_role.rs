//! What only an agent of the person's has in "Editar": its role (what it is
//! and does, sent with each of its turns) and what a call asks it first.
//! A built-in agent's role is DCC's. "Novo agente" opens the editor on a
//! blank agent of this kind.

use dcc_core::domain::agent::{AgentAvatar, MAX_AGENT_KICKOFF_CHARS, MAX_AGENT_ROLE_CHARS};
use gpui::{AnyElement, Context, div, prelude::*, px};

use super::editor::{AgentEditor, boxed, field};
use crate::data::agents::AgentRecord;
use crate::i18n::Translate;

/// The agent "Novo agente" starts from: no name or role yet, the octopus.
pub(crate) fn blank_agent() -> AgentRecord {
    AgentRecord {
        id: String::new(),
        name: String::new(),
        preset: None,
        avatar: AgentAvatar {
            color: "cyan".to_string(),
            arms: 4,
            eyes: "round".to_string(),
            shape: "octopus".to_string(),
            accessory: "none".to_string(),
        },
        provider_id: None,
        model: None,
        effort: None,
        kickoff_prompt: String::new(),
        role: String::new(),
        offer_prompt: String::new(),
        extra_instructions: String::new(),
        projects: Vec::new(),
        monthly_token_limit: None,
        tool_access: Default::default(),
        auto_start_tasks: false,
        may_call: Vec::new(),
        sessions: Vec::new(),
    }
}

impl AgentEditor {
    /// "Novo agente", not saved yet.
    pub(crate) fn is_new(&self) -> bool {
        self.agent.id.is_empty()
    }

    /// A name, and for the person's own agent a role, within their limits.
    pub(crate) fn can_save(&self, cx: &gpui::App) -> bool {
        if self.name.read(cx).text().trim().is_empty() {
            return false;
        }
        if self.agent.preset.is_some() {
            return true;
        }
        let role = self.role.read(cx).text().trim().chars().count();
        let kickoff = self.kickoff.read(cx).text().trim().chars().count();
        (1..=MAX_AGENT_ROLE_CHARS).contains(&role) && kickoff <= MAX_AGENT_KICKOFF_CHARS
    }

    /// "Papel" and "Ao chamar", for an agent of the person's.
    pub(super) fn role_fields(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.agent.preset.is_some() {
            return None;
        }
        Some(
            div()
                .flex()
                .flex_col()
                .gap(px(16.0))
                .child(field(
                    cx.t("agents.editor.role"),
                    Some(cx.t("agents.editor.roleHint")),
                    boxed(self.role.clone().into_any_element(), cx)
                        .debug_selector(|| "agent-editor-role".into())
                        .min_h(px(120.0))
                        .py(px(8.0))
                        .into_any_element(),
                    cx,
                ))
                .child(field(
                    cx.t("agents.editor.kickoff"),
                    Some(cx.t("agents.editor.kickoffHint")),
                    boxed(self.kickoff.clone().into_any_element(), cx)
                        .debug_selector(|| "agent-editor-kickoff".into())
                        .min_h(px(64.0))
                        .py(px(8.0))
                        .into_any_element(),
                    cx,
                ))
                .into_any_element(),
        )
    }
}

//! "Começar de" on "Novo agente": blank, or a template from
//! `dcc_core::domain::agent_templates` (Research, QA) that fills the name,
//! the role, what a call asks first and the mascot. Everything stays the
//! person's to change before saving; the agent has no code of its own.

use dcc_core::domain::agent_templates::{AGENT_TEMPLATES, template_draft};
use gpui::{AnyElement, Context, SharedString};

use super::editor::{AgentEditor, field};
use super::editor_role::blank_agent;
use crate::i18n::Translate;
use crate::ui::ToggleGroup;

impl AgentEditor {
    /// Fills the editor from `template` (`None`: blank).
    pub(crate) fn apply_template(
        &mut self,
        template: Option<&'static str>,
        cx: &mut Context<Self>,
    ) {
        self.template = template;
        let blank = blank_agent();
        let (name, role, kickoff, avatar) = match template.and_then(|id| {
            let name = cx.t(&format!("agents.templates.{id}.name"));
            template_draft(id, &name)
        }) {
            Some(draft) => (draft.name, draft.role, draft.kickoff_prompt, draft.avatar),
            None => (blank.name, blank.role, blank.kickoff_prompt, blank.avatar),
        };
        self.name.update(cx, |input, cx| input.set_text(name, cx));
        self.role.update(cx, |area, cx| area.set_text(&role, cx));
        self.kickoff
            .update(cx, |area, cx| area.set_text(&kickoff, cx));
        self.avatar = avatar;
        cx.notify();
    }

    /// The row on a new agent only.
    pub(super) fn template_row(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.is_new() {
            return None;
        }
        let mut labels: Vec<SharedString> = vec![cx.t("agents.templates.blank")];
        labels.extend(
            AGENT_TEMPLATES
                .iter()
                .map(|id| cx.t(&format!("agents.templates.{id}.label"))),
        );
        let picked = match self.template {
            None => 0,
            Some(id) => AGENT_TEMPLATES
                .iter()
                .position(|template| *template == id)
                .map_or(0, |index| index + 1),
        };
        let editor = cx.entity().downgrade();
        let group = ToggleGroup::new("agent-editor-template", labels, Some(picked)).on_select(
            move |index, _, cx| {
                let template = index.checked_sub(1).map(|index| AGENT_TEMPLATES[index]);
                let _ = editor.update(cx, |editor, cx| editor.apply_template(template, cx));
            },
        );
        Some(field(
            cx.t("agents.templates.title"),
            Some(cx.t("agents.templates.hint")),
            gpui::IntoElement::into_any_element(group),
            cx,
        ))
    }
}

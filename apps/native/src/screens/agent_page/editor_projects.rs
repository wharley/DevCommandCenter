//! "Projetos" in "Editar …": where the agent works — every project (the
//! default) or the ones the person picks. Its call buttons in a task and its
//! triggers follow this; a chip per project, "Todos" clears the choice.

use gpui::{AnyElement, Context, SharedString, div, prelude::*, px};

use super::editor::AgentEditor;
use crate::data::tasks::Project;
use crate::i18n::Translate;
use crate::theme::ActiveTheme;

/// One choice per project (a project can have several checkouts), labeled by
/// its first one's name, in the sidebar's order.
pub(crate) fn project_options(projects: &[Project]) -> Vec<(String, String)> {
    dcc_core::domain::room_list::project_names(
        projects
            .iter()
            .map(|project| (project.project_id.as_str(), project.display_name.as_str())),
    )
}

impl AgentEditor {
    /// Picks or drops `project`; none left is every project.
    pub(crate) fn toggle_project(&mut self, project: &str) {
        if let Some(index) = self.projects.iter().position(|id| id == project) {
            self.projects.remove(index);
        } else {
            self.projects.push(project.to_string());
        }
    }

    fn project_chip(
        &self,
        id: String,
        label: SharedString,
        selected: bool,
        project: Option<String>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        div()
            .id(SharedString::from(id.clone()))
            .debug_selector(move || id.clone())
            .px(px(10.0))
            .h(px(26.0))
            .flex()
            .items_center()
            .rounded(px(13.0))
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
                match &project {
                    Some(project) => editor.toggle_project(project),
                    None => editor.projects.clear(),
                }
                cx.notify();
            }))
            .child(label)
            .into_any_element()
    }

    /// "Todos" and a chip per project.
    pub(super) fn project_chips(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut chips = vec![self.project_chip(
            "agent-editor-project-all".to_string(),
            cx.t("agents.editor.projectsAll"),
            self.projects.is_empty(),
            None,
            cx,
        )];
        for (index, (id, name)) in self.project_options.clone().into_iter().enumerate() {
            let selected = self.projects.contains(&id);
            chips.push(self.project_chip(
                format!("agent-editor-project-{index}"),
                name.into(),
                selected,
                Some(id),
                cx,
            ));
        }
        div()
            .flex()
            .flex_wrap()
            .gap(px(6.0))
            .children(chips)
            .into_any_element()
    }
}

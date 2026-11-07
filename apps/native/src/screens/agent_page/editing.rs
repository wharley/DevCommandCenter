//! Opening "Editar" on the agent page, or the editor on a blank agent for
//! "Novo agente", and what the page does once it saves.

use gpui::{AppContext, Context, Window};

use super::{AgentPage, editor, editor_projects, editor_role};
use crate::data::agents::AgentRecord;

impl AgentPage {
    /// "Editar": the stored values, read when it opens.
    pub(crate) fn open_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(data) = self.sidebar.read(cx).agent_page(&self.agent_id) else {
            return;
        };
        self.edit(data.agent, window, cx);
    }

    /// The sidebar reads DCC's database again (tests feed it themselves).
    fn reload_agents(&mut self, cx: &mut Context<Self>) {
        if !cfg!(test) {
            self.sidebar
                .update(cx, |sidebar, cx| sidebar.reload_now(cx));
        }
    }

    /// "Novo agente": the editor on a blank agent of the person's.
    pub(crate) fn open_new_agent(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.edit(editor_role::blank_agent(), window, cx);
    }

    pub(super) fn edit(&mut self, agent: AgentRecord, window: &mut Window, cx: &mut Context<Self>) {
        let catalog = self
            .composer
            .read(cx)
            .catalog
            .as_deref()
            .cloned()
            .unwrap_or_default();
        let path = self.cleanup_path.clone();
        let options = editor_projects::project_options(self.sidebar.read(cx).projects());
        let editor = cx.new(|cx| {
            editor::AgentEditor::new(agent, catalog, path, window, cx).with_projects(options)
        });
        let events = cx.subscribe(&editor, |page, _, event, cx| match event {
            editor::EditorEvent::Closed => {
                page.editor = None;
                cx.notify();
            }
            // The sidebar reads the agents again right away.
            editor::EditorEvent::Saved => page.reload_agents(cx),
            // The new agent's page, once the sidebar has read it.
            editor::EditorEvent::Created(id) => {
                let id = id.clone();
                page.reload_agents(cx);
                page.show(id, cx);
            }
        });
        self.editor = Some((editor, events));
        cx.notify();
    }
}

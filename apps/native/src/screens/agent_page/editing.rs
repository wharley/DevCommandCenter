//! Opening "Editar" on the agent page, the editor on a blank agent for
//! "Novo agente" or on an agent proposed ("Editar antes"), and what the
//! page does once it saves.

use gpui::{AppContext, Context, Window};

use super::{AgentPage, editor, editor_chain, editor_projects, editor_role};
use dcc_core::domain::agent_proposal::AgentProposal;

use crate::data::agent_proposals::draft_record;
use crate::data::agents::AgentRecord;
use crate::i18n::Translate;

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

    /// "Editar antes" on an agent proposed: the editor filled with it; once
    /// created, the proposal is answered.
    pub(crate) fn edit_proposal(
        &mut self,
        proposal: AgentProposal,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.room_open = false;
        self.edit(draft_record(&proposal), window, cx);
        self.editing_proposal = Some(proposal);
    }

    pub(super) fn edit(&mut self, agent: AgentRecord, window: &mut Window, cx: &mut Context<Self>) {
        self.editing_proposal = None;
        let catalog = self
            .composer
            .read(cx)
            .catalog
            .as_deref()
            .cloned()
            .unwrap_or_default();
        let options = editor_projects::project_options(self.sidebar.read(cx).projects());
        let teammates = editor_chain::callable(self.sidebar.read(cx).agents(), &agent.id);
        // A new agent starts on the model the composer has now, as its own.
        let mut agent = agent;
        if agent.id.is_empty() && agent.provider_id.is_none() {
            let session = &self.composer.read(cx).session;
            agent.provider_id = session.provider_id.clone();
            agent.model = session.model.clone();
        }
        let editor = cx.new(|cx| {
            editor::AgentEditor::new(agent, catalog, window, cx)
                .with_projects(options)
                .with_teammates(teammates)
        });
        let events = cx.subscribe(&editor, |page, _, event, cx| match event {
            editor::EditorEvent::Closed => {
                page.editor = None;
                page.editing_proposal = None;
                cx.notify();
            }
            // "Conversando": the room, asked to build one.
            editor::EditorEvent::Talk => {
                page.editor = None;
                let draft = cx.t("agents.templates.talk.draft").to_string();
                page.open_room(Some(draft), cx);
            }
            // The sidebar reads the agents again right away.
            editor::EditorEvent::Saved => page.reload_agents(cx),
            // The new agent's page, once the sidebar has read it.
            editor::EditorEvent::Created(id) => {
                let id = id.clone();
                page.answer_edited_proposal(cx);
                page.reload_agents(cx);
                page.show(id, cx);
            }
        });
        self.editor = Some((editor, events));
        cx.notify();
    }

    /// The proposal just created from the editor leaves the room.
    fn answer_edited_proposal(&mut self, cx: &mut Context<Self>) {
        let (Some(proposal), Some(api)) = (self.editing_proposal.take(), self.api.clone()) else {
            return;
        };
        cx.background_executor()
            .spawn(async move {
                let _ = api.dismiss_agent_proposal(&proposal);
            })
            .detach();
    }
}

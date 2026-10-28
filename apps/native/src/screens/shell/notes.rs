//! "Anotações" over the window (`notesOpen`): the sidebar's row opens the
//! library, on the task shown (its project, task and conversation) and the
//! app's projects.

use gpui::{AnyElement, AppContext, Context, Entity, IntoElement, Window};

use super::{Shell, Surface};
use crate::data::notes::NotesStore;
use crate::screens::notes::{NoteProject, NotesScope, NotesWorkspace};

impl Shell {
    fn notes_workspace(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<NotesWorkspace> {
        if let Some(notes) = &self.notes {
            return notes.clone();
        }
        let store = cx.new(|_| {
            if cfg!(test) {
                let dir = std::env::temp_dir().join("dcc-native-test-notes");
                let _ = std::fs::create_dir_all(&dir);
                NotesStore::new(dir.join("database.sqlite"), dir.join("notes-pending.json"))
            } else {
                NotesStore::for_app()
            }
        });
        let notes = cx.new(|cx| NotesWorkspace::new(store, window, cx));
        self.notes = Some(notes.clone());
        notes
    }

    /// `scope`: the task on screen, while the conversation shows.
    fn notes_scope(&self, cx: &gpui::App) -> Option<NotesScope> {
        if self.surface != Surface::Conversation {
            return None;
        }
        let (project_id, task_id, session_id, title) = self.conversation.read(cx).note_source()?;
        let sidebar = self.sidebar.read(cx);
        let task = sidebar.tasks().iter().find(|task| task.id == task_id);
        let project_id = project_id
            .or_else(|| task.map(|task| task.project_id.clone()))
            .unwrap_or_else(|| task_id.clone());
        let project_name = sidebar
            .projects()
            .iter()
            .find(|project| project.project_id == project_id)
            .map(|project| project.display_name.clone())
            .or_else(|| task.map(|task| task.name.clone()))
            .unwrap_or_default();
        Some(NotesScope {
            project_id,
            project_name,
            workspace_id: Some(task_id),
            session_id,
            task_title: title,
        })
    }

    pub(super) fn open_notes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let scope = self.notes_scope(cx);
        let projects = self
            .sidebar
            .read(cx)
            .projects()
            .iter()
            .map(|project| NoteProject {
                id: project.project_id.clone(),
                name: project.display_name.clone(),
            })
            .collect();
        let notes = self.notes_workspace(window, cx);
        notes.update(cx, |notes, cx| {
            notes.set_context(scope, projects, cx);
            notes.open_library(cx);
        });
    }

    pub(super) fn notes_overlay(&self) -> Option<AnyElement> {
        self.notes
            .as_ref()
            .map(|notes| notes.clone().into_any_element())
    }
}

//! "Anotações" over the window (`notesOpen`): the notes and their balloons
//! from the start, told the task shown (its project, task and
//! conversation) and the app's projects as they change; the sidebar's row
//! opens the library.

use gpui::{AnyElement, AppContext, Context, Entity, IntoElement, Subscription, Window};

use super::{Shell, Surface};
use crate::data::notes::NotesStore;
use crate::screens::notes::{NoteProject, NotesScope, NotesWorkspace};

impl Shell {
    /// The notes, made with the shell: their balloons come back on entering a task.
    pub(super) fn new_notes(
        sidebar: &Entity<crate::screens::sidebar::Sidebar>,
        conversation: &Entity<crate::screens::conversation::ConversationView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Entity<NotesWorkspace>, Vec<Subscription>) {
        let store = cx.new(|_| {
            if cfg!(test) {
                let dir = std::env::temp_dir()
                    .join(format!("dcc-native-test-notes-{}", std::process::id()));
                let _ = std::fs::create_dir_all(&dir);
                NotesStore::new(dir.join("database.sqlite"), dir.join("notes-pending.json"))
            } else {
                NotesStore::for_app()
            }
        });
        let notes = cx.new(|cx| {
            let notes = NotesWorkspace::new(store, window, cx);
            if cfg!(test) {
                notes
            } else {
                notes.with_positions(crate::prefs::native_file("notes-positions.json"))
            }
        });
        let follow = vec![
            cx.observe(sidebar, |shell, _, cx| shell.feed_notes(cx)),
            cx.observe(conversation, |shell, _, cx| shell.feed_notes(cx)),
        ];
        (notes, follow)
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

    /// The task on screen and the projects, for the notes.
    pub(super) fn feed_notes(&mut self, cx: &mut Context<Self>) {
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
            .collect::<Vec<_>>();
        let notes = self.notes.clone();
        notes.update(cx, |notes, cx| {
            if notes.scope != scope || notes.projects != projects {
                notes.set_context(scope, projects, cx);
            }
        });
    }

    pub(super) fn open_notes(&mut self, cx: &mut Context<Self>) {
        self.feed_notes(cx);
        self.notes.update(cx, |notes, cx| notes.open_library(cx));
    }

    pub(super) fn notes_overlay(&self) -> AnyElement {
        self.notes.clone().into_any_element()
    }
}

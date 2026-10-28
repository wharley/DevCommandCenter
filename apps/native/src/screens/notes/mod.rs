//! Project notes on screen (`NotesWorkspace`): the library dialog —
//! search, "Nova anotação", Abertas/Concluídas, the project and
//! conversation filters, the notes as cards — and deleting them. A note
//! picked or created opens as a balloon; the balloons are drawn in the next
//! slice, this one keeps which are open.

mod card;
mod library;
mod palette;
#[cfg(test)]
mod tests;

use gpui::{
    AppContext, Context, Entity, EventEmitter, Render, Subscription, Window, div, prelude::*,
};

use crate::data::notes::{CreateProjectNote, NotesStore, ProjectNote};
use crate::i18n::Translate;
use crate::ui::Toaster;
use crate::ui::text_input::{TextInput, TextInputEvent};

/// `[...context].length > 12_000`
const CONTEXT_LIMIT: usize = 12_000;
/// Balloons kept open besides the one just shown.
const OTHER_BALLOONS: usize = 3;

/// `NotesScope`: the task on screen, where a new note comes from.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NotesScope {
    pub project_id: String,
    pub project_name: String,
    pub workspace_id: Option<String>,
    pub session_id: Option<String>,
    pub task_title: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteProject {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StatusFilter {
    #[default]
    Open,
    Completed,
}

impl StatusFilter {
    pub fn key(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Completed => "completed",
        }
    }
}

pub enum NotesEvent {
    /// The open notes' count changed (the sidebar's badge).
    Count(usize),
}

pub struct NotesWorkspace {
    pub store: Entity<NotesStore>,
    pub open: bool,
    pub scope: Option<NotesScope>,
    pub projects: Vec<NoteProject>,
    pub status: StatusFilter,
    /// `None`: every project.
    pub project_filter: Option<String>,
    /// A conversation's title; `None`: every one.
    pub conversation_filter: Option<String>,
    pub search: Entity<TextInput>,
    /// The balloons open, oldest first.
    pub floating: Vec<String>,
    /// The balloon whose title takes the focus.
    pub focus_note: Option<String>,
    /// Waiting for "Excluir".
    pub delete_ids: Vec<String>,
    pub busy: bool,
    pub creating: bool,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<NotesEvent> for NotesWorkspace {}

/// `filtered`: the status, project and conversation, then the search over
/// the title, text, context and conversation.
pub fn filtered<'a>(
    notes: &'a [ProjectNote],
    status: StatusFilter,
    project: Option<&str>,
    conversation: Option<&str>,
    search: &str,
) -> Vec<&'a ProjectNote> {
    let search = search.to_lowercase();
    notes
        .iter()
        .filter(|note| {
            note.status == status.key()
                && project.is_none_or(|project| note.project_id == project)
                && conversation.is_none_or(|title| note.source_task_title == title)
                && format!(
                    "{} {} {} {}",
                    note.title, note.content, note.context_snapshot, note.source_task_title
                )
                .to_lowercase()
                .contains(&search)
        })
        .collect()
}

/// The conversations notes came from, under the project filter, first seen first.
pub fn conversations(notes: &[ProjectNote], project: Option<&str>) -> Vec<String> {
    let mut seen = Vec::new();
    for note in notes {
        if project.is_none_or(|project| note.project_id == project)
            && !note.source_task_title.is_empty()
            && !seen.contains(&note.source_task_title)
        {
            seen.push(note.source_task_title.clone());
        }
    }
    seen
}

impl NotesWorkspace {
    pub fn new(store: Entity<NotesStore>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let placeholder = cx.t("notes.search");
        let search = cx.new(|cx| TextInput::new(window, cx).with_placeholder(placeholder));
        let subscriptions = vec![
            cx.subscribe(&search, |_, _, event, cx| {
                if matches!(event, TextInputEvent::Changed) {
                    cx.notify();
                }
            }),
            cx.observe(&store, |_, store, cx| {
                cx.emit(NotesEvent::Count(store.read(cx).open_count()));
                cx.notify();
            }),
        ];
        store.update(cx, |store, cx| store.load(cx));
        Self {
            store,
            open: false,
            scope: None,
            projects: Vec::new(),
            status: StatusFilter::Open,
            project_filter: None,
            conversation_filter: None,
            search,
            floating: Vec::new(),
            focus_note: None,
            delete_ids: Vec::new(),
            busy: false,
            creating: false,
            _subscriptions: subscriptions,
        }
    }

    /// The task on screen and the projects, from the shell.
    pub fn set_context(
        &mut self,
        scope: Option<NotesScope>,
        projects: Vec<NoteProject>,
        cx: &mut Context<Self>,
    ) {
        self.scope = scope;
        self.projects = projects;
        cx.notify();
    }

    /// Opening starts on the task's project and reads the notes again.
    pub fn open_library(&mut self, cx: &mut Context<Self>) {
        if !self.open {
            self.project_filter = self.scope.as_ref().map(|scope| scope.project_id.clone());
            self.conversation_filter = None;
            self.store.update(cx, |store, cx| store.load(cx));
        }
        self.open = true;
        cx.notify();
    }

    pub fn close_library(&mut self, cx: &mut Context<Self>) {
        self.open = false;
        cx.notify();
    }

    /// The notes' projects and the app's, each once.
    pub fn all_projects(&self, cx: &gpui::App) -> Vec<NoteProject> {
        let mut all: Vec<NoteProject> = Vec::new();
        let notes = self.store.read(cx).notes.iter().map(|note| NoteProject {
            id: note.project_id.clone(),
            name: note.project_name.clone(),
        });
        for project in notes.chain(self.projects.iter().cloned()) {
            match all.iter_mut().find(|known| known.id == project.id) {
                Some(known) => *known = project,
                None => all.push(project),
            }
        }
        all
    }

    pub fn set_status(&mut self, status: StatusFilter, cx: &mut Context<Self>) {
        self.status = status;
        cx.notify();
    }

    pub fn set_project_filter(&mut self, project: Option<String>, cx: &mut Context<Self>) {
        self.project_filter = project;
        self.conversation_filter = None;
        cx.notify();
    }

    pub fn set_conversation_filter(&mut self, title: Option<String>, cx: &mut Context<Self>) {
        self.conversation_filter = title;
        cx.notify();
    }

    /// `show`: its balloon in front (with three others at most); the library closes.
    pub fn show(&mut self, id: &str, cx: &mut Context<Self>) {
        self.focus_note = Some(id.to_string());
        self.floating.retain(|open| open != id);
        let extra = self.floating.len().saturating_sub(OTHER_BALLOONS);
        self.floating.drain(..extra);
        self.floating.push(id.to_string());
        self.open = false;
        cx.notify();
    }

    /// `close`: the balloon goes; the note stays.
    pub fn close_balloon(&mut self, id: &str, cx: &mut Context<Self>) {
        self.floating.retain(|open| open != id);
        cx.notify();
    }

    /// `create`: in the filter's project while the library shows, else the task's.
    pub fn create(&mut self, context: String, cx: &mut Context<Self>) {
        if self.creating {
            return;
        }
        let scope = self.scope.clone();
        let wanted = if self.open && self.project_filter.is_some() {
            self.project_filter.clone()
        } else {
            scope.as_ref().map(|scope| scope.project_id.clone())
        };
        let Some(project) = self
            .projects
            .iter()
            .find(|project| Some(&project.id) == wanted.as_ref())
            .cloned()
        else {
            self.open_library(cx);
            Toaster::info(cx.t("notes.chooseProject"), cx);
            return;
        };
        if context.chars().count() > CONTEXT_LIMIT {
            Toaster::error(cx.t("notes.contextTooLong"), cx);
            return;
        }
        let same = scope.filter(|scope| scope.project_id == project.id);
        let input = CreateProjectNote {
            project_id: project.id.clone(),
            project_name: project.name.clone(),
            title: String::new(),
            content: String::new(),
            context_snapshot: if same.is_some() {
                context
            } else {
                String::new()
            },
            source_session_id: same.as_ref().and_then(|scope| scope.session_id.clone()),
            source_workspace_id: same.as_ref().and_then(|scope| scope.workspace_id.clone()),
            source_task_title: same
                .as_ref()
                .map(|scope| scope.task_title.chars().take(300).collect())
                .unwrap_or_default(),
        };
        self.creating = true;
        cx.notify();
        let created = self.store.update(cx, |store, cx| store.create(input, cx));
        cx.spawn(async move |this, cx| {
            let note = created.await;
            let _ = this.update(cx, |workspace, cx| {
                workspace.creating = false;
                match note {
                    Ok(note) => workspace.show(&note.id, cx),
                    Err(_) => Toaster::error(cx.t("notes.createError"), cx),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn ask_delete(&mut self, ids: Vec<String>, cx: &mut Context<Self>) {
        self.delete_ids = ids;
        cx.notify();
    }

    pub fn cancel_delete(&mut self, cx: &mut Context<Self>) {
        if !self.busy {
            self.delete_ids.clear();
            cx.notify();
        }
    }

    pub fn confirm_delete(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.delete_ids.is_empty() {
            return;
        }
        self.busy = true;
        cx.notify();
        let ids = self.delete_ids.clone();
        let gone = self
            .store
            .update(cx, |store, cx| store.remove(ids.clone(), cx));
        cx.spawn(async move |this, cx| {
            let gone = gone.await;
            let _ = this.update(cx, |workspace, cx| {
                workspace.busy = false;
                match gone {
                    Ok(()) => {
                        workspace.floating.retain(|id| !ids.contains(id));
                        workspace.delete_ids.clear();
                    }
                    Err(_) => Toaster::error(cx.t("notes.deleteError"), cx),
                }
                cx.notify();
            });
        })
        .detach();
    }
}

impl Render for NotesWorkspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .children(self.open.then(|| self.library(window, cx)))
            .children(self.delete_dialog(cx))
    }
}

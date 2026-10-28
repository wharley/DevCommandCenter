//! Project notes (`notes-store.ts`), straight in the database through
//! `dcc_infra::notes` like the native app's other writes. Writes run one at
//! a time per note; typing waits 450ms before it saves, and a late answer
//! never replaces newer typing. Text not saved yet is kept in
//! `notes-pending.json` (the desktop's `localStorage`) and comes back on the
//! next read — only text, never a completion, task link or deletion.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use dcc_infra::db::SqliteSessionRepo;
use dcc_infra::notes::UpdateProjectNote;
pub use dcc_infra::notes::{CreateProjectNote, ProjectNote};
use gpui::{Context, EventEmitter, Task};
use serde::{Deserialize, Serialize};

/// The typing pause before a save.
pub const SAVE_DELAY: Duration = Duration::from_millis(450);
pub const COLORS: [&str; 4] = ["amber", "mint", "violet", "sky"];

/// `NotePatch`
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NotePatch {
    pub title: Option<String>,
    pub content: Option<String>,
    pub status: Option<String>,
    pub color: Option<String>,
    pub pinned: Option<bool>,
    pub implementation_task_id: Option<Option<String>>,
}

impl NotePatch {
    /// `{ ...self, ...newer }`
    fn merged(self, newer: &NotePatch) -> NotePatch {
        NotePatch {
            title: newer.title.clone().or(self.title),
            content: newer.content.clone().or(self.content),
            status: newer.status.clone().or(self.status),
            color: newer.color.clone().or(self.color),
            pinned: newer.pinned.or(self.pinned),
            implementation_task_id: newer
                .implementation_task_id
                .clone()
                .or(self.implementation_task_id),
        }
    }

    pub fn apply(&self, note: &ProjectNote) -> ProjectNote {
        let mut note = note.clone();
        if let Some(title) = &self.title {
            note.title = title.clone();
        }
        if let Some(content) = &self.content {
            note.content = content.clone();
        }
        if let Some(status) = &self.status {
            note.status = status.clone();
        }
        if let Some(color) = &self.color {
            note.color = color.clone();
        }
        if let Some(pinned) = self.pinned {
            note.pinned = pinned;
        }
        if let Some(task) = &self.implementation_task_id {
            note.implementation_task_id = task.clone();
        }
        note
    }
}

/// `notePrompt`: what "Adicionar à conversa atual" writes in the composer.
pub fn note_prompt(note: &ProjectNote) -> String {
    let context = (!note.context_snapshot.is_empty()).then(|| {
        format!(
            "---\n{}\n<context_snapshot>\n{}\n</context_snapshot>",
            note.source_task_title, note.context_snapshot
        )
    });
    [
        Some(note.title.clone()),
        Some(note.content.clone()),
        context,
    ]
    .into_iter()
    .flatten()
    .filter(|part| !part.is_empty())
    .collect::<Vec<_>>()
    .join("\n\n")
}

/// The draft text kept while a save is owed.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Draft {
    title: String,
    content: String,
}

pub enum NotesEvent {
    /// Notes came, went or changed.
    Changed,
}

pub struct NotesStore {
    db_path: PathBuf,
    drafts_path: PathBuf,
    pub notes: Vec<ProjectNote>,
    pub loading: bool,
    pub error: Option<String>,
    pub saving: HashSet<String>,
    pub failed: HashSet<String>,
    /// What the database last said of each note.
    base: HashMap<String, ProjectNote>,
    pending: HashMap<String, NotePatch>,
    running: HashSet<String>,
    timers: HashMap<String, Task<()>>,
    deleting: HashSet<String>,
    membership: u64,
    load_sequence: u64,
}

impl EventEmitter<NotesEvent> for NotesStore {}

fn open(path: &Path) -> Result<SqliteSessionRepo, String> {
    SqliteSessionRepo::open(path).map_err(|error| error.to_string())
}

impl NotesStore {
    pub fn new(db_path: PathBuf, drafts_path: PathBuf) -> Self {
        Self {
            db_path,
            drafts_path,
            notes: Vec::new(),
            loading: true,
            error: None,
            saving: HashSet::new(),
            failed: HashSet::new(),
            base: HashMap::new(),
            pending: HashMap::new(),
            running: HashSet::new(),
            timers: HashMap::new(),
            deleting: HashSet::new(),
            membership: 0,
            load_sequence: 0,
        }
    }

    /// The app's own database and drafts file.
    pub fn for_app() -> Self {
        Self::new(
            super::loader::database_path(),
            crate::prefs::native_file("notes-pending.json"),
        )
    }

    pub fn open_count(&self) -> usize {
        self.notes
            .iter()
            .filter(|note| note.status == "open")
            .count()
    }

    fn changed(&self, cx: &mut Context<Self>) {
        cx.emit(NotesEvent::Changed);
        cx.notify();
    }

    /// `backup`: the text of every note with a save owed.
    fn backup(&self) {
        let dirty: HashSet<&String> = self.pending.keys().chain(self.running.iter()).collect();
        let drafts: HashMap<&str, Draft> = self
            .notes
            .iter()
            .filter(|note| dirty.contains(&note.id))
            .map(|note| {
                (
                    note.id.as_str(),
                    Draft {
                        title: note.title.clone(),
                        content: note.content.clone(),
                    },
                )
            })
            .collect();
        if let Ok(json) = serde_json::to_string(&drafts) {
            let _ = std::fs::write(&self.drafts_path, json);
        }
    }

    fn recover(&mut self, notes: &[ProjectNote]) {
        let drafts: HashMap<String, Draft> = std::fs::read_to_string(&self.drafts_path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        for note in notes {
            if let Some(draft) = drafts.get(&note.id) {
                self.pending.insert(
                    note.id.clone(),
                    NotePatch {
                        title: Some(draft.title.chars().take(160).collect()),
                        content: Some(draft.content.chars().take(20_000).collect()),
                        ..NotePatch::default()
                    },
                );
            }
        }
    }

    /// `load`
    pub fn load(&mut self, cx: &mut Context<Self>) {
        let membership = self.membership;
        self.load_sequence += 1;
        let sequence = self.load_sequence;
        let path = self.db_path.clone();
        cx.spawn(async move |this, cx| {
            let read = cx
                .background_executor()
                .spawn(async move { open(&path)?.list_project_notes() })
                .await;
            let _ = this.update(cx, |store, cx| {
                if membership != store.membership || sequence != store.load_sequence {
                    return;
                }
                let notes = match read {
                    Ok(notes) => notes,
                    Err(error) => {
                        store.loading = false;
                        store.error = Some(error);
                        store.changed(cx);
                        return;
                    }
                };
                if store.loading {
                    store.recover(&notes);
                }
                let reconciled = notes
                    .into_iter()
                    .map(|note| {
                        if store.running.contains(&note.id) {
                            return store
                                .notes
                                .iter()
                                .find(|item| item.id == note.id)
                                .cloned()
                                .unwrap_or(note);
                        }
                        let latest = match store.base.get(&note.id) {
                            Some(current) if current.revision > note.revision => current.clone(),
                            _ => note,
                        };
                        store.base.insert(latest.id.clone(), latest.clone());
                        match store.pending.get(&latest.id) {
                            Some(patch) => patch.apply(&latest),
                            None => latest,
                        }
                    })
                    .collect();
                store.notes = reconciled;
                store.loading = false;
                store.error = None;
                store.changed(cx);
                let owed: Vec<String> = store
                    .pending
                    .keys()
                    .filter(|id| !store.failed.contains(*id))
                    .cloned()
                    .collect();
                for id in owed {
                    store.flush(&id, cx);
                }
            });
        })
        .detach();
    }

    /// `create`: the new note first in the list.
    pub fn create(
        &mut self,
        input: CreateProjectNote,
        cx: &mut Context<Self>,
    ) -> Task<Result<ProjectNote, String>> {
        let path = self.db_path.clone();
        cx.spawn(async move |this, cx| {
            let note = cx
                .background_executor()
                .spawn(async move { open(&path)?.create_project_note(input) })
                .await?;
            this.update(cx, |store, cx| {
                store.membership += 1;
                store.base.insert(note.id.clone(), note.clone());
                store.notes.insert(0, note.clone());
                store.changed(cx);
            })
            .map_err(|error| error.to_string())?;
            Ok(note)
        })
    }

    /// `edit`: on screen now, saved after the typing pause.
    pub fn edit(&mut self, id: &str, patch: NotePatch, cx: &mut Context<Self>) {
        if self.deleting.contains(id) {
            return;
        }
        let Some(note) = self.notes.iter_mut().find(|note| note.id == id) else {
            return;
        };
        *note = patch.apply(note);
        let merged = self.pending.remove(id).unwrap_or_default().merged(&patch);
        self.pending.insert(id.to_string(), merged);
        self.saving.insert(id.to_string());
        self.changed(cx);
        self.backup();
        let key = id.to_string();
        let timer = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SAVE_DELAY).await;
            let _ = this.update(cx, |store, cx| store.flush(&key, cx));
        });
        self.timers.insert(id.to_string(), timer);
    }

    /// Saved now, not after the pause (a completion, a task link).
    pub fn edit_now(&mut self, id: &str, patch: NotePatch, cx: &mut Context<Self>) {
        self.edit(id, patch, cx);
        self.flush(id, cx);
    }

    /// `flush`: one write at a time; what came meanwhile follows it.
    pub fn flush(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.deleting.contains(id) {
            return;
        }
        self.timers.remove(id);
        // The running write flushes again when it ends.
        if self.running.contains(id) {
            return;
        }
        let (Some(patch), Some(base)) = (self.pending.get(id).cloned(), self.base.get(id).cloned())
        else {
            return;
        };
        self.pending.remove(id);
        self.saving.insert(id.to_string());
        self.running.insert(id.to_string());
        self.changed(cx);
        let next = patch.apply(&base);
        let input = UpdateProjectNote {
            id: next.id.clone(),
            revision: next.revision,
            title: next.title.clone(),
            content: next.content.clone(),
            status: next.status.clone(),
            color: next.color.clone(),
            pinned: next.pinned,
            implementation_task_id: next.implementation_task_id.clone(),
        };
        let path = self.db_path.clone();
        let key = id.to_string();
        cx.spawn(async move |this, cx| {
            let saved = cx
                .background_executor()
                .spawn(async move { open(&path)?.update_project_note(input) })
                .await;
            let _ = this.update(cx, |store, cx| {
                match saved {
                    Ok(saved) => {
                        store.base.insert(key.clone(), saved.clone());
                        let shown = match store.pending.get(&key) {
                            Some(newer) => newer.apply(&saved),
                            None => saved,
                        };
                        if let Some(note) = store.notes.iter_mut().find(|note| note.id == key) {
                            *note = shown;
                        }
                        store.failed.remove(&key);
                    }
                    Err(_) => {
                        let newer = store.pending.remove(&key).unwrap_or_default();
                        store.pending.insert(key.clone(), patch.merged(&newer));
                        store.failed.insert(key.clone());
                    }
                }
                store.running.remove(&key);
                store.saving.remove(&key);
                store.backup();
                store.changed(cx);
                if store.pending.contains_key(&key) && !store.failed.contains(&key) {
                    store.flush(&key, cx);
                }
            });
        })
        .detach();
    }

    /// `retry`: the note read again, then saved on top of it.
    pub fn retry(&mut self, id: &str, cx: &mut Context<Self>) {
        let path = self.db_path.clone();
        let key = id.to_string();
        cx.spawn(async move |this, cx| {
            let read = cx
                .background_executor()
                .spawn(async move { open(&path)?.list_project_notes() })
                .await;
            let _ = this.update(cx, |store, cx| {
                if let Some(fresh) = read
                    .ok()
                    .and_then(|notes| notes.into_iter().find(|note| note.id == key))
                {
                    store.base.insert(key.clone(), fresh);
                    store.failed.remove(&key);
                    store.flush(&key, cx);
                }
            });
        })
        .detach();
    }

    /// `remove`: gone for good; their tasks stay.
    pub fn remove(&mut self, ids: Vec<String>, cx: &mut Context<Self>) -> Task<Result<(), String>> {
        for id in &ids {
            self.deleting.insert(id.clone());
            self.timers.remove(id);
        }
        let path = self.db_path.clone();
        cx.spawn(async move |this, cx| {
            let gone = {
                let ids = ids.clone();
                cx.background_executor()
                    .spawn(async move { open(&path)?.delete_project_notes(&ids) })
                    .await
            };
            this.update(cx, |store, cx| {
                for id in &ids {
                    store.deleting.remove(id);
                }
                if gone.is_ok() {
                    store.membership += 1;
                    for id in &ids {
                        store.pending.remove(id);
                        store.base.remove(id);
                        store.failed.remove(id);
                        store.saving.remove(id);
                    }
                    store.notes.retain(|note| !ids.contains(&note.id));
                    store.changed(cx);
                }
                store.backup();
            })
            .map_err(|error| error.to_string())?;
            gone
        })
    }
}

/// A store over a fresh database, for tests.
#[cfg(test)]
pub fn test_store(dir: &Path, cx: &mut gpui::App) -> gpui::Entity<NotesStore> {
    use gpui::AppContext;
    let db = dir.join("database.sqlite");
    drop(SqliteSessionRepo::open(&db).unwrap());
    cx.new(|_| NotesStore::new(db, dir.join("notes-pending.json")))
}

#[cfg(test)]
#[path = "notes_tests.rs"]
mod tests;

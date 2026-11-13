//! The merge conflict resolver (`WorkspaceMergeConflictResolver`), over the
//! conversation like the desktop's surface: the merge's files with their
//! progress, the open file's blocks one at a time (each side and "Aceitar
//! atual / base / ambos"), the whole file from one side or deleted, the
//! editable result (or compared with each side), and "Salvar e marcar
//! resolvido". With no merge going it offers to start one; with every file
//! resolved, the project's checks and "Concluir merge e enviar". Git runs on
//! the desktop.

mod agent;
mod agent_view;
mod blocks_view;
mod completion_view;
mod confirm;
mod files_view;
mod flows;
mod request;
mod view;
mod workspace_view;

#[cfg(test)]
mod agent_tests;
#[cfg(test)]
mod tests;

pub use request::{AgentPick, MergeResolverRequest};

use std::cell::Cell;
use std::rc::Rc;

use dcc_core::domain::indentation::DEFAULT_INDENTATION;
use dcc_core::domain::merge_conflict::{
    ConflictHunk, WorkspaceGitConflictEntry, WorkspaceGitConflictStateOutput,
    WorkspaceGitValidationConfigOutput, WorkspaceGitValidationReport, conflict_initial_result,
    parse_conflict_hunks, short_ref,
};
use gpui::{AppContext, Context, Entity, EventEmitter, Subscription, Task};

use crate::data::desktop_api::DesktopApi;
use crate::i18n::{Args, Translate};
use crate::screens::diff_view::DiffView;
use crate::screens::file_view::{FileView, FileViewEvent};
use crate::theme::ActiveTheme;

/// `MERGE_DIFF_SIDE_BY_SIDE_MIN_WIDTH`
const SIDE_BY_SIDE_MIN_WIDTH: f32 = 1_000.0;

/// A session's events, by id.
pub(crate) type EventsReader = std::sync::Arc<
    dyn Fn(&str) -> Vec<dcc_core::domain::session::SessionEventRecord> + Send + Sync,
>;

/// The desktop's database, read directly.
fn database_events() -> EventsReader {
    let path = crate::data::loader::database_path();
    std::sync::Arc::new(move |session| {
        crate::data::conversation::session_events(&path, session).unwrap_or_default()
    })
}

#[derive(Clone, Debug, PartialEq)]
pub enum MergeResolverEvent {
    /// The resolver closed (`onClose`).
    Closed,
    /// Git moved: the code column reads its status again (`onStateChanged`).
    StateChanged,
    /// "Ver sessão": the agent's conversation on screen (`onOpenAgentSession`).
    OpenSession(String),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Load<T> {
    Pending,
    Ready(T),
    Failed(String),
}

/// `ResultView`
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ResultView {
    Edit,
    CompareCurrent,
    CompareIncoming,
}

/// What is running (`busy`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Busy {
    Start,
    Save,
    Current,
    Incoming,
    Delete,
    Abort,
    Complete,
    /// "Resolver arquivo com agente"
    Agent,
}

/// A question before going on (the desktop's `window.confirm`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Pending {
    /// Another file over an unsaved edit.
    DiscardForFile(String),
    DiscardToClose,
    RemainingMarkers,
    EmptyResult,
    DeleteResult,
    Abort,
    ValidationCommands,
    /// "Ver sessão" over an unsaved result.
    OpenAgentSession(String),
}

/// The open file: its entry, the result as edited, and how it is shown.
pub(crate) struct Open {
    pub entry: WorkspaceGitConflictEntry,
    /// `initialResultRef`: the result's text as git had it.
    pub initial_result: Option<String>,
    /// `initialBufferRef`: what editing started from.
    pub initial_buffer: String,
    pub buffer: String,
    pub editor: Entity<FileView>,
    pub view: ResultView,
    pub compare: Option<Entity<DiffView>>,
    pub active_hunk: usize,
    _edits: Subscription,
}

impl Open {
    pub fn hunks(&self) -> Vec<ConflictHunk> {
        parse_conflict_hunks(&self.buffer)
    }

    /// The block on screen, kept inside the ones left.
    pub fn hunk(&self) -> Option<ConflictHunk> {
        let hunks = self.hunks();
        let at = self.active_hunk.min(hunks.len().saturating_sub(1));
        hunks.into_iter().nth(at)
    }

    pub fn dirty(&self) -> bool {
        self.buffer != self.initial_buffer
    }
}

pub struct MergeResolver {
    pub(crate) request: MergeResolverRequest,
    pub(crate) endpoint: Option<DesktopApi>,
    pub(crate) state: Load<WorkspaceGitConflictStateOutput>,
    pub(crate) validation: Option<Load<WorkspaceGitValidationConfigOutput>>,
    /// `totalConflictsRef`: the most files the merge had.
    pub(crate) total: usize,
    pub(crate) open: Option<Open>,
    pub(crate) busy: Option<Busy>,
    pub(crate) report: Option<WorkspaceGitValidationReport>,
    pub(crate) pending: Option<Pending>,
    /// The request asked for the base to be brought in at once: done at
    /// the first read that finds no merge here, and only then.
    pub(crate) start_pending: bool,
    /// The result area's width last frame (side by side from 1000px).
    pub(crate) width: Rc<Cell<f32>>,
    /// The agent's run on the open file, and the answer it left there.
    pub(crate) agent: Option<agent::AgentRun>,
    pub(crate) suggestion: Option<agent::AppliedSuggestion>,
    /// How the agent's session events are read (the database; tests their own).
    pub(crate) read_events: EventsReader,
    pub(crate) _agent: Option<Task<()>>,
    pub(crate) _read: Option<Task<()>>,
    pub(crate) _action: Option<Task<()>>,
}

impl EventEmitter<MergeResolverEvent> for MergeResolver {}

impl MergeResolver {
    pub fn new(request: MergeResolverRequest, cx: &mut Context<Self>) -> Self {
        let mut resolver = Self {
            start_pending: request.start,
            request,
            endpoint: None,
            state: Load::Pending,
            validation: None,
            total: 0,
            open: None,
            busy: None,
            report: None,
            pending: None,
            width: Rc::new(Cell::new(0.0)),
            agent: None,
            suggestion: None,
            read_events: database_events(),
            _agent: None,
            _read: None,
            _action: None,
        };
        resolver.read(cx);
        resolver
    }

    /// Points the resolver at another desktop (tests).
    pub fn with_endpoint(mut self, endpoint: DesktopApi, cx: &mut Context<Self>) -> Self {
        self.endpoint = Some(endpoint);
        self.read(cx);
        self
    }

    pub(crate) fn api(&self) -> Result<DesktopApi, crate::data::desktop_api::ApiError> {
        self.endpoint
            .clone()
            .or_else(DesktopApi::load)
            .ok_or(crate::data::desktop_api::ApiError::DesktopClosed)
    }

    pub(crate) fn conflicts(&self) -> &[WorkspaceGitConflictEntry] {
        match &self.state {
            Load::Ready(state) => &state.conflicts,
            _ => &[],
        }
    }

    pub(crate) fn resolved(&self) -> (usize, usize) {
        let total = self.total.max(self.conflicts().len());
        (total.saturating_sub(self.conflicts().len()), total)
    }

    /// `currentLabel` / `incomingLabel`, with their refs.
    pub(crate) fn labels(&self, cx: &gpui::App) -> ((String, String), (String, String)) {
        let state = match &self.state {
            Load::Ready(state) => Some(state),
            _ => None,
        };
        let current_ref = short_ref(
            state.and_then(|state| state.current_branch.as_deref()),
            &cx.t("mergeConflict.currentBranchFallback"),
        );
        let workspace = self
            .request
            .workspace_label
            .clone()
            .filter(|label| !label.trim().is_empty())
            .unwrap_or_else(|| current_ref.clone());
        let current = cx
            .t_with(
                "mergeConflict.currentWorkspaceLabel",
                &Args::default().with("workspace", workspace),
            )
            .to_string();
        let incoming_ref = short_ref(
            state
                .and_then(|state| state.incoming_ref.as_deref())
                .or(self.request.base_branch.as_deref()),
            &cx.t("mergeConflict.incomingBranchFallback"),
        );
        let incoming = cx
            .t_with(
                "mergeConflict.incomingBranchLabel",
                &Args::default().with("branch", incoming_ref.clone()),
            )
            .to_string();
        ((current, current_ref), (incoming, incoming_ref))
    }

    /// `openEntry`: the file's result (else a side) in the editor.
    pub(crate) fn open_entry(&mut self, entry: WorkspaceGitConflictEntry, cx: &mut Context<Self>) {
        let result = conflict_initial_result(&entry);
        let dark = cx.theme().appearance == crate::theme::Appearance::Dark;
        let path = entry.path.clone();
        let text = result.clone();
        let editor = cx.new(|cx| FileView::new(path, text, DEFAULT_INDENTATION, dark, cx));
        let edits = cx.subscribe(&editor, |resolver, editor, event: &FileViewEvent, cx| {
            if *event == FileViewEvent::Edited
                && let Some(open) = resolver.open.as_mut()
            {
                open.buffer = editor.read(cx).text().to_string();
                cx.notify();
            }
        });
        self.agent = None;
        self.suggestion = None;
        self.open = Some(Open {
            initial_result: entry.result.text.clone(),
            initial_buffer: result.clone(),
            buffer: result,
            entry,
            editor,
            view: ResultView::Edit,
            compare: None,
            active_hunk: 0,
            _edits: edits,
        });
        cx.notify();
    }

    /// `requestOpenEntry`: an unsaved edit asks first.
    pub(crate) fn request_open(&mut self, path: String, cx: &mut Context<Self>) {
        let Some(open) = self.open.as_ref() else {
            return self.open_path(&path, cx);
        };
        if open.entry.path == path || self.busy.is_some() {
            return;
        }
        if open.dirty() {
            self.pending = Some(Pending::DiscardForFile(path));
            cx.notify();
            return;
        }
        self.open_path(&path, cx);
    }

    pub(crate) fn open_path(&mut self, path: &str, cx: &mut Context<Self>) {
        if let Some(entry) = self
            .conflicts()
            .iter()
            .find(|entry| entry.path == path)
            .cloned()
        {
            self.open_entry(entry, cx);
        }
    }

    /// The file stays open while it is a conflict; else the first one opens.
    pub(crate) fn follow_conflicts(&mut self, cx: &mut Context<Self>) {
        let conflicts = self.conflicts().to_vec();
        self.total = self.total.max(conflicts.len());
        let Some(first) = conflicts.first().cloned() else {
            self.open = None;
            return;
        };
        let still = self
            .open
            .as_ref()
            .and_then(|open| conflicts.iter().find(|entry| entry.path == open.entry.path))
            .cloned();
        match still {
            // A fresh read of the open file keeps the edit going.
            Some(entry) => {
                if let Some(open) = self.open.as_mut() {
                    open.entry = entry;
                }
            }
            None => self.open_entry(first, cx),
        }
    }

    /// The result replaced as one edit (an accepted block or side).
    pub(crate) fn set_result(&mut self, text: String, cx: &mut Context<Self>) {
        let Some(open) = self.open.as_mut() else {
            return;
        };
        open.buffer = text.clone();
        open.view = ResultView::Edit;
        open.compare = None;
        let editor = open.editor.clone();
        self.suggestion = None;
        editor.update(cx, |editor, cx| editor.replace_text(&text, cx));
        cx.notify();
    }

    pub(crate) fn step_hunk(&mut self, forward: bool, cx: &mut Context<Self>) {
        let Some(open) = self.open.as_mut() else {
            return;
        };
        let count = open.hunks().len();
        open.active_hunk = if forward {
            (open.active_hunk + 1).min(count.saturating_sub(1))
        } else {
            open.active_hunk.saturating_sub(1)
        };
        cx.notify();
    }

    /// `restoreOriginalResult`
    pub(crate) fn restore(&mut self, cx: &mut Context<Self>) {
        let Some(original) = self.open.as_ref().map(|open| open.initial_buffer.clone()) else {
            return;
        };
        self.set_result(original, cx);
        if let Some(open) = self.open.as_mut() {
            open.active_hunk = 0;
        }
    }

    /// `changeResultView`: the result against one side, or the editor.
    pub(crate) fn set_view(&mut self, view: ResultView, cx: &mut Context<Self>) {
        let dark = cx.theme().appearance == crate::theme::Appearance::Dark;
        let side_by_side = self.width.get() >= SIDE_BY_SIDE_MIN_WIDTH;
        let Some(open) = self.open.as_mut() else {
            return;
        };
        open.view = view;
        open.compare = match view {
            ResultView::Edit => None,
            ResultView::CompareCurrent | ResultView::CompareIncoming => {
                let side = if view == ResultView::CompareCurrent {
                    open.entry.current.text.clone()
                } else {
                    open.entry.incoming.text.clone()
                };
                let (path, original, result) = (
                    open.entry.path.clone(),
                    side.unwrap_or_default(),
                    open.buffer.clone(),
                );
                Some(cx.new(|cx| {
                    if side_by_side {
                        DiffView::side_by_side(path, original, result, dark, cx)
                    } else {
                        DiffView::new(path, original, result, dark, cx)
                    }
                }))
            }
        };
        cx.notify();
    }

    /// `requestClose`: not while something runs; an unsaved edit asks.
    pub(crate) fn request_close(&mut self, cx: &mut Context<Self>) {
        if let Some(busy) = self.busy {
            let key = if busy == Busy::Agent {
                "mergeConflict.busy.agent"
            } else {
                "mergeConflict.busy.operation"
            };
            crate::ui::Toaster::info(cx.t(key), cx);
            return;
        }
        if self.open.as_ref().is_some_and(Open::dirty) {
            self.pending = Some(Pending::DiscardToClose);
            cx.notify();
            return;
        }
        cx.emit(MergeResolverEvent::Closed);
    }
}

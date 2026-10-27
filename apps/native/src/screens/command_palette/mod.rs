//! The command palette (⌘K, `WorkspaceCommandPalette`): actions, tasks,
//! the open task's recent conversations (or an `@` search over every
//! conversation's text) and the checkout's files, filtered as you type;
//! ↑/↓, Enter or a click runs one.

mod items;
#[cfg(test)]
mod tests;
mod view;

use std::path::PathBuf;
use std::time::Duration;

use gpui::{AppContext, Context, Entity, EventEmitter, ScrollHandle, Subscription, Task, Window};

use items::{Item, Row};
pub use items::{PaletteCommand, WorkbenchCommand};

use crate::data::command_palette::{PaletteSession, PaletteTask, session_search};
use crate::ui::text_input::{TextInput, TextInputEvent};

/// `SESSION_SEARCH_DEBOUNCE_MS`
const SESSION_SEARCH_DEBOUNCE: Duration = Duration::from_millis(180);

pub enum PaletteEvent {
    Run(PaletteCommand),
    Closed,
}

/// What the palette works over, read when it opens.
pub struct PaletteContext {
    /// The open task (`selectedWorkspaceId`).
    pub selected_task: Option<String>,
    /// Tasks that can be opened (not archived or completed).
    pub tasks: Vec<PaletteTask>,
    /// The open task's conversations.
    pub recent: Vec<PaletteSession>,
    /// The open task's checkout (`selectedWorkspacePath`).
    pub root: Option<String>,
    pub db_path: PathBuf,
}

/// An `@` search's answer, for the query it ran.
struct SessionHits {
    query: String,
    sessions: Vec<PaletteSession>,
}

pub struct CommandPalette {
    input: Entity<TextInput>,
    context: PaletteContext,
    files: Vec<String>,
    /// The `@` query whose 180ms have passed, and its answer once read.
    settled: Option<String>,
    hits: Option<SessionHits>,
    rows: Vec<Row>,
    items: Vec<Item>,
    highlighted: usize,
    scroll: ScrollHandle,
    _input_events: Subscription,
    _files: Option<Task<()>>,
    _sessions: Option<Task<()>>,
}

impl EventEmitter<PaletteEvent> for CommandPalette {}

impl CommandPalette {
    pub fn new(context: PaletteContext, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let placeholder = crate::i18n::Translate::t(&**cx, "commandPalette.searchPlaceholder");
        let input = cx.new(|cx| TextInput::new(window, cx).with_placeholder(placeholder));
        let events = cx.subscribe(&input, |palette, _, event, cx| match event {
            TextInputEvent::Changed => palette.query_changed(cx),
            TextInputEvent::Submitted => palette.run(palette.highlighted, cx),
            TextInputEvent::Cancelled => cx.emit(PaletteEvent::Closed),
            TextInputEvent::Blurred => {}
        });
        let files = context.root.clone().map(|root| {
            cx.spawn(async move |this, cx| {
                let listed = cx
                    .background_executor()
                    .spawn(async move { dcc_infra::workspace_git::git_tracked_files(&root) })
                    .await
                    .unwrap_or_default();
                let _ = this.update(cx, |palette, cx| {
                    palette.files = listed;
                    palette.rebuild(cx);
                });
            })
        });
        let mut palette = Self {
            input,
            context,
            files: Vec::new(),
            settled: None,
            hits: None,
            rows: Vec::new(),
            items: Vec::new(),
            highlighted: 0,
            scroll: ScrollHandle::new(),
            _input_events: events,
            _files: files,
            _sessions: None,
        };
        palette.rebuild(cx);
        palette
    }

    pub fn input(&self) -> Entity<TextInput> {
        self.input.clone()
    }

    fn query(&self, cx: &gpui::App) -> String {
        self.input.read(cx).text().to_string()
    }

    fn query_changed(&mut self, cx: &mut Context<Self>) {
        let search = session_search(&self.query(cx));
        if !search.enabled {
            self.settled = None;
            self._sessions = None;
        } else if self.settled.as_deref() != Some(search.query.as_str()) {
            // `useDebouncedValue`: the text search waits for typing to stop.
            self.settled = None;
            let wanted = search.query.clone();
            let db_path = self.context.db_path.clone();
            self._sessions = Some(cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(SESSION_SEARCH_DEBOUNCE)
                    .await;
                let _ = this.update(cx, |palette, cx| {
                    palette.settled = Some(wanted.clone());
                    palette.rebuild(cx);
                });
                let query = wanted.clone();
                let found = cx
                    .background_executor()
                    .spawn(async move { crate::data::session_search::search(&db_path, &query) })
                    .await;
                let _ = this.update(cx, |palette, cx| {
                    let sessions = found
                        .unwrap_or_else(|error| {
                            eprintln!("[dcc-native] palette session search: {error:#}");
                            Vec::new()
                        })
                        .into_iter()
                        .map(items::from_search)
                        .collect();
                    palette.hits = Some(SessionHits {
                        query: wanted,
                        sessions,
                    });
                    palette.rebuild(cx);
                });
            }));
        }
        self.rebuild(cx);
    }

    /// The rows for the current query; the first item is selected again.
    fn rebuild(&mut self, cx: &mut Context<Self>) {
        let (rows, items) = items::build(self, cx);
        self.rows = rows;
        self.items = items;
        self.highlighted = 0;
        self.scroll.scroll_to_item(0);
        cx.notify();
    }

    fn move_highlight(&mut self, down: bool, cx: &mut Context<Self>) {
        let total = self.items.len();
        if total == 0 {
            return;
        }
        self.highlighted = if down {
            (self.highlighted + 1).min(total - 1)
        } else {
            self.highlighted.saturating_sub(1)
        };
        if let Some(row) = self
            .rows
            .iter()
            .position(|row| *row == Row::Item(self.highlighted))
        {
            self.scroll.scroll_to_item(row);
        }
        cx.notify();
    }

    fn hover(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.highlighted != index {
            self.highlighted = index;
            cx.notify();
        }
    }

    fn run(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(item) = self.items.get(index) {
            cx.emit(PaletteEvent::Run(item.command.clone()));
        }
    }

    /// The label tests look for.
    #[cfg(test)]
    fn labels(&self) -> Vec<String> {
        self.items
            .iter()
            .map(|item| item.label.to_string())
            .collect()
    }
}

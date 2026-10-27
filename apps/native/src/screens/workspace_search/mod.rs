//! "Buscar no workspace" (⌘⇧F, `WorkspaceSearch`): `git grep` over the
//! checkout's tracked files from two letters on, the lines grouped by
//! file; ↑/↓, Enter or a click opens the file at that line on the stage.

#[cfg(test)]
mod tests;
mod view;

use std::time::Duration;

use dcc_infra::workspace_search::{SearchMatch, SearchResults, search_workspace};
use gpui::{AppContext, Context, Entity, EventEmitter, ScrollHandle, Subscription, Task, Window};

use crate::ui::text_input::{TextInput, TextInputEvent};

/// `MIN_QUERY_LENGTH`
const MIN_QUERY: usize = 2;
/// Typing settles before `git grep` runs.
const SETTLE: Duration = Duration::from_millis(120);

pub enum WorkspaceSearchEvent {
    /// A path relative to the checkout and its 1-based line.
    Picked {
        path: String,
        line: u32,
    },
    Closed,
}

/// One file's lines, in `git grep`'s order.
pub(crate) struct FileMatches {
    pub path: String,
    pub matches: Vec<SearchMatch>,
}

pub struct WorkspaceSearch {
    input: Entity<TextInput>,
    root: String,
    /// The query the results answer (trimmed).
    answered: String,
    searching: bool,
    groups: Vec<FileMatches>,
    truncated: bool,
    /// Across every group, in order.
    highlighted: usize,
    scroll: ScrollHandle,
    _input_events: Subscription,
    _search: Option<Task<()>>,
}

impl EventEmitter<WorkspaceSearchEvent> for WorkspaceSearch {}

/// "file → lines", like an IDE's search.
fn group(results: SearchResults) -> Vec<FileMatches> {
    let mut groups: Vec<FileMatches> = Vec::new();
    for found in results.matches {
        match groups.iter_mut().find(|group| group.path == found.path) {
            Some(group) => group.matches.push(found),
            None => groups.push(FileMatches {
                path: found.path.clone(),
                matches: vec![found],
            }),
        }
    }
    groups
}

impl WorkspaceSearch {
    /// `root` is the open task's checkout.
    pub fn new(root: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let placeholder = crate::i18n::Translate::t(&**cx, "workspaceSearch.placeholder");
        let input = cx.new(|cx| TextInput::new(window, cx).with_placeholder(placeholder));
        let events = cx.subscribe(&input, |search, _, event, cx| match event {
            TextInputEvent::Changed => search.run(cx),
            TextInputEvent::Submitted => search.pick(search.highlighted, cx),
            TextInputEvent::Cancelled => cx.emit(WorkspaceSearchEvent::Closed),
            TextInputEvent::Blurred => {}
        });
        Self {
            input,
            root,
            answered: String::new(),
            searching: false,
            groups: Vec::new(),
            truncated: false,
            highlighted: 0,
            scroll: ScrollHandle::new(),
            _input_events: events,
            _search: None,
        }
    }

    pub fn input(&self) -> Entity<TextInput> {
        self.input.clone()
    }

    fn query(&self, cx: &gpui::App) -> String {
        self.input.read(cx).text().trim().to_string()
    }

    fn has_query(&self, cx: &gpui::App) -> bool {
        self.query(cx).chars().count() >= MIN_QUERY
    }

    fn run(&mut self, cx: &mut Context<Self>) {
        let query = self.query(cx);
        if query == self.answered && !self.groups.is_empty() {
            return;
        }
        // A new query has no lines yet: "Buscando…" until they come.
        self.groups.clear();
        self.truncated = false;
        self.highlighted = 0;
        self.answered = String::new();
        if query.chars().count() < MIN_QUERY {
            self.searching = false;
            self._search = None;
            cx.notify();
            return;
        }
        self.searching = true;
        cx.notify();
        let root = self.root.clone();
        self._search = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SETTLE).await;
            let wanted = query.clone();
            let found = cx
                .background_executor()
                .spawn(async move { search_workspace(&root, &wanted) })
                .await;
            let _ = this.update(cx, |search, cx| {
                search.searching = false;
                search.answered = query;
                let results = found.unwrap_or_else(|error| {
                    eprintln!("[dcc-native] workspace search: {error}");
                    SearchResults::default()
                });
                search.truncated = results.truncated;
                search.groups = group(results);
                search.highlighted = 0;
                search.scroll.scroll_to_item(0);
                cx.notify();
            });
        }));
    }

    fn total(&self) -> usize {
        self.groups.iter().map(|group| group.matches.len()).sum()
    }

    /// The match at `index` across the groups.
    fn at(&self, index: usize) -> Option<&SearchMatch> {
        self.groups
            .iter()
            .flat_map(|group| group.matches.iter())
            .nth(index)
    }

    /// The list's row for match `index`: each group adds its heading.
    fn row_of(&self, index: usize) -> usize {
        let mut seen = 0;
        for (group_index, group) in self.groups.iter().enumerate() {
            if index < seen + group.matches.len() {
                return seen + group_index + 1 + (index - seen);
            }
            seen += group.matches.len();
        }
        0
    }

    fn move_highlight(&mut self, down: bool, cx: &mut Context<Self>) {
        let total = self.total();
        if total == 0 {
            return;
        }
        self.highlighted = if down {
            (self.highlighted + 1).min(total - 1)
        } else {
            self.highlighted.saturating_sub(1)
        };
        self.scroll.scroll_to_item(self.row_of(self.highlighted));
        cx.notify();
    }

    fn hover(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.highlighted != index {
            self.highlighted = index;
            cx.notify();
        }
    }

    fn pick(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(found) = self.at(index) {
            cx.emit(WorkspaceSearchEvent::Picked {
                path: found.path.clone(),
                line: found.line,
            });
        }
    }
}

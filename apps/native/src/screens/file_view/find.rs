//! The editor's search panel (`SearchPanelWidget`): ⌘F to find, ⌘⌥F to
//! replace. It opens on the word at the caret, finds as the query is typed
//! (the first match from the caret becomes the selection), Enter and ⇧Enter
//! (or ⌘G and ⇧⌘G) step through the matches, and "Replace" / "Replace All"
//! edit the file. Esc closes it and gives the text back the focus.

use gpui::{App, AppContext, Context, Entity, KeyBinding, Subscription, Window, actions};

use super::FileView;
use super::search::{SearchParams, find_matches, replacement};
use crate::ui::{TextInput, TextInputEvent};

actions!(
    code_editor_search,
    [OpenFind, OpenReplace, FindNext, FindPrevious]
);

pub(crate) const PANEL_CONTEXT: &str = "SearchPanel";

pub(crate) fn bind_keys(cx: &mut App) {
    let editor = Some(super::keys::CONTEXT);
    let panel = Some(PANEL_CONTEXT);
    cx.bind_keys([
        KeyBinding::new("cmd-f", OpenFind, editor),
        KeyBinding::new("cmd-alt-f", OpenReplace, editor),
        KeyBinding::new("cmd-g", FindNext, editor),
        KeyBinding::new("cmd-shift-g", FindPrevious, editor),
        KeyBinding::new("cmd-f", OpenFind, panel),
        KeyBinding::new("cmd-alt-f", OpenReplace, panel),
        KeyBinding::new("cmd-g", FindNext, panel),
        KeyBinding::new("cmd-shift-g", FindPrevious, panel),
        KeyBinding::new("shift-enter", FindPrevious, panel),
    ]);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FindMode {
    Find,
    Replace,
}

pub(crate) struct Search {
    pub mode: FindMode,
    pub query: Entity<TextInput>,
    pub replace: Entity<TextInput>,
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub regex: bool,
    pub matches: Vec<std::ops::Range<usize>>,
    /// The match the panel stands on.
    pub current: Option<usize>,
    _subscriptions: Vec<Subscription>,
}

impl Search {
    pub fn params(&self, cx: &App) -> SearchParams {
        SearchParams {
            text: self.query.read(cx).text().to_string(),
            replace: self.replace.read(cx).text().to_string(),
            case_sensitive: self.case_sensitive,
            whole_word: self.whole_word,
            regex: self.regex,
        }
    }

    /// "No results", "N results" or "i of N".
    pub fn summary(&self) -> String {
        match (self.matches.len(), self.current) {
            (0, _) => "No results".into(),
            (count, Some(at)) => format!("{} of {count}", at + 1),
            (count, None) => format!("{count} results"),
        }
    }
}

impl FileView {
    /// ⌘F / ⌘⌥F: the panel opens (or changes mode) with the word at the caret.
    pub(crate) fn open_search(
        &mut self,
        mode: FindMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(search) = self.search.as_mut() {
            search.mode = mode;
            let query = search.query.clone();
            query.update(cx, |input, cx| input.focus_and_select_all(window, cx));
            cx.notify();
            return;
        }
        let mut default_query = String::new();
        let mut initial = None;
        if self.buffer.selection.is_empty() {
            let word = self.buffer.word_at(self.buffer.cursor());
            self.buffer.select(word.clone());
            let text = self.buffer.selected_text();
            if !text.is_empty() && !text.contains('\n') && !text.trim().is_empty() {
                default_query = text.to_string();
                initial = Some(word);
            }
        }
        let query = cx.new(|cx| TextInput::new(window, cx).with_placeholder("Search"));
        let replace = cx.new(|cx| TextInput::new(window, cx).with_placeholder("Replace"));
        query.update(cx, |input, cx| input.set_text(default_query, cx));
        let subscriptions = vec![
            cx.subscribe_in(
                &query,
                window,
                |view, _, event: &TextInputEvent, window, cx| match event {
                    TextInputEvent::Changed => view.refresh_matches(true, cx),
                    TextInputEvent::Submitted => view.step_match(false, cx),
                    TextInputEvent::Cancelled => view.close_search(window, cx),
                    TextInputEvent::Blurred => {}
                },
            ),
            cx.subscribe_in(
                &replace,
                window,
                |view, _, event: &TextInputEvent, window, cx| match event {
                    TextInputEvent::Submitted => view.replace_current(cx),
                    TextInputEvent::Cancelled => view.close_search(window, cx),
                    _ => {}
                },
            ),
        ];
        self.search = Some(Search {
            mode,
            query: query.clone(),
            replace,
            case_sensitive: false,
            whole_word: false,
            regex: false,
            matches: Vec::new(),
            current: None,
            _subscriptions: subscriptions,
        });
        query.update(cx, |input, cx| input.focus_and_select_all(window, cx));
        if let Some(initial) = initial {
            self.refresh_matches(false, cx);
            if let Some(search) = self.search.as_mut() {
                search.current = search.matches.iter().position(|range| *range == initial);
            }
        }
        cx.notify();
    }

    /// The matches again; with `sync`, the first from the caret is selected.
    pub(crate) fn refresh_matches(&mut self, sync: bool, cx: &mut Context<Self>) {
        let Some(search) = self.search.as_ref() else {
            return;
        };
        let params = search.params(cx);
        let matches = find_matches(self.buffer.text(), &params);
        let selection = self.buffer.selection.clone();
        let search = self.search.as_mut().expect("search");
        search.matches = matches;
        search.current = if sync {
            let from = selection.start;
            search.matches.iter().position(|range| range.start >= from)
        } else {
            search.matches.iter().position(|range| *range == selection)
        };
        if sync && let Some(at) = search.current {
            let range = search.matches[at].clone();
            self.buffer.select(range);
            self.moved(cx);
        }
        cx.notify();
    }

    /// Enter / ⇧Enter: the next (or previous) match, around the file.
    pub(crate) fn step_match(&mut self, previous: bool, cx: &mut Context<Self>) {
        let Some(search) = self.search.as_mut() else {
            return;
        };
        if search.matches.is_empty() {
            return;
        }
        let current = search.current.map(|at| search.matches[at].clone());
        let next = if previous {
            let from = current.as_ref().map_or(0, |range| range.start);
            search
                .matches
                .iter()
                .rposition(|range| range.end <= from)
                .unwrap_or(search.matches.len() - 1)
        } else {
            let from = current.as_ref().map_or(0, |range| range.end);
            search
                .matches
                .iter()
                .position(|range| range.start >= from)
                .unwrap_or(0)
        };
        search.current = Some(next);
        let range = search.matches[next].clone();
        self.buffer.select(range);
        self.moved(cx);
    }

    /// "Replace": the current match (the next one first if none), as one edit.
    pub(crate) fn replace_current(&mut self, cx: &mut Context<Self>) {
        if self.read_only {
            return;
        }
        if self
            .search
            .as_ref()
            .is_none_or(|search| search.matches.is_empty())
        {
            return;
        }
        if self
            .search
            .as_ref()
            .is_some_and(|search| search.current.is_none())
        {
            self.step_match(false, cx);
        }
        let Some(search) = self.search.as_ref() else {
            return;
        };
        let Some(range) = search.current.map(|at| search.matches[at].clone()) else {
            return;
        };
        let params = search.params(cx);
        let text = replacement(self.buffer.text(), &params, &range);
        let lines = self.buffer.line_count();
        let line = self.buffer.line_of(range.start);
        self.buffer.replace(Some(range.clone()), &text);
        let after = range.start + text.len();
        self.buffer.go(after, false);
        self.edited(lines, line, cx);
        if let Some(search) = self.search.as_mut() {
            search.current = None;
        }
        self.refresh_matches(false, cx);
    }

    /// "Replace All": every match, as one step.
    pub(crate) fn replace_all(&mut self, cx: &mut Context<Self>) {
        if self.read_only {
            return;
        }
        let Some(search) = self.search.as_ref() else {
            return;
        };
        if search.matches.is_empty() {
            return;
        }
        let params = search.params(cx);
        let text = self.buffer.text().to_string();
        let mut out = String::with_capacity(text.len());
        let mut last = 0;
        for range in &search.matches {
            out.push_str(&text[last..range.start]);
            out.push_str(&replacement(&text, &params, range));
            last = range.end;
        }
        out.push_str(&text[last..]);
        let lines = self.buffer.line_count();
        let cursor = self.buffer.cursor().min(out.len());
        self.buffer.replace(Some(0..text.len()), &out);
        self.buffer.go(cursor, false);
        self.edited(lines, 0, cx);
        if let Some(search) = self.search.as_mut() {
            search.current = None;
        }
        self.refresh_matches(false, cx);
    }

    pub(crate) fn toggle_search_option(
        &mut self,
        option: fn(&mut Search) -> &mut bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(search) = self.search.as_mut() {
            let flag = option(search);
            *flag = !*flag;
        }
        self.refresh_matches(true, cx);
    }

    /// Esc / ×: the panel goes; the text has the focus again.
    pub(crate) fn close_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search.take().is_some() {
            window.focus(&self.focus, cx);
            cx.notify();
        }
    }
}

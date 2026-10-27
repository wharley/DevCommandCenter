//! "Abrir arquivo" (⌘P, `FileQuickOpen`): the checkout's tracked files
//! ranked by what is typed (`rankQuickOpenFiles`, 60 at most); ↑/↓, Enter
//! or a click opens one on the stage.

#[cfg(test)]
mod tests;
mod view;

use gpui::{AppContext, Context, Entity, EventEmitter, ScrollHandle, Subscription, Task, Window};

use crate::data::quick_open::rank;
use crate::ui::text_input::{TextInput, TextInputEvent};

pub enum QuickOpenEvent {
    /// A path relative to the checkout.
    Picked(String),
    Closed,
}

pub struct QuickOpen {
    input: Entity<TextInput>,
    paths: Vec<String>,
    loading: bool,
    results: Vec<String>,
    highlighted: usize,
    scroll: ScrollHandle,
    _input_events: Subscription,
    _load: Task<()>,
}

impl EventEmitter<QuickOpenEvent> for QuickOpen {}

impl QuickOpen {
    /// `root` is the open task's checkout.
    pub fn new(root: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let placeholder = crate::i18n::Translate::t(&**cx, "quickOpen.placeholder");
        let input = cx.new(|cx| TextInput::new(window, cx).with_placeholder(placeholder));
        let events = cx.subscribe(&input, |open, _, event, cx| match event {
            TextInputEvent::Changed => open.rerank(cx),
            TextInputEvent::Submitted => open.pick(open.highlighted, cx),
            TextInputEvent::Cancelled => cx.emit(QuickOpenEvent::Closed),
            TextInputEvent::Blurred => {}
        });
        let load = cx.spawn(async move |this, cx| {
            let listed = cx
                .background_executor()
                .spawn(async move { dcc_infra::workspace_git::git_tracked_files(&root) })
                .await;
            let _ = this.update(cx, |open, cx| {
                open.loading = false;
                open.paths = listed.unwrap_or_else(|error| {
                    eprintln!("[dcc-native] quick open: {error}");
                    Vec::new()
                });
                open.rerank(cx);
            });
        });
        Self {
            input,
            paths: Vec::new(),
            loading: true,
            results: Vec::new(),
            highlighted: 0,
            scroll: ScrollHandle::new(),
            _input_events: events,
            _load: load,
        }
    }

    pub fn input(&self) -> Entity<TextInput> {
        self.input.clone()
    }

    fn rerank(&mut self, cx: &mut Context<Self>) {
        let query = self.input.read(cx).text().trim().to_string();
        self.results = rank(&self.paths, &query);
        // cmdk selects the first item again when the list changes.
        self.highlighted = 0;
        self.scroll.scroll_to_item(0);
        cx.notify();
    }

    fn move_highlight(&mut self, down: bool, cx: &mut Context<Self>) {
        let total = self.results.len();
        if total == 0 {
            return;
        }
        self.highlighted = if down {
            (self.highlighted + 1).min(total - 1)
        } else {
            self.highlighted.saturating_sub(1)
        };
        self.scroll.scroll_to_item(self.highlighted);
        cx.notify();
    }

    fn hover(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.highlighted != index {
            self.highlighted = index;
            cx.notify();
        }
    }

    fn pick(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(path) = self.results.get(index).cloned() {
            cx.emit(QuickOpenEvent::Picked(path));
        }
    }
}

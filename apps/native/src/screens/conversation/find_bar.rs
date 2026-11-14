//! The find bar under the header (`ThreadFindBar`): the field (Enter for
//! the next hit, Shift+Enter the one before, Esc to close), how many there
//! are with the excerpt of the current one, ↑/↓ and ×. The current hit's
//! row scrolls into view with the ring around it. It closes with another
//! conversation.

use gpui::{AnyElement, AppContext, Context, Subscription, Window};

use super::ConversationView;
use super::find::{FindMatch, find_in_thread, step};
use crate::i18n::Translate;
use crate::ui::find_strip::{FindIds, FindStrip, find_strip};
use crate::ui::text_input::{TextInput, TextInputEvent};

gpui::actions!(
    conversation,
    [
        /// ⌘F outside a field: find in the conversation.
        OpenThreadFind
    ]
);

/// ⌘F anywhere but a field, the editor or the terminal: those keep the key
/// (the editor binds its own find; `NoAction` silences it in the others).
pub(crate) fn bind_keys(cx: &mut gpui::App) {
    cx.bind_keys([
        gpui::KeyBinding::new("cmd-f", OpenThreadFind, None),
        gpui::KeyBinding::new("cmd-f", gpui::NoAction, Some("TextArea")),
        gpui::KeyBinding::new("cmd-f", gpui::NoAction, Some("TextInput")),
        gpui::KeyBinding::new("cmd-f", gpui::NoAction, Some("Terminal")),
    ]);
}

pub(crate) struct FindBar {
    input: gpui::Entity<TextInput>,
    active: usize,
    matches: Vec<FindMatch>,
    _events: Subscription,
}

impl ConversationView {
    pub fn open_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.task_id.is_none() {
            return;
        }
        if self.find.is_none() {
            let placeholder = cx.t("conversation.find.placeholder");
            let input = cx.new(|cx| TextInput::new(window, cx).with_placeholder(placeholder));
            let events = cx.subscribe(&input, |view, _, event, cx| match event {
                TextInputEvent::Changed => view.search(true, cx),
                TextInputEvent::Submitted => view.step_find(true, cx),
                TextInputEvent::Cancelled => view.close_find(cx),
                TextInputEvent::Blurred => {}
            });
            self.find = Some(FindBar {
                input,
                active: 0,
                matches: Vec::new(),
                _events: events,
            });
        }
        if let Some(find) = &self.find {
            let input = find.input.clone();
            input.update(cx, |input, cx| input.focus_and_select_all(window, cx));
        }
        cx.notify();
    }

    pub fn find_open(&self) -> bool {
        self.find.is_some()
    }

    pub fn close_find(&mut self, cx: &mut Context<Self>) {
        if self.find.take().is_some() {
            cx.notify();
        }
    }

    /// The hits again (the query or the rows changed); a new query starts
    /// from the first.
    pub(super) fn search(&mut self, restart: bool, cx: &mut Context<Self>) {
        let before = self.find_focus().map(str::to_string);
        let Some(find) = self.find.as_mut() else {
            return;
        };
        let query = find.input.read(cx).text().to_string();
        let rows = self.data.as_ref().map(|data| data.rows.clone());
        find.matches = rows
            .map(|rows| find_in_thread(&rows, &query))
            .unwrap_or_default();
        find.active = if restart || find.matches.is_empty() {
            0
        } else {
            find.active.min(find.matches.len() - 1)
        };
        // Only a new current hit moves the list (new rows don't).
        if self.find_focus() != before.as_deref() {
            self.reveal_find(cx);
        }
        cx.notify();
    }

    fn step_find(&mut self, forward: bool, cx: &mut Context<Self>) {
        let Some(find) = self.find.as_mut() else {
            return;
        };
        find.active = step(find.active, find.matches.len(), forward);
        self.reveal_find(cx);
        cx.notify();
    }

    /// The row of the current hit, if the bar has one.
    pub(super) fn find_focus(&self) -> Option<&str> {
        let find = self.find.as_ref()?;
        find.matches
            .get(find.active)
            .map(|found| found.message_id.as_str())
    }

    fn reveal_find(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.find_focus().map(str::to_string) else {
            return;
        };
        let index = self
            .data
            .as_ref()
            .and_then(|data| data.rows.iter().position(|row| row.id == id));
        if let Some(index) = index {
            // The list starts with the top room. Following the end would
            // pull the view back from the hit.
            self.list.pause_following_tail();
            self.list.scroll_to_reveal_item(index + 1);
            cx.notify();
        }
    }

    pub(super) fn find_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let find = self.find.as_ref()?;
        Some(find_strip(
            FindStrip {
                ids: &IDS,
                input: &find.input,
                active: find.active,
                total: find.matches.len(),
                snippet: find
                    .matches
                    .get(find.active)
                    .map(|found| found.snippet.as_str()),
            },
            Self::step_find,
            |view, _, cx| view.close_find(cx),
            cx,
        ))
    }
}

const IDS: FindIds = FindIds {
    bar: "thread-find-bar",
    field: "thread-find-field",
    previous: "thread-find-previous",
    next: "thread-find-next",
    close: "thread-find-close",
    close_tip: "thread-find-close-tip",
};

#[cfg(test)]
mod tests {
    #[gpui::test]
    fn the_shortcut_binds_outside_fields(cx: &mut gpui::TestAppContext) {
        cx.update(super::bind_keys);
        let bindings = cx.update(|cx| {
            cx.key_bindings()
                .borrow()
                .bindings_for_action(&super::OpenThreadFind)
                .count()
        });
        assert_eq!(bindings, 1);
    }
}

//! The macOS text-field keys beyond the arrows: ⌘⇧←/→ (and ⇧Home/End)
//! select to an end, ⌥←/→ move by word and ⌥⇧ selects by word, ⌃A/⌃E
//! jump to an end, ⌥⌫/⌥⌦ delete a word and ⌘⌫/⌘⌦ to an end. A masked
//! field has no words to show, so word keys go to its ends.
//!
//! ↑/↓ stay unbound: a binding here would run before the key handlers of
//! the menus that filter with a field (their ↑/↓ move the highlight).

use gpui::{App, Context, KeyBinding, Window, actions};
use unicode_segmentation::UnicodeSegmentation;

use super::{CONTEXT, TextInput};

actions!(
    text_input,
    [
        SelectToStart,
        SelectToEnd,
        WordLeft,
        WordRight,
        SelectWordLeft,
        SelectWordRight,
        DeleteWordLeft,
        DeleteWordRight,
        DeleteToStart,
        DeleteToEnd,
    ]
);

pub(super) fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("cmd-shift-left", SelectToStart, Some(CONTEXT)),
        KeyBinding::new("cmd-shift-right", SelectToEnd, Some(CONTEXT)),
        KeyBinding::new("shift-home", SelectToStart, Some(CONTEXT)),
        KeyBinding::new("shift-end", SelectToEnd, Some(CONTEXT)),
        KeyBinding::new("ctrl-a", super::Home, Some(CONTEXT)),
        KeyBinding::new("ctrl-e", super::End, Some(CONTEXT)),
        KeyBinding::new("alt-left", WordLeft, Some(CONTEXT)),
        KeyBinding::new("alt-right", WordRight, Some(CONTEXT)),
        KeyBinding::new("alt-shift-left", SelectWordLeft, Some(CONTEXT)),
        KeyBinding::new("alt-shift-right", SelectWordRight, Some(CONTEXT)),
        KeyBinding::new("alt-backspace", DeleteWordLeft, Some(CONTEXT)),
        KeyBinding::new("alt-delete", DeleteWordRight, Some(CONTEXT)),
        KeyBinding::new("cmd-backspace", DeleteToStart, Some(CONTEXT)),
        KeyBinding::new("cmd-delete", DeleteToEnd, Some(CONTEXT)),
    ]);
}

/// Whether a segment is a word (letters or digits), not spaces or `/:.`.
fn is_word(segment: &str) -> bool {
    segment.chars().any(char::is_alphanumeric)
}

/// The start of the word before `offset` (⌥←).
pub(super) fn word_start(text: &str, offset: usize) -> usize {
    text.split_word_bound_indices()
        .rev()
        .find(|(start, segment)| *start < offset && is_word(segment))
        .map_or(0, |(start, _)| start)
}

/// The end of the word after `offset` (⌥→).
pub(super) fn word_end(text: &str, offset: usize) -> usize {
    text.split_word_bound_indices()
        .map(|(start, segment)| (start + segment.len(), segment))
        .find(|(end, segment)| *end > offset && is_word(segment))
        .map_or(text.len(), |(end, _)| end)
}

impl TextInput {
    fn word_left_of(&self, offset: usize) -> usize {
        if self.masked {
            0
        } else {
            word_start(&self.content, offset)
        }
    }

    fn word_right_of(&self, offset: usize) -> usize {
        if self.masked {
            self.content.len()
        } else {
            word_end(&self.content, offset)
        }
    }

    /// Deletes the selection, or from the caret to `to`.
    fn delete_toward(&mut self, to: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            if to == self.cursor_offset() {
                return;
            }
            self.select_to(to, cx);
        }
        self.replace(None, "", window, cx);
    }

    pub(super) fn select_to_start(
        &mut self,
        _: &SelectToStart,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_to(0, cx);
    }

    pub(super) fn select_to_end(
        &mut self,
        _: &SelectToEnd,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_to(self.content.len(), cx);
    }

    pub(super) fn word_left(&mut self, _: &WordLeft, _: &mut Window, cx: &mut Context<Self>) {
        let from = self.selected_range.start.min(self.cursor_offset());
        self.move_to(self.word_left_of(from), cx);
    }

    pub(super) fn word_right(&mut self, _: &WordRight, _: &mut Window, cx: &mut Context<Self>) {
        let from = self.selected_range.end.max(self.cursor_offset());
        self.move_to(self.word_right_of(from), cx);
    }

    pub(super) fn select_word_left(
        &mut self,
        _: &SelectWordLeft,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_to(self.word_left_of(self.cursor_offset()), cx);
    }

    pub(super) fn select_word_right(
        &mut self,
        _: &SelectWordRight,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_to(self.word_right_of(self.cursor_offset()), cx);
    }

    pub(super) fn delete_word_left(
        &mut self,
        _: &DeleteWordLeft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let to = self.word_left_of(self.cursor_offset());
        self.delete_toward(to, window, cx);
    }

    pub(super) fn delete_word_right(
        &mut self,
        _: &DeleteWordRight,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let to = self.word_right_of(self.cursor_offset());
        self.delete_toward(to, window, cx);
    }

    pub(super) fn delete_to_start(
        &mut self,
        _: &DeleteToStart,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.delete_toward(0, window, cx);
    }

    pub(super) fn delete_to_end(
        &mut self,
        _: &DeleteToEnd,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let end = self.content.len();
        self.delete_toward(end, window, cx);
    }
}

#[cfg(test)]
mod tests {
    use gpui::{Focusable, TestAppContext, WindowAppearance};

    use super::super::TextInput;
    use super::{word_end, word_start};
    use crate::theme::{Theme, ThemePreference};

    #[gpui::test]
    fn command_shift_left_selects_a_pasted_url(cx: &mut TestAppContext) {
        cx.update(|cx| {
            cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
            super::super::bind_keys(cx);
        });
        let (input, cx) = cx.add_window_view(TextInput::new);
        input.update_in(cx, |input, window, cx| {
            input.set_text(URL, cx);
            window.focus(&input.focus_handle(cx), cx);
        });
        cx.run_until_parked();
        let selected = |cx: &mut gpui::VisualTestContext| {
            input.read_with(cx, |input, _| {
                input.content[input.selected_range.clone()].to_string()
            })
        };
        cx.simulate_keystrokes("cmd-right cmd-shift-left");
        assert_eq!(selected(cx), URL);
        cx.simulate_keystrokes("cmd-right alt-shift-left");
        assert_eq!(selected(cx), "baz");
        cx.simulate_keystrokes("cmd-right alt-backspace");
        assert_eq!(
            input.read_with(cx, |input, _| input.content.to_string()),
            "https://github.com/foo/bar-"
        );
        // ↑ is left to the menus around a field.
        cx.simulate_keystrokes("up");
        assert_eq!(selected(cx), "");
    }

    const URL: &str = "https://github.com/foo/bar-baz";

    #[test]
    fn word_moves_stop_at_words_in_a_url() {
        assert_eq!(&URL[word_start(URL, URL.len())..], "baz");
        assert_eq!(&URL[word_start(URL, 23)..], "foo/bar-baz");
        assert_eq!(word_start(URL, 5), 0);
        assert_eq!(word_start("", 0), 0);
        assert_eq!(&URL[..word_end(URL, 0)], "https");
        assert_eq!(&URL[..word_end(URL, 5)], "https://github.com");
        assert_eq!(word_end(URL, URL.len()), URL.len());
    }
}

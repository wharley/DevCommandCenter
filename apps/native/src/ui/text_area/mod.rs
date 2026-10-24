//! A multi-line text field (the composer's): wrapping, selection across
//! lines, word and line moves, undo, IME and clipboard, scrolling inside its
//! maximum height. Enter submits and Shift+Enter breaks the line, like the
//! desktop's composer; the owner styles the box around it.

mod buffer;
pub mod chip;
mod element;
mod handler;

use std::ops::Range;

use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, EventEmitter, FocusHandle, Focusable,
    KeyBinding, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point,
    ScrollWheelEvent, SharedString, Subscription, Window, WrappedLine, actions, div, point,
    prelude::*, px,
};

pub use buffer::{Buffer, CHIP};
pub use chip::Chip;

/// A piece of the field's content, to fill it at once (a restored draft).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Part {
    Text(String),
    Chip(Chip),
}

/// Pastes this long become a snippet chip (`LARGE_PASTE_CHAR_THRESHOLD`).
const LARGE_PASTE: usize = 1200;
use element::{DisplayMap, TextAreaElement};

actions!(
    text_area,
    [
        Backspace,
        Delete,
        DeleteWordLeft,
        DeleteWordRight,
        DeleteToLineStart,
        Left,
        Right,
        Up,
        Down,
        SelectLeft,
        SelectRight,
        SelectUp,
        SelectDown,
        WordLeft,
        WordRight,
        SelectWordLeft,
        SelectWordRight,
        LineStart,
        LineEnd,
        SelectLineStart,
        SelectLineEnd,
        DocumentStart,
        DocumentEnd,
        SelectDocumentStart,
        SelectDocumentEnd,
        SelectAll,
        Newline,
        Submit,
        Cancel,
        Tab,
        Paste,
        Cut,
        Copy,
        Undo,
        Redo,
        ShowCharacterPalette,
    ]
);

const CONTEXT: &str = "TextArea";

/// Registers the field's keys; call once at startup.
pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("backspace", Backspace, Some(CONTEXT)),
        KeyBinding::new("shift-backspace", Backspace, Some(CONTEXT)),
        KeyBinding::new("delete", Delete, Some(CONTEXT)),
        KeyBinding::new("alt-backspace", DeleteWordLeft, Some(CONTEXT)),
        KeyBinding::new("alt-delete", DeleteWordRight, Some(CONTEXT)),
        KeyBinding::new("cmd-backspace", DeleteToLineStart, Some(CONTEXT)),
        KeyBinding::new("left", Left, Some(CONTEXT)),
        KeyBinding::new("right", Right, Some(CONTEXT)),
        KeyBinding::new("up", Up, Some(CONTEXT)),
        KeyBinding::new("down", Down, Some(CONTEXT)),
        KeyBinding::new("shift-left", SelectLeft, Some(CONTEXT)),
        KeyBinding::new("shift-right", SelectRight, Some(CONTEXT)),
        KeyBinding::new("shift-up", SelectUp, Some(CONTEXT)),
        KeyBinding::new("shift-down", SelectDown, Some(CONTEXT)),
        KeyBinding::new("alt-left", WordLeft, Some(CONTEXT)),
        KeyBinding::new("alt-right", WordRight, Some(CONTEXT)),
        KeyBinding::new("alt-shift-left", SelectWordLeft, Some(CONTEXT)),
        KeyBinding::new("alt-shift-right", SelectWordRight, Some(CONTEXT)),
        KeyBinding::new("cmd-left", LineStart, Some(CONTEXT)),
        KeyBinding::new("home", LineStart, Some(CONTEXT)),
        KeyBinding::new("cmd-right", LineEnd, Some(CONTEXT)),
        KeyBinding::new("end", LineEnd, Some(CONTEXT)),
        KeyBinding::new("cmd-shift-left", SelectLineStart, Some(CONTEXT)),
        KeyBinding::new("cmd-shift-right", SelectLineEnd, Some(CONTEXT)),
        KeyBinding::new("cmd-up", DocumentStart, Some(CONTEXT)),
        KeyBinding::new("cmd-down", DocumentEnd, Some(CONTEXT)),
        KeyBinding::new("cmd-shift-up", SelectDocumentStart, Some(CONTEXT)),
        KeyBinding::new("cmd-shift-down", SelectDocumentEnd, Some(CONTEXT)),
        KeyBinding::new("cmd-a", SelectAll, Some(CONTEXT)),
        KeyBinding::new("shift-enter", Newline, Some(CONTEXT)),
        KeyBinding::new("alt-enter", Newline, Some(CONTEXT)),
        KeyBinding::new("enter", Submit, Some(CONTEXT)),
        KeyBinding::new("escape", Cancel, Some(CONTEXT)),
        KeyBinding::new("tab", Tab, Some(CONTEXT)),
        KeyBinding::new("cmd-v", Paste, Some(CONTEXT)),
        KeyBinding::new("cmd-x", Cut, Some(CONTEXT)),
        KeyBinding::new("cmd-c", Copy, Some(CONTEXT)),
        KeyBinding::new("cmd-z", Undo, Some(CONTEXT)),
        KeyBinding::new("cmd-shift-z", Redo, Some(CONTEXT)),
        KeyBinding::new("ctrl-cmd-space", ShowCharacterPalette, Some(CONTEXT)),
    ]);
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextAreaEvent {
    Changed,
    /// Enter. The owner decides whether it sends.
    Submitted,
    Cancelled,
    Blurred,
    /// A paste the clipboard held as something other than text (an image).
    PastedNonText,
    /// ↑ / ↓ / Tab while the owner's menu is open (`set_menu_open`).
    MenuUp,
    MenuDown,
    MenuAccept,
}

/// The laid-out text of the last frame, for clicks, arrows and the IME.
pub(crate) struct Layout {
    /// Each hard line: its byte offset in the display copy, its wrapped
    /// layout and its top in content coordinates.
    lines: Vec<(usize, WrappedLine, Pixels)>,
    line_height: Pixels,
    /// Where the visible part was painted.
    bounds: Bounds<Pixels>,
    content_height: Pixels,
    map: DisplayMap,
    /// Each chip's close button and the chip's offset in the text.
    chip_closes: Vec<(Bounds<Pixels>, usize)>,
}

impl Layout {
    fn line_for_offset(&self, offset: usize) -> Option<&(usize, WrappedLine, Pixels)> {
        self.lines
            .iter()
            .rev()
            .find(|(start, line, _)| *start <= offset && offset <= start + line.len())
    }

    /// The caret's top-left for a text `offset`, in content coordinates.
    fn position_for(&self, offset: usize) -> Point<Pixels> {
        let offset = self.map.to_display(offset);
        match self.line_for_offset(offset) {
            Some((start, line, top)) => {
                let position = line
                    .position_for_index(offset - start, self.line_height)
                    .unwrap_or_default();
                point(position.x, position.y + *top)
            }
            None => point(px(0.0), px(0.0)),
        }
    }

    /// The text offset nearest a point in content coordinates.
    fn offset_for(&self, position: Point<Pixels>) -> usize {
        self.map.to_text(self.display_offset_for(position))
    }

    fn display_offset_for(&self, position: Point<Pixels>) -> usize {
        let Some(first) = self.lines.first() else {
            return 0;
        };
        if position.y < px(0.0) {
            return first.0;
        }
        let row = self
            .lines
            .iter()
            .rev()
            .find(|(_, _, top)| position.y >= *top)
            .unwrap_or(first);
        let (start, line, top) = row;
        let local = point(position.x.max(px(0.0)), position.y - *top);
        let height = line.size(self.line_height).height.max(self.line_height);
        let local = if local.y >= height {
            point(local.x, height - px(1.0))
        } else {
            local
        };
        let index = match line.closest_index_for_position(local, self.line_height) {
            Ok(index) | Err(index) => index,
        };
        start + index.min(line.len())
    }
}

pub struct TextArea {
    focus_handle: FocusHandle,
    pub(crate) buffer: Buffer<Chip>,
    /// The owner shows a menu over the field: ↑↓, Enter and Tab are its.
    menu_open: bool,
    placeholder: SharedString,
    max_height: Option<Pixels>,
    disabled: bool,
    pub(crate) layout: Option<Layout>,
    pub(crate) scroll_y: Pixels,
    /// Scroll so the caret shows at the next frame.
    pub(crate) reveal_cursor: bool,
    /// The x the caret keeps while moving up and down.
    goal_x: Option<Pixels>,
    is_selecting: bool,
    _blur: Option<Subscription>,
}

impl EventEmitter<TextAreaEvent> for TextArea {}

impl TextArea {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let blur = cx.on_blur(&focus_handle, window, |_, _, cx| {
            cx.emit(TextAreaEvent::Blurred)
        });
        Self {
            focus_handle,
            buffer: Buffer::default(),
            menu_open: false,
            placeholder: SharedString::default(),
            max_height: None,
            disabled: false,
            layout: None,
            scroll_y: px(0.0),
            reveal_cursor: false,
            goal_x: None,
            is_selecting: false,
            _blur: Some(blur),
        }
    }

    pub fn with_placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    /// Past this height the text scrolls inside the field.
    pub fn with_max_height(mut self, height: f32) -> Self {
        self.max_height = Some(px(height));
        self
    }

    pub fn set_placeholder(
        &mut self,
        placeholder: impl Into<SharedString>,
        cx: &mut Context<Self>,
    ) {
        self.placeholder = placeholder.into();
        cx.notify();
    }

    pub fn set_disabled(&mut self, disabled: bool, cx: &mut Context<Self>) {
        if self.disabled != disabled {
            self.disabled = disabled;
            cx.notify();
        }
    }

    pub fn text(&self) -> &str {
        self.buffer.text()
    }

    /// The chips in text order with each one's offset.
    pub fn chips(&self) -> Vec<(usize, Chip)> {
        self.buffer
            .chip_offsets()
            .map(|(offset, chip)| (offset, chip.clone()))
            .collect()
    }

    pub fn cursor(&self) -> usize {
        self.buffer.cursor()
    }

    pub fn has_selection(&self) -> bool {
        !self.buffer.selection.is_empty()
    }

    pub fn set_menu_open(&mut self, open: bool) {
        self.menu_open = open;
    }

    /// Replaces everything with these parts, with no undo history.
    pub fn set_parts(&mut self, parts: Vec<Part>, cx: &mut Context<Self>) {
        self.buffer.set_text("");
        for part in parts {
            match part {
                Part::Text(text) => self.buffer.replace(None, &text),
                Part::Chip(chip) => self.buffer.insert_chip(chip),
            }
        }
        self.buffer.forget_history();
        self.reveal_cursor = true;
        cx.emit(TextAreaEvent::Changed);
        cx.notify();
    }

    /// Puts a chip at the caret, or in place of `range` (the typed `@query`).
    pub fn insert_chip(
        &mut self,
        chip: Chip,
        range: Option<std::ops::Range<usize>>,
        cx: &mut Context<Self>,
    ) {
        if let Some(range) = range {
            self.buffer.move_to(range.start);
            self.buffer.select_to(range.end);
        }
        self.buffer.insert_chip(chip);
        self.after_edit(cx);
    }

    /// Adds a chip at the very end (a pasted image), keeping the caret.
    pub fn append_chip(&mut self, chip: Chip, cx: &mut Context<Self>) {
        let cursor = self.buffer.cursor();
        let end = self.buffer.text().len();
        self.buffer.move_to(end);
        self.buffer.insert_chip(chip);
        self.buffer.move_to(cursor);
        self.after_edit(cx);
    }

    pub fn set_text(&mut self, text: &str, cx: &mut Context<Self>) {
        self.buffer.set_text(text);
        self.reveal_cursor = true;
        cx.emit(TextAreaEvent::Changed);
        cx.notify();
    }

    /// Adds text at the very end, the caret after it (a request filled in).
    pub fn append_text(&mut self, text: &str, cx: &mut Context<Self>) {
        let end = self.buffer.text().len();
        self.buffer.move_to(end);
        self.buffer.replace(None, text);
        self.after_edit(cx);
    }

    /// Inserts at the caret (a mention, an attachment path), as one edit.
    pub fn insert(&mut self, text: &str, cx: &mut Context<Self>) {
        self.buffer.replace(None, text);
        self.after_edit(cx);
    }

    fn after_edit(&mut self, cx: &mut Context<Self>) {
        self.goal_x = None;
        self.reveal_cursor = true;
        cx.emit(TextAreaEvent::Changed);
        cx.notify();
    }

    fn moved(&mut self, cx: &mut Context<Self>) {
        self.reveal_cursor = true;
        cx.notify();
    }

    fn horizontal(&mut self, offset: usize, extend: bool, cx: &mut Context<Self>) {
        self.goal_x = None;
        self.buffer.go(offset, extend);
        self.moved(cx);
    }

    fn vertical(&mut self, rows: f32, extend: bool, cx: &mut Context<Self>) {
        let Some(layout) = self.layout.as_ref() else {
            return;
        };
        let caret = layout.position_for(self.buffer.cursor());
        let x = *self.goal_x.get_or_insert(caret.x);
        let target_y = caret.y + layout.line_height * rows + layout.line_height / 2.0;
        let offset = if target_y < px(0.0) {
            0
        } else if target_y >= layout.content_height {
            self.buffer.text().len()
        } else {
            layout.offset_for(point(x, target_y))
        };
        self.buffer.go(offset, extend);
        self.moved(cx);
    }

    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        let target = if self.buffer.selection.is_empty() {
            self.buffer.previous_boundary(self.buffer.cursor())
        } else {
            self.buffer.selection.start
        };
        self.horizontal(target, false, cx);
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        let target = if self.buffer.selection.is_empty() {
            self.buffer.next_boundary(self.buffer.cursor())
        } else {
            self.buffer.selection.end
        };
        self.horizontal(target, false, cx);
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.horizontal(
            self.buffer.previous_boundary(self.buffer.cursor()),
            true,
            cx,
        );
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.horizontal(self.buffer.next_boundary(self.buffer.cursor()), true, cx);
    }

    fn up(&mut self, _: &Up, _: &mut Window, cx: &mut Context<Self>) {
        if self.menu_open {
            cx.emit(TextAreaEvent::MenuUp);
            return;
        }
        self.vertical(-1.0, false, cx);
    }

    fn down(&mut self, _: &Down, _: &mut Window, cx: &mut Context<Self>) {
        if self.menu_open {
            cx.emit(TextAreaEvent::MenuDown);
            return;
        }
        self.vertical(1.0, false, cx);
    }

    fn tab(&mut self, _: &Tab, window: &mut Window, cx: &mut Context<Self>) {
        if self.menu_open {
            cx.emit(TextAreaEvent::MenuAccept);
        } else {
            window.focus_next(cx);
        }
    }

    fn select_up(&mut self, _: &SelectUp, _: &mut Window, cx: &mut Context<Self>) {
        self.vertical(-1.0, true, cx);
    }

    fn select_down(&mut self, _: &SelectDown, _: &mut Window, cx: &mut Context<Self>) {
        self.vertical(1.0, true, cx);
    }

    fn word_left(&mut self, _: &WordLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.horizontal(self.buffer.previous_word(self.buffer.cursor()), false, cx);
    }

    fn word_right(&mut self, _: &WordRight, _: &mut Window, cx: &mut Context<Self>) {
        self.horizontal(self.buffer.next_word(self.buffer.cursor()), false, cx);
    }

    fn select_word_left(&mut self, _: &SelectWordLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.horizontal(self.buffer.previous_word(self.buffer.cursor()), true, cx);
    }

    fn select_word_right(&mut self, _: &SelectWordRight, _: &mut Window, cx: &mut Context<Self>) {
        self.horizontal(self.buffer.next_word(self.buffer.cursor()), true, cx);
    }

    fn line_start(&mut self, _: &LineStart, _: &mut Window, cx: &mut Context<Self>) {
        self.horizontal(self.buffer.line_start(self.buffer.cursor()), false, cx);
    }

    fn line_end(&mut self, _: &LineEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.horizontal(self.buffer.line_end(self.buffer.cursor()), false, cx);
    }

    fn select_line_start(&mut self, _: &SelectLineStart, _: &mut Window, cx: &mut Context<Self>) {
        self.horizontal(self.buffer.line_start(self.buffer.cursor()), true, cx);
    }

    fn select_line_end(&mut self, _: &SelectLineEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.horizontal(self.buffer.line_end(self.buffer.cursor()), true, cx);
    }

    fn document_start(&mut self, _: &DocumentStart, _: &mut Window, cx: &mut Context<Self>) {
        self.horizontal(0, false, cx);
    }

    fn document_end(&mut self, _: &DocumentEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.horizontal(self.buffer.text().len(), false, cx);
    }

    fn select_document_start(
        &mut self,
        _: &SelectDocumentStart,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.horizontal(0, true, cx);
    }

    fn select_document_end(
        &mut self,
        _: &SelectDocumentEnd,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.horizontal(self.buffer.text().len(), true, cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.select_all();
        cx.notify();
    }

    fn edit(&mut self, edit: impl FnOnce(&mut Buffer<Chip>), cx: &mut Context<Self>) {
        if self.disabled {
            return;
        }
        edit(&mut self.buffer);
        self.after_edit(cx);
    }

    fn backspace(&mut self, _: &Backspace, _: &mut Window, cx: &mut Context<Self>) {
        self.edit(
            |buffer| buffer.delete_to(buffer.previous_boundary(buffer.cursor())),
            cx,
        );
    }

    fn delete(&mut self, _: &Delete, _: &mut Window, cx: &mut Context<Self>) {
        self.edit(
            |buffer| buffer.delete_to(buffer.next_boundary(buffer.cursor())),
            cx,
        );
    }

    fn delete_word_left(&mut self, _: &DeleteWordLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.edit(
            |buffer| buffer.delete_to(buffer.previous_word(buffer.cursor())),
            cx,
        );
    }

    fn delete_word_right(&mut self, _: &DeleteWordRight, _: &mut Window, cx: &mut Context<Self>) {
        self.edit(
            |buffer| buffer.delete_to(buffer.next_word(buffer.cursor())),
            cx,
        );
    }

    fn delete_to_line_start(
        &mut self,
        _: &DeleteToLineStart,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit(
            |buffer| buffer.delete_to(buffer.line_start(buffer.cursor())),
            cx,
        );
    }

    fn newline(&mut self, _: &Newline, _: &mut Window, cx: &mut Context<Self>) {
        self.edit(|buffer| buffer.replace(None, "\n"), cx);
    }

    fn submit(&mut self, _: &Submit, _: &mut Window, cx: &mut Context<Self>) {
        if self.buffer.marked.is_some() {
            return;
        }
        cx.emit(if self.menu_open {
            TextAreaEvent::MenuAccept
        } else {
            TextAreaEvent::Submitted
        });
    }

    fn cancel(&mut self, _: &Cancel, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(TextAreaEvent::Cancelled);
    }

    fn paste(&mut self, _: &Paste, _: &mut Window, cx: &mut Context<Self>) {
        match cx.read_from_clipboard().and_then(|item| item.text()) {
            Some(text) => {
                let text = text.replace("\r\n", "\n");
                if text.encode_utf16().count() >= LARGE_PASTE {
                    // The whole text still goes in the prompt.
                    self.edit(|buffer| buffer.insert_chip(Chip::Snippet(text)), cx);
                } else {
                    self.edit(|buffer| buffer.replace(None, &text), cx);
                }
            }
            None => cx.emit(TextAreaEvent::PastedNonText),
        }
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.buffer.selection.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.buffer.selected_text().to_string(),
            ));
        }
    }

    fn cut(&mut self, _: &Cut, _: &mut Window, cx: &mut Context<Self>) {
        if !self.buffer.selection.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.buffer.selected_text().to_string(),
            ));
            self.edit(|buffer| buffer.replace(None, ""), cx);
        }
    }

    fn undo(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        if self.buffer.undo() {
            self.after_edit(cx);
        }
    }

    fn redo(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
        if self.buffer.redo() {
            self.after_edit(cx);
        }
    }

    fn show_character_palette(
        &mut self,
        _: &ShowCharacterPalette,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        window.show_character_palette();
    }

    fn offset_at(&self, position: Point<Pixels>) -> usize {
        let Some(layout) = self.layout.as_ref() else {
            return self.buffer.text().len();
        };
        let local = point(
            position.x - layout.bounds.left(),
            position.y - layout.bounds.top() + self.scroll_y,
        );
        layout.offset_for(local)
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus_handle, cx);
        if let Some(chip) = self.layout.as_ref().and_then(|layout| {
            layout
                .chip_closes
                .iter()
                .find(|(close, _)| close.contains(&event.position))
                .map(|(_, offset)| *offset)
        }) {
            self.buffer.remove_chip_at(chip);
            self.after_edit(cx);
            return;
        }
        let offset = self.offset_at(event.position);
        self.goal_x = None;
        if event.modifiers.shift {
            self.buffer.select_to(offset);
        } else if event.click_count >= 3 {
            self.buffer.move_to(self.buffer.line_start(offset));
            self.buffer.select_to(self.buffer.line_end(offset));
        } else if event.click_count == 2 {
            let word = self.buffer.word_at(offset);
            self.buffer.move_to(word.start);
            self.buffer.select_to(word.end);
        } else {
            self.buffer.move_to(offset);
            self.is_selecting = true;
        }
        cx.notify();
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting {
            let offset = self.offset_at(event.position);
            self.buffer.select_to(offset);
            self.moved(cx);
        }
    }

    fn on_scroll(&mut self, event: &ScrollWheelEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(layout) = self.layout.as_ref() else {
            return;
        };
        let visible = layout.bounds.size.height;
        let max = (layout.content_height - visible).max(px(0.0));
        if max <= px(0.0) {
            return;
        }
        let delta = event.delta.pixel_delta(layout.line_height).y;
        let next = (self.scroll_y - delta).clamp(px(0.0), max);
        if next != self.scroll_y {
            self.scroll_y = next;
            cx.stop_propagation();
            cx.notify();
        }
        let _ = window;
    }

    pub(crate) fn selected_range(&self) -> Range<usize> {
        self.buffer.selection.clone()
    }
}

impl Focusable for TextArea {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for TextArea {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("text-area")
            .flex()
            .w_full()
            .min_w_0()
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle(cx))
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::delete_word_left))
            .on_action(cx.listener(Self::delete_word_right))
            .on_action(cx.listener(Self::delete_to_line_start))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::up))
            .on_action(cx.listener(Self::down))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_up))
            .on_action(cx.listener(Self::select_down))
            .on_action(cx.listener(Self::word_left))
            .on_action(cx.listener(Self::word_right))
            .on_action(cx.listener(Self::select_word_left))
            .on_action(cx.listener(Self::select_word_right))
            .on_action(cx.listener(Self::line_start))
            .on_action(cx.listener(Self::line_end))
            .on_action(cx.listener(Self::select_line_start))
            .on_action(cx.listener(Self::select_line_end))
            .on_action(cx.listener(Self::document_start))
            .on_action(cx.listener(Self::document_end))
            .on_action(cx.listener(Self::select_document_start))
            .on_action(cx.listener(Self::select_document_end))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::newline))
            .on_action(cx.listener(Self::submit))
            .on_action(cx.listener(Self::cancel))
            .on_action(cx.listener(Self::tab))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::redo))
            .on_action(cx.listener(Self::show_character_palette))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_scroll_wheel(cx.listener(Self::on_scroll))
            .child(TextAreaElement::new(cx.entity(), self.max_height))
    }
}

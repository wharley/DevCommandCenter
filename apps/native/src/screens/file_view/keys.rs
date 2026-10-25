//! The editor's keys: moving and selecting by character, word, line and
//! file, deleting, Enter with the line's indentation, Tab and Shift+Tab,
//! the clipboard and undo. Escape only drops the selection: inside the
//! text it never leaves the file (`shouldIgnoreGlobalShortcutTarget`).

use dcc_core::domain::indentation::{indent_unit, line_break_insertion};
use gpui::{App, ClipboardItem, Context, InteractiveElement, KeyBinding, Window, actions};

use super::FileView;
use super::buffer::{column_of, offset_at_column};
use super::lines::{indent_lines, tab_over_selection};

actions!(
    code_editor,
    [
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
        Backspace,
        Delete,
        DeleteWordLeft,
        DeleteWordRight,
        DeleteToLineStart,
        Newline,
        Indent,
        Outdent,
        Copy,
        Cut,
        Paste,
        Undo,
        Redo,
        Cancel,
    ]
);

pub(crate) const CONTEXT: &str = "CodeEditor";

/// Registers the editor's keys; call once at startup.
pub(crate) fn bind_keys(cx: &mut App) {
    let context = Some(CONTEXT);
    cx.bind_keys([
        KeyBinding::new("left", Left, context),
        KeyBinding::new("right", Right, context),
        KeyBinding::new("up", Up, context),
        KeyBinding::new("down", Down, context),
        KeyBinding::new("shift-left", SelectLeft, context),
        KeyBinding::new("shift-right", SelectRight, context),
        KeyBinding::new("shift-up", SelectUp, context),
        KeyBinding::new("shift-down", SelectDown, context),
        KeyBinding::new("alt-left", WordLeft, context),
        KeyBinding::new("alt-right", WordRight, context),
        KeyBinding::new("alt-shift-left", SelectWordLeft, context),
        KeyBinding::new("alt-shift-right", SelectWordRight, context),
        KeyBinding::new("cmd-left", LineStart, context),
        KeyBinding::new("cmd-right", LineEnd, context),
        KeyBinding::new("home", LineStart, context),
        KeyBinding::new("end", LineEnd, context),
        KeyBinding::new("cmd-shift-left", SelectLineStart, context),
        KeyBinding::new("cmd-shift-right", SelectLineEnd, context),
        KeyBinding::new("cmd-up", DocumentStart, context),
        KeyBinding::new("cmd-down", DocumentEnd, context),
        KeyBinding::new("cmd-home", DocumentStart, context),
        KeyBinding::new("cmd-end", DocumentEnd, context),
        KeyBinding::new("cmd-shift-home", SelectDocumentStart, context),
        KeyBinding::new("cmd-shift-end", SelectDocumentEnd, context),
        KeyBinding::new("cmd-shift-up", SelectDocumentStart, context),
        KeyBinding::new("cmd-shift-down", SelectDocumentEnd, context),
        KeyBinding::new("cmd-a", SelectAll, context),
        KeyBinding::new("backspace", Backspace, context),
        KeyBinding::new("shift-backspace", Backspace, context),
        KeyBinding::new("delete", Delete, context),
        KeyBinding::new("alt-backspace", DeleteWordLeft, context),
        KeyBinding::new("alt-delete", DeleteWordRight, context),
        KeyBinding::new("cmd-backspace", DeleteToLineStart, context),
        KeyBinding::new("enter", Newline, context),
        KeyBinding::new("tab", Indent, context),
        KeyBinding::new("shift-tab", Outdent, context),
        KeyBinding::new("cmd-c", Copy, context),
        KeyBinding::new("cmd-x", Cut, context),
        KeyBinding::new("cmd-v", Paste, context),
        KeyBinding::new("cmd-z", Undo, context),
        KeyBinding::new("cmd-shift-z", Redo, context),
        KeyBinding::new("escape", Cancel, context),
    ]);
}

impl FileView {
    fn go(&mut self, offset: usize, extend: bool, cx: &mut Context<Self>) {
        self.buffer.go(offset, extend);
        self.goal_column = None;
        self.moved(cx);
    }

    /// Left/right collapse a selection to its side first.
    fn step(&mut self, forward: bool, extend: bool, cx: &mut Context<Self>) {
        let selection = self.buffer.selection.clone();
        if !extend && !selection.is_empty() {
            return self.go(
                if forward {
                    selection.end
                } else {
                    selection.start
                },
                false,
                cx,
            );
        }
        let cursor = self.buffer.cursor();
        let to = if forward {
            self.buffer.next_boundary(cursor)
        } else {
            self.buffer.previous_boundary(cursor)
        };
        self.go(to, extend, cx);
    }

    /// Up and down keep the screen column they started from.
    fn vertical(&mut self, rows: isize, extend: bool, cx: &mut Context<Self>) {
        let tab = self.indentation.size;
        let cursor = self.buffer.cursor();
        let line = self.buffer.line_of(cursor);
        let goal = *self.goal_column.get_or_insert_with(|| {
            column_of(
                self.buffer.line(line),
                cursor - self.buffer.line_start(line),
                tab,
            ) as f32
        });
        let target = line as isize + rows;
        let to = if target < 0 {
            0
        } else if target as usize >= self.buffer.line_count() {
            self.buffer.text().len()
        } else {
            let target = target as usize;
            self.buffer.line_start(target) + offset_at_column(self.buffer.line(target), goal, tab)
        };
        self.buffer.go(to, extend);
        self.moved(cx);
    }

    /// Replaces the selection with `text` as typing does.
    pub(crate) fn insert(&mut self, text: &str, cx: &mut Context<Self>) {
        if self.read_only {
            return;
        }
        let (lines, line) = (
            self.buffer.line_count(),
            self.buffer.line_of(self.buffer.selection.start),
        );
        self.buffer.replace(None, text);
        self.goal_column = None;
        self.edited(lines, line, cx);
    }

    fn delete(&mut self, to: impl Fn(&FileView) -> usize, cx: &mut Context<Self>) {
        if self.read_only {
            return;
        }
        let (lines, line) = (
            self.buffer.line_count(),
            self.buffer.line_of(self.buffer.selection.start),
        );
        let target = to(self);
        let before = self.buffer.version;
        self.buffer.delete_to(target);
        if self.buffer.version != before {
            self.goal_column = None;
            let line = line.min(self.buffer.line_of(self.buffer.cursor()));
            self.edited(lines, line, cx);
        }
    }

    /// Enter: the line's indentation (`lineBreakInsertion`).
    fn newline(&mut self, cx: &mut Context<Self>) {
        if self.read_only {
            return;
        }
        let selection = self.buffer.selection.clone();
        let (start_line, end_line) = (
            self.buffer.line_of(selection.start),
            self.buffer.line_of(selection.end),
        );
        let start_column = selection.start - self.buffer.line_start(start_line);
        let end_column = selection.end - self.buffer.line_start(end_line);
        let joined = format!(
            "{}{}",
            &self.buffer.line(start_line)[..start_column.min(self.buffer.line(start_line).len())],
            &self.buffer.line(end_line)[end_column.min(self.buffer.line(end_line).len())..]
        );
        let eol = if self.buffer.text().contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        };
        let unit = indent_unit(self.indentation);
        let insertion = line_break_insertion(&joined, start_column, &unit, eol);
        let lines = self.buffer.line_count();
        self.buffer.replace(None, &insertion.text);
        let caret = selection.start + eol.len() + insertion.caret_column;
        self.buffer.go(caret, false);
        self.goal_column = None;
        self.edited(lines, start_line, cx);
    }

    /// Tab: the project's indent at the caret; over a selection on one
    /// line, a tab or spaces as the line starts; over several lines, each
    /// one level in (the Pierre editor's own `indent`).
    fn indent(&mut self, cx: &mut Context<Self>) {
        let selection = self.buffer.selection.clone();
        if selection.is_empty() {
            return self.insert(&indent_unit(self.indentation), cx);
        }
        let size = self.indentation.size.max(1);
        if self.buffer.line_of(selection.start) == self.buffer.line_of(selection.end) {
            return self.run_rule(|doc, sel, _| Some(tab_over_selection(doc, sel, size)), cx);
        }
        self.run_rule(
            |doc, sel, reversed| indent_lines(doc, sel, reversed, size, false),
            cx,
        );
    }

    /// Shift+Tab: each selected line one level out.
    fn outdent(&mut self, cx: &mut Context<Self>) {
        let size = self.indentation.size.max(1);
        self.run_rule(
            |doc, sel, reversed| indent_lines(doc, sel, reversed, size, true),
            cx,
        );
    }

    fn copy(&mut self, cx: &mut Context<Self>) {
        let text = self.buffer.selected_text();
        if !text.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(text.to_string()));
        }
    }

    pub(crate) fn register_actions(div: gpui::Div, cx: &mut Context<Self>) -> gpui::Div {
        Self::register_line_commands(div, cx)
            .key_context(CONTEXT)
            .on_action(cx.listener(|view, _: &Left, _, cx| view.step(false, false, cx)))
            .on_action(cx.listener(|view, _: &Right, _, cx| view.step(true, false, cx)))
            .on_action(cx.listener(|view, _: &SelectLeft, _, cx| view.step(false, true, cx)))
            .on_action(cx.listener(|view, _: &SelectRight, _, cx| view.step(true, true, cx)))
            .on_action(cx.listener(|view, _: &Up, _, cx| view.vertical(-1, false, cx)))
            .on_action(cx.listener(|view, _: &Down, _, cx| view.vertical(1, false, cx)))
            .on_action(cx.listener(|view, _: &SelectUp, _, cx| view.vertical(-1, true, cx)))
            .on_action(cx.listener(|view, _: &SelectDown, _, cx| view.vertical(1, true, cx)))
            .on_action(cx.listener(|view, _: &WordLeft, _, cx| {
                let to = view.buffer.previous_word(view.buffer.cursor());
                view.go(to, false, cx)
            }))
            .on_action(cx.listener(|view, _: &WordRight, _, cx| {
                let to = view.buffer.next_word(view.buffer.cursor());
                view.go(to, false, cx)
            }))
            .on_action(cx.listener(|view, _: &SelectWordLeft, _, cx| {
                let to = view.buffer.previous_word(view.buffer.cursor());
                view.go(to, true, cx)
            }))
            .on_action(cx.listener(|view, _: &SelectWordRight, _, cx| {
                let to = view.buffer.next_word(view.buffer.cursor());
                view.go(to, true, cx)
            }))
            .on_action(cx.listener(|view, _: &LineStart, _, cx| {
                let to = view.buffer.line_start_of(view.buffer.cursor());
                view.go(to, false, cx)
            }))
            .on_action(cx.listener(|view, _: &LineEnd, _, cx| {
                let to = view.buffer.line_end_of(view.buffer.cursor());
                view.go(to, false, cx)
            }))
            .on_action(cx.listener(|view, _: &SelectLineStart, _, cx| {
                let to = view.buffer.line_start_of(view.buffer.cursor());
                view.go(to, true, cx)
            }))
            .on_action(cx.listener(|view, _: &SelectLineEnd, _, cx| {
                let to = view.buffer.line_end_of(view.buffer.cursor());
                view.go(to, true, cx)
            }))
            .on_action(cx.listener(|view, _: &DocumentStart, _, cx| view.go(0, false, cx)))
            .on_action(cx.listener(|view, _: &DocumentEnd, _, cx| {
                let end = view.buffer.text().len();
                view.go(end, false, cx)
            }))
            .on_action(cx.listener(|view, _: &SelectDocumentStart, _, cx| view.go(0, true, cx)))
            .on_action(cx.listener(|view, _: &SelectDocumentEnd, _, cx| {
                let end = view.buffer.text().len();
                view.go(end, true, cx)
            }))
            .on_action(cx.listener(|view, _: &SelectAll, _, cx| {
                view.buffer.select_all();
                view.moved(cx)
            }))
            .on_action(cx.listener(|view, _: &Backspace, _, cx| {
                view.delete(
                    |view| view.buffer.previous_boundary(view.buffer.cursor()),
                    cx,
                )
            }))
            .on_action(cx.listener(|view, _: &Delete, _, cx| {
                view.delete(|view| view.buffer.next_boundary(view.buffer.cursor()), cx)
            }))
            .on_action(cx.listener(|view, _: &DeleteWordLeft, _, cx| {
                view.delete(|view| view.buffer.previous_word(view.buffer.cursor()), cx)
            }))
            .on_action(cx.listener(|view, _: &DeleteWordRight, _, cx| {
                view.delete(|view| view.buffer.next_word(view.buffer.cursor()), cx)
            }))
            .on_action(cx.listener(|view, _: &DeleteToLineStart, _, cx| {
                view.delete(|view| view.buffer.line_start_of(view.buffer.cursor()), cx)
            }))
            .on_action(cx.listener(|view, _: &Newline, _, cx| view.newline(cx)))
            .on_action(cx.listener(|view, _: &Indent, _, cx| view.indent(cx)))
            .on_action(cx.listener(|view, _: &Outdent, _, cx| view.outdent(cx)))
            .on_action(cx.listener(|view, _: &Copy, _, cx| view.copy(cx)))
            .on_action(cx.listener(|view, _: &Cut, _, cx| {
                view.copy(cx);
                if !view.buffer.selection.is_empty() {
                    view.insert("", cx);
                }
            }))
            .on_action(cx.listener(|view, _: &Paste, _, cx| {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    view.insert(&text, cx);
                }
            }))
            .on_action(cx.listener(|view, _: &Undo, _, cx| {
                if view.read_only {
                    return;
                }
                let lines = view.buffer.line_count();
                if view.buffer.undo() {
                    let line = view.buffer.line_of(view.buffer.cursor());
                    view.edited(lines, line, cx);
                }
            }))
            .on_action(cx.listener(|view, _: &Redo, _, cx| {
                if view.read_only {
                    return;
                }
                let lines = view.buffer.line_count();
                if view.buffer.redo() {
                    let line = view.buffer.line_of(view.buffer.cursor());
                    view.edited(lines, line, cx);
                }
            }))
            .on_action(cx.listener(|view, _: &Cancel, _: &mut Window, cx| {
                let cursor = view.buffer.cursor();
                view.go(cursor, false, cx)
            }))
            .on_action(cx.listener(|view, _: &super::find::OpenFind, window, cx| {
                view.open_search(super::find::FindMode::Find, window, cx)
            }))
            .on_action(
                cx.listener(|view, _: &super::find::OpenReplace, window, cx| {
                    view.open_search(super::find::FindMode::Replace, window, cx)
                }),
            )
            .on_action(
                cx.listener(|view, _: &super::find::FindNext, _, cx| view.step_match(false, cx)),
            )
            .on_action(
                cx.listener(|view, _: &super::find::FindPrevious, _, cx| view.step_match(true, cx)),
            )
    }
}

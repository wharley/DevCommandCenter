//! The rest of the Pierre editor's keymap (`command.js`): ⌘] and ⌘[ shift
//! the lines, ⌥↑/⌥↓ (and ⌃⌥P/⌃⌥N) move them, ⇧⌥↑/⇧⌥↓ copy them, ⌘Enter
//! opens a line under the caret's, ⌃K deletes to the line's end, ⌘/ and
//! ⇧⌥A comment, and ⌘D takes the word at the caret (without its next
//! match: one selection here, no multiple carets).

use gpui::{App, Context, InteractiveElement, KeyBinding, actions};

use super::FileView;
use super::comments::{comment_tokens, toggle_block_comment, toggle_comment};
use super::lines::{
    Change, Doc, blank_line, copy_lines, delete_to_line_end, indent_lines, move_lines,
};

actions!(
    code_editor_lines,
    [
        IndentMore,
        IndentLess,
        MoveLineUp,
        MoveLineDown,
        CopyLineUp,
        CopyLineDown,
        InsertBlankLine,
        DeleteToLineEnd,
        ToggleComment,
        ToggleBlockComment,
        SelectWord,
    ]
);

pub(crate) fn bind_keys(cx: &mut App) {
    let context = Some(super::keys::CONTEXT);
    cx.bind_keys([
        KeyBinding::new("cmd-]", IndentMore, context),
        KeyBinding::new("cmd-[", IndentLess, context),
        KeyBinding::new("alt-up", MoveLineUp, context),
        KeyBinding::new("alt-down", MoveLineDown, context),
        KeyBinding::new("ctrl-alt-p", MoveLineUp, context),
        KeyBinding::new("ctrl-alt-n", MoveLineDown, context),
        KeyBinding::new("shift-alt-up", CopyLineUp, context),
        KeyBinding::new("shift-alt-down", CopyLineDown, context),
        KeyBinding::new("cmd-enter", InsertBlankLine, context),
        KeyBinding::new("ctrl-k", DeleteToLineEnd, context),
        KeyBinding::new("cmd-/", ToggleComment, context),
        KeyBinding::new("shift-alt-a", ToggleBlockComment, context),
        KeyBinding::new("cmd-d", SelectWord, context),
    ]);
}

impl FileView {
    /// A command's edits as one undo step, its selection after them.
    pub(crate) fn apply_change(&mut self, change: Change, cx: &mut Context<Self>) {
        if change.edits.is_empty() || self.read_only {
            return;
        }
        let (range, text) = change.splice(self.buffer.text());
        let (lines, line) = (self.buffer.line_count(), self.buffer.line_of(range.start));
        self.buffer
            .replace_selecting(range, &text, change.selection, change.reversed);
        self.goal_column = None;
        self.edited(lines, line, cx);
    }

    /// Runs a rule over the text and the selection, then applies it.
    pub(crate) fn run_rule(
        &mut self,
        rule: impl FnOnce(&Doc, &std::ops::Range<usize>, bool) -> Option<Change>,
        cx: &mut Context<Self>,
    ) {
        let doc = Doc::new(self.buffer.text());
        let change = rule(&doc, &self.buffer.selection, self.buffer.reversed);
        if let Some(change) = change {
            self.apply_change(change, cx);
        }
    }

    fn shift_indent(&mut self, outdent: bool, cx: &mut Context<Self>) {
        let size = self.indentation.size.max(1);
        self.run_rule(
            |doc, selection, reversed| indent_lines(doc, selection, reversed, size, outdent),
            cx,
        );
    }

    fn comment(&mut self, block: bool, cx: &mut Context<Self>) {
        let tokens = comment_tokens(&self.path);
        self.run_rule(
            |doc, selection, reversed| {
                if block {
                    toggle_block_comment(doc, selection, reversed, tokens)
                } else {
                    toggle_comment(doc, selection, reversed, tokens)
                }
            },
            cx,
        );
    }

    pub(crate) fn register_line_commands(div: gpui::Div, cx: &mut Context<Self>) -> gpui::Div {
        div.on_action(cx.listener(|view, _: &IndentMore, _, cx| view.shift_indent(false, cx)))
            .on_action(cx.listener(|view, _: &IndentLess, _, cx| view.shift_indent(true, cx)))
            .on_action(cx.listener(|view, _: &MoveLineUp, _, cx| {
                view.run_rule(|doc, sel, rev| move_lines(doc, sel, rev, true), cx)
            }))
            .on_action(cx.listener(|view, _: &MoveLineDown, _, cx| {
                view.run_rule(|doc, sel, rev| move_lines(doc, sel, rev, false), cx)
            }))
            .on_action(cx.listener(|view, _: &CopyLineUp, _, cx| {
                view.run_rule(|doc, sel, rev| Some(copy_lines(doc, sel, rev, false)), cx)
            }))
            .on_action(cx.listener(|view, _: &CopyLineDown, _, cx| {
                view.run_rule(|doc, sel, rev| Some(copy_lines(doc, sel, rev, true)), cx)
            }))
            .on_action(cx.listener(|view, _: &InsertBlankLine, _, cx| {
                let caret = view.buffer.cursor();
                view.run_rule(|doc, _, _| Some(blank_line(doc, caret)), cx)
            }))
            .on_action(cx.listener(|view, _: &DeleteToLineEnd, _, cx| {
                view.run_rule(|doc, sel, _| delete_to_line_end(doc, sel), cx)
            }))
            .on_action(cx.listener(|view, _: &ToggleComment, _, cx| view.comment(false, cx)))
            .on_action(cx.listener(|view, _: &ToggleBlockComment, _, cx| view.comment(true, cx)))
            .on_action(cx.listener(|view, _: &SelectWord, _, cx| {
                if view.buffer.selection.is_empty() {
                    let word = view.buffer.word_at(view.buffer.cursor());
                    view.buffer.select(word);
                    view.moved(cx);
                }
            }))
    }
}

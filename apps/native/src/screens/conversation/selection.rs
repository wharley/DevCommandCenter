//! Selecting the thread's text (`markdown::selection`): a press on prose
//! starts a selection and takes the keyboard so ⌘C copies it; dragging
//! extends it, a double click takes a word and a triple the block. A right
//! click on it offers "Copiar". Anywhere else, a press drops it.

use gpui::{
    AnyElement, ClipboardItem, Context, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    Pixels, Point, Window,
};

use super::ConversationView;

impl ConversationView {
    pub(super) fn select_press(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let menu = self.copy_menu.take().is_some();
        if self.selection.press(event.position, event.click_count) {
            window.focus(&self.focus, cx);
            cx.notify();
        } else if self.selection.clear() || menu {
            cx.notify();
        }
    }

    pub(super) fn select_drag(
        &mut self,
        event: &MouseMoveEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.pressed_button == Some(MouseButton::Left) && self.selection.drag(event.position) {
            cx.notify();
        }
    }

    pub(super) fn select_release(
        &mut self,
        _: &MouseUpEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selection.dragging() {
            self.selection.release();
            cx.notify();
        }
    }

    /// A right click on the selection opens "Copiar" there.
    pub(super) fn select_menu(
        &mut self,
        event: &MouseDownEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selection.covers(event.position) {
            cx.stop_propagation();
            self.copy_menu = Some(event.position);
            cx.notify();
        }
    }

    /// ⌘C (or "Copiar"): the selected text to the clipboard.
    pub(super) fn copy_selection(&mut self, cx: &mut Context<Self>) {
        if let Some(text) = self.selection.text() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
        if self.copy_menu.take().is_some() {
            cx.notify();
        }
    }

    pub(super) fn copy_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let at: Point<Pixels> = self.copy_menu?;
        Some(crate::ui::copy_popup(
            "conversation-copy-selection",
            at,
            cx.listener(|view, _, _, cx| view.copy_selection(cx)),
            cx,
        ))
    }
}

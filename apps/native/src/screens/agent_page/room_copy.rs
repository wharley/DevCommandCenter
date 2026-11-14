//! Copying from the room: its lines' text is selected with the mouse as
//! the task conversation's is (`markdown::selection`: a press on prose
//! starts a selection and takes the keyboard, dragging extends it, a
//! double click takes a word and a triple the block), ⌘C or a right
//! click's "Copiar" copies it, and each message has its own copy button
//! beside it. The room drew the same text without any of it.

use gpui::{
    AnyElement, ClipboardItem, Context, FocusHandle, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, Point, SharedString, Window, div, prelude::*,
};

use super::room::TeamRoom;
use crate::markdown::selection::Selection;

/// The room's selection, the keyboard focus that makes ⌘C its own, and
/// where "Copiar" is open.
pub(crate) struct RoomCopy {
    pub selection: Selection,
    pub keys: FocusHandle,
    pub menu: Option<Point<Pixels>>,
}

impl RoomCopy {
    pub fn new(cx: &mut Context<TeamRoom>) -> Self {
        Self {
            selection: Selection::default(),
            keys: cx.focus_handle(),
            menu: None,
        }
    }
}

impl TeamRoom {
    pub(super) fn select_press(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let menu = self.copy.menu.take().is_some();
        if self.copy.selection.press(event.position, event.click_count) {
            window.focus(&self.copy.keys, cx);
            cx.notify();
        } else if self.copy.selection.clear() || menu {
            cx.notify();
        }
    }

    pub(super) fn select_drag(
        &mut self,
        event: &MouseMoveEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.pressed_button == Some(MouseButton::Left)
            && self.copy.selection.drag(event.position)
        {
            cx.notify();
        }
    }

    pub(super) fn select_release(
        &mut self,
        _: &MouseUpEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.copy.selection.dragging() {
            self.copy.selection.release();
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
        if self.copy.selection.covers(event.position) {
            cx.stop_propagation();
            self.copy.menu = Some(event.position);
            cx.notify();
        }
    }

    /// ⌘C (or "Copiar"): the selected text to the clipboard.
    pub(crate) fn copy_selection(&mut self, cx: &mut Context<Self>) {
        if let Some(text) = self.copy.selection.text() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
        if self.copy.menu.take().is_some() {
            cx.notify();
        }
    }

    pub(super) fn copy_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let at = self.copy.menu?;
        Some(crate::ui::copy_popup(
            "room-copy-selection",
            at,
            cx.listener(|room, _, _, cx| room.copy_selection(cx)),
            cx,
        ))
    }

    /// A message's own copy button: its whole text, as written.
    pub(super) fn copy_button(&self, key: &str, text: &str, cx: &Context<Self>) -> AnyElement {
        let id = format!("room-copy-{key}");
        let selector = id.clone();
        div()
            .debug_selector(move || selector.clone())
            .child(crate::screens::conversation::hover_actions::copy(
                SharedString::from(id),
                text.to_string(),
                cx,
            ))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use gpui::{Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, TestAppContext, point, px};

    use crate::data::agents::AgentState;
    use crate::data::team_room::{RoomLine, Speaker};
    use crate::screens::agent_page::room_list_tests::room_with_lines;

    fn line(key: &str, speaker: Speaker, text: &str) -> RoomLine {
        RoomLine {
            key: key.into(),
            agent_id: "dev".into(),
            session_id: "s-geral".into(),
            speaker,
            text: text.into(),
            at: "2026-11-10T09:00:00Z".into(),
            receipt: None,
            tasks: Vec::new(),
            steps: Default::default(),
        }
    }

    fn clipboard(cx: &mut gpui::VisualTestContext) -> Option<String> {
        cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()))
    }

    #[gpui::test]
    fn a_message_is_copied_whole_by_its_button_and_in_part_by_selecting(cx: &mut TestAppContext) {
        let lines = vec![
            line("l-0", Speaker::Person, "pedido de teste"),
            line("l-1", Speaker::Agent, "Feito agora mesmo."),
        ];
        let (_page, room, cx) = room_with_lines(cx, lines, AgentState::Idle);
        // Each message's own button copies it as written.
        let agent = cx.debug_bounds("room-copy-1").expect("the answer's copy");
        cx.simulate_mouse_move(agent.center(), None, Modifiers::none());
        cx.simulate_click(agent.center(), Modifiers::none());
        assert_eq!(clipboard(cx).as_deref(), Some("Feito agora mesmo."));
        let person = cx
            .debug_bounds("room-copy-room-said-0")
            .expect("the request's copy");
        cx.simulate_click(person.center(), Modifiers::none());
        assert_eq!(clipboard(cx).as_deref(), Some("pedido de teste"));
        // A double click on the answer's text takes a word; ⌘C copies it.
        let footer = cx.debug_bounds("room-footer-1").unwrap();
        let on_text = point(footer.left() + px(30.0), footer.top() - px(10.0));
        cx.simulate_event(MouseDownEvent {
            button: MouseButton::Left,
            position: on_text,
            modifiers: Modifiers::none(),
            click_count: 2,
            first_mouse: false,
        });
        cx.simulate_event(MouseUpEvent {
            button: MouseButton::Left,
            position: on_text,
            modifiers: Modifiers::none(),
            click_count: 2,
        });
        cx.run_until_parked();
        let selected = room.read_with(cx, |room, _| room.copy.selection.text());
        let word = selected.expect("a word of the answer is selected");
        assert!("Feito agora mesmo.".contains(word.trim()), "{word:?}");
        room.update(cx, |room, cx| room.copy_selection(cx));
        assert_eq!(clipboard(cx).as_deref(), Some(word.as_str()));
    }
}

//! The find bar under the header (`ThreadFindBar`): the field (Enter for
//! the next hit, Shift+Enter the one before, Esc to close), how many there
//! are with the excerpt of the current one, ↑/↓ and ×. The current hit's
//! row scrolls into view with the ring around it. It closes with another
//! conversation.

use gpui::{
    AnyElement, AppContext, Context, KeyDownEvent, Subscription, Window, div, prelude::*, px,
};

use super::ConversationView;
use super::find::{FindMatch, find_in_thread, step};
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::ui::text_input::{TextInput, TextInputEvent};
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, Tooltip, tabular_nums};

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

    fn reveal_find(&mut self, _cx: &mut Context<Self>) {
        let Some(id) = self.find_focus().map(str::to_string) else {
            return;
        };
        let index = self
            .data
            .as_ref()
            .and_then(|data| data.rows.iter().position(|row| row.id == id));
        if let Some(index) = index {
            // The list starts with the top room.
            self.list.scroll_to_reveal_item(index + 1);
        }
    }

    pub(super) fn find_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let find = self.find.as_ref()?;
        let colors = cx.theme().colors.clone();
        let radius = cx.theme().radius;
        let query = find.input.read(cx).text().trim().to_string();
        let total = find.matches.len();
        let status = if query.is_empty() {
            cx.t("conversation.find.hint").to_string()
        } else if total == 0 {
            cx.t("conversation.find.empty").to_string()
        } else {
            cx.t_with(
                "conversation.find.count",
                &Args::default()
                    .with("current", (find.active + 1).to_string())
                    .with("total", total.to_string()),
            )
            .to_string()
        };
        let snippet = find
            .matches
            .get(find.active)
            .map(|found| format!(" · {}", found.snippet))
            .unwrap_or_default();
        let nav =
            |id: &'static str, icon: &'static str, label: &'static str, cx: &mut Context<Self>| {
                let forward = icon == "chevron-down";
                Tooltip::text(
                    id,
                    Button::new(id)
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::IconSm)
                        .disabled(total == 0)
                        .on_click(cx.listener(move |view, _, _, cx| view.step_find(forward, cx)))
                        .child(Icon::new(icon).size(14.0)),
                    cx.t(label),
                )
            };
        Some(
            div()
                .debug_selector(|| "thread-find-bar".into())
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(8.0))
                .border_b_1()
                .border_color(colors.border.opacity(0.6))
                .bg(colors.background.opacity(0.95))
                .px(px(16.0))
                .py(px(6.0))
                .child(
                    div()
                        .id("thread-find-field")
                        .flex()
                        .h(px(28.0))
                        .w_full()
                        .max_w(px(320.0))
                        .items_center()
                        .rounded(radius.lg)
                        .border_1()
                        .border_color(colors.input)
                        .px(px(10.0))
                        .text_size(px(12.0))
                        .on_key_down(cx.listener(|view, event: &KeyDownEvent, _, cx| {
                            let keystroke = &event.keystroke;
                            if keystroke.key == "enter" && keystroke.modifiers.shift {
                                cx.stop_propagation();
                                view.step_find(false, cx);
                            }
                        }))
                        .child(find.input.clone()),
                )
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .truncate()
                        .text_size(px(11.0))
                        .font_features(tabular_nums())
                        .text_color(colors.muted_foreground)
                        .child(format!("{status}{snippet}")),
                )
                .child(nav(
                    "thread-find-previous",
                    "chevron-up",
                    "conversation.find.previous",
                    cx,
                ))
                .child(nav(
                    "thread-find-next",
                    "chevron-down",
                    "conversation.find.next",
                    cx,
                ))
                .child(Tooltip::text(
                    "thread-find-close-tip",
                    Button::new("thread-find-close")
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::IconSm)
                        .on_click(cx.listener(|view, _, _, cx| view.close_find(cx)))
                        .child(Icon::new("x").size(14.0)),
                    cx.t("conversation.find.close"),
                ))
                .into_any_element(),
        )
    }
}

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

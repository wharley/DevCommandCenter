//! The list as `FileQuickOpen` draws it: each file with the `FileCode`
//! icon, its name in medium and the folder in mono on the right, cut at
//! its start (`dir="rtl"`).

use gpui::{Context, FontWeight, KeyDownEvent, Render, Window, div, prelude::*, px};

use super::{QuickOpen, QuickOpenEvent};
use crate::data::quick_open::{basename, dirname};
use crate::i18n::Translate;
use crate::theme::{ActiveTheme, fonts};
use crate::ui::Icon;
use crate::ui::command::{
    command_dialog, command_empty, command_field, command_item, command_list_height, command_note,
};

impl Render for QuickOpen {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors.clone();
        let items = self.results.iter().enumerate().map(|(index, path)| {
            let dir = dirname(path);
            command_item(("quick-open-item", index), index == self.highlighted, cx)
                .debug_selector(move || format!("quick-open-item-{index}"))
                .items_center()
                .gap(px(10.0))
                .on_mouse_move(cx.listener(move |open, _, _, cx| open.hover(index, cx)))
                .on_click(cx.listener(move |open, _, _, cx| open.pick(index, cx)))
                .child(
                    div().flex_none().child(
                        Icon::new("file-code-corner")
                            .size(16.0)
                            .stroke(1.8)
                            .color(colors.muted_foreground),
                    ),
                )
                .child(
                    div()
                        .flex_none()
                        .truncate()
                        .font_weight(FontWeight::MEDIUM)
                        .child(basename(path).to_string()),
                )
                .when(!dir.is_empty(), |item| {
                    item.child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            // `dir="rtl"`: a long folder keeps its end.
                            .text_ellipsis_start()
                            .text_right()
                            .font_family(fonts::MONO)
                            .text_size(px(11.0))
                            .text_color(colors.muted_foreground)
                            .child(dir.to_string()),
                    )
                })
        });
        let status = if self.loading && self.results.is_empty() {
            Some(command_note(cx.t("quickOpen.loading"), cx).into_any_element())
        } else if self.results.is_empty() {
            Some(command_empty(cx.t("quickOpen.empty")).into_any_element())
        } else {
            None
        };
        let close = cx.entity().downgrade();
        let focus = gpui::Focusable::focus_handle(self.input.read(cx), cx);
        command_dialog(
            "quick-open-dialog",
            focus,
            move |_, cx| {
                let _ = close.update(cx, |_, cx| cx.emit(QuickOpenEvent::Closed));
            },
            window,
            cx,
        )
        .child(
            command_field("quick-open-field", self.input.clone(), cx).on_key_down(cx.listener(
                |open, event: &KeyDownEvent, _, cx| {
                    match event.keystroke.key.as_str() {
                        "down" => open.move_highlight(true, cx),
                        "up" => open.move_highlight(false, cx),
                        _ => return,
                    }
                    cx.stop_propagation();
                },
            )),
        )
        .child(
            div()
                .id("quick-open-list")
                .max_h(px(command_list_height(window)))
                .overflow_y_scroll()
                .track_scroll(&self.scroll)
                .p(px(4.0))
                .children(status)
                .children(items),
        )
    }
}

//! One row of a menu: icon, label (with its second line and right mark),
//! the submenu's chevron; hover highlights it, a click picks it.

use gpui::{AnyElement, App, ClickEvent, ElementId, SharedString, div, prelude::*, px};

use super::{ItemStyle, MenuState, OnSelect, set_open};
use crate::theme::ActiveTheme;
use crate::ui::Icon;

/// An item's text: its label, an optional second line and right mark.
pub(super) struct Label {
    pub(super) text: SharedString,
    pub(super) detail: Option<SharedString>,
    pub(super) trailing: Option<SharedString>,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn row(
    root: &gpui::Entity<MenuState>,
    state: &gpui::Entity<MenuState>,
    index: usize,
    highlighted: bool,
    icon: Option<Icon>,
    label: Label,
    submenu: bool,
    disabled: bool,
    destructive: bool,
    style: ItemStyle,
    on_select: Option<OnSelect>,
    cx: &App,
) -> AnyElement {
    let colors = &cx.theme().colors;
    let radius = cx.theme().radius;
    let hover_state = state.clone();
    let click_state = state.clone();
    let click_root = root.clone();
    div()
        .id(ElementId::NamedInteger("menu-item".into(), index as u64))
        .flex()
        .items_center()
        .gap(px(style.gap))
        .px(px(6.0))
        .py(px(4.0))
        .rounded(radius.md)
        .text_size(px(style.text_size))
        .line_height(px(20.0))
        .when(highlighted, |row| {
            row.bg(colors.accent).text_color(colors.accent_foreground)
        })
        .when(destructive, |row| row.text_color(colors.destructive))
        .when(disabled, |row| row.opacity(0.5))
        .when(!disabled, |row| {
            row.cursor_pointer()
                .on_hover(move |hovered, _, cx| {
                    if *hovered {
                        hover_state.update(cx, |state, cx| {
                            state.highlighted = Some(index);
                            state.open_submenu = submenu.then_some(index);
                            cx.notify();
                        });
                    }
                })
                .on_click(move |_: &ClickEvent, window, cx| {
                    cx.stop_propagation();
                    match &on_select {
                        Some(on_select) => {
                            set_open(&click_root, false, None, window, cx);
                            on_select(window, cx);
                        }
                        None => click_state.update(cx, |state, cx| {
                            state.open_submenu = Some(index);
                            cx.notify();
                        }),
                    }
                })
        })
        .when(label.detail.is_some(), |row| row.items_start())
        .children(icon.map(|icon| {
            div()
                .flex_shrink_0()
                .when(label.detail.is_some(), |slot| slot.mt(px(3.0)))
                .child(icon)
        }))
        .child(match label.detail {
            None => div().child(label.text),
            Some(detail) => div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .truncate()
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .child(label.text),
                )
                .child(
                    div()
                        .truncate()
                        .text_size(px(11.0))
                        .line_height(px(16.0))
                        .text_color(colors.muted_foreground)
                        .child(detail),
                ),
        })
        .children(
            label
                .trailing
                .map(|mark| div().ml_auto().flex_shrink_0().child(mark)),
        )
        .when(submenu, |row| {
            row.child(div().ml_auto().child(Icon::new("chevron-right")))
        })
        .into_any_element()
}

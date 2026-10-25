//! The two ways into the card, as `code-review-controls.css` draws them:
//! the "+" on a line number (`[data-utility-button]`, 1lh square) and the
//! "Enviar ao agente ↗" pill (`.dcc-snippet-trigger`). The views that
//! show them add what pressing them does.

use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    App, Bounds, BoxShadow, Div, Pixels, SharedString, Stateful, canvas, div, point, prelude::*, px,
};

use crate::i18n::Translate;
use crate::theme::ActiveTheme;
use crate::theme::color::mix_lab;
use crate::ui::Icon;

/// `--dcc-shadow-card`
pub fn card_shadow() -> Vec<BoxShadow> {
    let shadow = |alpha: f32, y: f32, blur: f32| BoxShadow {
        color: gpui::black().opacity(alpha),
        offset: point(px(0.0), px(y)),
        blur_radius: px(blur),
        spread_radius: px(0.0),
        inset: false,
    };
    vec![shadow(0.025, 2.0, 3.0), shadow(0.04, 8.0, 24.0)]
}

/// The "+": 20px, its left at `left` (the number column's right edge less
/// 1ch, so it reaches `1lh - 1ch` into the code); shown while `group` is
/// hovered unless `pinned`.
pub fn plus_button(
    id: SharedString,
    left: f32,
    pinned: bool,
    group: &'static str,
    cx: &App,
) -> Stateful<Div> {
    let colors = &cx.theme().colors;
    let calm = colors.accent_calm;
    let background = colors.background;
    let selector = id.to_string();
    div()
        .id(id)
        .debug_selector(move || selector)
        .absolute()
        .top_0()
        .left(px(left))
        .flex()
        .size(px(20.0))
        .items_center()
        .justify_center()
        .rounded(px(7.0))
        .border_1()
        .border_color(mix_lab(calm, 0.35, colors.border))
        .bg(colors.popover)
        .text_color(calm)
        .shadow(vec![BoxShadow {
            color: gpui::black().opacity(0x18 as f32 / 255.0),
            offset: point(px(0.0), px(2.0)),
            blur_radius: px(6.0),
            spread_radius: px(0.0),
            inset: false,
        }])
        .cursor_pointer()
        .hover(move |style| style.bg(calm).text_color(background))
        .when(!pinned, |plus| {
            plus.opacity(0.0)
                .group_hover(group, |style| style.opacity(1.0))
        })
        .child(Icon::new("diffs-icon-plus").size(16.0))
}

/// The pill; its bounds go to `record` each frame, for the card's anchor.
pub fn snippet_trigger(
    id: &'static str,
    icon: &'static str,
    record: Rc<Cell<Bounds<Pixels>>>,
    cx: &App,
) -> Stateful<Div> {
    let colors = &cx.theme().colors;
    let calm = colors.accent_calm;
    let rest = colors.popover;
    div()
        .id(id)
        .debug_selector(move || id.to_string())
        .relative()
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .gap(px(7.0))
        .px(px(12.0))
        .py(px(8.0))
        .rounded_full()
        .border_1()
        .border_color(mix_lab(calm, 0.32, colors.border))
        .bg(rest)
        .text_color(colors.foreground)
        .shadow(card_shadow())
        .font_family(cx.theme().fonts.sans.clone())
        .text_size(px(11.0))
        .line_height(gpui::relative(1.4))
        .font_weight(gpui::FontWeight::MEDIUM)
        .whitespace_nowrap()
        .cursor_pointer()
        .hover(move |style| style.bg(mix_lab(calm, 0.10, rest)))
        .child(
            canvas(move |bounds, _, _| record.set(bounds), |_, _, _, _| {})
                .absolute()
                .inset_0(),
        )
        .child(div().text_color(calm).child(Icon::new(icon).size(14.0)))
        .child(cx.t("diffAnnotate.sendToAgent"))
}

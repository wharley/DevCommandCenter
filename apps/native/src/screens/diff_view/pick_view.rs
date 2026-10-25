//! How a pick looks: the picked lines toward the selection color
//! (`[data-selected-line]`), the "+" at the number column's right edge
//! (`[data-utility-button]` as `code-review-controls.css` restyles it) and
//! the "Enviar ao agente" pill 12px from the diff's top right
//! (`.dcc-snippet-trigger`).

use gpui::{
    AnyElement, Context, Div, Hsla, MouseButton, MouseDownEvent, MouseMoveEvent, SharedString, div,
    point, prelude::*, px,
};

use super::DiffView;
use super::palette::DiffPalette;
use super::pick::LinePoint;
use crate::screens::annotation::trigger;
use crate::theme::color::mix_lab;

/// A line cell's part in the pick.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PickCell {
    pub point: LinePoint,
    pub selected: bool,
    /// `None` hidden, `Some(false)` shown on hover, `Some(true)` always.
    pub plus: Option<bool>,
}

impl DiffView {
    /// What a line cell shows of the pick; `None` when the diff doesn't pick.
    pub(crate) fn pick_cell(&self, point: Option<LinePoint>) -> Option<PickCell> {
        let picking = self.picking.as_ref()?;
        let point = point?;
        let pick = picking.pick;
        let selected = pick.is_some_and(|pick| pick.covers(point.row, point.column, self.split));
        let plus = match pick {
            Some(pick) => {
                let bottom = pick.bottom();
                (bottom.row == point.row && bottom.column == point.column).then_some(true)
            }
            None => Some(false),
        };
        Some(PickCell {
            point,
            selected,
            plus,
        })
    }
}

/// `--diffs-computed-selected-line-bg` and the number column's, with the
/// number's color: each toward `--diffs-selection-base` (the theme's
/// modified color).
pub(crate) fn selected_colors(
    palette: &DiffPalette,
    line_bg: Hsla,
    gutter_bg: Hsla,
) -> (Hsla, Hsla, Hsla) {
    let base = palette.selection_base;
    let mixer = if palette.dark {
        gpui::white()
    } else {
        gpui::black()
    };
    let (line, number, fg) = if palette.dark {
        (0.75, 0.60, 0.75)
    } else {
        (0.82, 0.75, 0.65)
    };
    (
        mix_lab(line_bg, line, base),
        mix_lab(gutter_bg, number, base),
        mix_lab(base, fg, mixer),
    )
}

/// A press on the number picks; the pointer over the row drags the pick.
pub(crate) fn pick_listeners(
    number: Div,
    row: Div,
    point: LinePoint,
    cx: &mut Context<DiffView>,
) -> (Div, Div) {
    let number = number.cursor_pointer().on_mouse_down(
        MouseButton::Left,
        cx.listener(move |view, event: &MouseDownEvent, _, cx| {
            cx.stop_propagation();
            view.press_number(point, event.modifiers.shift, cx);
        }),
    );
    let row = row.on_mouse_move(cx.listener(move |view, event: &MouseMoveEvent, _, cx| {
        if event.pressed_button == Some(MouseButton::Left) {
            view.drag_over(point, cx);
        }
    }));
    (number, row)
}

/// The "+" on a line's number: pressed, it picks from the pick's top (or
/// this line) and the drag goes on from there.
pub(crate) fn plus_button(
    at: LinePoint,
    pinned: bool,
    group: &'static str,
    number_width: f32,
    cell: f32,
    cx: &mut Context<DiffView>,
) -> AnyElement {
    let id = SharedString::from(format!("diff-plus-{}-{}", at.row, at.column as u8));
    trigger::plus_button(id, number_width - cell, pinned, group, cx)
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                view.press_plus(at, cx);
            }),
        )
        .into_any_element()
}

/// "Enviar ao agente ↗", 12px from the top right while lines are picked.
pub(crate) fn pill(view: &DiffView, cx: &mut Context<DiffView>) -> Option<AnyElement> {
    let picking = view.picking.as_ref()?;
    picking.pick?;
    let record = picking.pill.clone();
    Some(
        div()
            .absolute()
            .top(px(12.0))
            .right(px(12.0))
            .child(
                trigger::snippet_trigger("diff-annotate-pill", "message-square", record, cx)
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(|view, _, _, cx| {
                        let Some(bounds) = view.picking.as_ref().map(|picking| picking.pill.get())
                        else {
                            return;
                        };
                        view.annotate(point(bounds.left(), bounds.bottom()), cx);
                    })),
            )
            .into_any_element(),
    )
}

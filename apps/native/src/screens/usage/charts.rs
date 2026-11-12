//! Stacked bars, for the days and for the hours: one column per entry,
//! its parts stacked from the bottom in the legend's order, the tallest
//! column reaching the top; dashed guides at a quarter, half and three
//! quarters.

use gpui::{Bounds, Hsla, PathBuilder, Window, canvas, div, fill, point, prelude::*, px, size};

/// A column's parts, bottom first.
pub(super) type Column = Vec<(Hsla, f64)>;

/// The tallest column's total.
pub(super) fn tallest(columns: &[Column]) -> f64 {
    columns
        .iter()
        .map(|column| column.iter().map(|(_, value)| value).sum::<f64>())
        .fold(0.0, f64::max)
}

/// Each part's height in pixels for a chart `height` tall: (column, part
/// top from the chart's bottom, part height).
pub(super) fn layout(columns: &[Column], height: f32) -> Vec<Vec<(f32, f32)>> {
    let top = tallest(columns);
    columns
        .iter()
        .map(|column| {
            let mut bottom = 0.0;
            column
                .iter()
                .map(|(_, value)| {
                    let part = if top > 0.0 {
                        (*value / top) as f32 * height
                    } else {
                        0.0
                    };
                    let placed = (bottom, part);
                    bottom += part;
                    placed
                })
                .collect()
        })
        .collect()
}

fn paint(columns: &[Column], guide: Hsla, bounds: Bounds<gpui::Pixels>, window: &mut Window) {
    let width = f32::from(bounds.size.width);
    let height = f32::from(bounds.size.height);
    for ratio in [0.25, 0.5, 0.75] {
        let y = bounds.origin.y + px(height * ratio);
        let mut line = PathBuilder::stroke(px(1.0)).dash_array(&[px(3.0), px(5.0)]);
        line.move_to(point(bounds.origin.x, y));
        line.line_to(point(bounds.origin.x + px(width), y));
        if let Ok(path) = line.build() {
            window.paint_path(path, guide);
        }
    }
    if columns.is_empty() {
        return;
    }
    let slot = width / columns.len() as f32;
    let bar = (slot * 0.72).clamp(1.0, 28.0);
    for (index, parts) in layout(columns, height).iter().enumerate() {
        let left = bounds.origin.x + px(slot * index as f32 + (slot - bar) / 2.0);
        for ((color, _), (bottom, part)) in columns[index].iter().zip(parts) {
            if *part <= 0.0 {
                continue;
            }
            let top = bounds.origin.y + px(height - bottom - part);
            window.paint_quad(fill(
                Bounds::new(point(left, top), size(px(bar), px(part.max(1.0)))),
                *color,
            ));
        }
    }
}

/// The bars stretched to the card's width, `height` tall.
pub(super) fn stacked_bars(columns: Vec<Column>, guide: Hsla, height: f32) -> impl IntoElement {
    div().h(px(height)).w_full().child(
        canvas(
            |_, _, _| (),
            move |bounds, _, window, _| paint(&columns, guide, bounds, window),
        )
        .size_full(),
    )
}

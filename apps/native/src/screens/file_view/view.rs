//! The rows: the number (`padding-inline: 1ch` past a 2ch start, at least
//! 3 digits) and its 2px edge, then the code at 1ch, 20px a line.

use gpui::{
    AnyElement, Context, Render, ScrollWheelEvent, SharedString, StyledText, Window, div, font,
    list, prelude::*, px,
};

use super::FileView;
use crate::screens::diff_view::{DiffPalette, styled_runs};

/// The diff's font (`--diffs-font-family`).
const FONT: &str = "Monaco";
const FONT_SIZE: f32 = 13.0;
const LINE: f32 = 20.0;
const GAP: f32 = 8.0;
const GROUP: &str = "file-line";

fn row(
    view: &FileView,
    index: usize,
    palette: &DiffPalette,
    cell: f32,
    number_width: f32,
) -> AnyElement {
    let text = view.lines.get(index).cloned().unwrap_or_default();
    let spans = view.spans.get(index).map(Vec::as_slice).unwrap_or_default();
    let runs = styled_runs(text.len(), spans, &[], palette.added_word);
    let styled = StyledText::new(SharedString::from(text)).with_highlights(runs);
    let hover = palette.hover;
    div()
        .group(GROUP)
        .flex()
        .h(px(LINE))
        .w_full()
        .child(
            div()
                .flex()
                .flex_shrink_0()
                .justify_end()
                .w(px(number_width))
                .pr(px(cell))
                .bg(palette.background)
                .text_color(palette.number)
                .group_hover(GROUP, move |style| style.bg(hover))
                .child((index + 1).to_string()),
        )
        .child(
            div()
                .w(px(2.0))
                .flex_shrink_0()
                .h_full()
                .bg(palette.background),
        )
        .child(
            div()
                .relative()
                .flex_1()
                .min_w_0()
                .h_full()
                .overflow_hidden()
                .group_hover(GROUP, move |style| style.bg(hover))
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .left(px(cell - view.scroll_x))
                        .whitespace_nowrap()
                        .text_color(palette.foreground)
                        .child(styled),
                ),
        )
        .into_any_element()
}

impl Render for FileView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = DiffPalette::new(self.dark);
        let mono = font(FONT);
        let cell = window
            .text_system()
            .resolve_font(&mono)
            .pipe(|id| window.text_system().advance(id, px(FONT_SIZE), '0').ok())
            .map(|size| f32::from(size.width))
            .unwrap_or(7.8);
        let digits = self.lines.len().max(1).to_string().len().max(3) as f32;
        let number_width = (digits + 3.0) * cell;
        let longest = self
            .lines
            .iter()
            .map(|line| line.chars().count())
            .max()
            .unwrap_or(0) as f32
            * cell;
        let entity = cx.entity();
        let rows = list(self.list.clone(), move |index, _, cx| {
            entity.update(cx, |view, _| {
                let palette = DiffPalette::new(view.dark);
                row(view, index, &palette, cell, number_width)
            })
        })
        .size_full();
        div()
            .id("file-view")
            .debug_selector(|| "file-view".into())
            .size_full()
            .overflow_hidden()
            .bg(palette.background)
            .text_color(palette.foreground)
            .font_family(FONT)
            .text_size(px(FONT_SIZE))
            .line_height(px(LINE))
            .pt(px(GAP))
            .on_scroll_wheel(
                cx.listener(move |view, event: &ScrollWheelEvent, window, cx| {
                    let delta = event.delta.pixel_delta(window.line_height());
                    let dx = f32::from(delta.x);
                    if dx.abs() > f32::from(delta.y).abs() {
                        view.scroll_x = (view.scroll_x - dx).clamp(0.0, longest.max(0.0));
                        cx.stop_propagation();
                        cx.notify();
                    }
                }),
            )
            .child(rows)
    }
}

/// `value.pipe(f)`: keeps the font lookup a single expression.
trait Pipe: Sized {
    fn pipe<T>(self, f: impl FnOnce(Self) -> T) -> T {
        f(self)
    }
}

impl<T> Pipe for T {}

//! The rows: 20px lines (number, a 2px edge, `+`/`-` and the code) and
//! 32px separators ("N unmodified lines" with the expand mark), in the
//! diff's monospace at 13px.

use std::ops::Range;

use gpui::{
    AnyElement, ClickEvent, Context, FontStyle, FontWeight, HighlightStyle, Hsla, Render,
    ScrollWheelEvent, SharedString, StyledText, Window, div, font, list, prelude::*, px,
};

use super::{Computed, DiffPalette, DiffView, State};
use crate::data::diff::{GapPlace, LineKind, Row, unmodified_lines};
use crate::data::highlight::Span;
use crate::ui::Icon;

/// `--diffs-font-family` falls back to "SF Mono", which macOS keeps to its own
/// apps; WebKit lands on Monaco.
const FONT: &str = "Monaco";
const FONT_SIZE: f32 = 13.0;
const LINE: f32 = 20.0;
const SEPARATOR: f32 = 32.0;
/// `--diffs-gap-block` / `--diffs-gap-inline`
const GAP: f32 = 8.0;
const LINE_GROUP: &str = "diff-line";

fn span_color(rgba: u32) -> Hsla {
    gpui::Rgba {
        r: ((rgba >> 24) & 0xff) as f32 / 255.0,
        g: ((rgba >> 16) & 0xff) as f32 / 255.0,
        b: ((rgba >> 8) & 0xff) as f32 / 255.0,
        a: (rgba & 0xff) as f32 / 255.0,
    }
    .into()
}

/// The syntax runs cut where the changed words start and end, so a word
/// keeps its color over the emphasis.
pub(crate) fn styled_runs(
    len: usize,
    spans: &[Span],
    emphasis: &[Range<usize>],
    emphasis_background: Hsla,
) -> Vec<(Range<usize>, HighlightStyle)> {
    let mut cuts: Vec<usize> = vec![0, len];
    for span in spans {
        cuts.extend([span.range.start.min(len), span.range.end.min(len)]);
    }
    for range in emphasis {
        cuts.extend([range.start.min(len), range.end.min(len)]);
    }
    cuts.sort_unstable();
    cuts.dedup();
    let mut runs = Vec::new();
    for pair in cuts.windows(2) {
        let range = pair[0]..pair[1];
        if range.is_empty() {
            continue;
        }
        let span = spans
            .iter()
            .find(|span| span.range.start <= range.start && range.end <= span.range.end);
        let emphasized = emphasis
            .iter()
            .any(|word| word.start <= range.start && range.end <= word.end);
        if span.is_none() && !emphasized {
            continue;
        }
        runs.push((
            range,
            HighlightStyle {
                color: span.map(|span| span_color(span.color)),
                font_weight: span.filter(|span| span.bold).map(|_| FontWeight::BOLD),
                font_style: span.filter(|span| span.italic).map(|_| FontStyle::Italic),
                background_color: emphasized.then_some(emphasis_background),
                ..HighlightStyle::default()
            },
        ));
    }
    runs
}

/// The number column: `padding-left: 2ch`, `padding-right: 1ch` and at
/// least 3 digits wide.
pub(crate) fn number_width(computed: &Computed, cell: f32) -> f32 {
    let digits = computed.last_number.max(1).to_string().len().max(3) as f32;
    (digits + 3.0) * cell
}

fn line_row(
    view: &DiffView,
    computed: &Computed,
    index: usize,
    palette: &DiffPalette,
    cell: f32,
    number_width: f32,
) -> AnyElement {
    let line = &computed.diff.lines[index];
    let (spans, number) = match line.kind {
        LineKind::Deletion => (
            line.old.and_then(|old| computed.old_spans.get(old - 1)),
            line.old,
        ),
        _ => (
            line.new.and_then(|new| computed.new_spans.get(new - 1)),
            line.new,
        ),
    };
    let (line_bg, gutter_bg, number_color, mark) = match line.kind {
        LineKind::Context => (palette.background, palette.background, palette.number, None),
        LineKind::Addition => (
            palette.added_line,
            palette.added_gutter,
            palette.added,
            Some(("+", palette.added)),
        ),
        LineKind::Deletion => (
            palette.deleted_line,
            palette.deleted_gutter,
            palette.deleted,
            Some(("-", palette.deleted)),
        ),
    };
    let (line_hover, gutter_hover) = match line.kind {
        LineKind::Context => (palette.hover, palette.hover),
        LineKind::Addition => (palette.added_line_hover, palette.added_gutter_hover),
        LineKind::Deletion => (palette.deleted_line_hover, palette.deleted_gutter_hover),
    };
    let word = match line.kind {
        LineKind::Deletion => palette.deleted_word,
        _ => palette.added_word,
    };
    let runs = styled_runs(
        line.text.len(),
        spans.map(Vec::as_slice).unwrap_or_default(),
        &line.emphasis,
        word,
    );
    let text = StyledText::new(SharedString::from(line.text.clone())).with_highlights(runs);
    let left = -view.scroll_x;
    div()
        .group(LINE_GROUP)
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
                .bg(gutter_bg)
                .group_hover(LINE_GROUP, move |style| style.bg(gutter_hover))
                .text_color(number_color)
                .children(number.map(|number| number.to_string())),
        )
        // The gutter's `border-right: 2px solid var(--diffs-bg)`.
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
                .bg(line_bg)
                .group_hover(LINE_GROUP, move |style| style.bg(line_hover))
                .children(mark.map(|(mark, color)| {
                    div()
                        .absolute()
                        .top_0()
                        .left(px(left))
                        .w(px(cell))
                        .text_color(color)
                        .child(mark)
                }))
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .left(px(2.0 * cell + left))
                        .whitespace_nowrap()
                        .text_color(palette.foreground)
                        .child(text),
                ),
        )
        .into_any_element()
}

/// `createSeparator`: the expand button (pointing up before the first
/// change, both ways between two) and "N unmodified lines"; either opens
/// 20 more lines, Shift the whole run.
fn separator_row(
    gap: usize,
    hidden: usize,
    place: GapPlace,
    palette: &DiffPalette,
    cell: f32,
    cx: &mut Context<DiffView>,
) -> AnyElement {
    let number = palette.number;
    let foreground = palette.foreground;
    let (icon, flipped) = match place {
        GapPlace::First => ("diffs-expand", true),
        GapPlace::Middle => ("diffs-expand-all", false),
        GapPlace::Last => ("diffs-expand", false),
    };
    let entity = cx.entity().downgrade();
    let on_click = move || {
        let entity = entity.clone();
        move |event: &ClickEvent, _: &mut Window, cx: &mut gpui::App| {
            let shift = event.modifiers().shift;
            let _ = entity.update(cx, |view, cx| view.expand(gap, shift, cx));
        }
    };
    // Padding instead of `margin-block`: the list measures an item's box.
    div()
        .when(place != GapPlace::First, |row| row.pt(px(GAP)))
        .when(place != GapPlace::Last, |row| row.pb(px(GAP)))
        .px(px(GAP))
        .child(
            div()
                .flex()
                .h(px(SEPARATOR))
                .child(
                    div()
                        .id(("diff-expand", gap))
                        .debug_selector(move || format!("diff-expand-{gap}"))
                        .flex()
                        .w(px(32.0))
                        .flex_shrink_0()
                        .items_center()
                        .justify_center()
                        .rounded_l(px(6.0))
                        .bg(palette.separator)
                        .border_r_2()
                        .border_color(palette.background)
                        .text_color(number)
                        .cursor_pointer()
                        .hover(move |style| style.text_color(foreground))
                        .on_click(on_click())
                        // `[data-expand-down] [data-icon] { transform: scaleY(-1) }`
                        .child(Icon::new(icon).rotate(if flipped { 180.0 } else { 0.0 })),
                )
                .child(
                    div()
                        .id(("diff-unmodified", gap))
                        .flex()
                        .flex_1()
                        .min_w_0()
                        .items_center()
                        .rounded_r(px(6.0))
                        .bg(palette.separator)
                        .px(px(cell))
                        .text_color(number)
                        .cursor_pointer()
                        .hover(|style| style.underline())
                        .on_click(on_click())
                        .child(div().truncate().child(unmodified_lines(hidden))),
                ),
        )
        .into_any_element()
}

impl Render for DiffView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = DiffPalette::new(self.dark);
        self.seen_top = Some(self.list_top());
        let mono = font(FONT);
        let cell = window
            .text_system()
            .resolve_font(&mono)
            .pipe(|id| window.text_system().advance(id, px(FONT_SIZE), '0').ok())
            .map(|size| f32::from(size.width))
            .unwrap_or(7.8);
        let body: AnyElement = match &self.state {
            State::Loading => div().size_full().into_any_element(),
            State::Ready(computed) => {
                let number_width = number_width(computed, cell);
                let entity = cx.entity();
                list(self.list.clone(), move |index, _, cx| {
                    entity.update(cx, |view, cx| {
                        let palette = DiffPalette::new(view.dark);
                        let State::Ready(computed) = &view.state else {
                            return div().into_any_element();
                        };
                        match view.rows.get(index).cloned() {
                            Some(Row::Line(line)) => {
                                line_row(view, computed, line, &palette, cell, number_width)
                            }
                            Some(Row::Separator { gap, hidden, place }) => {
                                separator_row(gap, hidden, place, &palette, cell, cx)
                            }
                            None => div().into_any_element(),
                        }
                    })
                })
                .size_full()
                .into_any_element()
            }
        };
        let longest = match &self.state {
            State::Ready(computed) => computed
                .diff
                .lines
                .iter()
                .map(|line| line.text.chars().count())
                .max()
                .unwrap_or(0),
            State::Loading => 0,
        } as f32
            * cell;
        div()
            .id("diff-view")
            .debug_selector(|| "diff-view".into())
            .size_full()
            .overflow_hidden()
            .bg(palette.background)
            .text_color(palette.foreground)
            .font_family(FONT)
            .text_size(px(FONT_SIZE))
            .line_height(px(LINE))
            // `padding-bottom: max(0, gap - scrollbar gutter)`
            .pt(px(GAP))
            .pb(px(GAP - 6.0))
            .on_scroll_wheel(
                cx.listener(move |view, event: &ScrollWheelEvent, window, cx| {
                    let delta = event.delta.pixel_delta(window.line_height());
                    let dx = f32::from(delta.x);
                    if dx.abs() > f32::from(delta.y).abs() {
                        view.scroll_x = (view.scroll_x - dx).clamp(0.0, longest.max(0.0));
                        cx.stop_propagation();
                        cx.notify();
                        return;
                    }
                    // The list moved first (children before parents): while the
                    // diff itself scrolls the page around it stays; at its ends
                    // the wheel goes on to the page, as a browser chains it.
                    let top = view.list_top();
                    if view.seen_top != Some(top) {
                        cx.stop_propagation();
                    }
                    view.seen_top = Some(top);
                }),
            )
            .child(body)
    }
}

/// `value.pipe(f)`: keeps the font lookup a single expression.
trait Pipe: Sized {
    fn pipe<T>(self, f: impl FnOnce(Self) -> T) -> T {
        f(self)
    }
}

impl<T> Pipe for T {}

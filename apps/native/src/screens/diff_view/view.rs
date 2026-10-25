//! The rows: 20px lines (number, a 2px edge, `+`/`-` and the code) and
//! 32px separators ("N unmodified lines" with the expand mark), in the
//! diff's monospace at 13px.

use std::ops::Range;

use gpui::{
    AnyElement, ClickEvent, ContentMask, Context, FontStyle, FontWeight, HighlightStyle, Hsla,
    Path, Render, ScrollWheelEvent, SharedString, StyledText, Window, canvas, div, font, list,
    point, prelude::*, px,
};

use super::{Computed, DiffPalette, DiffView, State};
use crate::data::diff::{GapPlace, LineKind, Row, SplitRow, unmodified_lines};
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

/// Which side of the diff a cell shows.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Side {
    /// The unified view: removed lines by their old number, the rest by the new.
    Unified,
    Old,
    New,
}

impl Side {
    fn group(self) -> &'static str {
        match self {
            Self::Unified => LINE_GROUP,
            Self::Old => "diff-line-old",
            Self::New => "diff-line-new",
        }
    }
}

/// `[data-content-buffer]`: a side left empty, striped at -45° every 8px
/// (`repeating-linear-gradient`, 1.4px bands, the tile 5px in).
fn buffer_stripes(color: Hsla) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let (width, height) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
            let origin = bounds.origin;
            window.with_content_mask(Some(ContentMask { bounds }), |window| {
                let mut start = 11.0 - 8.0 * ((height + 11.0) / 8.0).ceil();
                while start < width + height {
                    let (a, b) = (start, start + 2.0);
                    let at = |x: f32, y: f32| point(origin.x + px(x), origin.y + px(y));
                    let mut band = Path::new(at(a, 0.0));
                    band.line_to(at(b, 0.0));
                    band.line_to(at(b - height, height));
                    band.line_to(at(a - height, height));
                    band.line_to(at(a, 0.0));
                    window.paint_path(band, color);
                    start += 8.0;
                }
            });
        },
    )
    .size_full()
}

/// One side of a row: the number, the gutter's 2px edge, then `+`/`-` and
/// the code scrolled sideways by `scroll`; `None` is a side left empty.
fn line_cells(
    computed: &Computed,
    index: Option<usize>,
    side: Side,
    palette: &DiffPalette,
    cell: f32,
    number_width: f32,
    scroll: f32,
) -> gpui::Div {
    let edge = div()
        .w(px(2.0))
        .flex_shrink_0()
        .h_full()
        .bg(palette.background);
    let Some(line) = index.map(|index| &computed.diff.lines[index]) else {
        return div()
            .flex()
            .h(px(LINE))
            .w_full()
            .child(
                div()
                    .w(px(number_width))
                    .flex_shrink_0()
                    .h_full()
                    .bg(palette.context_gutter),
            )
            .child(edge)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .overflow_hidden()
                    .child(buffer_stripes(palette.buffer)),
            );
    };
    let old_side = match side {
        Side::Old => true,
        Side::New => false,
        Side::Unified => line.kind == LineKind::Deletion,
    };
    let (spans, number) = if old_side {
        (
            line.old.and_then(|old| computed.old_spans.get(old - 1)),
            line.old,
        )
    } else {
        (
            line.new.and_then(|new| computed.new_spans.get(new - 1)),
            line.new,
        )
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
    let group = side.group();
    let left = -scroll;
    div()
        .group(group)
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
                .group_hover(group, move |style| style.bg(gutter_hover))
                .text_color(number_color)
                .children(number.map(|number| number.to_string())),
        )
        // The gutter's `border-right: 2px solid var(--diffs-bg)`.
        .child(edge)
        .child(
            div()
                .relative()
                .flex_1()
                .min_w_0()
                .h_full()
                .overflow_hidden()
                .bg(line_bg)
                .group_hover(group, move |style| style.bg(line_hover))
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
}

fn line_row(
    view: &DiffView,
    computed: &Computed,
    index: usize,
    palette: &DiffPalette,
    cell: f32,
    number_width: f32,
) -> AnyElement {
    line_cells(
        computed,
        Some(index),
        Side::Unified,
        palette,
        cell,
        number_width,
        view.scroll_x,
    )
    .into_any_element()
}

/// Side by side (`grid-template-columns: 1fr 1fr`, a 1px `--diffs-bg` edge
/// on each side of the middle): each side scrolls sideways on its own.
#[allow(clippy::too_many_arguments)]
fn pair_row(
    view: &DiffView,
    computed: &Computed,
    old: Option<usize>,
    new: Option<usize>,
    palette: &DiffPalette,
    cell: f32,
    number_width: f32,
    longest: (f32, f32),
    cx: &mut Context<DiffView>,
) -> AnyElement {
    let half = |cells: gpui::Div, new_side: bool, cx: &mut Context<DiffView>| {
        let limit = if new_side { longest.1 } else { longest.0 };
        div()
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .map(|half| {
                if new_side {
                    half.border_l_1()
                } else {
                    half.border_r_1()
                }
            })
            .border_color(palette.background)
            .on_scroll_wheel(
                cx.listener(move |view, event: &ScrollWheelEvent, window, cx| {
                    let delta = event.delta.pixel_delta(window.line_height());
                    let dx = f32::from(delta.x);
                    if dx.abs() <= f32::from(delta.y).abs() {
                        return;
                    }
                    let scroll = if new_side {
                        &mut view.scroll_x_new
                    } else {
                        &mut view.scroll_x
                    };
                    *scroll = (*scroll - dx).clamp(0.0, limit.max(0.0));
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .child(cells)
    };
    let old_cells = line_cells(
        computed,
        old,
        Side::Old,
        palette,
        cell,
        number_width,
        view.scroll_x,
    );
    let new_cells = line_cells(
        computed,
        new,
        Side::New,
        palette,
        cell,
        number_width,
        view.scroll_x_new,
    );
    div()
        .flex()
        .w_full()
        .child(half(old_cells, false, cx))
        .child(half(new_cells, true, cx))
        .into_any_element()
}

/// `createSeparator`: the expand button (pointing up before the first
/// change, both ways between two) and "N unmodified lines"; either opens
/// 20 more lines, Shift the whole run. A patch's left-out lines (`gap` is
/// `None`) only say how many: there is nothing to open. Side by side the
/// bar runs across both columns: the old side carries it, the new side's
/// part is the bar's end.
fn separator_row(
    gap: Option<usize>,
    hidden: usize,
    place: GapPlace,
    split: bool,
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
    let on_click = move |gap: usize| {
        let entity = entity.clone();
        move |event: &ClickEvent, _: &mut Window, cx: &mut gpui::App| {
            let shift = event.modifiers().shift;
            let _ = entity.update(cx, |view, cx| view.expand(gap, shift, cx));
        }
    };
    let text = div()
        .flex()
        .flex_1()
        .min_w_0()
        .items_center()
        .bg(palette.separator)
        .px(px(cell))
        .text_color(number)
        .child(div().truncate().child(unmodified_lines(hidden)));
    let bar = match gap {
        Some(gap) => div()
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
                    .on_click(on_click(gap))
                    // `[data-expand-down] [data-icon] { transform: scaleY(-1) }`
                    .child(Icon::new(icon).rotate(if flipped { 180.0 } else { 0.0 })),
            )
            .child(
                text.id(("diff-unmodified", gap))
                    .when(!split, |text| text.rounded_r(px(6.0)))
                    .cursor_pointer()
                    .hover(|style| style.underline())
                    .on_click(on_click(gap)),
            ),
        None => div().flex().h(px(SEPARATOR)).child(text.rounded(px(6.0))),
    };
    let bar = if split {
        div()
            .flex()
            .child(div().flex_1().min_w_0().child(bar))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h(px(SEPARATOR))
                    .ml(px(2.0))
                    .rounded_r(px(6.0))
                    .bg(palette.separator),
            )
    } else {
        bar
    };
    // Padding instead of `margin-block`: the list measures an item's box.
    div()
        .when(place != GapPlace::First, |row| row.pt(px(GAP)))
        .when(place != GapPlace::Last, |row| row.pb(px(GAP)))
        .px(px(GAP))
        .child(bar)
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
        // The longest line on each side (old, new), for the sideways scroll.
        let longest = match &self.state {
            State::Ready(computed) => {
                let widest = |old: bool| {
                    computed
                        .diff
                        .lines
                        .iter()
                        .filter(|line| {
                            !self.split
                                || if old {
                                    line.old.is_some()
                                } else {
                                    line.new.is_some()
                                }
                        })
                        .map(|line| line.text.chars().count())
                        .max()
                        .unwrap_or(0) as f32
                        * cell
                };
                (widest(true), widest(false))
            }
            State::Loading => (0.0, 0.0),
        };
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
                        if view.split {
                            return match view.split_rows.get(index).cloned() {
                                Some(SplitRow::Pair { old, new }) => pair_row(
                                    view,
                                    computed,
                                    old,
                                    new,
                                    &palette,
                                    cell,
                                    number_width,
                                    longest,
                                    cx,
                                ),
                                Some(SplitRow::Separator { gap, hidden, place }) => separator_row(
                                    Some(gap),
                                    hidden,
                                    place,
                                    true,
                                    &palette,
                                    cell,
                                    cx,
                                ),
                                Some(SplitRow::Skipped { hidden, place, .. }) => {
                                    separator_row(None, hidden, place, true, &palette, cell, cx)
                                }
                                None => div().into_any_element(),
                            };
                        }
                        match view.rows.get(index).cloned() {
                            Some(Row::Line(line)) => {
                                line_row(view, computed, line, &palette, cell, number_width)
                            }
                            Some(Row::Separator { gap, hidden, place }) => {
                                separator_row(Some(gap), hidden, place, false, &palette, cell, cx)
                            }
                            Some(Row::Skipped { hidden, place, .. }) => {
                                separator_row(None, hidden, place, false, &palette, cell, cx)
                            }
                            None => div().into_any_element(),
                        }
                    })
                })
                .size_full()
                .into_any_element()
            }
        };
        let longest = longest.0.max(longest.1);
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
                    // Side by side each side takes its own sideways wheel.
                    if dx.abs() > f32::from(delta.y).abs() && !view.split {
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

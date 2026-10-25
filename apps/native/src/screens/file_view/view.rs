//! The rows: the number (`padding-inline: 1ch` past a 2ch start, at least
//! 3 digits) and its 2px edge, then the code at 1ch, 20px a line, with the
//! active line, the selection and the caret over it. The pointer places
//! the caret, drags a selection, double-clicks a word, triple-clicks a line.

use std::time::Duration;

use gpui::{
    AnyElement, Context, ElementInputHandler, Hsla, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Render, ScrollWheelEvent, SharedString, StyledText, Window, canvas, div, font,
    list, prelude::*, px,
};

use super::buffer::expand_tabs;
use super::{FileView, Geometry, LINE};
use crate::data::highlight::{Span, editor_colors};
use crate::screens::diff_view::{DiffPalette, selected_colors, styled_runs};

/// The diff's font (`--diffs-font-family`).
const FONT: &str = "Monaco";
const FONT_SIZE: f32 = 13.0;
/// `padding-top: var(--diffs-gap-block)`
pub(crate) const TOP: f32 = 8.0;
const GROUP: &str = "file-line";

fn color(rgba: u32) -> Hsla {
    gpui::Rgba {
        r: ((rgba >> 24) & 0xff) as f32 / 255.0,
        g: ((rgba >> 16) & 0xff) as f32 / 255.0,
        b: ((rgba >> 8) & 0xff) as f32 / 255.0,
        a: (rgba & 0xff) as f32 / 255.0,
    }
    .into()
}

/// The line's colors moved to where its bytes land once tabs are spelled out.
fn mapped(spans: &[Span], map: &[usize]) -> Vec<Span> {
    let at = |offset: usize| map[offset.min(map.len() - 1)];
    spans
        .iter()
        .map(|span| Span {
            range: at(span.range.start)..at(span.range.end),
            ..span.clone()
        })
        .collect()
}

struct RowStyle {
    palette: DiffPalette,
    selection: Hsla,
    /// `[data-match-range]:not([data-focus])`
    match_other: Hsla,
    /// `[data-bracket-match-range]`
    bracket_bg: Hsla,
    bracket_border: Hsla,
    /// The caret's bracket and its pair.
    brackets: Option<(usize, usize)>,
    line_highlight: Hsla,
    caret: Hsla,
    cell: f32,
    number_width: f32,
    caret_visible: bool,
}

fn row(view: &FileView, index: usize, style: &RowStyle, cx: &mut Context<FileView>) -> AnyElement {
    let palette = &style.palette;
    let cell = style.cell;
    let raw = view.buffer.line(index);
    let (display, map) = expand_tabs(raw, view.indentation.size);
    let spans = view
        .spans
        .get(index)
        .map(|spans| mapped(spans, &map))
        .unwrap_or_default();
    let runs = styled_runs(display.len(), &spans, &[], palette.added_word);
    let styled = StyledText::new(SharedString::from(display.clone())).with_highlights(runs);
    let hover = palette.hover;
    let start = view.buffer.line_start(index);
    let end = start + raw.len();
    let cursor = view.buffer.cursor();
    let selection = view.buffer.selection.clone();
    let caret_line = view.buffer.line_of(cursor) == index;
    let active = caret_line && selection.is_empty();
    let display_column = |offset: usize| {
        let byte = offset.saturating_sub(start).min(raw.len());
        display[..map[byte]].chars().count() as f32
    };
    // The part of the selection on this line, one cell past its end when
    // the selection goes on to the next line.
    let selected = (!selection.is_empty() && selection.start <= end && selection.end >= start)
        .then(|| {
            let from = display_column(selection.start.max(start));
            let to = display_column(selection.end.min(end));
            let past = if selection.end > end { 1.0 } else { 0.0 };
            (from, to + past)
        })
        .filter(|(from, to)| to > from);
    let caret = (style.caret_visible && caret_line).then(|| display_column(cursor));
    let left = cell - view.scroll_x;
    // The panel's matches on this line (the current one is the selection).
    let matches: Vec<(f32, f32)> = view
        .search
        .as_ref()
        .map(|search| {
            let first = search.matches.partition_point(|range| range.end < start);
            search.matches[first..]
                .iter()
                .take_while(|range| range.start <= end)
                .filter(|range| **range != selection)
                .map(|range| {
                    (
                        display_column(range.start.max(start)),
                        display_column(range.end.min(end)),
                    )
                })
                .filter(|(from, to)| to > from)
                .collect()
        })
        .unwrap_or_default();
    // Lines the "+" is picking, toward the selection color.
    let picked = view
        .picked_lines()
        .is_some_and(|lines| lines.contains(&index));
    let (picked_line, picked_number, picked_fg) =
        selected_colors(palette, palette.background, palette.background);
    let plus = view.line_plus(index, GROUP, style.number_width, cell, cx);
    let brackets: Vec<f32> = style
        .brackets
        .into_iter()
        .flat_map(|(open, close)| [open, close])
        .filter(|at| (start..end).contains(at))
        .map(display_column)
        .collect();
    div()
        .group(GROUP)
        .relative()
        .flex()
        .h(px(LINE))
        .w_full()
        .child(
            div()
                .flex()
                .flex_shrink_0()
                .justify_end()
                .w(px(style.number_width))
                .pr(px(cell))
                .bg(palette.background)
                .text_color(palette.number)
                .group_hover(GROUP, move |row| row.bg(hover))
                .when(picked, |number| {
                    number
                        .bg(picked_number)
                        .text_color(picked_fg)
                        .group_hover(GROUP, move |row| row.bg(picked_number))
                })
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
                .when(active, |line| line.bg(style.line_highlight))
                .when(!active, |line| {
                    line.group_hover(GROUP, move |row| row.bg(hover))
                })
                .when(picked, |line| {
                    line.bg(picked_line)
                        .group_hover(GROUP, move |row| row.bg(picked_line))
                })
                .children(matches.into_iter().map(|(from, to)| {
                    div()
                        .absolute()
                        .top_0()
                        .h_full()
                        .left(px(left + from * cell))
                        .w(px((to - from) * cell))
                        .bg(style.match_other)
                }))
                .children(brackets.into_iter().map(|column| {
                    div()
                        .absolute()
                        .top_0()
                        .h_full()
                        .left(px(left + column * cell))
                        .w(px(cell))
                        .bg(style.bracket_bg)
                        .border_b_1()
                        .border_color(style.bracket_border)
                }))
                .children(selected.map(|(from, to)| {
                    div()
                        .absolute()
                        .top_0()
                        .h_full()
                        .left(px(left + from * cell))
                        .w(px((to - from) * cell))
                        .bg(style.selection)
                }))
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .left(px(left))
                        .whitespace_nowrap()
                        .text_color(palette.foreground)
                        .child(styled),
                )
                .children(caret.map(|column| {
                    div()
                        .debug_selector(|| "file-caret".into())
                        .absolute()
                        .top_0()
                        .h_full()
                        .left(px(left + column * cell))
                        .w(px(2.0))
                        .bg(style.caret)
                })),
        )
        .children(plus)
        .into_any_element()
}

impl Render for FileView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = DiffPalette::new(self.dark);
        let colors = editor_colors(self.dark);
        let mono = font(FONT);
        let cell = window
            .text_system()
            .resolve_font(&mono)
            .pipe(|id| window.text_system().advance(id, px(FONT_SIZE), '0').ok())
            .map(|size| f32::from(size.width))
            .unwrap_or(7.8);
        let digits = self.buffer.line_count().max(1).to_string().len().max(3) as f32;
        let number_width = (digits + 3.0) * cell;
        let geometry = self.geometry.get();
        if self.reveal && geometry.bounds.size.height > px(0.0) {
            let text_width = f32::from(geometry.bounds.size.width) - number_width - 2.0 - cell;
            self.reveal_caret(
                f32::from(geometry.bounds.size.height) - TOP,
                text_width,
                cell,
            );
            self.reveal = false;
        }
        let focused = self.focus.is_focused(window);
        let elapsed = self.caret_moved.elapsed();
        let still = Duration::from_millis(800);
        let caret_visible =
            focused && (elapsed < still || (elapsed - still).as_millis() % 1200 < 600);
        let longest = (0..self.buffer.line_count())
            .map(|line| self.buffer.line(line).chars().count())
            .max()
            .unwrap_or(0) as f32
            * cell;
        let brackets = super::search::bracket_pair(self.buffer.text(), self.buffer.cursor())
            .filter(|_| self.buffer.selection.is_empty() && focused);
        let style = std::rc::Rc::new(RowStyle {
            match_other: if self.dark {
                color(0xff963266)
            } else {
                color(0xff963288)
            },
            bracket_bg: palette.foreground.opacity(0.05),
            bracket_border: palette.foreground.opacity(0.6),
            brackets,
            palette,
            selection: color(colors.selection),
            line_highlight: color(colors.line_highlight),
            caret: color(colors.cursor),
            cell,
            number_width,
            caret_visible,
        });
        let entity = cx.entity();
        let rows = list(self.list.clone(), move |index, _, cx| {
            entity.update(cx, |view, cx| row(view, index, &style, cx))
        })
        .size_full();
        let record = self.geometry.clone();
        let text_left = number_width + 2.0 + cell;
        let input = (focused, self.focus.clone(), cx.entity());
        let editor = div()
            .id("file-view")
            .debug_selector(|| "file-view".into())
            .track_focus(&self.focus)
            .relative()
            .size_full()
            .overflow_hidden()
            .cursor_text()
            .bg(palette.background)
            .text_color(palette.foreground)
            .font_family(FONT)
            .text_size(px(FONT_SIZE))
            .line_height(px(LINE))
            .pt(px(TOP))
            .child(
                canvas(
                    move |bounds, _, _| {
                        record.set(Geometry {
                            bounds,
                            text_left,
                            cell,
                        })
                    },
                    move |bounds, _, window, cx| {
                        let (focused, focus, entity) = input;
                        if focused {
                            window.handle_input(
                                &focus,
                                ElementInputHandler::new(bounds, entity),
                                cx,
                            );
                        }
                    },
                )
                .absolute()
                .inset_0(),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|view, event: &MouseDownEvent, window, cx| {
                    window.focus(&view.focus, cx);
                    let offset = view.offset_at(event.position);
                    match event.click_count {
                        2 => {
                            let word = view.buffer.word_at(offset);
                            view.buffer.select(word);
                        }
                        count if count >= 3 => {
                            let line = view.buffer.line_of(offset);
                            let start = view.buffer.line_start(line);
                            let end = if line + 1 >= view.buffer.line_count() {
                                view.buffer.text().len()
                            } else {
                                view.buffer.line_start(line + 1)
                            };
                            view.buffer.select(start..end);
                        }
                        _ => {
                            view.buffer.go(offset, event.modifiers.shift);
                            view.selecting = true;
                        }
                    }
                    view.goal_column = None;
                    view.moved(cx);
                }),
            )
            .on_mouse_move(cx.listener(|view, event: &MouseMoveEvent, _, cx| {
                if event.pressed_button == Some(MouseButton::Left) && view.picked_lines().is_some()
                {
                    view.drag_plus(event.position, cx);
                    return;
                }
                if view.selecting && event.pressed_button == Some(MouseButton::Left) {
                    let offset = view.offset_at(event.position);
                    view.buffer.go(offset, true);
                    view.moved(cx);
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|view, _: &MouseUpEvent, _, cx| {
                    view.selecting = false;
                    view.release_plus(cx);
                    cx.notify();
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|view, _: &MouseUpEvent, _, cx| {
                    view.selecting = false;
                    view.release_plus(cx);
                    cx.notify();
                }),
            )
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
            .child(rows);
        // The search panel floats over the text, outside its keys.
        let panel = super::find_view::panel(self, cx);
        let pill = self.selection_pill(number_width, cx);
        div()
            .relative()
            .size_full()
            .child(FileView::register_actions(
                div().size_full().child(editor),
                cx,
            ))
            .children(pill)
            .children(panel)
    }
}

/// `value.pipe(f)`: keeps the font lookup a single expression.
trait Pipe: Sized {
    fn pipe<T>(self, f: impl FnOnce(Self) -> T) -> T {
        f(self)
    }
}

impl<T> Pipe for T {}

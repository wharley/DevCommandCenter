//! Lays the field's text out wrapped to its width and paints it with the
//! selection, the caret and the inline chips, clipped to the visible part and
//! scrolled so the caret stays in sight.
//!
//! A chip is one character in the text; on screen it needs its own width.
//! The text is laid out from a display copy where each chip's character is
//! replaced by enough narrow no-break spaces (U+202F, which never wrap) to
//! hold it, and `DisplayMap` turns offsets between the two.

use std::ops::Range;

use gpui::{
    App, AvailableSpace, Bounds, ElementId, ElementInputHandler, Entity, GlobalElementId, Hsla,
    LayoutId, PaintQuad, Pixels, SharedString, Size, Style, TextRun, UnderlineStyle, Window,
    WrappedLine, fill, point, prelude::*, px, relative, size,
};

use super::buffer::CHIP;
use super::chip::{self, ChipLayout};
use super::{Layout, TextArea};
use crate::theme::ActiveTheme;

/// Holds a chip's width in the laid-out text.
const SPACER: char = '\u{202F}';

pub struct TextAreaElement {
    area: Entity<TextArea>,
    max_height: Option<Pixels>,
}

impl TextAreaElement {
    pub fn new(area: Entity<TextArea>, max_height: Option<Pixels>) -> Self {
        Self { area, max_height }
    }
}

/// Offsets between the text and its display copy.
#[derive(Clone, Debug, Default)]
pub struct DisplayMap {
    /// Each chip: its offset in the text, in the display copy, and its
    /// length there.
    chips: Vec<(usize, usize, usize)>,
}

impl DisplayMap {
    pub fn to_display(&self, offset: usize) -> usize {
        let mut shift: isize = 0;
        for &(text, display, len) in &self.chips {
            if offset <= text {
                break;
            }
            shift = (display + len) as isize - (text + CHIP.len_utf8()) as isize;
        }
        (offset as isize + shift).max(0) as usize
    }

    /// The text offset for a display offset; inside a chip, its nearer edge.
    pub fn to_text(&self, offset: usize) -> usize {
        let mut shift: isize = 0;
        for &(text, display, len) in &self.chips {
            if offset <= display {
                break;
            }
            if offset < display + len {
                return if offset - display < len / 2 {
                    text
                } else {
                    text + CHIP.len_utf8()
                };
            }
            shift = (display + len) as isize - (text + CHIP.len_utf8()) as isize;
        }
        (offset as isize - shift).max(0) as usize
    }
}

pub struct LayoutState {
    text: SharedString,
    runs: Vec<TextRun>,
    map: DisplayMap,
    chips: Vec<(usize, chip::Chip, ChipLayout)>,
}

pub struct PrepaintState {
    lines: Vec<(usize, WrappedLine, Pixels)>,
    selections: Vec<PaintQuad>,
    cursor: Option<PaintQuad>,
    content_height: Pixels,
    line_height: Pixels,
}

impl IntoElement for TextAreaElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

fn spacer_width(window: &Window) -> Pixels {
    let style = window.text_style();
    let run = TextRun {
        len: SPACER.len_utf8(),
        font: style.font(),
        color: style.color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let width = window
        .text_system()
        .shape_line(
            SharedString::from(SPACER.to_string()),
            style.font_size.to_pixels(window.rem_size()),
            &[run],
            None,
        )
        .width;
    if width > px(0.5) { width } else { px(3.0) }
}

/// The display copy (or the placeholder), its runs and the chips laid out.
fn display(
    area: &TextArea,
    color: Hsla,
    placeholder: Hsla,
    marker: Hsla,
    window: &Window,
    cx: &App,
) -> LayoutState {
    let style = window.text_style();
    let text = area.buffer.text();
    if text.is_empty() {
        let run = TextRun {
            len: area.placeholder.len(),
            font: style.font(),
            color: placeholder,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        return LayoutState {
            text: area.placeholder.clone(),
            runs: vec![run],
            map: DisplayMap::default(),
            chips: Vec::new(),
        };
    }
    let spacer = spacer_width(window);
    let mut out = String::with_capacity(text.len());
    let mut map = DisplayMap::default();
    let mut chips = Vec::new();
    let mut previous = 0;
    for (offset, data) in area.buffer.chip_offsets() {
        out.push_str(&text[previous..offset]);
        let laid = chip::layout(data, window, cx);
        let count = ((f32::from(laid.width) / f32::from(spacer)).ceil() as usize).max(1);
        let display = out.len();
        for _ in 0..count {
            out.push(SPACER);
        }
        map.chips.push((offset, display, count * SPACER.len_utf8()));
        chips.push((offset, data.clone(), laid));
        previous = offset + CHIP.len_utf8();
    }
    out.push_str(&text[previous..]);
    let run = TextRun {
        len: out.len(),
        font: style.font(),
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let marked = area
        .buffer
        .marked
        .as_ref()
        .map(|marked| map.to_display(marked.start)..map.to_display(marked.end));
    let faint: Vec<_> = if area.lists {
        super::lists::marker_spans(text)
            .into_iter()
            .map(|span| map.to_display(span.start)..map.to_display(span.end))
            .collect()
    } else {
        Vec::new()
    };
    let runs = styled_runs(&run, out.len(), &faint, marker, marked);
    LayoutState {
        text: SharedString::from(out),
        runs,
        map,
        chips,
    }
}

/// The line's runs: list markers in `faint`, the IME's text underlined.
fn styled_runs(
    run: &TextRun,
    len: usize,
    faint: &[Range<usize>],
    faint_color: Hsla,
    marked: Option<Range<usize>>,
) -> Vec<TextRun> {
    let mut cuts = vec![0, len];
    for range in faint.iter().chain(marked.as_ref()) {
        cuts.extend([range.start.min(len), range.end.min(len)]);
    }
    cuts.sort_unstable();
    cuts.dedup();
    cuts.windows(2)
        .map(|cut| {
            let inside = |range: &Range<usize>| range.start <= cut[0] && cut[1] <= range.end;
            TextRun {
                len: cut[1] - cut[0],
                color: if faint.iter().any(inside) {
                    faint_color
                } else {
                    run.color
                },
                underline: marked
                    .as_ref()
                    .filter(|range| inside(range))
                    .map(|_| UnderlineStyle {
                        color: Some(run.color),
                        thickness: px(1.0),
                        wavy: false,
                    }),
                ..run.clone()
            }
        })
        .filter(|run| run.len > 0)
        .collect()
}

fn shape(
    text: &SharedString,
    runs: &[TextRun],
    width: Pixels,
    window: &Window,
) -> Vec<(usize, WrappedLine, Pixels)> {
    let style = window.text_style();
    let font_size = style.font_size.to_pixels(window.rem_size());
    let line_height = window.line_height();
    let wrapped = window
        .text_system()
        .shape_text(text.clone(), font_size, runs, Some(width), None)
        .unwrap_or_default();
    let mut start = 0;
    let mut top = px(0.0);
    let mut lines = Vec::with_capacity(wrapped.len());
    for line in wrapped {
        let height = line.size(line_height).height.max(line_height);
        let len = line.len();
        lines.push((start, line, top));
        start += len + 1;
        top += height;
    }
    lines
}

/// Rectangles covering `range` across wrapped rows.
fn selection_quads(
    layout: &Layout,
    range: std::ops::Range<usize>,
    width: Pixels,
    scroll: Pixels,
    origin: gpui::Point<Pixels>,
    color: Hsla,
) -> Vec<PaintQuad> {
    let line_height = layout.line_height;
    let start = layout.position_for(range.start);
    let end = layout.position_for(range.end);
    let quad = |left: Pixels, top: Pixels, right: Pixels| {
        fill(
            Bounds::from_corners(
                point(origin.x + left, origin.y + top - scroll),
                point(
                    origin.x + right.max(left + px(4.0)),
                    origin.y + top - scroll + line_height,
                ),
            ),
            color,
        )
    };
    if start.y == end.y {
        return vec![quad(start.x, start.y, end.x)];
    }
    let mut quads = vec![quad(start.x, start.y, width)];
    let mut row = start.y + line_height;
    while row < end.y {
        quads.push(quad(px(0.0), row, width));
        row += line_height;
    }
    quads.push(quad(px(0.0), end.y, end.x));
    quads
}

impl Element for TextAreaElement {
    type RequestLayoutState = LayoutState;
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let colors = cx.theme().colors.clone();
        let state = display(
            self.area.read(cx),
            colors.foreground,
            colors.muted_foreground.opacity(0.7),
            colors.muted_foreground,
            window,
            cx,
        );
        let (text, runs) = (state.text.clone(), state.runs.clone());
        let max_height = self.max_height;
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        let layout = window.request_measured_layout(style, move |known, available, window, _cx| {
            let width = known.width.unwrap_or(match available.width {
                AvailableSpace::Definite(width) => width,
                _ => px(10_000.0),
            });
            let line_height = window.line_height();
            let height = shape(&text, &runs, width, window)
                .last()
                .map(|(_, line, top)| *top + line.size(line_height).height.max(line_height))
                .unwrap_or(line_height);
            let height = max_height.map_or(height, |max| height.min(max));
            Size { width, height }
        });
        (layout, state)
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        state: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let theme = cx.theme();
        let caret_color = theme.colors.foreground;
        let selection_color = theme.colors.sidebar_primary.opacity(0.26);
        let area = self.area.read(cx);
        let lines = shape(&state.text, &state.runs, bounds.size.width, window);
        let line_height = window.line_height();
        let content_height = lines
            .last()
            .map(|(_, line, top)| *top + line.size(line_height).height.max(line_height))
            .unwrap_or(line_height);
        let empty = area.buffer.is_empty();
        let selection = area.selected_range();
        let cursor_offset = area.buffer.cursor();
        let mut scroll = area.scroll_y;
        let reveal = area.reveal_cursor;
        let layout = Layout {
            lines,
            line_height,
            bounds,
            content_height,
            map: state.map.clone(),
            chip_closes: Vec::new(),
        };
        let caret = if empty {
            point(px(0.0), px(0.0))
        } else {
            layout.position_for(cursor_offset)
        };
        let visible = bounds.size.height;
        if reveal {
            if caret.y < scroll {
                scroll = caret.y;
            } else if caret.y + line_height > scroll + visible {
                scroll = caret.y + line_height - visible;
            }
        }
        scroll = scroll.clamp(px(0.0), (content_height - visible).max(px(0.0)));
        let origin = bounds.origin;
        let (selections, cursor) = if empty || selection.is_empty() {
            let quad = fill(
                Bounds::new(
                    point(origin.x + caret.x, origin.y + caret.y - scroll),
                    size(px(1.0), line_height),
                ),
                caret_color,
            );
            (Vec::new(), Some(quad))
        } else {
            (
                selection_quads(
                    &layout,
                    selection,
                    bounds.size.width,
                    scroll,
                    origin,
                    selection_color,
                ),
                None,
            )
        };
        let Layout { lines, .. } = layout;
        self.area.update(cx, |area, _| {
            area.scroll_y = scroll;
            area.reveal_cursor = false;
        });
        PrepaintState {
            lines,
            selections,
            cursor,
            content_height,
            line_height,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        state: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let (focus_handle, scroll, disabled) = {
            let area = self.area.read(cx);
            (area.focus_handle.clone(), area.scroll_y, area.disabled)
        };
        if !disabled {
            window.handle_input(
                &focus_handle,
                ElementInputHandler::new(bounds, self.area.clone()),
                cx,
            );
        }
        let line_height = prepaint.line_height;
        let lines = std::mem::take(&mut prepaint.lines);
        let mut layout = Layout {
            lines,
            line_height,
            bounds,
            content_height: prepaint.content_height,
            map: std::mem::take(&mut state.map),
            chip_closes: Vec::new(),
        };
        window.with_content_mask(Some(gpui::ContentMask { bounds }), |window| {
            for quad in prepaint.selections.drain(..) {
                window.paint_quad(quad);
            }
            for (_, line, top) in &layout.lines {
                let origin = point(bounds.left(), bounds.top() + *top - scroll);
                let _ = line.paint(origin, line_height, gpui::TextAlign::Left, None, window, cx);
            }
            for (offset, data, laid) in &state.chips {
                let at = layout.position_for(*offset);
                let origin = point(bounds.left() + at.x, bounds.top() + at.y - scroll);
                let close = chip::paint(data, laid, origin, line_height, window, cx);
                layout.chip_closes.push((close, *offset));
            }
            if focus_handle.is_focused(window)
                && !disabled
                && let Some(cursor) = prepaint.cursor.take()
            {
                window.paint_quad(cursor);
            }
        });
        self.area.update(cx, |area, _| area.layout = Some(layout));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offsets_cross_chips_both_ways() {
        // "a" CHIP "b": the chip holds 4 spacers (12 bytes) on screen.
        let map = DisplayMap {
            chips: vec![(1, 1, 12)],
        };
        assert_eq!(map.to_display(0), 0);
        assert_eq!(map.to_display(1), 1);
        assert_eq!(map.to_display(4), 13);
        assert_eq!(map.to_display(5), 14);
        assert_eq!(map.to_text(13), 4);
        assert_eq!(map.to_text(3), 1, "the chip's left half snaps before it");
        assert_eq!(map.to_text(10), 4, "its right half after it");
        assert_eq!(map.to_text(14), 5);
    }
}

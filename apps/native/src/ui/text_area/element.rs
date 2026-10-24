//! Lays the field's text out wrapped to its width and paints it with the
//! selection and caret, clipped to the visible part and scrolled so the caret
//! stays in sight.

use gpui::{
    App, AvailableSpace, Bounds, ElementId, ElementInputHandler, Entity, GlobalElementId, Hsla,
    LayoutId, PaintQuad, Pixels, SharedString, Size, Style, TextRun, UnderlineStyle, Window,
    WrappedLine, fill, point, prelude::*, px, relative, size,
};

use super::{Layout, TextArea};
use crate::theme::ActiveTheme;

pub struct TextAreaElement {
    area: Entity<TextArea>,
    max_height: Option<Pixels>,
}

impl TextAreaElement {
    pub fn new(area: Entity<TextArea>, max_height: Option<Pixels>) -> Self {
        Self { area, max_height }
    }
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

/// The text to draw (or the placeholder) and its runs.
fn display(
    area: &TextArea,
    color: Hsla,
    placeholder: Hsla,
    window: &Window,
) -> (SharedString, Vec<TextRun>) {
    let style = window.text_style();
    let text = area.buffer.text();
    let (display, color) = if text.is_empty() {
        (area.placeholder.clone(), placeholder)
    } else {
        (SharedString::from(text.to_string()), color)
    };
    let run = TextRun {
        len: display.len(),
        font: style.font(),
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let runs = match area.buffer.marked.as_ref().filter(|_| !text.is_empty()) {
        Some(marked) => [
            TextRun {
                len: marked.start,
                ..run.clone()
            },
            TextRun {
                len: marked.end - marked.start,
                underline: Some(UnderlineStyle {
                    color: Some(color),
                    thickness: px(1.0),
                    wavy: false,
                }),
                ..run.clone()
            },
            TextRun {
                len: display.len() - marked.end,
                ..run
            },
        ]
        .into_iter()
        .filter(|run| run.len > 0)
        .collect(),
        None => vec![run],
    };
    (display, runs)
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
    type RequestLayoutState = ();
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
        let colors = &cx.theme().colors;
        let (text, runs) = display(
            self.area.read(cx),
            colors.foreground,
            colors.muted_foreground,
            window,
        );
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
        (layout, ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let theme = cx.theme();
        let caret_color = theme.colors.foreground;
        let selection_color = theme.colors.sidebar_primary.opacity(0.26);
        let (text_color, placeholder) = (
            theme.colors.foreground,
            theme.colors.muted_foreground.opacity(0.7),
        );
        let area = self.area.read(cx);
        let (text, runs) = display(area, text_color, placeholder, window);
        let lines = shape(&text, &runs, bounds.size.width, window);
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
        _request_layout: &mut Self::RequestLayoutState,
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
        window.with_content_mask(Some(gpui::ContentMask { bounds }), |window| {
            for quad in prepaint.selections.drain(..) {
                window.paint_quad(quad);
            }
            for (_, line, top) in &prepaint.lines {
                let origin = point(bounds.left(), bounds.top() + *top - scroll);
                let _ = line.paint(origin, line_height, gpui::TextAlign::Left, None, window, cx);
            }
            if focus_handle.is_focused(window)
                && !disabled
                && let Some(cursor) = prepaint.cursor.take()
            {
                window.paint_quad(cursor);
            }
        });
        let lines = std::mem::take(&mut prepaint.lines);
        let content_height = prepaint.content_height;
        self.area.update(cx, |area, _| {
            area.layout = Some(Layout {
                lines,
                line_height,
                bounds,
                content_height,
            });
        });
    }
}

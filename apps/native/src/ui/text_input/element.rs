//! Paints the field's line: text (or placeholder), selection and caret, in
//! the theme's colors.

use gpui::{
    App, Bounds, ContentMask, ElementId, ElementInputHandler, Entity, GlobalElementId, LayoutId,
    PaintQuad, Pixels, ShapedLine, Style, TextRun, UnderlineStyle, Window, fill, point, prelude::*,
    px, relative, size,
};

use super::TextInput;
use crate::theme::ActiveTheme;

pub struct TextElement {
    input: Entity<TextInput>,
}

impl TextElement {
    pub fn new(input: Entity<TextInput>) -> Self {
        Self { input }
    }
}

pub struct PrepaintState {
    line: Option<ShapedLine>,
    scroll_x: Pixels,
    cursor: Option<PaintQuad>,
    selection: Option<PaintQuad>,
}

impl IntoElement for TextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TextElement {
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
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = window.line_height().into();
        (window.request_layout(style, [], cx), ())
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
        let caret = theme.colors.foreground;
        // `::selection`: the accent at 26%.
        let selection_color = theme.colors.sidebar_primary.opacity(0.26);
        let placeholder_color = theme.colors.muted_foreground;
        let input = self.input.read(cx);
        let content = input.display_text();
        let selected_range = input.to_display(input.selected_range.start)
            ..input.to_display(input.selected_range.end);
        let cursor = input.to_display(input.cursor_offset());
        let marked_range = input
            .marked_range
            .as_ref()
            .map(|marked| input.to_display(marked.start)..input.to_display(marked.end));
        let style = window.text_style();
        let (display_text, color) = if content.is_empty() {
            (input.placeholder.clone(), placeholder_color)
        } else {
            (content, style.color)
        };
        let run = TextRun {
            len: display_text.len(),
            font: style.font(),
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let runs = match marked_range.as_ref() {
            Some(marked) => [
                TextRun {
                    len: marked.start,
                    ..run.clone()
                },
                TextRun {
                    len: marked.end - marked.start,
                    underline: Some(UnderlineStyle {
                        color: Some(run.color),
                        thickness: px(1.0),
                        wavy: false,
                    }),
                    ..run.clone()
                },
                TextRun {
                    len: display_text.len() - marked.end,
                    ..run
                },
            ]
            .into_iter()
            .filter(|run| run.len > 0)
            .collect(),
            None => vec![run],
        };
        let font_size = style.font_size.to_pixels(window.rem_size());
        let line = window
            .text_system()
            .shape_line(display_text, font_size, &runs, None);
        let focused = input.focus_handle.is_focused(window);
        let scroll_x = scroll_for(
            input.scroll_x,
            line.x_for_index(cursor),
            line.width,
            bounds.size.width,
            focused,
        );
        let left = bounds.left() - scroll_x;
        let (selection, cursor) = if selected_range.is_empty() {
            let x = line.x_for_index(cursor);
            (
                None,
                Some(fill(
                    Bounds::new(
                        point(left + x, bounds.top()),
                        size(px(1.0), bounds.size.height),
                    ),
                    caret,
                )),
            )
        } else {
            (
                Some(fill(
                    Bounds::from_corners(
                        point(left + line.x_for_index(selected_range.start), bounds.top()),
                        point(left + line.x_for_index(selected_range.end), bounds.bottom()),
                    ),
                    selection_color,
                )),
                None,
            )
        };
        PrepaintState {
            line: Some(line),
            scroll_x,
            cursor,
            selection,
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
        let focus_handle = self.input.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );
        let Some(line) = prepaint.line.take() else {
            return;
        };
        let scroll_x = prepaint.scroll_x;
        // A line wider than the field stays inside it.
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            if let Some(selection) = prepaint.selection.take() {
                window.paint_quad(selection)
            }
            let _ = line.paint(
                point(bounds.left() - scroll_x, bounds.top()),
                window.line_height(),
                gpui::TextAlign::Left,
                None,
                window,
                cx,
            );
            if focus_handle.is_focused(window)
                && let Some(cursor) = prepaint.cursor.take()
            {
                window.paint_quad(cursor);
            }
        });
        self.input.update(cx, |input, _| {
            input.last_layout = Some(line);
            input.last_bounds = Some(bounds);
            input.scroll_x = scroll_x;
        });
    }
}

/// How far a line `width` wide scrolls in a field `visible` wide: unfocused,
/// it shows its start (an address bar's host); focused, it keeps the caret
/// in sight, moving only as much as it must.
fn scroll_for(
    previous: Pixels,
    caret: Pixels,
    width: Pixels,
    visible: Pixels,
    focused: bool,
) -> Pixels {
    // Room for the caret after the last letter.
    let most = (width + px(1.0) - visible).max(Pixels::ZERO);
    if !focused || most == Pixels::ZERO {
        return Pixels::ZERO;
    }
    let scroll = if caret < previous {
        caret
    } else if caret + px(1.0) > previous + visible {
        caret + px(1.0) - visible
    } else {
        previous
    };
    scroll.clamp(Pixels::ZERO, most)
}

#[cfg(test)]
mod tests {
    use super::scroll_for;
    use gpui::px;

    #[test]
    fn a_line_that_fits_never_scrolls() {
        assert_eq!(
            scroll_for(px(0.0), px(80.0), px(80.0), px(200.0), true),
            px(0.0)
        );
    }

    #[test]
    fn unfocused_a_long_line_shows_its_start() {
        assert_eq!(
            scroll_for(px(120.0), px(500.0), px(500.0), px(200.0), false),
            px(0.0)
        );
    }

    #[test]
    fn focused_the_caret_stays_in_sight() {
        // At the end of a 500px line in a 200px field.
        assert_eq!(
            scroll_for(px(0.0), px(500.0), px(500.0), px(200.0), true),
            px(301.0)
        );
        // Moving left inside the view keeps the scroll.
        assert_eq!(
            scroll_for(px(301.0), px(400.0), px(500.0), px(200.0), true),
            px(301.0)
        );
        // Past the left edge, the view follows the caret.
        assert_eq!(
            scroll_for(px(301.0), px(40.0), px(500.0), px(200.0), true),
            px(40.0)
        );
    }
}

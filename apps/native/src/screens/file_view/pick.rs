//! Sending the file's code to the agent, as the stage's editor does
//! (`enabledSelectionAction` and `enableGutterUtility` in
//! `WorkspaceFileEditor`): with text selected and the pointer up, the pill
//! sits under the selection's end (over its start when it was made
//! backwards, the other side when it doesn't fit) and sends the selected
//! text; the "+" on a line's number is pressed and dragged over lines,
//! and letting go sends them whole.

use std::ops::Range;

use dcc_core::domain::code_annotation::{AnnotationSide, CodeAnnotation, snippet_for_lines};
use gpui::{
    AnyElement, Bounds, Context, MouseButton, MouseDownEvent, Pixels, SharedString, div, point,
    prelude::*, px,
};

use super::{FileView, FileViewEvent, LINE};
use crate::screens::annotation::{AnnotationRequest, trigger};

/// `--diffs-popover-gap`
const GAP: f32 = 8.0;
/// The pill and the 2px around it (`[data-selection-action-popover]`).
const PILL_HEIGHT: f32 = 37.4;
const PILL_WIDTH: f32 = 150.0;

#[derive(Default)]
pub(crate) struct Annotating {
    /// Lines being picked from the "+": where the press began, where the
    /// pointer is (zero-based).
    pub lines: Option<(usize, usize)>,
    /// The selection the pill already sent: it shows again on another one.
    pub sent: Option<Range<usize>>,
    pub pill: std::rc::Rc<std::cell::Cell<Bounds<Pixels>>>,
}

impl FileView {
    /// The code can be sent to the agent (the stage's files).
    pub fn annotatable(mut self) -> Self {
        self.annotating = Some(Annotating::default());
        self
    }

    /// The lines the "+" is picking, top first.
    pub(crate) fn picked_lines(&self) -> Option<Range<usize>> {
        let (anchor, head) = self.annotating.as_ref()?.lines?;
        Some(anchor.min(head)..anchor.max(head) + 1)
    }

    pub(crate) fn press_plus(&mut self, line: usize, cx: &mut Context<Self>) {
        if let Some(annotating) = self.annotating.as_mut() {
            annotating.lines = Some((line, line));
            cx.notify();
        }
    }

    /// The pointer moved with the button down: over which line.
    pub(crate) fn drag_plus(
        &mut self,
        position: gpui::Point<Pixels>,
        cx: &mut Context<Self>,
    ) -> bool {
        let line = self.buffer.line_of(self.offset_at(position));
        let Some((_, head)) = self
            .annotating
            .as_mut()
            .and_then(|annotating| annotating.lines.as_mut())
        else {
            return false;
        };
        if *head != line {
            *head = line;
            cx.notify();
        }
        true
    }

    /// Letting go of the "+": the lines go, anchored at the editor's corner
    /// (the desktop's trigger is the host then).
    pub(crate) fn release_plus(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(lines) = self.picked_lines() else {
            return false;
        };
        if let Some(annotating) = self.annotating.as_mut() {
            annotating.lines = None;
        }
        let (start_line, end_line) = (lines.start + 1, lines.end);
        let snippet = snippet_for_lines(self.buffer.text(), start_line, end_line);
        let bounds = self.geometry.get().bounds;
        self.send_annotation(
            start_line,
            end_line,
            snippet,
            point(bounds.left(), bounds.bottom()),
            cx,
        );
        true
    }

    fn send_annotation(
        &mut self,
        start_line: usize,
        end_line: usize,
        snippet: String,
        anchor: gpui::Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let annotation = CodeAnnotation {
            path: self.path.clone(),
            side: AnnotationSide::Modified,
            start_line,
            end_line,
            snippet,
        };
        cx.emit(FileViewEvent::Annotate(AnnotationRequest {
            annotation,
            anchor,
        }));
        cx.notify();
    }

    /// The pill: the selected text, its lines as the selection's.
    fn send_selection(&mut self, cx: &mut Context<Self>) {
        let selection = self.buffer.selection.clone();
        if selection.is_empty() {
            return;
        }
        let start_line = self.buffer.line_of(selection.start) + 1;
        let end_line = self.buffer.line_of(selection.end) + 1;
        let snippet = self.buffer.selected_text().to_string();
        let Some(annotating) = self.annotating.as_mut() else {
            return;
        };
        annotating.sent = Some(selection);
        let bounds = annotating.pill.get();
        self.send_annotation(
            start_line,
            end_line,
            snippet,
            point(bounds.left(), bounds.bottom()),
            cx,
        );
    }

    /// Where the pill goes, from the editor's top left: under the head of a
    /// forward selection, over it for a backward one; the other end when
    /// that side doesn't fit.
    fn pill_place(&self, number_width: f32) -> Option<(f32, f32)> {
        let annotating = self.annotating.as_ref()?;
        let selection = &self.buffer.selection;
        if selection.is_empty()
            || self.selecting
            || annotating.lines.is_some()
            || annotating.sent.as_ref() == Some(selection)
        {
            return None;
        }
        let geometry = self.geometry.get();
        let origin = geometry.bounds.origin;
        let height = f32::from(geometry.bounds.size.height);
        let width = f32::from(geometry.bounds.size.width);
        let place = |offset: usize, above: bool| {
            let at = self.point_for(offset);
            let top = f32::from(at.y - origin.y);
            let y = if above { top - PILL_HEIGHT } else { top + LINE };
            (f32::from(at.x - origin.x), y, top)
        };
        let backward = self.buffer.reversed;
        let preferred = place(self.buffer.cursor(), backward);
        let fallback = if backward {
            place(selection.end, false)
        } else {
            place(selection.start, true)
        };
        let fits = |(_, y, _): (f32, f32, f32)| y >= 0.0 && y + PILL_HEIGHT <= height;
        let (x, y, line_top) = if fits(preferred) || !fits(fallback) {
            preferred
        } else {
            fallback
        };
        // The line it hangs from scrolled out of view: no pill.
        if line_top < -LINE || line_top > height {
            return None;
        }
        let pill_width = match f32::from(annotating.pill.get().size.width) {
            0.0 => PILL_WIDTH,
            measured => measured + 4.0,
        };
        let min = number_width + 2.0 + GAP;
        let max = (width - GAP - pill_width).max(min);
        Some((
            x.clamp(min, max),
            y.clamp(0.0, (height - PILL_HEIGHT).max(0.0)),
        ))
    }

    /// The pill over the text, when a selection waits for it.
    pub(crate) fn selection_pill(
        &self,
        number_width: f32,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let (left, top) = self.pill_place(number_width)?;
        let record = self.annotating.as_ref()?.pill.clone();
        Some(
            div()
                .absolute()
                .left(px(left))
                .top(px(top))
                .p(px(2.0))
                .child(
                    trigger::snippet_trigger(
                        "file-annotate-pill",
                        "dcc-snippet-trigger",
                        record,
                        cx,
                    )
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(|view, _, _, cx| view.send_selection(cx))),
                )
                .into_any_element(),
        )
    }

    /// The "+" on line `index`: on hover, or kept where the pick ends.
    pub(crate) fn line_plus(
        &self,
        index: usize,
        group: &'static str,
        number_width: f32,
        cell: f32,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.annotating.as_ref()?;
        let pinned = match self.picked_lines() {
            Some(lines) => {
                if lines.end - 1 != index {
                    return None;
                }
                true
            }
            None => false,
        };
        let id = SharedString::from(format!("file-plus-{index}"));
        Some(
            trigger::plus_button(id, number_width - cell, pinned, group, cx)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |view, _: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        window.focus(&view.focus, cx);
                        view.press_plus(index, cx);
                    }),
                )
                .into_any_element(),
        )
    }
}

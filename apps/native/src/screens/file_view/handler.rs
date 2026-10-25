//! The bridge to the platform's text input (typing, IME composition, the
//! character palette), in UTF-16 offsets as macOS speaks them.

use std::ops::Range;

use gpui::{Bounds, Context, EntityInputHandler, Pixels, UTF16Selection, Window, point, px};

use super::buffer::{column_of, offset_at_column};
use super::{FileView, LINE};

fn utf16_to_utf8(text: &str, offset: usize) -> usize {
    let mut utf16 = 0;
    for (index, character) in text.char_indices() {
        if utf16 >= offset {
            return index;
        }
        utf16 += character.len_utf16();
    }
    text.len()
}

impl FileView {
    /// Where an offset is drawn, in window coordinates (its line's top-left).
    pub(crate) fn point_for(&self, offset: usize) -> gpui::Point<Pixels> {
        let geometry = self.geometry.get();
        let line = self.buffer.line_of(offset);
        let top = self.list.logical_scroll_top();
        let rows = line as f32 - top.item_ix as f32;
        let text = self.buffer.line(line);
        let column = column_of(
            text,
            offset - self.buffer.line_start(line),
            self.indentation.size,
        );
        point(
            geometry.bounds.left()
                + px(geometry.text_left + column as f32 * geometry.cell - self.scroll_x),
            geometry.bounds.top() + px(rows * LINE) - top.offset_in_item + px(super::view::TOP),
        )
    }

    /// The text offset under a window point.
    pub(crate) fn offset_at(&self, position: gpui::Point<Pixels>) -> usize {
        let geometry = self.geometry.get();
        let top = self.list.logical_scroll_top();
        let y = f32::from(position.y - geometry.bounds.top()) - super::view::TOP
            + f32::from(top.offset_in_item);
        let line = (top.item_ix as f32 + (y / LINE).floor()).max(0.0) as usize;
        if line >= self.buffer.line_count() {
            return self.buffer.text().len();
        }
        let x = f32::from(position.x - geometry.bounds.left()) - geometry.text_left + self.scroll_x;
        let column = (x / geometry.cell.max(1.0)).max(0.0);
        self.buffer.line_start(line)
            + offset_at_column(self.buffer.line(line), column, self.indentation.size)
    }
}

impl EntityInputHandler for FileView {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.buffer.range_from_utf16(&range_utf16);
        actual_range.replace(self.buffer.range_to_utf16(&range));
        Some(self.buffer.text()[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.buffer.range_to_utf16(&self.buffer.selection),
            reversed: self.buffer.reversed,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.buffer
            .marked
            .as_ref()
            .map(|range| self.buffer.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.buffer.unmark();
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.read_only {
            return;
        }
        let range = range_utf16
            .as_ref()
            .map(|range| self.buffer.range_from_utf16(range));
        // A bracket or quote typed over the selection wraps it.
        let over_selection = self.buffer.marked.is_none()
            && range
                .as_ref()
                .is_none_or(|range| *range == self.buffer.selection);
        if over_selection {
            let doc = super::lines::Doc::new(self.buffer.text());
            if let Some(change) = super::lines::surround(&doc, &self.buffer.selection, new_text) {
                return self.apply_change(change, cx);
            }
        }
        let lines = self.buffer.line_count();
        let line = self.buffer.line_of(
            range
                .as_ref()
                .map_or(self.buffer.selection.start, |range| range.start),
        );
        self.buffer.replace(range, new_text);
        self.goal_column = None;
        self.edited(lines, line, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range| self.buffer.range_from_utf16(range));
        if self.read_only {
            return;
        }
        let selected = new_selected_range_utf16.map(|selected| {
            utf16_to_utf8(new_text, selected.start)..utf16_to_utf8(new_text, selected.end)
        });
        let lines = self.buffer.line_count();
        let line = self.buffer.line_of(self.buffer.selection.start);
        self.buffer.replace_and_mark(range, new_text, selected);
        self.edited(lines, line, cx);
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        _bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let range = self.buffer.range_from_utf16(&range_utf16);
        let start = self.point_for(range.start);
        let end = self.point_for(range.end);
        Some(Bounds::from_corners(
            start,
            point(end.x.max(start.x), start.y + px(LINE)),
        ))
    }

    fn character_index_for_point(
        &mut self,
        position: gpui::Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let offset = self.offset_at(position);
        Some(self.buffer.offset_to_utf16(offset))
    }
}

//! The pointer on the surface: a drag selects cells, a double click a word,
//! a triple click the line, Shift extends; ⌘C copies. An `http(s)` link
//! (`createHttpLinkProvider`) is underlined under the pointer and opens
//! with a click.

use std::ops::Range;

use gpui::{Context, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, Window};

use super::emulator::{SelectBy, SelectedSpan};
use super::session::TerminalSession;
use super::view::ROW_HEIGHT;

/// A link on a visible row, in columns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link {
    pub line: usize,
    pub columns: Range<usize>,
    pub url: String,
}

/// `URL_PATTERN` and `sanitizeHttpUrl`'s trailing punctuation.
pub fn links_in(text: &str) -> Vec<(Range<usize>, String)> {
    static PATTERN: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r#"https?://[^\s<>"'`]+"#).expect("link pattern")
    });
    PATTERN
        .find_iter(text)
        .filter_map(|found| {
            let raw = found.as_str();
            let url = raw.trim_end_matches([')', ',', '.', ';', ':', '!', '?']);
            let rest = url.split_once("://")?.1;
            if rest.is_empty() {
                return None;
            }
            let start = text[..found.start()].chars().count();
            Some((start..start + url.chars().count(), url.to_string()))
        })
        .collect()
}

impl TerminalSession {
    /// The visible cell under a window point, and whether it is its right half.
    fn cell_at(&self, position: Point<Pixels>) -> Option<(usize, usize, bool)> {
        let origin = self.text_origin.get()?;
        let x = f32::from(position.x - origin.x).max(0.0);
        let y = f32::from(position.y - origin.y).max(0.0);
        let (cols, rows) = self.emulator.size();
        let cell = self.cell.0.max(1.0);
        let column = ((x / cell).floor() as usize).min(cols.saturating_sub(1) as usize);
        let line = ((y / ROW_HEIGHT).floor() as usize).min(rows.saturating_sub(1) as usize);
        let right = x - column as f32 * cell > cell / 2.0;
        Some((line, column, right))
    }

    fn link_at(&self, line: usize, column: usize) -> Option<Link> {
        let palette = self.palette.clone()?;
        let screen = self.emulator.screen(&palette);
        let text = &screen.rows.get(line)?.text;
        links_in(text)
            .into_iter()
            .find(|(columns, _)| columns.contains(&column))
            .map(|(columns, url)| Link { line, columns, url })
    }

    pub(super) fn mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus, cx);
        let Some((line, column, right)) = self.cell_at(event.position) else {
            return;
        };
        self.pressed_link = self
            .link_at(line, column)
            .filter(|_| event.click_count == 1);
        let by = match event.click_count {
            2 => SelectBy::Words,
            3.. => SelectBy::Lines,
            _ => SelectBy::Cells,
        };
        if event.modifiers.shift
            && by == SelectBy::Cells
            && self.emulator.selection_text().is_some()
        {
            self.emulator.extend_selection(line, column, right);
        } else {
            self.emulator.start_selection(by, line, column, right);
        }
        self.selecting = true;
        cx.notify();
    }

    pub(super) fn mouse_move(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        let Some((line, column, right)) = self.cell_at(event.position) else {
            return;
        };
        if self.selecting && event.dragging() {
            self.pressed_link = None;
            self.emulator.extend_selection(line, column, right);
            cx.notify();
        }
        let hovered = self.link_at(line, column);
        if hovered != self.hovered_link {
            self.hovered_link = hovered;
            cx.notify();
        }
    }

    pub(super) fn mouse_up(&mut self, _event: &MouseUpEvent, cx: &mut Context<Self>) {
        self.selecting = false;
        if let Some(link) = self.pressed_link.take() {
            self.emulator.clear_selection();
            cx.open_url(&link.url);
        }
        cx.notify();
    }

    pub(super) fn selection_rects(&self) -> Vec<SelectedSpan> {
        self.emulator.selected_spans()
    }

    pub fn selection_text(&self) -> Option<String> {
        self.emulator.selection_text()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_lose_their_trailing_punctuation() {
        let found = links_in("open http://localhost:5173/, or (https://x.dev/a?b=1).");
        assert_eq!(
            found,
            vec![
                (5..27, "http://localhost:5173/".to_string()),
                (33..52, "https://x.dev/a?b=1".to_string()),
            ]
        );
        assert!(links_in("ftp://x and https://").is_empty());
        assert_eq!(links_in("→ http://a.b")[0].0, 2..12);
    }
}

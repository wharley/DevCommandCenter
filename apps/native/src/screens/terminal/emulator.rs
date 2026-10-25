//! What the desktop's xterm.js does for its dock: the shell's bytes kept
//! as a screen and 5000 lines of scrollback (`alacritty_terminal`), read
//! back as rows of colored runs; the answers a program asks for (where the
//! cursor is, which color is the background) go back to the shell.

use std::sync::{Arc, Mutex};

use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Config, Term, TermMode};
use alacritty_terminal::vte::ansi::{Color, NamedColor, Processor, Rgb};
use gpui::Hsla;

use super::palette::{TerminalPalette, channels, rgb};

/// `scrollback: 5000`
pub const SCROLLBACK: usize = 5000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Size {
    cols: usize,
    rows: usize,
}

impl Dimensions for Size {
    fn total_lines(&self) -> usize {
        self.rows
    }

    fn screen_lines(&self) -> usize {
        self.rows
    }

    fn columns(&self) -> usize {
        self.cols
    }
}

/// The terminal's requests, kept until the view answers them.
#[derive(Clone, Default)]
struct Requests(Arc<Mutex<Vec<Event>>>);

impl EventListener for Requests {
    fn send_event(&self, event: Event) {
        if matches!(
            event,
            Event::PtyWrite(_) | Event::ColorRequest(..) | Event::TextAreaSizeRequest(_)
        ) && let Ok(mut events) = self.0.lock()
        {
            events.push(event);
        }
    }
}

/// A run of cells drawn alike, `len` bytes of its row's text.
#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    pub len: usize,
    pub foreground: Hsla,
    /// `None` leaves the surface's own background.
    pub background: Option<Hsla>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Row {
    pub text: String,
    pub runs: Vec<Run>,
}

/// How a drag selects (one click, a double click, a triple click).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectBy {
    Cells,
    Words,
    Lines,
}

/// A selected stretch of one visible row, `start..end` in columns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelectedSpan {
    pub line: usize,
    pub start: usize,
    pub end: usize,
}

/// What shows now, top to bottom.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Screen {
    pub rows: Vec<Row>,
    /// The caret's row and column, while it shows (at the bottom, not
    /// scrolled back, and not hidden by the program).
    pub cursor: Option<(usize, usize)>,
}

pub struct Emulator {
    term: Term<Requests>,
    parser: Processor,
    requests: Requests,
}

impl Emulator {
    pub fn new(cols: u16, rows: u16) -> Self {
        let requests = Requests::default();
        let config = Config {
            scrolling_history: SCROLLBACK,
            ..Config::default()
        };
        let size = Size {
            cols: cols.max(2) as usize,
            rows: rows.max(1) as usize,
        };
        Self {
            term: Term::new(config, &size, requests.clone()),
            parser: Processor::new(),
            requests,
        }
    }

    pub fn feed(&mut self, bytes: &[u8]) {
        self.parser.advance(&mut self.term, bytes);
    }

    pub fn size(&self) -> (u16, u16) {
        (self.term.columns() as u16, self.term.screen_lines() as u16)
    }

    /// Returns whether the size changed.
    pub fn resize(&mut self, cols: u16, rows: u16) -> bool {
        let size = Size {
            cols: cols.max(2) as usize,
            rows: rows.max(1) as usize,
        };
        if (size.cols, size.rows) == (self.term.columns(), self.term.screen_lines()) {
            return false;
        }
        self.term.resize(size);
        true
    }

    pub fn mode(&self) -> TermMode {
        *self.term.mode()
    }

    /// Lines scrolled back (0 at the bottom).
    pub fn display_offset(&self) -> usize {
        self.term.grid().display_offset()
    }

    /// Scrolls the view `lines` up (positive) or down.
    pub fn scroll(&mut self, lines: i32) {
        self.term.scroll_display(Scroll::Delta(lines));
    }

    pub fn scroll_to_bottom(&mut self) {
        self.term.scroll_display(Scroll::Bottom);
    }

    /// `terminal.clear()`: the scrollback and the screen go, the program
    /// draws its prompt again on the `\f` the dock sends.
    pub fn clear(&mut self) {
        // Erasing the screen pushes it into the history: that goes after.
        self.feed(b"\x1b[H\x1b[2J");
        self.term.grid_mut().clear_history();
    }

    /// The last `max_lines` lines of history and screen as plain text, a
    /// wrapped line joined back, trailing blanks dropped
    /// (`getTerminalContextExcerpt`).
    pub fn recent_text(&self, max_lines: usize, max_chars: usize) -> String {
        let grid = self.term.grid();
        let top = grid.topmost_line().0;
        let bottom = grid.bottommost_line().0;
        let cols = self.term.columns();
        let mut lines: Vec<String> = Vec::new();
        let mut joining = false;
        for line in top..=bottom {
            let row = &grid[Line(line)];
            let mut text = String::new();
            for column in 0..cols {
                let cell = &row[Column(column)];
                if cell
                    .flags
                    .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
                {
                    continue;
                }
                text.push(if cell.c == '\0' { ' ' } else { cell.c });
            }
            let wrapped = row[Column(cols - 1)].flags.contains(Flags::WRAPLINE);
            let text = if wrapped {
                text
            } else {
                text.trim_end().to_string()
            };
            match lines.last_mut() {
                Some(last) if joining => last.push_str(&text),
                _ => lines.push(text),
            }
            joining = wrapped;
        }
        let start = lines.len().saturating_sub(max_lines);
        let joined = lines[start..].join("\n");
        let trimmed = joined.trim();
        let count = trimmed.chars().count();
        trimmed
            .chars()
            .skip(count.saturating_sub(max_chars))
            .collect()
    }

    /// A visible cell as a grid point (scrollback lines are negative).
    fn point(&self, line: usize, column: usize) -> Point {
        let line = line.min(self.term.screen_lines().saturating_sub(1)) as i32
            - self.display_offset() as i32;
        Point::new(
            Line(line),
            Column(column.min(self.term.columns().saturating_sub(1))),
        )
    }

    /// Starts a selection at a visible cell; `right` when the pointer sits
    /// on the cell's right half.
    pub fn start_selection(&mut self, by: SelectBy, line: usize, column: usize, right: bool) {
        let kind = match by {
            SelectBy::Cells => SelectionType::Simple,
            SelectBy::Words => SelectionType::Semantic,
            SelectBy::Lines => SelectionType::Lines,
        };
        let side = if right { Side::Right } else { Side::Left };
        self.term.selection = Some(Selection::new(kind, self.point(line, column), side));
    }

    pub fn extend_selection(&mut self, line: usize, column: usize, right: bool) {
        let point = self.point(line, column);
        let side = if right { Side::Right } else { Side::Left };
        if let Some(selection) = self.term.selection.as_mut() {
            selection.update(point, side);
        }
    }

    pub fn clear_selection(&mut self) {
        self.term.selection = None;
    }

    /// Like xterm's, a whole line comes without its line break.
    pub fn selection_text(&self) -> Option<String> {
        self.term
            .selection_to_string()
            .map(|text| text.strip_suffix('\n').map(str::to_string).unwrap_or(text))
            .filter(|text| !text.is_empty())
    }

    /// The selection's stretches on the visible rows.
    pub fn selected_spans(&self) -> Vec<SelectedSpan> {
        let Some(range) = self
            .term
            .selection
            .as_ref()
            .and_then(|selection| selection.to_range(&self.term))
        else {
            return Vec::new();
        };
        let offset = self.display_offset() as i32;
        let rows = self.term.screen_lines() as i32;
        let cols = self.term.columns();
        (range.start.line.0..=range.end.line.0)
            .filter_map(|line| {
                let visible = line + offset;
                if !(0..rows).contains(&visible) {
                    return None;
                }
                let first = line == range.start.line.0;
                let last = line == range.end.line.0;
                let start = if first || range.is_block {
                    range.start.column.0
                } else {
                    0
                };
                let end = if last || range.is_block {
                    range.end.column.0 + 1
                } else {
                    cols
                };
                (start < end).then_some(SelectedSpan {
                    line: visible as usize,
                    start,
                    end: end.min(cols),
                })
            })
            .collect()
    }

    /// The answers owed to the program, in order.
    pub fn take_replies(&mut self, palette: &TerminalPalette, cell: (f32, f32)) -> Vec<String> {
        let events = self
            .requests
            .0
            .lock()
            .map(|mut events| std::mem::take(&mut *events))
            .unwrap_or_default();
        let (cols, rows) = self.size();
        events
            .into_iter()
            .filter_map(|event| match event {
                Event::PtyWrite(text) => Some(text),
                Event::ColorRequest(index, format) => {
                    let color = self.color_at(index, palette);
                    let (r, g, b) = channels(color);
                    Some(format(Rgb { r, g, b }))
                }
                Event::TextAreaSizeRequest(format) => Some(format(WindowSize {
                    num_lines: rows,
                    num_cols: cols,
                    cell_width: cell.0.round() as u16,
                    cell_height: cell.1.round() as u16,
                })),
                _ => None,
            })
            .collect()
    }

    /// A color slot as the program set it, else the palette's.
    fn color_at(&self, index: usize, palette: &TerminalPalette) -> Hsla {
        if let Some(Rgb { r, g, b }) = self.term.colors()[index] {
            return rgb(r, g, b);
        }
        match index {
            0..=255 => palette.indexed(index as u8),
            index if index == NamedColor::Background as usize => palette.background,
            index if index == NamedColor::Cursor as usize => palette.cursor,
            _ => palette.foreground,
        }
    }

    fn resolve(&self, color: Color, palette: &TerminalPalette) -> Hsla {
        match color {
            Color::Spec(Rgb { r, g, b }) => rgb(r, g, b),
            Color::Indexed(index) => self.color_at(index as usize, palette),
            Color::Named(named) => {
                let index = named as usize;
                if index < 16 {
                    return self.color_at(index, palette);
                }
                match named {
                    NamedColor::Background => self.color_at(index, palette),
                    NamedColor::Cursor => self.color_at(index, palette),
                    NamedColor::Foreground | NamedColor::BrightForeground => {
                        self.color_at(NamedColor::Foreground as usize, palette)
                    }
                    // The dim colors are the normal ones at half strength.
                    NamedColor::DimForeground => self
                        .color_at(NamedColor::Foreground as usize, palette)
                        .opacity(0.5),
                    dim => {
                        let normal = (dim as usize).saturating_sub(NamedColor::DimBlack as usize);
                        self.color_at(normal.min(7), palette).opacity(0.5)
                    }
                }
            }
        }
    }

    pub fn screen(&self, palette: &TerminalPalette) -> Screen {
        let content = self.term.renderable_content();
        let offset = content.display_offset as i32;
        let rows_count = self.term.screen_lines();
        let mut rows: Vec<Row> = vec![Row::default(); rows_count];
        for indexed in content.display_iter {
            let line = indexed.point.line.0 + offset;
            let Some(row) = usize::try_from(line)
                .ok()
                .and_then(|line| rows.get_mut(line))
            else {
                continue;
            };
            let cell = indexed.cell;
            if cell
                .flags
                .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
            {
                continue;
            }
            // xterm draws bold text in the bright colors.
            let foreground = match cell.fg {
                Color::Named(named) if cell.flags.contains(Flags::BOLD) && (named as usize) < 8 => {
                    Color::Named(named.to_bright())
                }
                color => color,
            };
            let mut fg = self.resolve(foreground, palette);
            let mut bg = match cell.bg {
                Color::Named(NamedColor::Background) => None,
                color => Some(self.resolve(color, palette)),
            };
            if cell.flags.contains(Flags::INVERSE) {
                let under = bg.unwrap_or(palette.background);
                bg = Some(fg);
                fg = under;
            }
            if cell.flags.contains(Flags::DIM) {
                fg = fg.opacity(0.5);
            }
            if cell.flags.contains(Flags::HIDDEN) {
                fg = bg.unwrap_or(palette.background);
            }
            let mut text = String::new();
            text.push(if cell.c == '\0' || cell.c == '\t' {
                ' '
            } else {
                cell.c
            });
            for zero_width in cell.zerowidth().into_iter().flatten() {
                text.push(*zero_width);
            }
            let run = Run {
                len: text.len(),
                foreground: fg,
                background: bg,
                bold: cell.flags.contains(Flags::BOLD),
                italic: cell.flags.contains(Flags::ITALIC),
                underline: cell.flags.intersects(Flags::ALL_UNDERLINES),
                strikethrough: cell.flags.contains(Flags::STRIKEOUT),
            };
            row.text.push_str(&text);
            match row.runs.last_mut() {
                Some(last) if same_look(last, &run) => last.len += run.len,
                _ => row.runs.push(run),
            }
        }
        let cursor = (content.display_offset == 0 && content.mode.contains(TermMode::SHOW_CURSOR))
            .then(|| {
                let point = content.cursor.point;
                (point.line.0.max(0) as usize, point.column.0)
            })
            .filter(|(line, _)| *line < rows_count);
        Screen { rows, cursor }
    }
}

fn same_look(a: &Run, b: &Run) -> bool {
    a.foreground == b.foreground
        && a.background == b.background
        && a.bold == b.bold
        && a.italic == b.italic
        && a.underline == b.underline
        && a.strikethrough == b.strikethrough
}

#[cfg(test)]
#[path = "emulator_tests.rs"]
mod tests;

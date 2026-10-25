//! The emulator: text and colors, scrollback, answers, selection.
use super::*;
use crate::theme::palette::Palette;

fn palette() -> TerminalPalette {
    TerminalPalette::new(true, &Palette::dark())
}

fn text(screen: &Screen, row: usize) -> String {
    screen.rows[row].text.trim_end().to_string()
}

#[test]
fn text_and_colors_land_on_their_rows() {
    let mut emulator = Emulator::new(20, 4);
    emulator.feed(b"plain \x1b[31mred\x1b[0m\r\n\x1b[1;32mbold\x1b[0m");
    let palette = palette();
    let screen = emulator.screen(&palette);
    assert_eq!(text(&screen, 0), "plain red");
    assert_eq!(text(&screen, 1), "bold");
    let runs = &screen.rows[0].runs;
    assert_eq!(runs[0].len, 6);
    assert_eq!(runs[0].foreground, palette.foreground);
    assert_eq!(runs[1].len, 3);
    assert_eq!(runs[1].foreground, palette.ansi[1]);
    // Bold green is drawn bright.
    let bold = &screen.rows[1].runs[0];
    assert!(bold.bold);
    assert_eq!(bold.foreground, palette.ansi[10]);
    assert_eq!(screen.cursor, Some((1, 4)));
}

#[test]
fn inverse_and_backgrounds() {
    let mut emulator = Emulator::new(10, 2);
    emulator.feed(b"\x1b[7mA\x1b[0m\x1b[44mB\x1b[0m\x1b[38;5;196mC");
    let palette = palette();
    let runs = emulator.screen(&palette).rows[0].runs.clone();
    assert_eq!(runs[0].background, Some(palette.foreground));
    assert_eq!(runs[0].foreground, palette.background);
    assert_eq!(runs[1].background, Some(palette.ansi[4]));
    assert_eq!(channels(runs[2].foreground), (255, 0, 0));
    assert_eq!(runs.last().unwrap().background, None);
}

#[test]
fn scrollback_and_scrolling_back() {
    let mut emulator = Emulator::new(10, 3);
    for line in 0..10 {
        emulator.feed(format!("line {line}\r\n").as_bytes());
    }
    let palette = palette();
    assert_eq!(text(&emulator.screen(&palette), 0), "line 8");
    emulator.scroll(3);
    let screen = emulator.screen(&palette);
    assert_eq!(text(&screen, 0), "line 5");
    assert_eq!(screen.cursor, None, "no caret while scrolled back");
    emulator.scroll_to_bottom();
    assert_eq!(emulator.display_offset(), 0);
    emulator.clear();
    assert_eq!(text(&emulator.screen(&palette), 0), "");
    emulator.scroll(5);
    assert_eq!(emulator.display_offset(), 0, "the scrollback went too");
}

#[test]
fn programs_get_their_answers() {
    let mut emulator = Emulator::new(80, 24);
    // Where is the cursor? Which color is the background? How big?
    emulator.feed(b"ab\x1b[6n\x1b]11;?\x07\x1b[18t");
    let replies = emulator.take_replies(&palette(), (7.0, 16.0));
    assert_eq!(replies[0], "\x1b[1;3R");
    assert!(replies[1].starts_with("\x1b]11;rgb:"), "{:?}", replies[1]);
    assert_eq!(replies[2], "\x1b[8;24;80t");
    assert!(emulator.take_replies(&palette(), (7.0, 16.0)).is_empty());
}

#[test]
fn resizing_reports_a_change_once() {
    let mut emulator = Emulator::new(80, 24);
    assert!(!emulator.resize(80, 24));
    assert!(emulator.resize(100, 30));
    assert_eq!(emulator.size(), (100, 30));
}

#[test]
fn selecting_cells_words_and_lines() {
    let mut emulator = Emulator::new(20, 3);
    emulator.feed(b"hello big world\r\nnext line");
    emulator.start_selection(SelectBy::Cells, 0, 6, false);
    emulator.extend_selection(0, 8, true);
    assert_eq!(emulator.selection_text().as_deref(), Some("big"));
    assert_eq!(
        emulator.selected_spans(),
        vec![SelectedSpan {
            line: 0,
            start: 6,
            end: 9
        }]
    );
    emulator.start_selection(SelectBy::Words, 0, 13, false);
    assert_eq!(emulator.selection_text().as_deref(), Some("world"));
    emulator.start_selection(SelectBy::Lines, 1, 2, false);
    assert_eq!(emulator.selection_text().as_deref(), Some("next line"));
    // Across two rows: the first runs to the edge.
    emulator.start_selection(SelectBy::Cells, 0, 10, false);
    emulator.extend_selection(1, 3, true);
    let spans = emulator.selected_spans();
    assert_eq!(
        spans[0],
        SelectedSpan {
            line: 0,
            start: 10,
            end: 20
        }
    );
    assert_eq!(
        spans[1],
        SelectedSpan {
            line: 1,
            start: 0,
            end: 4
        }
    );
    emulator.clear_selection();
    assert_eq!(emulator.selection_text(), None);
}

#[test]
fn the_recent_text_joins_wrapped_lines() {
    let mut emulator = Emulator::new(5, 3);
    for line in 0..6 {
        emulator.feed(format!("l{line}\r\n").as_bytes());
    }
    emulator.feed(b"abcdefgh");
    assert_eq!(emulator.recent_text(3, 100), "l4\nl5\nabcdefgh");
    assert_eq!(emulator.recent_text(120, 4), "efgh");
}

#[test]
fn wide_characters_take_one_glyph() {
    let mut emulator = Emulator::new(10, 2);
    emulator.feed("日本".as_bytes());
    let screen = emulator.screen(&palette());
    assert!(screen.rows[0].text.starts_with("日本 "));
    assert_eq!(screen.cursor, Some((0, 4)));
}

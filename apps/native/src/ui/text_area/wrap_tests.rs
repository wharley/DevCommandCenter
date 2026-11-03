//! A chip that the soft wrap pushes to the next row is drawn there, at the
//! row's start, and the text after it follows it — not drawn past the edge
//! of the row above with an empty gap opening the next one.

use gpui::{TestAppContext, WindowAppearance, px};

use super::TextArea;
use super::chip::Chip;
use crate::theme::{Theme, ThemePreference};

#[gpui::test]
fn a_chip_pushed_to_the_next_row_opens_it(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
    });
    let (area, cx) = cx.add_window_view(TextArea::new);
    // Longer and longer text until the chip's spacers start the second row.
    for words in 1..120 {
        area.update(cx, |area, cx| {
            area.set_text(&"tela ".repeat(words), cx);
            area.insert_chip(
                Chip::Image("/tmp/dcc/CleanShot 2026-11-03.png".into()),
                None,
                cx,
            );
            area.buffer.replace(None, " agenda");
        });
        cx.run_until_parked();
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
        let wrapped = area.read_with(cx, |area, _| {
            let layout = area.layout.as_ref().unwrap();
            let (offset, _) = area.chips()[0];
            let caret = layout.position_for(offset);
            let after = layout.position_for(offset + super::CHIP.len_utf8());
            if after.y == caret.y {
                return false;
            }
            // The wrap fell right before the chip: its room is on the next row.
            let origin = layout.insert_origin(offset);
            assert_eq!(
                origin.y, after.y,
                "the chip is drawn on the row with its room"
            );
            assert_eq!(origin.x, px(0.0), "and opens that row");
            assert!(
                after.x <= layout.bounds.size.width,
                "the chip fits the field: ends at {:?} of {:?}",
                after.x,
                layout.bounds.size.width
            );
            true
        });
        if wrapped {
            return;
        }
    }
    panic!("no length put the chip at the start of a row");
}

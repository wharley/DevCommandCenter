//! The multi-line field's line keys, pressed for real: ⇧Home/⇧End select to
//! the line's ends, ⌃A/⌃E move there, ⌘⌦ deletes to the line's end.

use gpui::{Focusable, TestAppContext, VisualTestContext, WindowAppearance};

use super::TextArea;
use crate::theme::{Theme, ThemePreference};

#[gpui::test]
fn line_keys_select_move_and_delete_within_the_line(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        super::bind_keys(cx);
    });
    let (area, cx) = cx.add_window_view(TextArea::new);
    area.update_in(cx, |area, window, cx| {
        area.set_text("primeira linha\nsegunda linha", cx);
        window.focus(&area.focus_handle(cx), cx);
    });
    cx.run_until_parked();
    let selected = |cx: &mut VisualTestContext| {
        area.read_with(cx, |area, _| area.buffer.selected_text().to_string())
    };
    // The caret ends the text, on the second line.
    cx.simulate_keystrokes("cmd-down shift-home");
    assert_eq!(selected(cx), "segunda linha");
    cx.simulate_keystrokes("ctrl-a shift-end");
    assert_eq!(selected(cx), "segunda linha");
    // ⌃A then ⌥→ past "segunda", then ⌘⌦ drops the rest of that line only.
    cx.simulate_keystrokes("ctrl-a alt-right cmd-delete");
    assert_eq!(
        area.read_with(cx, |area, _| area.text().to_string()),
        "primeira linha\nsegunda"
    );
    cx.simulate_keystrokes("ctrl-e");
    assert_eq!(
        area.read_with(cx, |area, _| area.cursor()),
        "primeira linha\nsegunda".len()
    );
}

#[gpui::test]
fn a_paste_goes_in_at_the_caret(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        super::bind_keys(cx);
        cx.bind_keys([gpui::KeyBinding::new(
            "cmd-v",
            crate::app::menus::EditPaste,
            None,
        )]);
    });
    let (area, cx) = cx.add_window_view(TextArea::new);
    area.update_in(cx, |area, window, cx| {
        area.set_text("abcdef", cx);
        window.focus(&area.focus_handle(cx), cx);
    });
    cx.run_until_parked();
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("XYZ".into()));
    cx.simulate_keystrokes("cmd-left right right right cmd-v");
    assert_eq!(
        area.read_with(cx, |area, _| area.text().to_string()),
        "abcXYZdef"
    );
}

#[gpui::test]
fn copied_files_are_attached_where_the_field_takes_them(cx: &mut TestAppContext) {
    use std::path::PathBuf;

    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        super::bind_keys(cx);
        cx.bind_keys([gpui::KeyBinding::new(
            "cmd-v",
            crate::app::menus::EditPaste,
            None,
        )]);
    });
    let shot = PathBuf::from("/Users/me/CleanShot 2026-11-02 at 21.33.43@2x.png");
    let copied = gpui::ClipboardItem {
        entries: vec![gpui::ClipboardEntry::ExternalPaths(gpui::ExternalPaths(
            [shot.clone()].into_iter().collect(),
        ))],
    };
    // The composer's field: the file comes as an attachment, not its path.
    let (area, cx) = cx.add_window_view(|window, cx| TextArea::new(window, cx).with_attachments());
    let pasted = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let seen = pasted.clone();
    area.update_in(cx, |area, window, cx| {
        cx.subscribe_self(move |_, event: &super::TextAreaEvent, _| {
            if let super::TextAreaEvent::PastedFiles(paths) = event {
                seen.borrow_mut().extend(paths.clone());
            }
        })
        .detach();
        window.focus(&area.focus_handle(cx), cx);
    });
    cx.run_until_parked();
    cx.write_to_clipboard(copied.clone());
    cx.simulate_keystrokes("cmd-v");
    assert_eq!(pasted.borrow().as_slice(), [shot.clone()]);
    assert_eq!(area.read_with(cx, |area, _| area.text().to_string()), "");

    // Any other field still writes the path.
    let (plain, cx) = cx.add_window_view(TextArea::new);
    plain.update_in(cx, |area, window, cx| {
        window.focus(&area.focus_handle(cx), cx)
    });
    cx.run_until_parked();
    cx.write_to_clipboard(copied);
    cx.simulate_keystrokes("cmd-v");
    assert_eq!(
        plain.read_with(cx, |area, _| area.text().to_string()),
        shot.display().to_string()
    );
}

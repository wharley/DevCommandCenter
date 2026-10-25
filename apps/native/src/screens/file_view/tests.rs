//! The file view: its lines, the last empty one after a newline, the
//! grammar's colors and a line brought into view.

use gpui::{TestAppContext, WindowAppearance, px, size};

use super::FileView;
use crate::theme::{Theme, ThemePreference};

#[gpui::test]
fn a_file_shows_its_lines_colored_and_at_a_line(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light)));
    let text: String = (1..=200)
        .map(|n| format!("let line_{n} = {n};\n"))
        .collect();
    let (view, cx) =
        cx.add_window_view(|_, cx| FileView::new("src/lib.rs".into(), text, false, cx));
    cx.simulate_resize(size(px(800.0), px(400.0)));
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(view.lines.len(), 201, "the newline ends with an empty line");
        assert_eq!(view.lines[0], "let line_1 = 1;");
        assert!(!view.spans.is_empty(), "colored");
    });
    assert!(cx.debug_bounds("file-view").is_some());
    view.update(cx, |view, cx| view.focus(Some(150), cx));
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(view.focus_line, Some(150));
        let top = view.list.logical_scroll_top().item_ix;
        assert_eq!(top, 149, "line 150 at the top");
    });
}

#[gpui::test]
fn tabs_take_two_columns(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light)));
    let (view, cx) = cx.add_window_view(|_, cx| {
        FileView::new("a.rs".into(), "fn a() {\n\tb();\n}".into(), false, cx)
    });
    view.read_with(cx, |view, _| {
        assert_eq!(view.lines, vec!["fn a() {", "  b();", "}"]);
    });
}

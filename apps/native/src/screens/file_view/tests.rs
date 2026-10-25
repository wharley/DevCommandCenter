//! The file editor: its lines and colors, a line brought into view, and
//! editing with the keyboard, the platform's input and the pointer.

use dcc_core::domain::indentation::{DEFAULT_INDENTATION, FileIndentation};
use gpui::{Modifiers, TestAppContext, WindowAppearance, point, px, size};

use super::FileView;
use crate::theme::{Theme, ThemePreference};

fn open<'a>(
    cx: &'a mut TestAppContext,
    text: &str,
    indentation: FileIndentation,
) -> (gpui::Entity<FileView>, &'a mut gpui::VisualTestContext) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        super::bind_keys(cx);
    });
    let text = text.to_string();
    let (view, cx) = cx.add_window_view(move |_, cx| {
        FileView::new("src/lib.rs".into(), text, indentation, false, cx)
    });
    cx.simulate_resize(size(px(800.0), px(400.0)));
    view.update_in(cx, |view, window, cx| window.focus(&view.focus, cx));
    cx.run_until_parked();
    (view, cx)
}

fn text(view: &gpui::Entity<FileView>, cx: &mut gpui::VisualTestContext) -> String {
    view.read_with(cx, |view, _| view.text().to_string())
}

#[gpui::test]
fn a_file_shows_its_lines_colored_and_at_a_line(cx: &mut TestAppContext) {
    let source: String = (1..=200)
        .map(|n| format!("let line_{n} = {n};\n"))
        .collect();
    let (view, cx) = open(cx, &source, DEFAULT_INDENTATION);
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.buffer.line_count(),
            201,
            "the newline ends with an empty line"
        );
        assert_eq!(view.buffer.line(0), "let line_1 = 1;");
        assert!(!view.spans.is_empty(), "colored");
        assert!(!view.is_dirty());
    });
    assert!(cx.debug_bounds("file-view").is_some());
    view.update(cx, |view, cx| view.focus(Some(150), cx));
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.list.logical_scroll_top().item_ix,
            149,
            "line 150 at the top"
        );
        assert_eq!(view.buffer.cursor(), view.buffer.line_start(149));
    });
}

#[gpui::test]
fn typing_enter_and_undo_edit_the_text(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "fn a() {}\n", DEFAULT_INDENTATION);
    // The caret inside the braces: typing, then Enter opens the block.
    view.update(cx, |view, _| view.buffer.go(8, false));
    cx.simulate_input("x");
    assert_eq!(text(&view, cx), "fn a() {x}\n");
    view.update(cx, |view, _| view.buffer.go(8, false));
    cx.simulate_keystrokes("delete");
    cx.simulate_keystrokes("enter");
    assert_eq!(text(&view, cx), "fn a() {\n  \n}\n");
    view.read_with(cx, |view, _| {
        assert_eq!(view.buffer.cursor(), "fn a() {\n  ".len());
        assert!(view.is_dirty());
    });
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(text(&view, cx), "fn a() {}\n");
    cx.simulate_keystrokes("cmd-shift-z");
    assert_eq!(text(&view, cx), "fn a() {\n  \n}\n");
}

#[gpui::test]
fn tab_types_the_projects_indent_and_shifts_selected_lines(cx: &mut TestAppContext) {
    let tabs = FileIndentation {
        use_tabs: true,
        size: 4,
    };
    let (view, cx) = open(cx, "a\nb\n", tabs);
    cx.simulate_keystrokes("tab");
    assert_eq!(text(&view, cx), "\ta\nb\n");
    // Both lines selected: each one level in, then out again.
    cx.simulate_keystrokes("cmd-a tab");
    assert_eq!(text(&view, cx), "\t\ta\n\tb\n");
    cx.simulate_keystrokes("shift-tab");
    assert_eq!(text(&view, cx), "\ta\nb\n");
    // A tab is drawn to its stop.
    view.read_with(cx, |view, _| {
        assert_eq!(super::buffer::column_of(view.buffer.line(0), 1, 4), 4);
    });
}

#[gpui::test]
fn the_pointer_places_the_caret_and_selects(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "first line\nsecond line\n", DEFAULT_INDENTATION);
    let bounds = cx.debug_bounds("file-view").unwrap();
    let geometry = view.read_with(cx, |view, _| view.geometry.get());
    // The middle of line 2, three characters in.
    let at = point(
        bounds.origin.x + px(geometry.text_left + geometry.cell * 3.2),
        bounds.origin.y + px(super::view::TOP + super::LINE * 1.5),
    );
    cx.simulate_click(at, Modifiers::none());
    view.read_with(cx, |view, _| {
        assert_eq!(view.buffer.cursor(), "first line\n".len() + 3)
    });
    cx.simulate_event(gpui::MouseDownEvent {
        button: gpui::MouseButton::Left,
        position: at,
        modifiers: Modifiers::none(),
        click_count: 2,
        first_mouse: false,
    });
    view.read_with(cx, |view, _| {
        assert_eq!(view.buffer.selected_text(), "second")
    });
    cx.simulate_keystrokes("escape");
    view.read_with(cx, |view, _| assert!(view.buffer.selection.is_empty()));
}

fn summary(view: &gpui::Entity<FileView>, cx: &mut gpui::VisualTestContext) -> String {
    view.read_with(cx, |view, _| {
        view.search
            .as_ref()
            .map(|search| search.summary())
            .unwrap_or_default()
    })
}

#[gpui::test]
fn find_opens_on_the_word_and_steps_through_its_matches(cx: &mut TestAppContext) {
    cx.update(crate::ui::text_input::bind_keys);
    let (view, cx) = open(cx, "let total = 1;\ntotal += total;\n", DEFAULT_INDENTATION);
    view.update(cx, |view, _| view.buffer.go(5, false));
    cx.simulate_keystrokes("cmd-f");
    cx.run_until_parked();
    assert!(cx.debug_bounds("search-panel").is_some());
    assert_eq!(summary(&view, cx), "1 of 3");
    view.read_with(cx, |view, _| assert_eq!(view.buffer.selection, 4..9));
    // Enter and Shift+Enter in the field go around the matches.
    cx.simulate_keystrokes("enter");
    assert_eq!(summary(&view, cx), "2 of 3");
    cx.simulate_keystrokes("shift-enter shift-enter");
    assert_eq!(summary(&view, cx), "3 of 3");
    // ⌘Z in the field never reaches the file.
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(text(&view, cx), "let total = 1;\ntotal += total;\n");
    // Typing another query finds again from the selection on: the one
    // match sits before it, so none is current (as the Pierre editor).
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("+=");
    assert_eq!(summary(&view, cx), "1 results");
    cx.simulate_keystrokes("enter");
    assert_eq!(summary(&view, cx), "1 of 1");
    cx.simulate_input("x");
    assert_eq!(summary(&view, cx), "No results");
    cx.simulate_keystrokes("escape");
    view.read_with(cx, |view, _| assert!(view.search.is_none()));
    let focused = view.update_in(cx, |view, window, _| view.focus.is_focused(window));
    assert!(focused, "the text has the focus again");
}

#[gpui::test]
fn replace_edits_the_current_match_and_replace_all_the_rest(cx: &mut TestAppContext) {
    cx.update(crate::ui::text_input::bind_keys);
    let (view, cx) = open(cx, "a1 a2 a3\n", DEFAULT_INDENTATION);
    view.update(cx, |view, _| view.buffer.go(0, false));
    cx.simulate_keystrokes("cmd-alt-f");
    cx.run_until_parked();
    // The word at the caret is "a1"; ask for a regex instead.
    view.update(cx, |view, cx| {
        let search = view.search.as_ref().unwrap();
        let (query, replace) = (search.query.clone(), search.replace.clone());
        query.update(cx, |input, cx| input.set_text(r"a(\d)", cx));
        replace.update(cx, |input, cx| input.set_text("b$1", cx));
        view.toggle_search_option(|search| &mut search.regex, cx);
    });
    assert_eq!(summary(&view, cx), "1 of 3");
    let replace = cx.debug_bounds("search-replace").expect("Replace");
    cx.simulate_click(replace.center(), Modifiers::none());
    assert_eq!(text(&view, cx), "b1 a2 a3\n");
    let all = cx.debug_bounds("search-replace-all").unwrap();
    cx.simulate_click(all.center(), Modifiers::none());
    assert_eq!(text(&view, cx), "b1 b2 b3\n");
    assert_eq!(summary(&view, cx), "No results");
    cx.simulate_keystrokes("cmd-z");
}

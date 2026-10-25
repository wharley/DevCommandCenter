//! The diff view driven headlessly: computed off the UI thread, painted,
//! and a separator opened by a click.

use gpui::{Modifiers, TestAppContext, WindowAppearance, size};

use super::view::{number_width, styled_runs};
use super::{DiffView, State, compute};
use crate::data::diff::Row;
use crate::data::highlight::Span;
use crate::theme::{Theme, ThemePreference};

fn numbered(count: usize) -> String {
    (1..=count)
        .map(|n| format!("let line_{n} = {n};\n"))
        .collect()
}

fn separators(view: &DiffView) -> Vec<usize> {
    view.rows
        .iter()
        .filter_map(|row| match row {
            Row::Separator { hidden, .. } => Some(*hidden),
            _ => None,
        })
        .collect()
}

#[gpui::test]
fn the_diff_paints_and_a_separator_opens_on_click(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light)));
    let original = numbered(100);
    let modified = original.replace("let line_50 = 50;", "let line_50 = 5000;");
    let (view, cx) = cx
        .add_window_view(|_, cx| DiffView::new("src/lib.rs".into(), original, modified, false, cx));
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(matches!(view.state, State::Ready(_)));
        assert_eq!(separators(view), vec![46, 47]);
        assert_eq!(view.list.item_count(), view.rows.len());
    });
    cx.simulate_resize(size(gpui::px(900.0), gpui::px(600.0)));
    cx.run_until_parked();
    assert!(cx.debug_bounds("diff-view").is_some());
    // The first run opens 20 lines next to the change.
    let expand = cx
        .debug_bounds("diff-expand-0")
        .expect("the first separator painted");
    cx.simulate_click(expand.center(), Modifiers::none());
    view.read_with(cx, |view, _| {
        assert_eq!(separators(view), vec![26, 47]);
        assert_eq!(view.list.item_count(), view.rows.len());
    });
    // Shift opens the whole run.
    cx.run_until_parked();
    let expand = cx.debug_bounds("diff-expand-0").unwrap();
    cx.simulate_click(expand.center(), Modifiers::shift());
    view.read_with(cx, |view, _| assert_eq!(separators(view), vec![47]));
}

#[test]
fn tabs_take_two_columns_and_both_sides_are_colored() {
    let computed = compute(
        "a.rs",
        "fn a() {\n\tx();\n}\n",
        "fn a() {\n\ty();\n}\n",
        true,
    );
    let added = computed
        .diff
        .lines
        .iter()
        .find(|line| line.new == Some(2) && line.old.is_none())
        .unwrap();
    assert_eq!(added.text, "  y();");
    // The old side is colored down to its last removed line only.
    assert_eq!(computed.old_spans.len(), 2);
    assert_eq!(computed.new_spans.len(), 3);
    // 3 digits at least, plus 2ch before and 1ch after.
    assert_eq!(number_width(&computed, 8.0), 48.0);
}

#[test]
fn changed_words_cut_the_syntax_runs() {
    let span = |range: std::ops::Range<usize>, color: u32| Span {
        range,
        color,
        bold: false,
        italic: false,
    };
    let background = gpui::red();
    // `let total = 3;`: the keyword, then the number emphasized inside a run.
    let runs = styled_runs(
        14,
        &[span(0..3, 0xff0000ff), span(4..14, 0x00ff00ff)],
        std::slice::from_ref(&(12..13)),
        background,
    );
    let ranges: Vec<_> = runs.iter().map(|(range, _)| range.clone()).collect();
    assert_eq!(ranges, vec![0..3, 4..12, 12..13, 13..14]);
    assert_eq!(runs[2].1.background_color, Some(background));
    assert_eq!(runs[1].1.background_color, None);
    assert_eq!(runs[2].1.color, runs[1].1.color, "the word keeps its color");
}

//! Picking lines in the diff and sending them: by the numbers, by the "+",
//! and the pill.

use std::cell::RefCell;
use std::rc::Rc;

use dcc_core::domain::code_annotation::{AnnotationSide, DiffColumn};
use gpui::{Modifiers, MouseButton, TestAppContext, WindowAppearance, px, size};

use super::{DiffView, DiffViewEvent};
use crate::screens::annotation::AnnotationRequest;
use crate::theme::{Theme, ThemePreference};

type Seen = Rc<RefCell<Vec<AnnotationRequest>>>;

fn open(
    cx: &mut TestAppContext,
    split: bool,
) -> (gpui::Entity<DiffView>, &mut gpui::VisualTestContext, Seen) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
    });
    let (original, modified) = ("a\nb\nc\n".to_string(), "a\nB\nc\nd\n".to_string());
    let (view, cx) = cx.add_window_view(move |_, cx| {
        if split {
            DiffView::side_by_side("src/a.rs".into(), original, modified, false, cx)
        } else {
            DiffView::new("src/a.rs".into(), original, modified, false, cx)
        }
        .annotatable()
    });
    cx.simulate_resize(size(px(900.0), px(400.0)));
    cx.run_until_parked();
    let seen: Seen = Rc::default();
    let sink = seen.clone();
    cx.update(|_, cx| {
        cx.subscribe(&view, move |_, event: &DiffViewEvent, _| {
            let DiffViewEvent::Annotate(request) = event;
            sink.borrow_mut().push(request.clone());
        })
        .detach();
    });
    (view, cx, seen)
}

/// The unified row showing `line` on `column`.
fn row_of(view: &DiffView, column: DiffColumn, line: usize) -> super::pick::LinePoint {
    (0..view.rows.len())
        .filter_map(|row| view.unified_point(row))
        .find(|point| point.column == column && point.line == line)
        .expect("a row with that line")
}

#[gpui::test]
fn numbers_pick_lines_and_the_pill_sends_them(cx: &mut TestAppContext) {
    let (view, cx, seen) = open(cx, false);
    view.update(cx, |view, cx| {
        let from = row_of(view, DiffColumn::Additions, 2);
        let to = row_of(view, DiffColumn::Additions, 4);
        view.press_number(from, false, cx);
        view.drag_over(to, cx);
        view.release(cx);
        assert!(view.picking.as_ref().unwrap().pick.is_some(), "kept");
    });
    cx.run_until_parked();
    assert!(seen.borrow().is_empty(), "picking alone sends nothing");
    let pill = cx
        .debug_bounds("diff-annotate-pill")
        .expect("the pill shows");
    cx.simulate_click(pill.center(), Modifiers::none());
    let seen = seen.borrow();
    let annotation = &seen[0].annotation;
    assert_eq!(annotation.path, "src/a.rs");
    assert_eq!(annotation.side, AnnotationSide::Modified);
    assert_eq!((annotation.start_line, annotation.end_line), (2, 4));
    assert_eq!(annotation.snippet, "B\nc\nd");
    view.read_with(cx, |view, _| {
        assert!(view.picking.as_ref().unwrap().pick.is_none(), "let go")
    });
}

#[gpui::test]
fn shift_extends_and_the_only_picked_line_drops_when_pressed(cx: &mut TestAppContext) {
    let (view, cx, _) = open(cx, false);
    view.update(cx, |view, cx| {
        let first = row_of(view, DiffColumn::Additions, 1);
        let third = row_of(view, DiffColumn::Additions, 3);
        view.press_number(first, false, cx);
        view.release(cx);
        view.press_number(third, true, cx);
        view.release(cx);
        let pick = view.picking.as_ref().unwrap().pick.unwrap();
        assert_eq!((pick.anchor, pick.head), (first, third));
        // A single line pressed again goes away on release.
        view.press_number(third, false, cx);
        view.release(cx);
        view.press_number(third, false, cx);
        view.release(cx);
        assert!(view.picking.as_ref().unwrap().pick.is_none());
    });
}

#[gpui::test]
fn the_plus_sends_on_release_and_reads_the_removed_side(cx: &mut TestAppContext) {
    let (view, cx, seen) = open(cx, false);
    let removed = view.read_with(cx, |view, _| row_of(view, DiffColumn::Deletions, 2));
    let id = format!("diff-plus-{}-{}", removed.row, DiffColumn::Deletions as u8);
    let plus = cx
        .debug_bounds(Box::leak(id.into_boxed_str()))
        .expect("the + on its row");
    cx.simulate_mouse_down(plus.center(), MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(plus.center(), MouseButton::Left, Modifiers::none());
    let seen = seen.borrow();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].annotation.side, AnnotationSide::Original);
    assert_eq!(seen[0].annotation.snippet, "b");
}

#[gpui::test]
fn side_by_side_each_column_picks_its_own_lines(cx: &mut TestAppContext) {
    let (view, cx, seen) = open(cx, true);
    view.update(cx, |view, cx| {
        let row = (0..view.split_rows.len())
            .find(|row| {
                view.split_point(*row, DiffColumn::Deletions)
                    .map(|p| p.line)
                    == Some(2)
            })
            .expect("the changed row");
        let old = view.split_point(row, DiffColumn::Deletions).unwrap();
        let new = view.split_point(row, DiffColumn::Additions).unwrap();
        assert_eq!((old.line, new.line), (2, 2));
        view.press_plus(new, cx);
        view.release(cx);
    });
    assert_eq!(seen.borrow()[0].annotation.snippet, "B");
}

//! Conversations marked on the diff: the callout opens its thread, and a
//! folded line comes into view on request.

use std::cell::RefCell;
use std::rc::Rc;

use dcc_core::domain::code_annotation::{AnnotationSide, DiffColumn};
use dcc_core::domain::review_thread_detail::ReviewMark;
use gpui::{Modifiers, TestAppContext, WindowAppearance, px, size};

use super::{DiffView, DiffViewEvent};
use crate::theme::{Theme, ThemePreference};

#[gpui::test]
fn a_mark_shows_its_callout_and_its_line_comes_into_view(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light)));
    let original: String = (1..=120).map(|n| format!("line {n}\n")).collect();
    let modified = original.replace("line 60\n", "line sixty\n");
    let (view, cx) = cx.add_window_view(move |_, cx| {
        DiffView::new("src/a.rs".into(), original, modified, false, cx)
    });
    cx.simulate_resize(size(px(900.0), px(400.0)));
    cx.run_until_parked();
    let opened: Rc<RefCell<Vec<String>>> = Rc::default();
    let sink = opened.clone();
    cx.update(|_, cx| {
        cx.subscribe(&view, move |_, event: &DiffViewEvent, _| {
            if let DiffViewEvent::OpenMark(id) = event {
                sink.borrow_mut().push(id.clone());
            }
        })
        .detach();
    });
    view.update(cx, |view, cx| {
        view.set_marks(
            vec![
                ReviewMark {
                    id: "t1".into(),
                    side: AnnotationSide::Modified,
                    start_line: 59,
                    end_line: 60,
                    title: "@ana: rename".into(),
                },
                ReviewMark {
                    id: "t2".into(),
                    side: AnnotationSide::Modified,
                    start_line: 5,
                    end_line: 5,
                    title: "@bia: folded".into(),
                },
            ],
            cx,
        );
        assert!(view.marked(DiffColumn::Additions, 59));
        assert!(!view.marked(DiffColumn::Deletions, 59));
    });
    cx.run_until_parked();
    let callout = cx.debug_bounds("review-mark-t1").expect("under line 60");
    cx.simulate_click(callout.center(), Modifiers::none());
    assert_eq!(*opened.borrow(), vec!["t1".to_string()]);
    // Line 5 is folded away: revealing it opens its run.
    assert!(cx.debug_bounds("review-mark-t2").is_none());
    view.update(cx, |view, cx| {
        view.reveal_line(AnnotationSide::Modified, 5, cx)
    });
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("review-mark-t2").is_some(),
        "opened and in view"
    );
}

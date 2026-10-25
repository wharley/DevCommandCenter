//! Sending the file's code: the "+" dragged over lines, and the pill over a
//! selection.

use std::cell::RefCell;
use std::rc::Rc;

use dcc_core::domain::indentation::DEFAULT_INDENTATION;
use gpui::{Modifiers, MouseButton, TestAppContext, WindowAppearance, px, size};

use super::{FileView, FileViewEvent};
use crate::screens::annotation::AnnotationRequest;
use crate::theme::{Theme, ThemePreference};

type Seen = Rc<RefCell<Vec<AnnotationRequest>>>;

fn open(cx: &mut TestAppContext) -> (gpui::Entity<FileView>, &mut gpui::VisualTestContext, Seen) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        super::bind_keys(cx);
    });
    let text = "fn a() {\n    let x = 1;\n    let y = 2;\n}\n".to_string();
    let (view, cx) = cx.add_window_view(move |_, cx| {
        FileView::new("src/lib.rs".into(), text, DEFAULT_INDENTATION, false, cx).annotatable()
    });
    cx.simulate_resize(size(px(800.0), px(400.0)));
    view.update_in(cx, |view, window, cx| window.focus(&view.focus, cx));
    cx.run_until_parked();
    let seen: Seen = Rc::default();
    let sink = seen.clone();
    cx.update(|_, cx| {
        cx.subscribe(&view, move |_, event: &FileViewEvent, _| {
            if let FileViewEvent::Annotate(request) = event {
                sink.borrow_mut().push(request.clone());
            }
        })
        .detach();
    });
    (view, cx, seen)
}

#[gpui::test]
fn the_plus_dragged_over_lines_sends_them_whole(cx: &mut TestAppContext) {
    let (view, cx, seen) = open(cx);
    let from = cx.debug_bounds("file-plus-1").expect("the + on line 2");
    let to = cx.debug_bounds("file-plus-2").expect("the + on line 3");
    cx.simulate_mouse_down(from.center(), MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(to.center(), Some(MouseButton::Left), Modifiers::none());
    view.read_with(cx, |view, _| assert_eq!(view.picked_lines(), Some(1..3)));
    cx.simulate_mouse_up(to.center(), MouseButton::Left, Modifiers::none());
    let seen = seen.borrow();
    assert_eq!(seen.len(), 1);
    let annotation = &seen[0].annotation;
    assert_eq!((annotation.start_line, annotation.end_line), (2, 3));
    assert_eq!(annotation.snippet, "    let x = 1;\n    let y = 2;");
    view.read_with(cx, |view, _| {
        assert_eq!(view.picked_lines(), None);
        assert_eq!(
            view.text(),
            "fn a() {\n    let x = 1;\n    let y = 2;\n}\n",
            "untouched"
        );
    });
}

#[gpui::test]
fn a_selection_gets_the_pill_and_it_sends_the_selected_text(cx: &mut TestAppContext) {
    let (view, cx, seen) = open(cx);
    assert!(
        cx.debug_bounds("file-annotate-pill").is_none(),
        "nothing selected"
    );
    // "x = 1;\n    let y" across two lines.
    view.update(cx, |view, cx| {
        view.buffer.select(17..33);
        view.moved(cx);
    });
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    let pill = cx.debug_bounds("file-annotate-pill").expect("the pill");
    cx.simulate_click(pill.center(), Modifiers::none());
    {
        let seen = seen.borrow();
        let annotation = &seen[0].annotation;
        assert_eq!(annotation.snippet, "x = 1;\n    let y");
        assert_eq!((annotation.start_line, annotation.end_line), (2, 3));
    }
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("file-annotate-pill").is_none(),
        "sent: gone until another selection"
    );
    view.read_with(cx, |view, _| assert_eq!(view.buffer.selection, 17..33));
}

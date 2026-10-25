//! The card: where it opens, what it shows, and its keys and buttons.

use std::cell::RefCell;
use std::rc::Rc;

use dcc_core::domain::code_annotation::{AnnotationSide, CodeAnnotation};
use gpui::{TestAppContext, WindowAppearance, point, px, size};

use super::AnnotationRequest;
use super::card::{AnnotationCard, AnnotationCardEvent};
use super::card_view::placed;
use crate::theme::{Theme, ThemePreference};

#[test]
fn the_card_opens_over_its_trigger_else_under_it_inside_the_window() {
    // Room above: its bottom 8px over the trigger.
    assert_eq!(
        placed((100.0, 500.0), (396.0, 300.0), (1200.0, 800.0)),
        (100.0, 192.0)
    );
    // No room above: 24px under it.
    assert_eq!(
        placed((100.0, 200.0), (396.0, 300.0), (1200.0, 800.0)),
        (100.0, 224.0)
    );
    // Kept 12px inside the right and bottom edges.
    assert_eq!(
        placed((1000.0, 790.0), (396.0, 900.0), (1200.0, 800.0)),
        (792.0, 12.0)
    );
    assert_eq!(
        placed((2.0, 260.0), (396.0, 300.0), (1200.0, 400.0)),
        (12.0, 88.0)
    );
}

fn open(
    cx: &mut TestAppContext,
    side: AnnotationSide,
) -> (
    gpui::Entity<AnnotationCard>,
    &mut gpui::VisualTestContext,
    Rc<RefCell<Vec<AnnotationCardEvent>>>,
) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::text_area::bind_keys(cx);
        super::bind_keys(cx);
    });
    let request = AnnotationRequest {
        annotation: CodeAnnotation {
            path: "src/lib.rs".into(),
            side,
            start_line: 2,
            end_line: 3,
            snippet: "let x = 1;\nlet y = 2;".into(),
        },
        anchor: point(px(200.0), px(500.0)),
    };
    let (card, cx) = cx.add_window_view(|window, cx| AnnotationCard::new(request, window, cx));
    cx.simulate_resize(size(px(1200.0), px(800.0)));
    cx.run_until_parked();
    let events = Rc::new(RefCell::new(Vec::new()));
    let sink = events.clone();
    cx.update(|_, cx| {
        cx.subscribe(&card, move |_, event: &AnnotationCardEvent, _| {
            sink.borrow_mut().push(event.clone())
        })
        .detach();
    });
    (card, cx, events)
}

#[gpui::test]
fn it_shows_the_code_and_sends_with_cmd_enter(cx: &mut TestAppContext) {
    let (card, cx, events) = open(cx, AnnotationSide::Original);
    let shown = cx
        .debug_bounds("annotation-card")
        .expect("placed once measured");
    assert_eq!(f32::from(shown.size.width), 396.0);
    assert!(cx.debug_bounds("annotation-snippet").is_some());
    // Without an instruction nothing goes.
    cx.simulate_keystrokes("cmd-enter");
    assert!(events.borrow().is_empty());
    // Enter is a new line in the field.
    cx.simulate_input("Renomeie");
    cx.simulate_keystrokes("enter");
    cx.simulate_input("as duas");
    card.read_with(cx, |card, cx| {
        assert_eq!(card.text(cx), "Renomeie\nas duas")
    });
    cx.simulate_keystrokes("cmd-enter");
    assert_eq!(
        events.borrow().as_slice(),
        &[AnnotationCardEvent::Submit {
            instruction: "Renomeie\nas duas".into(),
            new_session: false,
        }]
    );
}

#[gpui::test]
fn the_other_ways_out(cx: &mut TestAppContext) {
    let (card, cx, events) = open(cx, AnnotationSide::Modified);
    cx.simulate_input("Veja");
    card.update(cx, |card, cx| {
        card.submit(true, cx);
        card.edit_in_composer(cx);
        card.add_to_review(cx);
    });
    cx.simulate_keystrokes("escape");
    assert_eq!(
        events.borrow().as_slice(),
        &[
            AnnotationCardEvent::Submit {
                instruction: "Veja".into(),
                new_session: true,
            },
            AnnotationCardEvent::EditInComposer("Veja".into()),
            AnnotationCardEvent::AddToReview("Veja".into()),
            AnnotationCardEvent::Cancel,
        ]
    );
    // A click on the backdrop closes it too.
    let backdrop = cx.debug_bounds("annotation-backdrop").unwrap();
    cx.simulate_click(
        point(backdrop.origin.x + px(4.0), backdrop.origin.y + px(4.0)),
        gpui::Modifiers::none(),
    );
    assert_eq!(events.borrow().last(), Some(&AnnotationCardEvent::Cancel));
    assert_eq!(events.borrow().len(), 5);
}

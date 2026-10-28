//! The guide driven headlessly: the three steps forward and back, what the
//! last one offers, Esc closing it, and the CLI terminal's answer.

use gpui::{Modifiers, TestAppContext, WindowAppearance, px, size};

use super::{Onboarding, OnboardingEvent};
use crate::data::onboarding::Step;
use crate::theme::{Theme, ThemePreference};

#[gpui::test]
fn the_guide_walks_its_steps_and_finishes(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
    });
    let (onboarding, cx) = cx.add_window_view(|_, cx| Onboarding::new(cx));
    cx.simulate_resize(size(px(1400.0), px(900.0)));
    let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let sink = events.clone();
    onboarding.update(cx, |_, cx| {
        cx.subscribe_self(move |_, event: &OnboardingEvent, _| {
            sink.borrow_mut().push(match event {
                OnboardingEvent::Closed => "closed",
                OnboardingEvent::Finished => "finished",
                OnboardingEvent::FinishedIntoHelp => "help",
            })
        })
        .detach();
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("onboarding-mockup").is_some());
    assert!(cx.debug_bounds("onboarding-panel-project").is_some());
    assert!(cx.debug_bounds("onboarding-skip").is_some());
    assert!(cx.debug_bounds("onboarding-help").is_none());
    // Voltar is off on the first step.
    let bounds = cx.debug_bounds("onboarding-back").unwrap();
    cx.simulate_click(bounds.center(), Modifiers::none());
    assert_eq!(
        onboarding.read_with(cx, |onboarding, _| onboarding.step),
        Step::Project
    );
    for expected in [Step::Task, Step::Workbench] {
        let bounds = cx.debug_bounds("onboarding-next").unwrap();
        cx.simulate_click(bounds.center(), Modifiers::none());
        cx.run_until_parked();
        assert_eq!(
            onboarding.read_with(cx, |onboarding, _| onboarding.step),
            expected
        );
    }
    assert!(cx.debug_bounds("onboarding-panel-workbench").is_some());
    assert!(
        cx.debug_bounds("onboarding-skip").is_none(),
        "no skipping on the last"
    );
    assert!(cx.debug_bounds("onboarding-next").is_none());
    let bounds = cx.debug_bounds("onboarding-back").unwrap();
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
    assert_eq!(
        onboarding.read_with(cx, |onboarding, _| onboarding.step),
        Step::Task
    );
    onboarding.update(cx, |onboarding, cx| onboarding.next(cx));
    cx.run_until_parked();
    let bounds = cx.debug_bounds("onboarding-help").unwrap();
    cx.simulate_click(bounds.center(), Modifiers::none());
    let bounds = cx.debug_bounds("onboarding-finish").unwrap();
    cx.simulate_click(bounds.center(), Modifiers::none());
    // Esc only closes.
    cx.update(|window, cx| {
        let focus = onboarding.read(cx).focus.clone();
        window.focus(&focus, cx);
    });
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert_eq!(events.borrow().as_slice(), ["help", "finished", "closed"]);
}

#[gpui::test]
fn the_first_step_opens_a_terminal_for_the_cli(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::Toaster::init(cx);
    });
    let (_onboarding, cx) = cx.add_window_view(|_, cx| Onboarding::new(cx));
    cx.simulate_resize(size(px(500.0), px(800.0)));
    cx.run_until_parked();
    // Narrow: the cards stack, the panel still fits.
    assert!(cx.debug_bounds("onboarding-panel-project").is_some());
    // Wide again, the button in view.
    cx.simulate_resize(size(px(1400.0), px(900.0)));
    cx.run_until_parked();
    let bounds = cx.debug_bounds("onboarding-cli").unwrap();
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
    let toasts = cx.update(|_, cx| crate::ui::Toaster::messages(cx));
    assert!(
        toasts.iter().any(|toast| toast.contains("gh auth login")),
        "{toasts:?}"
    );
}

//! Settings driven headlessly: it opens on a section the native app has,
//! the search narrows the navigation (and Esc clears it before closing),
//! the appearance's choices go out as events.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext, WindowAppearance};

use super::{SectionId, Settings, SettingsEvent, SettingsState};
use crate::i18n::Locale;
use crate::theme::{Density, Theme, ThemePreference};

type Events = Rc<RefCell<Vec<String>>>;

fn open(
    section: Option<SectionId>,
    cx: &mut TestAppContext,
) -> (Entity<Settings>, &mut VisualTestContext, Events) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(Locale::PtBr, cx);
        crate::ui::text_input::bind_keys(cx);
    });
    let state = SettingsState {
        theme: ThemePreference::System,
        locale: Locale::PtBr,
        density: Density::Comfortable,
        workspace: (Some("Checkout".into()), Some("/repo".into())),
    };
    let (settings, cx) =
        cx.add_window_view(move |window, cx| Settings::new(state, section, window, cx));
    let events: Events = Rc::default();
    let sink = events.clone();
    cx.update(|_, cx| {
        cx.subscribe(&settings, move |_, event: &SettingsEvent, _| {
            sink.borrow_mut().push(match event {
                SettingsEvent::Closed => "closed".into(),
                SettingsEvent::Theme(theme) => format!("theme {theme:?}"),
                SettingsEvent::Locale(locale) => format!("locale {locale:?}"),
                SettingsEvent::Density(density) => format!("density {density:?}"),
            })
        })
        .detach();
    });
    cx.run_until_parked();
    (settings, cx, events)
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("no {selector}"));
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
}

#[gpui::test]
fn it_opens_on_a_section_it_has_and_the_search_narrows(cx: &mut TestAppContext) {
    // "Geral" is not here yet: the first section the native app has.
    let (settings, cx, events) = open(Some(SectionId::General), cx);
    assert_eq!(
        cx.update(|_, cx| settings.read(cx).active),
        SectionId::Appearance
    );
    assert!(cx.debug_bounds("settings-section-appearance").is_some());
    assert!(cx.debug_bounds("settings-section-experimental").is_some());
    assert!(cx.debug_bounds("settings-section-general").is_none());
    cx.simulate_input("experim");
    cx.run_until_parked();
    assert!(cx.debug_bounds("settings-section-appearance").is_none());
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(
        cx.update(|_, cx| settings.read(cx).active),
        SectionId::Experimental
    );
    // Nothing matches: "Mostrar todas as seções".
    cx.simulate_input(" zzz");
    cx.run_until_parked();
    assert!(cx.debug_bounds("settings-show-all").is_some());
    // Esc clears the search first, then closes.
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(cx.debug_bounds("settings-section-appearance").is_some());
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert_eq!(events.borrow().as_slice(), ["closed"]);
}

#[gpui::test]
fn the_appearance_choices_go_out(cx: &mut TestAppContext) {
    let (settings, cx, events) = open(None, cx);
    click(cx, "settings-theme-dark");
    click(cx, "settings-locale-1");
    click(cx, "settings-density-1");
    assert_eq!(
        events.borrow().as_slice(),
        ["theme Dark", "locale En", "density Compact"]
    );
    cx.update(|_, cx| {
        let settings = settings.read(cx);
        assert_eq!(settings.theme, ThemePreference::Dark);
        assert_eq!(settings.density, Density::Compact);
    });
    click(cx, "settings-section-experimental");
    assert!(cx.debug_bounds("settings-reset-metrics").is_some());
}

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
    // "Provedores" is not here yet: the first section the native app has.
    let (settings, cx, events) = open(Some(SectionId::Model), cx);
    assert_eq!(
        cx.update(|_, cx| settings.read(cx).active),
        SectionId::General
    );
    assert!(cx.debug_bounds("settings-section-appearance").is_some());
    assert!(cx.debug_bounds("settings-section-experimental").is_some());
    assert!(cx.debug_bounds("settings-section-model").is_none());
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
    let (settings, cx, events) = open(Some(SectionId::Appearance), cx);
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

#[gpui::test]
fn general_keeps_the_mac_awake_for_the_limit_through_the_desktop(cx: &mut TestAppContext) {
    use crate::test_support::fake_desktop_routes;
    let (settings, cx, _) = open(None, cx);
    assert_eq!(
        cx.update(|_, cx| settings.read(cx).active),
        SectionId::General
    );
    // No desktop: no keep-awake card.
    assert!(cx.debug_bounds("settings-keep-awake").is_none());
    assert!(cx.debug_bounds("settings-copy-diagnostics").is_some());
    let (port, server) = fake_desktop_routes(vec![
        (
            "/api/v1/keep-awake",
            200,
            r#"{"supported":true,"mode":"off","active":null}"#.into(),
        ),
        (
            "/api/v1/keep-awake",
            200,
            r#"{"supported":true,"mode":"during_agents","active":{"reason":{"kind":"agentsWorking","count":1},"label":"x"}}"#.into(),
        ),
    ]);
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().to_path_buf();
    cx.update(|_, cx| {
        settings.update(cx, |settings, cx| {
            settings.set_api(
                crate::data::desktop_api::DesktopApi::new(port, "k"),
                data.clone(),
                cx,
            )
        })
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("settings-keep-awake").is_some());
    // On by default for new tasks: the Mac may sleep, so the warning offers
    // the hold. (The cards sit below the fold of the test window: the
    // handlers are called as the clicks would.)
    cx.update(|_, cx| settings.update(cx, |settings, cx| settings.set_auto_resume(true, cx)));
    cx.run_until_parked();
    assert!(dcc_infra::usage_limit_settings::load_settings(&data).auto_resume);
    assert!(cx.debug_bounds("settings-limit-keep-awake").is_some());
    cx.update(|_, cx| {
        settings.update(cx, |settings, cx| {
            settings.set_keep_awake(crate::data::keep_awake::Mode::DuringAgents, cx)
        })
    });
    cx.run_until_parked();
    let seen = server.join().unwrap();
    assert_eq!(seen[1].0, "PUT /api/v1/keep-awake");
    assert!(seen[1].1.contains("during_agents"), "{}", seen[1].1);
    assert!(cx.debug_bounds("settings-limit-keep-awake").is_none());
    // Copying the diagnostics says so.
    click(cx, "settings-copy-diagnostics");
    let copied = cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()));
    assert!(copied.unwrap().contains("schemaVersion"));
}

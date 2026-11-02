//! The update on offer, where the person sees it: the sky button in the
//! sidebar's footer, and Settings' version card with "Instalar atualização".

use gpui::{TestAppContext, WindowAppearance};

use super::offer_for_tests;
use crate::data::updater::{Asset, Update};
use crate::screens::settings::SectionId;
use crate::screens::sidebar::Sidebar;
use crate::theme::{Theme, ThemePreference};

fn update() -> Update {
    Update {
        version: "0.3.0".into(),
        notes: None,
        date: None,
        asset: Asset {
            signature: "s".into(),
            url: "https://x".into(),
        },
    }
}

#[gpui::test]
fn an_update_on_offer_shows_in_the_sidebar(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        offer_for_tests("0.2.0", update(), cx);
    });
    let dir = tempfile::tempdir().unwrap();
    let seen = dir.path().join("seen.json");
    let (_sidebar, cx) = cx.add_window_view(|_, cx| Sidebar::new(false, false, false, seen, cx));
    cx.run_until_parked();
    assert!(cx.debug_bounds("sidebar-update").is_some());
}

#[gpui::test]
fn settings_offer_the_update_and_a_check(cx: &mut TestAppContext) {
    cx.update(|cx| {
        offer_for_tests("0.2.0", update(), cx);
    });
    let (_settings, cx, _) = crate::screens::settings::tests::open(Some(SectionId::General), cx);
    cx.run_until_parked();
    assert!(cx.debug_bounds("settings-update-state").is_some());
    assert!(cx.debug_bounds("settings-install-update").is_some());
    assert!(cx.debug_bounds("settings-check-updates").is_some());
}

//! The dialog over a checkout of its own and a desktop that answers the
//! writes: it lists the source's skills and the context, a new skill is
//! checked then saved and compiled, a card is deleted, the catalog's preset
//! opens with its agents, no project asks for one.

use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext, WindowAppearance};

use super::{Section, Skills};
use crate::data::desktop_api::DesktopApi;
use crate::test_support::fake_desktop_routes;
use crate::theme::{Theme, ThemePreference};

fn setup(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::text_input::bind_keys(cx);
        crate::ui::text_area::bind_keys(cx);
    });
}

/// A checkout with one skill in its source.
fn checkout() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_string_lossy().into_owned();
    dcc_infra::skills::save_skill(
        &root,
        dcc_infra::skills::SkillRecord {
            name: "review-pr".into(),
            description: "Reviews a pull request".into(),
            body: "Read the diff.".into(),
            target_agents: vec!["claude".into()],
            disable_model_invocation: true,
            scope: "project".into(),
            source_url: None,
        },
    )
    .unwrap();
    dir
}

fn open(root: Option<String>, cx: &mut TestAppContext) -> (Entity<Skills>, &mut VisualTestContext) {
    setup(cx);
    let (skills, cx) =
        cx.add_window_view(move |window, cx| Skills::new(root, Some("t-1".into()), window, cx));
    cx.run_until_parked();
    (skills, cx)
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("no {selector}"));
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
}

#[gpui::test]
fn it_lists_the_source_and_saves_a_new_skill(cx: &mut TestAppContext) {
    let dir = checkout();
    let root = dir.path().to_string_lossy().into_owned();
    let (skills, cx) = open(Some(root.clone()), cx);
    cx.update(|_, cx| {
        let skills = skills.read(cx);
        assert_eq!(skills.skills.len(), 1);
        assert!(!skills.detections.is_empty());
    });
    assert!(cx.debug_bounds("skills-card-0").is_some());
    // The search narrows the cards.
    cx.simulate_input("nada");
    cx.run_until_parked();
    assert!(cx.debug_bounds("skills-card-0").is_none());
    cx.simulate_keystrokes("cmd-a backspace");
    cx.run_until_parked();
    let ok = r#"{"ok":true}"#.to_string();
    let (port, server) = fake_desktop_routes(vec![
        ("/api/v1/skills/save", 200, ok.clone()),
        ("/api/v1/skills/compile", 200, ok.clone()),
        ("/api/v1/skills/delete", 200, ok.clone()),
        ("/api/v1/skills/compile", 200, ok),
    ]);
    cx.update(|_, cx| skills.update(cx, |skills, _| skills.set_api(DesktopApi::new(port, "k"))));
    click(cx, "skills-new");
    // A bad name is refused before anything goes out.
    cx.simulate_input("Bad Name");
    click(cx, "skills-save");
    assert!(cx.update(|_, cx| skills.read(cx).form.is_some()));
    cx.simulate_keystrokes("cmd-a backspace");
    cx.simulate_input("ship-it");
    cx.update(|window, cx| {
        let form = skills.read(cx).form.as_ref().unwrap();
        let focus = gpui::Focusable::focus_handle(form.description.read(cx), cx);
        window.focus(&focus, cx);
    });
    cx.simulate_input("Ships the change");
    click(cx, "skills-target-codex");
    click(cx, "skills-save");
    cx.update(|_, cx| assert!(skills.read(cx).form.is_none()));
    click(cx, "skills-delete-0");
    let seen = server.join().unwrap();
    let paths: Vec<&str> = seen.iter().map(|(request, _)| request.as_str()).collect();
    assert_eq!(
        paths,
        [
            "POST /api/v1/skills/save",
            "POST /api/v1/skills/compile",
            "POST /api/v1/skills/delete",
            "POST /api/v1/skills/compile",
        ]
    );
    let saved: serde_json::Value = serde_json::from_str(&seen[0].1).unwrap();
    assert_eq!(saved["projectRoot"], root.as_str());
    assert_eq!(saved["workspaceId"], "t-1");
    assert_eq!(saved["skill"]["name"], "ship-it");
    assert_eq!(
        saved["skill"]["targetAgents"],
        serde_json::json!(["claude", "codex"])
    );
    let deleted: serde_json::Value = serde_json::from_str(&seen[2].1).unwrap();
    assert_eq!(deleted["name"], "review-pr");
}

#[gpui::test]
fn the_sections_and_the_catalogs_preset(cx: &mut TestAppContext) {
    let dir = checkout();
    let (skills, cx) = open(Some(dir.path().to_string_lossy().into_owned()), cx);
    click(cx, "skills-section-1");
    assert_eq!(
        cx.update(|_, cx| skills.read(cx).section),
        Section::Detected
    );
    assert!(cx.debug_bounds("skills-detection-0").is_some());
    click(cx, "skills-section-2");
    assert!(cx.debug_bounds("skills-preset-0").is_some());
    click(cx, "skills-preset-use-0");
    cx.update(|_, cx| {
        let form = skills.read(cx).form.as_ref().expect("the preset's form");
        assert!(form.from_catalog);
        assert_eq!(form.targets.len(), 5);
    });
    click(cx, "skills-back");
    assert!(cx.update(|_, cx| skills.read(cx).form.is_none()));
}

#[gpui::test]
fn without_a_project_it_asks_for_one(cx: &mut TestAppContext) {
    let (skills, cx) = open(None, cx);
    assert!(cx.update(|_, cx| skills.read(cx).skills.is_empty()));
    assert!(cx.debug_bounds("skills-section-0").is_none());
}

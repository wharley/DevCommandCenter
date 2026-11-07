//! "Gatilhos" on the Reviewer's page, driven headlessly: off until the
//! person turns it on, with the ceilings and today's runs in sight; no other
//! agent has the card.

use gpui::{AppContext, Modifiers, TestAppContext, WindowAppearance};

use super::AgentPage;
use crate::data::agent_triggers::{AgentTriggers, Trigger};
use crate::data::agents::AgentRecord;
use crate::data::loader::SidebarData;
use crate::screens::composer::Composer;
use crate::screens::sidebar::Sidebar;
use crate::theme::{Theme, ThemePreference};

fn agent(id: &str, preset: &str) -> AgentRecord {
    AgentRecord {
        id: id.into(),
        name: "Revisor".into(),
        preset: Some(preset.into()),
        avatar: dcc_core::domain::agent::AgentAvatar {
            color: "violet".into(),
            arms: 3,
            eyes: "round".into(),
        },
        provider_id: None,
        model: None,
        effort: None,
        kickoff_prompt: String::new(),
        role: String::new(),
        offer_prompt: String::new(),
        extra_instructions: String::new(),
        sessions: Vec::new(),
    }
}

#[gpui::test]
fn the_reviewers_trigger_is_off_until_turned_on(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("agent-triggers.json");
    let seen = dir.path().join("seen.json");
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::text_input::bind_keys(cx);
        cx.set_global(AgentTriggers::load(file.clone()));
    });
    let (page, cx) = cx.add_window_view(move |window, cx| {
        let sidebar = cx.new(|cx| Sidebar::new(false, false, false, seen, cx));
        let composer = cx.new(|cx| Composer::new(window, cx));
        AgentPage::new("a-1".into(), sidebar, composer, cx)
    });
    page.update(cx, |page, cx| {
        page.sidebar.update(cx, |sidebar, cx| {
            sidebar.apply_for_tests(
                SidebarData {
                    agents: vec![agent("a-1", "reviewer"), agent("a-2", "researcher")],
                    ..SidebarData::default()
                },
                cx,
            )
        })
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("agent-triggers").is_some());
    assert!(cx.debug_bounds("agent-triggers-limits").is_some());
    // The switch sits inside the card, however long its text.
    let card = cx.debug_bounds("agent-triggers").unwrap();
    let switch = cx.debug_bounds("agent-trigger-prUpdated").unwrap();
    assert!(
        switch.right() <= card.right() && switch.left() >= card.left(),
        "{switch:?} outside {card:?}"
    );
    let on = |cx: &mut gpui::VisualTestContext| {
        cx.update(|_, cx| {
            cx.global::<AgentTriggers>()
                .is_on("a-1", Trigger::PrUpdated)
        })
    };
    assert!(!on(cx));
    let mood = |cx: &mut gpui::VisualTestContext| {
        page.update(cx, |page, cx| page.sidebar.read(cx).mood("a-1", cx))
    };
    assert_eq!(
        mood(cx),
        Some(crate::agent::Mood::Asleep),
        "nothing wakes it"
    );
    assert!(cx.debug_bounds("agent-page-mood").is_some());

    let switch = cx.debug_bounds("agent-trigger-prUpdated").unwrap();
    cx.simulate_click(switch.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(on(cx));
    // The CI trigger has its own switch, still off.
    assert!(cx.debug_bounds("agent-trigger-ciFailed").is_some());
    assert!(cx.debug_bounds("agent-trigger-reviewReceived").is_some());
    assert!(cx.update(|_, cx| !cx.global::<AgentTriggers>().is_on("a-1", Trigger::CiFailed)));
    assert!(AgentTriggers::load(file).is_on("a-1", Trigger::PrUpdated));
    assert_eq!(
        mood(cx),
        Some(crate::agent::Mood::Watching),
        "a trigger is on"
    );

    page.update(cx, |page, cx| page.show("a-2".into(), cx));
    cx.run_until_parked();
    assert!(cx.debug_bounds("agent-triggers").is_none());
}

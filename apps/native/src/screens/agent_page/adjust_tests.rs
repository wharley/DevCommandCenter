//! A change an agent proposed, driven headlessly: the card shows what it
//! is about with its before and after; "Aplicar" writes it in the desktop
//! and the card leaves; with two waiting, "Aplicar todos" answers both.

use dcc_core::domain::agent_change::{AgentChange, Change};
use gpui::{AppContext, Modifiers, TestAppContext, WindowAppearance};

use super::AgentPage;
use crate::data::agents::AgentRecord;
use crate::data::loader::SidebarData;
use crate::screens::composer::Composer;
use crate::screens::sidebar::Sidebar;
use crate::test_support::fake_desktop_routes;
use crate::theme::{Theme, ThemePreference};

fn agent(id: &str, name: &str) -> AgentRecord {
    let mut record = super::editor_role::blank_agent();
    record.id = id.into();
    record.name = name.into();
    record.role = "r".into();
    record
}

fn change(id: &str, change: Change) -> AgentChange {
    AgentChange {
        id: id.into(),
        agent_id: "guia".into(),
        session_id: "s-home".into(),
        change,
        reason: "A fonte está na Geral, que não tem projeto.".into(),
        created_at: "2026-11-17T12:00:00.000Z".into(),
    }
}

#[gpui::test]
fn a_change_proposed_is_applied_from_its_card_one_or_all(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::text_input::bind_keys(cx);
        crate::ui::Toaster::init(cx);
    });
    let dir = tempfile::tempdir().unwrap();
    let seen = dir.path().join("seen.json");
    let ok = || r#"{"agentId":"sup"}"#.to_string();
    let (port, desktop) = fake_desktop_routes(vec![
        ("/api/v1/agent-changes/c-1/accept", 200, ok()),
        ("/api/v1/agent-changes/c-2/accept", 200, ok()),
    ]);
    let (page, cx) = cx.add_window_view(move |window, cx| {
        let sidebar = cx.new(|cx| Sidebar::new(false, false, false, seen, cx));
        let composer = cx.new(|cx| Composer::new(window, cx));
        AgentPage::new("guia".into(), sidebar, composer, cx)
    });
    cx.simulate_resize(gpui::size(gpui::px(1200.0), gpui::px(1400.0)));
    let moved = Change::Source {
        source_id: "src".into(),
        agent_id: "sup".into(),
        connector: "Slack".into(),
        watch: "menções a mim".into(),
        room: Some((None, Some("app".into()))),
        every_minutes: None,
        enabled: None,
    };
    let projects = Change::Projects {
        agent_id: "sup".into(),
        before: Vec::new(),
        after: vec!["app".into()],
    };
    page.update(cx, |page, cx| {
        page.sidebar.update(cx, |sidebar, cx| {
            sidebar.apply_for_tests(
                SidebarData {
                    agents: vec![agent("guia", "Guia"), agent("sup", "Suporte")],
                    agent_changes: vec![change("c-1", moved), change("c-2", projects)],
                    ..SidebarData::default()
                },
                cx,
            )
        });
        page.open_room(None, cx);
    });
    cx.run_until_parked();
    page.update(cx, |page, cx| {
        let room = page.room.as_ref().unwrap().0.clone();
        room.update(cx, |room, _| {
            room.api = Some(crate::data::desktop_api::DesktopApi::new(port, "k"));
        });
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("room-agent-change-0").is_some());
    assert!(cx.debug_bounds("room-agent-change-1").is_some());

    // Two waiting: one click applies both, and the cards leave.
    let all = cx.debug_bounds("room-change-apply-all").unwrap();
    cx.simulate_click(all.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(cx.debug_bounds("room-agent-change-0").is_none());
    assert!(
        crate::test_support::desktop_done(&desktop, || cx.run_until_parked()),
        "the desktop never got the calls"
    );
    let seen = desktop.join().unwrap();
    let calls: Vec<&str> = seen.iter().map(|(call, _)| call.as_str()).collect();
    assert_eq!(
        calls,
        [
            "POST /api/v1/agent-changes/c-1/accept",
            "POST /api/v1/agent-changes/c-2/accept"
        ]
    );
}

#[gpui::test]
fn a_grant_takes_its_own_click_and_the_app_writes_it(cx: &mut TestAppContext) {
    use crate::data::agent_triggers::AgentTriggers;
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::text_input::bind_keys(cx);
        crate::ui::Toaster::init(cx);
    });
    let dir = tempfile::tempdir().unwrap();
    let seen = dir.path().join("seen.json");
    let switches = dir.path().join("agent-triggers.json");
    cx.update(|cx| cx.set_global(AgentTriggers::load(switches)));
    let ok = || r#"{"answered":true}"#.to_string();
    let (port, desktop) = fake_desktop_routes(vec![
        ("/api/v1/agent-changes/c-1/accept", 200, ok()),
        ("/api/v1/agent-changes/c-2/accept", 200, ok()),
        ("/api/v1/agent-changes/g-1/dismiss", 200, ok()),
    ]);
    let (page, cx) = cx.add_window_view(move |window, cx| {
        let sidebar = cx.new(|cx| Sidebar::new(false, false, false, seen, cx));
        let composer = cx.new(|cx| Composer::new(window, cx));
        AgentPage::new("guia".into(), sidebar, composer, cx)
    });
    cx.simulate_resize(gpui::size(gpui::px(1200.0), gpui::px(1600.0)));
    let every = Change::Source {
        source_id: "src".into(),
        agent_id: "sup".into(),
        connector: "Slack".into(),
        watch: "menções a mim".into(),
        room: None,
        every_minutes: Some((15, 30)),
        enabled: None,
    };
    let projects = Change::Projects {
        agent_id: "sup".into(),
        before: Vec::new(),
        after: vec!["app".into()],
    };
    let grant = Change::Grants {
        agent_id: "sup".into(),
        review: None,
        review_projects: None,
        reply: Some((false, true)),
        reply_rooms: Some((Vec::new(), vec!["app".into()])),
        deliver: None,
        deliver_projects: None,
        fix_in_task: None,
        triggers: Vec::new(),
    };
    page.update(cx, |page, cx| {
        page.sidebar.update(cx, |sidebar, cx| {
            sidebar.apply_for_tests(
                SidebarData {
                    agents: vec![agent("guia", "Guia"), agent("sup", "Suporte")],
                    agent_changes: vec![
                        change("c-1", every),
                        change("g-1", grant),
                        change("c-2", projects),
                    ],
                    ..SidebarData::default()
                },
                cx,
            )
        });
        page.open_room(None, cx);
    });
    cx.run_until_parked();
    page.update(cx, |page, cx| {
        let room = page.room.as_ref().unwrap().0.clone();
        room.update(cx, |room, _| {
            room.api = Some(crate::data::desktop_api::DesktopApi::new(port, "k"));
        });
    });
    cx.run_until_parked();

    // "Aplicar todos" takes the two changes and leaves the grant waiting,
    // with nothing granted.
    let all = cx.debug_bounds("room-change-apply-all").unwrap();
    cx.simulate_click(all.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(cx.debug_bounds("room-agent-change-0").is_some());
    assert!(cx.debug_bounds("room-agent-change-1").is_none());
    cx.update(|_, cx| {
        assert!(!cx.global::<AgentTriggers>().reply_send_on("sup"));
    });

    // Its own click: the app writes it, and the desktop only hears the
    // card was answered.
    let apply = cx.debug_bounds("room-change-apply-0").unwrap();
    cx.simulate_click(apply.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(cx.debug_bounds("room-agent-change-0").is_none());
    cx.update(|_, cx| {
        let triggers = cx.global::<AgentTriggers>();
        assert!(triggers.reply_send_on("sup") && triggers.reply_send_in("sup", "app"));
        assert!(!triggers.deliver_on("sup"));
    });
    assert!(
        crate::test_support::desktop_done(&desktop, || cx.run_until_parked()),
        "the desktop never got the calls"
    );
    let seen = desktop.join().unwrap();
    let calls: Vec<&str> = seen.iter().map(|(call, _)| call.as_str()).collect();
    assert_eq!(
        calls,
        [
            "POST /api/v1/agent-changes/c-1/accept",
            "POST /api/v1/agent-changes/c-2/accept",
            "POST /api/v1/agent-changes/g-1/dismiss"
        ]
    );
}

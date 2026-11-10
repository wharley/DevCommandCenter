//! Sources on screen, driven headlessly: a proposed one is a card in the
//! room, a source's news reads as one line, and the agent's page lists the
//! ones it watches.

use dcc_core::domain::agent_source::{AgentSource, SourceItem, source_prompt};
use gpui::{AppContext, TestAppContext, WindowAppearance};

use super::AgentPage;
use crate::data::agent_sources::Sources;
use crate::data::loader::SidebarData;
use crate::data::team_room::{HomeFeed, RoomLine, Speaker};
use crate::screens::composer::Composer;
use crate::screens::sidebar::Sidebar;
use crate::theme::{Theme, ThemePreference};

fn source(id: &str, watch: &str, proposed: bool) -> AgentSource {
    AgentSource {
        id: id.into(),
        agent_id: "sup".into(),
        connector_id: "slack".into(),
        connector: "Slack".into(),
        watch: watch.into(),
        tool: "slack_search_public".into(),
        arguments: "{}".into(),
        every_minutes: 15,
        enabled: true,
        proposed,
        last_checked_at: None,
        created_at: "2026-11-09T10:00:00Z".into(),
        room: None,
    }
}

#[gpui::test]
fn sources_show_as_cards_lines_and_on_the_page(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        cx.set_global(Sources {
            list: vec![
                source("s-1", "menções a mim", false),
                source("s-2", "#suporte-privado", true),
            ],
        });
    });
    let dir = tempfile::tempdir().unwrap();
    let seen = dir.path().join("seen.json");
    let (page, cx) = cx.add_window_view(move |window, cx| {
        let sidebar = cx.new(|cx| Sidebar::new(false, false, false, seen, cx));
        let composer = cx.new(|cx| Composer::new(window, cx));
        AgentPage::new("sup".into(), sidebar, composer, cx)
    });
    cx.simulate_resize(gpui::size(gpui::px(1200.0), gpui::px(2400.0)));
    let mut suporte = super::editor_role::blank_agent();
    suporte.id = "sup".into();
    suporte.name = "Suporte".into();
    page.update(cx, |page, cx| {
        page.sidebar.update(cx, |sidebar, cx| {
            sidebar.apply_for_tests(
                SidebarData {
                    agents: vec![suporte],
                    ..SidebarData::default()
                },
                cx,
            )
        });
        page.open_room(None, cx);
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("room-source-proposal-0").is_some());
    // One asked in the Checkout's room shows there, not in Geral.
    cx.update(|_, cx| {
        let mut asked_in_checkout = source("s-3", "#checkout-alertas", true);
        asked_in_checkout.room = Some("checkout".into());
        cx.set_global(Sources {
            list: vec![asked_in_checkout],
        });
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("room-source-proposal-0").is_none());
    page.update(cx, |page, cx| {
        page.open_room_of(Some("checkout".into()), cx)
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("room-source-proposal-0").is_some());
    // Back to Geral and to the sources it started with.
    cx.update(|_, cx| {
        cx.set_global(Sources {
            list: vec![
                source("s-1", "menções a mim", false),
                source("s-2", "#suporte-privado", true),
            ],
        });
    });
    page.update(cx, |page, cx| page.open_room(None, cx));
    cx.run_until_parked();
    page.update(cx, |page, cx| {
        let room = page.room.as_ref().unwrap().0.clone();
        room.update(cx, |room, cx| {
            let news = [SourceItem {
                id: "x".into(),
                text: "Giovanni: @Wharley o campo X?".into(),
            }];
            let mut feed = HomeFeed::new("sup", "s-home");
            feed.lines = vec![RoomLine {
                key: "s-home:m-1".into(),
                agent_id: "sup".into(),
                session_id: "s-home".into(),
                speaker: Speaker::Person,
                text: source_prompt(&source("s-1", "menções a mim", false), &news),
                at: "2026-11-09T10:00:00Z".into(),
                receipt: None,
                tasks: Vec::new(),
            }];
            room.take(vec![feed]);
            cx.notify();
        });
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("room-source-0").is_some());

    // The page lists the one it watches, not the proposal.
    page.update(cx, |page, cx| page.show("sup".into(), cx));
    cx.run_until_parked();
    assert!(cx.debug_bounds("agent-sources").is_some());
    assert!(cx.debug_bounds("agent-source-0").is_some());
    assert!(cx.debug_bounds("agent-source-1").is_none());
}

//! 👍 / 👎 in the team room, driven headlessly: a 👍 is sent and shows; a 👎
//! opens "Guardar como regra?", and what the person writes goes to the
//! agent's memory as their note.

use gpui::{AppContext, Modifiers, TestAppContext, WindowAppearance};

use super::AgentPage;
use crate::data::agents::{AgentSession, AgentState};
use crate::data::loader::SidebarData;
use crate::data::team_room::{HomeFeed, RoomLine, Speaker};
use crate::screens::composer::Composer;
use crate::screens::sidebar::Sidebar;
use crate::test_support::fake_desktop_routes;
use crate::theme::{Theme, ThemePreference};
use dcc_core::domain::agent_reaction::Reaction;

#[gpui::test]
fn an_answer_is_marked_and_a_thumbs_down_keeps_a_rule(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::text_input::bind_keys(cx);
    });
    let dir = tempfile::tempdir().unwrap();
    let seen = dir.path().join("seen.json");
    let (port, desktop) = fake_desktop_routes(vec![
        ("/api/v1/agents/rev/reactions", 200, "{}".into()),
        ("/api/v1/agents/rev/reactions", 200, "{}".into()),
        (
            "/api/v1/agents/rev/memory",
            200,
            r#"{"notes":[],"versions":[]}"#.into(),
        ),
    ]);
    let (page, cx) = cx.add_window_view(move |window, cx| {
        let sidebar = cx.new(|cx| Sidebar::new(false, false, false, seen, cx));
        let composer = cx.new(|cx| Composer::new(window, cx));
        AgentPage::new("rev".into(), sidebar, composer, cx)
    });
    cx.simulate_resize(gpui::size(gpui::px(1200.0), gpui::px(1400.0)));
    let mut reviewer = super::editor_role::blank_agent();
    reviewer.id = "rev".into();
    reviewer.name = "Revisor".into();
    reviewer.sessions = vec![AgentSession {
        session_id: "s-home".into(),
        workspace_id: "w-home".into(),
        state: AgentState::Idle,
        completed_at: None,
        project_id: "dcc-agent-rev".into(),
        title: None,
        updated_at: "2026-11-09T10:01:00Z".into(),
        room: None,
    }];
    page.update(cx, |page, cx| {
        page.sidebar.update(cx, |sidebar, cx| {
            sidebar.apply_for_tests(
                SidebarData {
                    agents: vec![reviewer],
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
        room.update(cx, |room, cx| {
            room.api = Some(crate::data::desktop_api::DesktopApi::new(port, "k"));
            let mut feed = HomeFeed::new("rev", "s-home");
            feed.read_at = "2026-11-09T10:01:00Z".into();
            feed.lines = vec![RoomLine {
                key: "s-home:t-1".into(),
                agent_id: "rev".into(),
                session_id: "s-home".into(),
                speaker: Speaker::Agent,
                text: "Revisei os 3 PRs.".into(),
                at: "2026-11-09T10:00:42Z".into(),
                receipt: None,
                tasks: Vec::new(),
                steps: Default::default(),
            }];
            room.take(vec![feed]);
            cx.notify();
        });
    });
    cx.run_until_parked();

    let up = cx.debug_bounds("room-react-up-0").unwrap();
    cx.simulate_click(up.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(cx.debug_bounds("room-rule").is_none());
    // The reaction given leads the line, before the receipt and its link.
    let up = cx.debug_bounds("room-react-up-0").unwrap();
    let open = cx.debug_bounds("room-open-0").unwrap();
    assert!(
        up.right() <= open.left(),
        "the pill sits before \"Abrir conversa\""
    );
    let down = cx.debug_bounds("room-react-down-0").unwrap();
    cx.simulate_click(down.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(cx.debug_bounds("room-rule").is_some());
    page.update(cx, |page, cx| {
        let room = page.room.as_ref().unwrap().0.clone();
        room.update(cx, |room, cx| {
            let line = room.lines()[0].clone();
            assert_eq!(room.reaction(&line), Some(Reaction::Down));
            let input = room.rule.as_ref().unwrap().input.clone();
            input.update(cx, |input, cx| input.set_text("Sempre cite o PR", cx));
        });
    });
    let keep = cx.debug_bounds("room-rule-keep").unwrap();
    cx.simulate_click(keep.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(cx.debug_bounds("room-rule").is_none());
    assert!(
        crate::test_support::desktop_done(&desktop, || cx.run_until_parked()),
        "the desktop never got the call"
    );
    let seen = desktop.join().unwrap();
    assert!(seen[0].1.contains(r#""reaction":"up""#), "{}", seen[0].1);
    assert!(seen[1].1.contains(r#""reaction":"down""#));
    assert_eq!(seen[2].0, "POST /api/v1/agents/rev/memory");
    assert!(seen[2].1.contains("Sempre cite o PR"));
}

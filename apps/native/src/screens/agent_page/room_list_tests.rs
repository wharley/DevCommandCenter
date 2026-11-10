//! The room's list, headlessly: a long room builds only the rows on
//! screen, a spinner's tick keeps the rows while a real change reads them
//! again, and the list keeps to the bottom until the person scrolls up,
//! starting over at the bottom on another room.

use gpui::{AppContext, Entity, TestAppContext, VisualTestContext, WindowAppearance, px};

use super::AgentPage;
use super::room::TeamRoom;
use crate::data::agents::{AgentRecord, AgentSession, AgentState};
use crate::data::loader::SidebarData;
use crate::data::team_room::{HomeFeed, RoomLine, Speaker};
use crate::screens::composer::Composer;
use crate::screens::sidebar::Sidebar;
use crate::theme::{Theme, ThemePreference};

/// Lines in the long room.
const LINES: usize = 500;

fn session(id: &str, room: Option<&str>, state: AgentState) -> AgentSession {
    AgentSession {
        session_id: id.into(),
        workspace_id: "w-home".into(),
        state,
        completed_at: None,
        project_id: "dcc-agent-dev".into(),
        title: None,
        updated_at: "2026-11-10T10:00:00Z".into(),
        room: room.map(str::to_string),
    }
}

pub(super) fn dev(state: AgentState) -> AgentRecord {
    AgentRecord {
        id: "dev".into(),
        name: "Dev".into(),
        preset: None,
        avatar: dcc_core::domain::agent::AgentAvatar {
            color: "purple".into(),
            arms: 2,
            eyes: "round".into(),
            shape: "robot".into(),
            accessory: "none".into(),
        },
        provider_id: Some("codex".into()),
        model: None,
        effort: None,
        kickoff_prompt: String::new(),
        role: "r".into(),
        offer_prompt: String::new(),
        extra_instructions: String::new(),
        projects: Vec::new(),
        monthly_token_limit: None,
        tool_access: Default::default(),
        auto_start_tasks: false,
        may_call: Vec::new(),
        account_id: None,
        sessions: vec![
            session("s-geral", None, state),
            session("s-checkout", Some("checkout"), AgentState::Idle),
        ],
    }
}

fn line(index: usize) -> RoomLine {
    let agent = index % 2 == 1;
    RoomLine {
        key: format!("l-{index}"),
        agent_id: "dev".into(),
        session_id: "s-geral".into(),
        speaker: if agent {
            Speaker::Agent
        } else {
            Speaker::Person
        },
        text: if agent {
            format!("Feito, **linha {index}**:\n\n- um\n- dois")
        } else {
            format!("pedido {index}")
        },
        at: format!(
            "2026-11-0{}T{:02}:{:02}:00Z",
            1 + index / 1440,
            index / 60 % 24,
            index % 60
        ),
        receipt: None,
        tasks: Vec::new(),
        steps: Default::default(),
    }
}

/// The team room open on Geral with `count` lines.
fn long_room(
    cx: &mut TestAppContext,
    count: usize,
    state: AgentState,
) -> (Entity<AgentPage>, Entity<TeamRoom>, &mut VisualTestContext) {
    room_with_lines(cx, (0..count).map(line).collect(), state)
}

/// The team room open on Geral with these lines of "Dev".
pub(super) fn room_with_lines(
    cx: &mut TestAppContext,
    lines: Vec<RoomLine>,
    state: AgentState,
) -> (Entity<AgentPage>, Entity<TeamRoom>, &mut VisualTestContext) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::text_input::bind_keys(cx);
        cx.set_reduce_motion(false);
    });
    let dir = tempfile::tempdir().unwrap();
    let seen = dir.path().join("seen.json");
    let (page, cx) = cx.add_window_view(move |window, cx| {
        let sidebar = cx.new(|cx| Sidebar::new(false, false, false, seen, cx));
        let composer = cx.new(|cx| Composer::new(window, cx));
        AgentPage::new("dev".into(), sidebar, composer, cx)
    });
    cx.simulate_resize(gpui::size(px(1200.0), px(1400.0)));
    page.update(cx, |page, cx| {
        page.sidebar.update(cx, |sidebar, cx| {
            sidebar.apply_for_tests(
                SidebarData {
                    agents: vec![dev(state)],
                    ..SidebarData::default()
                },
                cx,
            )
        });
        page.open_room(None, cx);
    });
    cx.run_until_parked();
    let room = page.update(cx, |page, _| page.room.as_ref().unwrap().0.clone());
    room.update(cx, |room, cx| {
        let mut feed = HomeFeed::new("dev", "s-geral");
        feed.read_at = "2026-11-10T10:00:00Z".into();
        feed.lines = lines;
        room.take(vec![feed]);
        cx.notify();
    });
    cx.run_until_parked();
    (page, room, cx)
}

#[gpui::test]
fn a_long_room_builds_only_the_rows_on_screen(cx: &mut TestAppContext) {
    let (_page, room, cx) = long_room(cx, LINES, AgentState::Idle);
    // The last of the `LINES`.
    assert!(
        cx.debug_bounds("room-answer-499").is_some(),
        "it opens at the bottom"
    );
    assert!(cx.debug_bounds("room-said-0").is_none());
    let parsed = room.update(cx, |room, _| room.markdown.borrow().parses);
    assert!(
        parsed < 60,
        "only the answers drawn are parsed, not {} ({parsed})",
        LINES / 2
    );
    // One more frame: the rows on screen, not the whole room.
    room.update(cx, |room, cx| {
        room.list.rows_built.set(0);
        cx.notify();
    });
    cx.run_until_parked();
    let built = room.update(cx, |room, _| room.list.rows_built.get());
    assert!(
        built > 0 && built < 80,
        "{built} rows built of {}",
        LINES + 1
    );
    // The answers drawn again are not parsed again.
    assert_eq!(
        room.update(cx, |room, _| room.markdown.borrow().parses),
        parsed
    );
}

#[gpui::test]
fn a_spinners_tick_keeps_the_rows_and_a_change_reads_them_again(cx: &mut TestAppContext) {
    // Working: "Dev está trabalhando…" bobs on the shared clock.
    let (_page, room, cx) = long_room(cx, 40, AgentState::Working);
    assert!(cx.debug_bounds("room-busy-dev").is_some());
    let (builds, built) = room.update(cx, |room, _| (room.list.builds, room.list.rows_built.get()));
    cx.executor().advance_clock(crate::ui::pulse::TICK * 6);
    cx.run_until_parked();
    let (after, drawn) = room.update(cx, |room, _| (room.list.builds, room.list.rows_built.get()));
    assert!(drawn > built, "the bob redraws the rows on screen");
    assert_eq!(after, builds, "the room's data is not read again on a tick");

    // A real change in the room reads it again, and the new line shows.
    room.update(cx, |room, cx| {
        let mut feed = room.feeds[0].clone();
        feed.lines.push(line(40));
        room.take(vec![feed]);
        cx.notify();
    });
    cx.run_until_parked();
    assert_eq!(room.update(cx, |room, _| room.list.builds), builds + 1);
    assert!(cx.debug_bounds("room-said-40").is_some());
}

#[gpui::test]
fn it_keeps_to_the_bottom_until_scrolled_up_and_a_switch_starts_over(cx: &mut TestAppContext) {
    let (page, room, cx) = long_room(cx, 200, AgentState::Idle);
    let following =
        |cx: &mut VisualTestContext| room.update(cx, |room, _| room.list.state.is_following_tail());
    let add = |index: usize, cx: &mut VisualTestContext| {
        room.update(cx, |room, cx| {
            let mut feed = room.feeds[0].clone();
            feed.lines.push(line(index));
            room.take(vec![feed]);
            cx.notify();
        });
        cx.run_until_parked();
    };
    assert!(following(cx));
    add(200, cx);
    assert!(
        cx.debug_bounds("room-said-200").is_some(),
        "new lines follow"
    );

    // Scrolled up, a new line does not pull the person down.
    room.update(cx, |room, cx| {
        room.list.state.scroll_by(px(-3000.0));
        cx.notify();
    });
    cx.run_until_parked();
    assert!(!following(cx));
    add(201, cx);
    assert!(cx.debug_bounds("room-answer-201").is_none());

    // Another room opens at its bottom, and so does Geral again.
    page.update(cx, |page, cx| {
        page.open_room_of(Some("checkout".into()), cx)
    });
    cx.run_until_parked();
    assert!(following(cx));
    page.update(cx, |page, cx| page.open_room_of(None, cx));
    cx.run_until_parked();
    assert!(following(cx));

    // On the agent alone (`room_focus`) too.
    room.update(cx, |room, cx| {
        room.list.state.scroll_by(px(-3000.0));
        cx.notify();
    });
    cx.run_until_parked();
    room.update(cx, |room, cx| room.set_focus(Some("dev".into()), cx));
    cx.run_until_parked();
    assert!(following(cx));
}

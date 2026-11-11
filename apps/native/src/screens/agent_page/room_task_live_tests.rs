//! A task at work under the room, headlessly: its row follows the task's own
//! conversation — what it does now, its last steps folded under "+N passos",
//! a clock that moves — and leaves once that turn ends.

use dcc_core::domain::session::{SessionEventKind, SessionEventRecord, SessionId};
use gpui::{AppContext, Entity, TestAppContext, VisualTestContext, WindowAppearance};

use super::AgentPage;
use super::room::TeamRoom;
use crate::data::activity::TaskActivity;
use crate::data::agents::{AgentRecord, AgentSession, AgentState};
use crate::data::loader::SidebarData;
use crate::data::task_agents::OpenedTask;
use crate::data::tasks::TaskSummary;
use crate::data::team_room::HomeFeed;
use crate::data::team_room_poll::{LivePoll, Polled};
use crate::screens::composer::Composer;
use crate::screens::sidebar::Sidebar;
use crate::theme::{Theme, ThemePreference};
use dcc_core::domain::attention::{AgentActivity, AgentActivityState};

fn ago(seconds: i64) -> String {
    (chrono::Utc::now() - chrono::Duration::seconds(seconds))
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn event(sequence: u64, json: serde_json::Value) -> SessionEventRecord {
    SessionEventRecord {
        event_id: format!("e-{sequence}"),
        session_id: SessionId("s-task".into()),
        sequence,
        occurred_at: ago(90 - sequence as i64),
        kind: serde_json::from_value::<SessionEventKind>(json).unwrap(),
    }
}

fn tool(
    sequence: u64,
    id: &str,
    action: &str,
    file: Option<&str>,
    command: Option<&str>,
) -> SessionEventRecord {
    event(
        sequence,
        serde_json::json!({
            "type": "turn_tool_call_started", "turnId": "t1", "toolCallId": id,
            "action": action, "command": command, "file": file
        }),
    )
}

fn done(sequence: u64, id: &str) -> SessionEventRecord {
    event(
        sequence,
        serde_json::json!({ "type": "turn_tool_call_completed", "turnId": "t1", "toolCallId": id }),
    )
}

/// The task's turn: four steps done and an edit running.
fn at_work() -> Vec<SessionEventRecord> {
    vec![
        event(
            1,
            serde_json::json!({
                "type": "turn_started", "turnId": "t1", "prompt": "corrige o domínio",
                "planMode": false, "model": null, "evidence": null,
                "retryOfTurnId": null, "approvalPolicy": "ask"
            }),
        ),
        tool(2, "c1", "Read", Some("/repo/src/a.ts"), None),
        done(3, "c1"),
        tool(4, "c2", "Read", Some("/repo/src/b.ts"), None),
        done(5, "c2"),
        tool(6, "c3", "Bash", None, Some("yarn test")),
        done(7, "c3"),
        tool(8, "c4", "Read", Some("/repo/src/c.ts"), None),
        done(9, "c4"),
        tool(10, "c5", "Edit", Some("/repo/src/checkout.tsx"), None),
    ]
}

fn dev() -> AgentRecord {
    AgentRecord {
        id: "dev".into(),
        name: "Dev".into(),
        preset: None,
        avatar: dcc_core::domain::agent_proposal::mascot_for("Dev"),
        provider_id: Some("claude_code".into()),
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
        sessions: vec![AgentSession {
            session_id: "s-geral".into(),
            workspace_id: "w-home".into(),
            state: AgentState::Idle,
            completed_at: None,
            project_id: "dcc-agent-dev".into(),
            title: None,
            updated_at: "2026-11-10T15:39:00Z".into(),
            room: None,
        }],
    }
}

/// Geral, with a task the Dev opened from it running.
fn room_with_task(
    cx: &mut TestAppContext,
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
    cx.simulate_resize(gpui::size(gpui::px(1200.0), gpui::px(1400.0)));
    let mut task = TaskSummary::test("w-dominio");
    task.name = "Corrigir domínio principal".into();
    task.project_id = "checkout".into();
    let activity = TaskActivity {
        activity: AgentActivity {
            state: AgentActivityState::Active,
            started_at: Some(ago(120)),
            completed_at: None,
            waiting_since: None,
            usage_limit: None,
        },
        last_interaction_at: None,
        waiting_session_ids: Vec::new(),
        running_session_ids: vec!["s-task".into()],
        completed_by_session: Vec::new(),
    };
    page.update(cx, |page, cx| {
        page.sidebar.update(cx, |sidebar, cx| {
            sidebar.apply_for_tests(
                SidebarData {
                    agents: vec![dev()],
                    tasks: vec![task],
                    activities: [("w-dominio".to_string(), activity)].into(),
                    opened_tasks: [(
                        "w-dominio".to_string(),
                        OpenedTask {
                            agent_id: "dev".into(),
                            session_id: "s-geral".into(),
                            task_session_id: Some("s-task".into()),
                        },
                    )]
                    .into(),
                    ..SidebarData::default()
                },
                cx,
            )
        });
        page.open_room(None, cx);
    });
    cx.run_until_parked();
    let room = page.update(cx, |page, _| page.room.as_ref().unwrap().0.clone());
    (page, room, cx)
}

/// "1:29" in seconds.
fn seconds(clock: &str) -> i64 {
    let (minutes, seconds) = clock.split_once(':').unwrap();
    minutes.parse::<i64>().unwrap() * 60 + seconds.parse::<i64>().unwrap()
}

fn drawn(room: &Entity<TeamRoom>, cx: &mut VisualTestContext) -> (String, String) {
    room.update(cx, |room, _| {
        room.live
            .drawn
            .borrow()
            .get("room-running-0")
            .cloned()
            .unwrap_or_default()
    })
}

/// A beat's read of the task's conversation as `events`.
fn beat(room: &Entity<TeamRoom>, events: Vec<SessionEventRecord>, cx: &mut VisualTestContext) {
    room.update(cx, |room, cx| {
        let feed = room
            .live
            .tasks
            .iter()
            .find(|feed| feed.session_id == "s-task")
            .unwrap();
        let poll = LivePoll::for_tests(
            feed,
            HomeFeed::projected("dev", "s-task", events),
            Polled::Turn,
        );
        let generation = room.room_generation;
        room.take_live(vec![poll], generation, cx);
    });
    cx.run_until_parked();
}

#[gpui::test]
fn a_running_task_shows_what_it_does_and_leaves_when_its_turn_ends(cx: &mut TestAppContext) {
    let (_page, room, cx) = room_with_task(cx);
    // Before its conversation is read: the row, its clock, no label yet.
    assert!(cx.debug_bounds("room-running-0").is_some());
    assert!(cx.debug_bounds("room-running-0-clock").is_some());
    assert!(cx.debug_bounds("room-running-0-doing").is_none());
    // The live loop follows the task's own conversation, and only it.
    let followed = room.update(cx, |room, cx| {
        room.task_probes(cx);
        room.followed_tasks(cx)
    });
    assert_eq!(followed, vec![("dev".to_string(), "s-task".to_string())]);

    beat(&room, at_work(), cx);
    for selector in [
        "room-running-0-doing",
        "room-running-0-open",
        "room-running-0-step-more",
        "room-running-0-step-0",
        "room-running-0-step-2",
    ] {
        assert!(cx.debug_bounds(selector).is_some(), "{selector}");
    }
    assert!(
        cx.debug_bounds("room-running-0-step-3").is_none(),
        "three steps shown"
    );
    let (first, label) = drawn(&room, cx);
    assert_eq!(label, "Editando src/checkout.tsx");
    // Since the turn started, 89 s before.
    assert!((85..=95).contains(&seconds(&first)), "{first}");

    // The clock moves on the shared one, the steps as they were (under the 2 s the
    // sidebar's own read waits, which would drop the test's data).
    let builds = room.update(cx, |room, _| room.list.builds);
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(1500));
    cx.run_until_parked();
    let (later, _) = drawn(&room, cx);
    assert!(seconds(&later) > seconds(&first), "{first} → {later}");
    assert_eq!(
        room.update(cx, |room, _| room.list.builds),
        builds,
        "a beat inside the turn does not rebuild the rows"
    );

    // The turn ends: the row leaves.
    let mut events = at_work();
    events.push(done(11, "c5"));
    events.push(event(
        12,
        serde_json::json!({ "type": "turn_completed", "turnId": "t1" }),
    ));
    beat(&room, events, cx);
    assert!(cx.debug_bounds("room-running-0").is_none());
}

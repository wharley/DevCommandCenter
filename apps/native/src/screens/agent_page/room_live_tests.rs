//! An agent at work in the room, headlessly: its live row with what it does,
//! its steps and the answer streaming, "Parar" stopping it; a beat that only
//! moved the turn keeps the rows, its end brings the answer in the row's
//! place; a turn that stopped offers "Tentar de novo" with its prompt.

use dcc_core::domain::session::{SessionEventKind, SessionEventRecord, SessionId};
use gpui::{Modifiers, TestAppContext};

use super::room_list_tests::room_with_lines;
use crate::data::agents::AgentState;
use crate::data::team_room::HomeFeed;
use crate::data::team_room_poll::{LivePoll, Polled};
use crate::test_support::fake_desktop_routes;

/// `seconds` ago, as the events say it.
fn ago(seconds: i64) -> String {
    (chrono::Utc::now() - chrono::Duration::seconds(seconds))
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn event(sequence: u64, json: serde_json::Value) -> SessionEventRecord {
    SessionEventRecord {
        event_id: format!("e-{sequence}"),
        session_id: SessionId("s-geral".into()),
        sequence,
        occurred_at: ago(90 - sequence as i64),
        kind: serde_json::from_value::<SessionEventKind>(json).unwrap(),
    }
}

fn started(prompt: &str) -> SessionEventRecord {
    event(
        1,
        serde_json::json!({
            "type": "turn_started", "turnId": "t1", "prompt": prompt,
            "planMode": false, "model": null, "evidence": null,
            "retryOfTurnId": null, "approvalPolicy": "ask"
        }),
    )
}

/// A turn reading a file, with the start of its answer.
fn at_work() -> Vec<SessionEventRecord> {
    vec![
        started("revisa o cache"),
        event(
            2,
            serde_json::json!({
                "type": "turn_tool_call_started", "turnId": "t1", "toolCallId": "c1",
                "action": "Read", "command": null, "file": "/repo/src/cache.rs"
            }),
        ),
        event(
            3,
            serde_json::json!({ "type": "turn_delta", "turnId": "t1", "content": "Olhando o **cache**" }),
        ),
    ]
}

fn feed(events: Vec<SessionEventRecord>) -> HomeFeed {
    let mut feed = HomeFeed::projected("dev", "s-geral", events);
    feed.read_at = "2026-11-10T10:00:00Z".into();
    feed
}

#[gpui::test]
fn a_turn_at_work_shows_live_and_its_answer_takes_its_place(cx: &mut TestAppContext) {
    let (_page, room, cx) = room_with_lines(cx, Vec::new(), AgentState::Working);
    // Before anything is read, "Dev está trabalhando…".
    assert!(cx.debug_bounds("room-busy-dev").is_some());
    room.update(cx, |room, cx| {
        room.take(vec![feed(at_work())]);
        cx.notify();
    });
    cx.run_until_parked();
    for selector in [
        "room-live-0",
        "room-live-stop-0",
        "room-live-open-0",
        "room-live-clock-0",
        "room-live-doing-0",
        "room-live-step-0-0",
        "room-live-text-0",
    ] {
        assert!(cx.debug_bounds(selector).is_some(), "{selector}");
    }
    assert!(
        cx.debug_bounds("room-busy-dev").is_none(),
        "the live row replaces it"
    );
    assert!(
        cx.debug_bounds("room-answer-1").is_none(),
        "not settled yet"
    );
    let label = room.update(cx, |room, cx| {
        let feed = &room.feeds[0];
        let turn = feed.now.live().unwrap().clone();
        room.doing_label(&turn, 5, cx).to_string()
    });
    assert_eq!(label, "Lendo src/cache.rs");

    // A beat that moved only the turn: the rows are kept, the tail grows.
    let builds = room.update(cx, |room, _| room.list.builds);
    let before = cx.debug_bounds("room-live-0").unwrap();
    room.update(cx, |room, cx| {
        let mut events = at_work();
        events.push(event(
            4,
            serde_json::json!({
                "type": "turn_tool_call_completed", "turnId": "t1", "toolCallId": "c1"
            }),
        ));
        events.push(event(
            5,
            serde_json::json!({
                "type": "turn_delta", "turnId": "t1",
                "content": "\n\n- primeiro ponto\n- segundo ponto\n- terceiro ponto"
            }),
        ));
        let poll = LivePoll::for_tests(&room.feeds[0], feed(events), Polled::Turn);
        let generation = room.room_generation;
        room.take_live(vec![poll], generation, cx);
        let turn = room.feeds[0].now.live().unwrap().clone();
        // The answer is coming in: it writes.
        assert_eq!(room.doing_label(&turn, 5, cx).to_string(), "Escrevendo…");
    });
    cx.run_until_parked();
    // Time for the paced reveal to show what came.
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    assert_eq!(
        room.update(cx, |room, _| room.list.builds),
        builds,
        "a beat inside the turn does not rebuild the rows"
    );
    assert!(cx.debug_bounds("room-live-0").is_some());
    let _ = before;

    // "Parar" asks the desktop to stop that conversation.
    let (port, desktop) =
        fake_desktop_routes(vec![("/api/v1/sessions/s-geral/abort", 200, "{}".into())]);
    room.update(cx, |room, _| {
        room.api = Some(crate::data::desktop_api::DesktopApi::new(port, "k"));
    });
    let stop = cx.debug_bounds("room-live-stop-0").unwrap();
    cx.simulate_click(stop.center(), Modifiers::none());
    assert!(
        crate::test_support::desktop_done(&desktop, || cx.run_until_parked()),
        "the desktop never got the call"
    );
    let seen = desktop.join().unwrap();
    assert_eq!(seen[0].0, "POST /api/v1/sessions/s-geral/abort");

    // The turn ends: the live row leaves, the answer shows in its place.
    room.update(cx, |room, cx| {
        let mut events = at_work();
        events.push(event(
            4,
            serde_json::json!({ "type": "turn_completed", "turnId": "t1" }),
        ));
        let poll = LivePoll::for_tests(&room.feeds[0], feed(events), Polled::Lines);
        let generation = room.room_generation;
        room.take_live(vec![poll], generation, cx);
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("room-live-0").is_none());
    assert!(cx.debug_bounds("room-said-0").is_some());
    assert!(cx.debug_bounds("room-answer-1").is_some());
    assert!(room.update(cx, |room, _| room.list.builds) > builds);
    assert!(
        room.update(cx, |room, _| room.live.texts.borrow().is_empty()),
        "what streamed is forgotten"
    );
}

#[gpui::test]
fn a_turn_that_stopped_offers_its_prompt_again(cx: &mut TestAppContext) {
    let (_page, room, cx) = room_with_lines(cx, Vec::new(), AgentState::Idle);
    let mut events = at_work();
    events.push(event(
        4,
        serde_json::json!({ "type": "turn_aborted", "turnId": "t1", "reason": "provider crashed" }),
    ));
    room.update(cx, |room, cx| {
        room.composer.update(cx, |composer, _| {
            let providers: Vec<dcc_core::domain::provider::ProviderDescriptor> =
                serde_json::from_value(serde_json::json!([{
                    "id": "codex", "label": "Codex", "description": "",
                    "models": [{ "id": "gpt-5", "label": "GPT-5",
                                 "description": "", "recommended": true, "effortLevels": [] }],
                    "capabilities": {
                        "streaming": true, "mcpSupport": "unsupported", "tools": true,
                        "vision": true, "resumable": true, "experimental": false,
                        "canBeDelegationTarget": true, "canRequestDelegation": true,
                        "supportsReadOnlyDelegation": true, "supportsEditDelegation": true,
                        "approvalPolicies": ["read_only", "ask", "auto", "full_access"],
                        "planModeSupport": "native"
                    },
                    "health": "Healthy", "stable": true
                }]))
                .unwrap();
            composer.catalog = Some(std::sync::Arc::new(providers));
        });
        room.take(vec![feed(events)]);
        cx.notify();
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("room-live-0").is_none());
    assert!(cx.debug_bounds("room-stopped-0").is_some());

    let (port, desktop) = fake_desktop_routes(vec![(
        "/api/v1/sessions/s-geral/turns",
        200,
        r#"{"turn":{"id":"t2"}}"#.into(),
    )]);
    room.update(cx, |room, _| {
        room.api = Some(crate::data::desktop_api::DesktopApi::new(port, "k"));
    });
    let retry = cx.debug_bounds("room-retry-0").unwrap();
    cx.simulate_click(retry.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(
        crate::test_support::desktop_done(&desktop, || cx.run_until_parked()),
        "the desktop never got the call"
    );
    let seen = desktop.join().unwrap();
    assert_eq!(seen[0].0, "POST /api/v1/sessions/s-geral/turns");
    let body: serde_json::Value = serde_json::from_str(&seen[0].1).unwrap();
    assert_eq!(body["prompt"], "revisa o cache");
    assert_eq!(body["retryOfTurnId"], "t1");
    assert_eq!(body["providerId"], "codex");
    assert!(
        cx.debug_bounds("room-stopped-0").is_none(),
        "sent again: the row leaves"
    );
}

#[gpui::test]
fn a_wake_that_stopped_is_said_but_not_sent_again(cx: &mut TestAppContext) {
    let (_page, room, cx) = room_with_lines(cx, Vec::new(), AgentState::Idle);
    let events = vec![
        started("[DCC] Routine \"Bom dia\" woke you."),
        event(
            2,
            serde_json::json!({ "type": "turn_aborted", "turnId": "t1", "reason": "boom" }),
        ),
    ];
    room.update(cx, |room, cx| {
        room.take(vec![feed(events)]);
        cx.notify();
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("room-stopped-0").is_some());
    assert!(cx.debug_bounds("room-retry-0").is_none());
}

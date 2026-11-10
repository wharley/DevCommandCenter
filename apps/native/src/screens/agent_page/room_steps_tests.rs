//! The folded steps line and the receipt under an answer, headlessly: the
//! line says what the turn did and unfolds into its steps on a click, the
//! duration is not said twice, and the footer keeps its place on hover.

use dcc_core::domain::session::{SessionEventKind, SessionEventRecord, SessionId};
use dcc_core::domain::team_room_steps::TurnSteps;
use dcc_core::domain::timeline::activity::ToolKind;
use gpui::{Modifiers, TestAppContext};

use super::room_list_tests::room_with_lines;
use super::room_rows::Row;
use crate::data::agents::AgentState;
use crate::data::team_room::{HomeFeed, Receipt, RoomLine, Speaker};

fn answer(key: &str, steps: TurnSteps) -> RoomLine {
    RoomLine {
        key: key.into(),
        agent_id: "dev".into(),
        session_id: "s-geral".into(),
        speaker: Speaker::Agent,
        text: "Feito.".into(),
        at: "2026-11-10T10:04:12Z".into(),
        receipt: Some(Receipt {
            seconds: Some(252),
            tokens: Some(42_000),
        }),
        tasks: Vec::new(),
        steps,
    }
}

fn worked() -> TurnSteps {
    TurnSteps {
        counts: vec![(ToolKind::Command, 1), (ToolKind::Read, 2)],
        failures: 0,
        tools: 3,
    }
}

fn event(sequence: u64, json: serde_json::Value) -> SessionEventRecord {
    SessionEventRecord {
        event_id: format!("e-{sequence}"),
        session_id: SessionId("s-geral".into()),
        sequence,
        occurred_at: format!("2026-11-10T10:0{sequence}:00Z"),
        kind: serde_json::from_value::<SessionEventKind>(json).unwrap(),
    }
}

fn turn_events() -> Vec<SessionEventRecord> {
    let call = |id: &str, action: &str, file: Option<&str>| {
        serde_json::json!({
            "type": "turn_tool_call_started", "turnId": "t1", "toolCallId": id,
            "action": action, "command": null, "file": file
        })
    };
    vec![
        event(
            1,
            serde_json::json!({
                "type": "turn_started", "turnId": "t1", "prompt": "olha",
                "planMode": false, "model": null, "evidence": null,
                "retryOfTurnId": null, "approvalPolicy": "ask"
            }),
        ),
        event(2, call("c1", "Read", Some("/a/b/one.rs"))),
        event(3, call("c2", "Read", Some("/a/b/two.rs"))),
        event(4, call("c3", "Bash", None)),
    ]
}

#[gpui::test]
fn the_steps_line_unfolds_into_the_turns_steps_and_folds_back(cx: &mut TestAppContext) {
    let (_page, room, cx) = room_with_lines(
        cx,
        vec![
            answer("s-geral:t1", worked()),
            answer("s-geral:t2", TurnSteps::default()),
        ],
        AgentState::Idle,
    );
    room.update(cx, |room, cx| {
        let mut feed = HomeFeed::with_events("dev", "s-geral", turn_events());
        feed.lines = vec![
            answer("s-geral:t1", worked()),
            answer("s-geral:t2", TurnSteps::default()),
        ];
        room.feeds = vec![feed];
        cx.notify();
    });
    cx.run_until_parked();
    // Counted when the lines were built: a turn without a tool shows nothing.
    let label = room.update(cx, |room, _| {
        room.list.model().rows.iter().find_map(|row| match row {
            Row::Line { steps, .. } => steps.as_ref().map(|steps| steps.label.to_string()),
            _ => None,
        })
    });
    assert_eq!(
        label.as_deref(),
        Some("Trabalhou 4 min · rodou 1 comando · leu 2 arquivos")
    );
    assert!(cx.debug_bounds("room-steps-0").is_some());
    assert!(cx.debug_bounds("room-steps-1").is_none());
    assert!(cx.debug_bounds("room-step-0-0").is_none(), "folded");
    let folded = cx.debug_bounds("room-answer-0").unwrap();

    let line = cx.debug_bounds("room-steps-0").unwrap();
    cx.simulate_click(line.center(), Modifiers::none());
    cx.run_until_parked();
    for step in ["room-step-0-0", "room-step-0-1", "room-step-0-2"] {
        assert!(cx.debug_bounds(step).is_some());
    }
    assert!(cx.debug_bounds("room-step-0-3").is_none());
    assert!(
        cx.debug_bounds("room-answer-0").unwrap().size.height > folded.size.height,
        "the row was measured again"
    );

    let line = cx.debug_bounds("room-steps-0").unwrap();
    cx.simulate_click(line.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(cx.debug_bounds("room-step-0-0").is_none());
    assert_eq!(cx.debug_bounds("room-answer-0").unwrap().size, folded.size);
}

#[gpui::test]
fn the_duration_is_said_once_and_the_footer_keeps_its_place(cx: &mut TestAppContext) {
    let (_page, room, cx) = room_with_lines(
        cx,
        vec![
            answer("s-geral:t1", worked()),
            answer("s-geral:t2", TurnSteps::default()),
        ],
        AgentState::Idle,
    );
    let receipt = Receipt {
        seconds: Some(252),
        tokens: Some(42_000),
    };
    cx.update(|_, cx| {
        let alone = super::room::TeamRoom::receipt_text(receipt, false, cx).unwrap();
        assert_eq!(alone, "42k tokens");
        let whole = super::room::TeamRoom::receipt_text(receipt, true, cx).unwrap();
        assert_eq!(whole, "4 min 12 s · 42k tokens");
    });
    // Hovering the answer reveals the line without moving anything.
    let before = (
        cx.debug_bounds("room-footer-0").unwrap(),
        cx.debug_bounds("room-answer-0").unwrap(),
        cx.debug_bounds("room-footer-1").unwrap(),
    );
    cx.simulate_mouse_move(before.0.center(), None, Modifiers::none());
    cx.run_until_parked();
    let after = (
        cx.debug_bounds("room-footer-0").unwrap(),
        cx.debug_bounds("room-answer-0").unwrap(),
        cx.debug_bounds("room-footer-1").unwrap(),
    );
    assert_eq!(before, after);
    assert!(cx.debug_bounds("room-open-0").is_some());
    drop(room);
}

//! The flat conversation, headlessly: answers of one agent within five
//! minutes group under one face, a new day has its pill, `@Agent` is a pill
//! in the person's words, and what the field sends still says `@Agent`.

use chrono::TimeZone;
use gpui::TestAppContext;

use super::room_list_tests::room_with_lines;
use crate::data::agents::AgentState;
use crate::data::team_room::{RoomLine, Speaker};

fn said(key: &str, speaker: Speaker, text: &str, at: chrono::DateTime<chrono::Local>) -> RoomLine {
    RoomLine {
        key: key.into(),
        agent_id: "dev".into(),
        session_id: "s-geral".into(),
        speaker,
        text: text.into(),
        at: at.to_rfc3339(),
        receipt: None,
        tasks: Vec::new(),
    }
}

fn local(day: u32, hour: u32, minute: u32) -> chrono::DateTime<chrono::Local> {
    chrono::Local
        .with_ymd_and_hms(2026, 11, day, hour, minute, 0)
        .unwrap()
}

#[gpui::test]
fn a_group_shows_one_face_and_a_new_day_its_pill(cx: &mut TestAppContext) {
    let (_page, room, cx) = room_with_lines(
        cx,
        vec![
            said("a", Speaker::Agent, "primeira", local(8, 9, 0)),
            said("b", Speaker::Agent, "segunda", local(8, 9, 3)),
            said("c", Speaker::Agent, "terceira", local(8, 9, 20)),
            said("d", Speaker::Agent, "no outro dia", local(9, 8, 0)),
        ],
        AgentState::Idle,
    );
    // The model decides, not the frame.
    let (firsts, days) = room.update(cx, |room, _| {
        let model = room.list.model();
        let firsts: Vec<bool> = model
            .rows
            .iter()
            .filter_map(|row| match row {
                super::room_rows::Row::Line { first, .. } => Some(*first),
                _ => None,
            })
            .collect();
        let days = model
            .rows
            .iter()
            .filter(|row| matches!(row, super::room_rows::Row::Day { .. }))
            .count();
        (firsts, days)
    });
    assert_eq!(firsts, [true, false, true, true]);
    assert_eq!(days, 1);
    assert!(cx.debug_bounds("room-answer-0").is_some());
    assert!(cx.debug_bounds("room-face-0").is_some());
    assert!(cx.debug_bounds("room-answer-1").is_some());
    assert!(
        cx.debug_bounds("room-face-1").is_none(),
        "the grouped answer has no face of its own"
    );
    assert!(cx.debug_bounds("room-face-2").is_some());
    assert!(cx.debug_bounds("room-day-2026-11-09").is_some());
}

#[gpui::test]
fn a_mention_in_the_words_is_marked_and_the_text_sent_keeps_it(cx: &mut TestAppContext) {
    let (_page, room, cx) = room_with_lines(
        cx,
        vec![said(
            "p",
            Speaker::Person,
            "@Dev olha @/tmp/x.png e a@Dev.com",
            local(8, 9, 0),
        )],
        AgentState::Idle,
    );
    assert!(cx.debug_bounds("room-said-0").is_some());
    let marked = room.update(cx, |room, _| {
        let model = room.list.model();
        match &model.rows[0] {
            super::room_rows::Row::Line { said, .. } => {
                let said = said.as_ref().expect("the person's words are split");
                said.mentions
                    .iter()
                    .map(|(range, _)| said.words[range.clone()].to_string())
                    .collect::<Vec<_>>()
            }
            _ => Vec::new(),
        }
    });
    // Not the path, not the e-mail.
    assert_eq!(marked, ["@Dev"]);
    // The field tints the name and still holds, and sends, `@Dev`.
    room.update(cx, |room, cx| {
        room.input
            .update(cx, |area, cx| area.set_text("@Dev revise", cx));
    });
    cx.run_until_parked();
    room.update(cx, |room, cx| {
        assert_eq!(room.prompt_text(cx), "@Dev revise");
    });
}

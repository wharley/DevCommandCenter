//! "Ir para o fim", headlessly: scrolled up, the round button shows; a new
//! line adds "1 nova" to it; a click reaches the end and follows it again.

use gpui::{Modifiers, TestAppContext, px};

use super::room_list_tests::room_with_lines;
use crate::data::agents::AgentState;
use crate::data::team_room::{RoomLine, Speaker};
use crate::i18n::{Args, Translate};

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
            format!("Feito, linha {index}:\n\n- um\n- dois")
        } else {
            format!("pedido {index}")
        },
        at: format!("2026-11-10T{:02}:{:02}:00Z", index / 60 % 24, index % 60),
        receipt: None,
        tasks: Vec::new(),
        steps: Default::default(),
    }
}

#[gpui::test]
fn scrolled_up_a_button_counts_the_new_lines_and_goes_back_down(cx: &mut TestAppContext) {
    let (_page, room, cx) = room_with_lines(cx, (0..200).map(line).collect(), AgentState::Idle);
    assert!(cx.debug_bounds("room-jump").is_none(), "at the end");

    room.update(cx, |room, cx| {
        room.list.state.scroll_by(px(-3000.0));
        cx.notify();
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("room-jump").is_some());
    assert!(
        cx.debug_bounds("room-jump-count").is_none(),
        "nothing new yet"
    );

    let builds = room.update(cx, |room, _| room.list.builds);
    room.update(cx, |room, cx| {
        let mut feed = room.feeds[0].clone();
        feed.lines.push(line(201));
        room.take(vec![feed]);
        cx.notify();
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("room-jump-count").is_some());
    room.update(cx, |room, cx| {
        assert_eq!(room.unseen_lines(), Some(1));
        assert_eq!(cx.t_with("agents.room.newLines", &Args::count(1)), "1 nova");
        assert_eq!(
            cx.t_with("agents.room.newLines", &Args::count(3)),
            "3 novas"
        );
    });
    assert_eq!(room.update(cx, |room, _| room.list.builds), builds + 1);

    // Back down: the rows are kept.
    room.update(cx, |room, cx| room.jump_to_end(cx));
    cx.run_until_parked();
    assert!(room.update(cx, |room, _| room.list.state.is_following_tail()));
    assert!(cx.debug_bounds("room-answer-200").is_some(), "at the end");
    assert!(cx.debug_bounds("room-jump").is_none());
    assert_eq!(room.update(cx, |room, _| room.list.builds), builds + 1);

    // The button itself does it.
    room.update(cx, |room, cx| {
        room.list.state.scroll_by(px(-3000.0));
        cx.notify();
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("room-answer-200").is_none());
    let jump = cx.debug_bounds("room-jump").unwrap();
    cx.simulate_click(jump.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(room.update(cx, |room, _| room.list.state.is_following_tail()));
    assert!(cx.debug_bounds("room-answer-200").is_some());
    assert!(cx.debug_bounds("room-jump").is_none());
}

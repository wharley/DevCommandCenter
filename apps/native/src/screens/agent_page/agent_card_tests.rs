//! The agent card, headlessly: a click on an answer's face opens it with the
//! agent's name, its state and both actions; "Mensagem" addresses it in the
//! field; Esc closes it; a hover opens it after half a second and not
//! before, and a hover that leaves earlier opens nothing.

use std::time::Duration;

use gpui::{Modifiers, TestAppContext, point, px};

use super::page_tabs::AgentTab;
use super::room_list_tests::room_with_lines;
use crate::data::agents::AgentState;
use crate::data::team_room::{RoomLine, Speaker};
use crate::i18n::Translate;

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
        text: if agent { "Feito." } else { "corrige o login" }.into(),
        at: format!("2026-11-10T10:0{index}:00Z"),
        receipt: None,
        tasks: Vec::new(),
        steps: Default::default(),
    }
}

#[gpui::test]
fn a_face_opens_the_agents_card_and_its_actions_work(cx: &mut TestAppContext) {
    let (page, room, cx) = room_with_lines(cx, (0..2).map(line).collect(), AgentState::Idle);
    let open_on = |room: &gpui::Entity<super::room::TeamRoom>, cx: &mut gpui::VisualTestContext| {
        room.read_with(cx, |room, _| {
            room.card.open.as_ref().map(|open| open.agent_id.clone())
        })
    };
    assert!(cx.debug_bounds("agent-card").is_none());

    // A click on the face: the card, with what the team column says.
    let face = cx.debug_bounds("room-face-1").unwrap();
    cx.simulate_click(face.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(cx.debug_bounds("agent-card").is_some());
    assert_eq!(open_on(&room, cx).as_deref(), Some("dev"));
    assert!(cx.debug_bounds("agent-card-name").is_some());
    assert!(cx.debug_bounds("agent-card-status").is_some());
    assert!(cx.debug_bounds("agent-card-rooms").is_some());
    assert!(cx.debug_bounds("agent-card-message").is_some());
    assert!(cx.debug_bounds("agent-card-profile").is_some());
    cx.update(|_, cx| {
        assert_eq!(cx.t("agents.team.line.idle"), "Disponível");
        assert_eq!(cx.t("agents.card.message"), "Mensagem");
        assert_eq!(cx.t("agents.card.profile"), "Ver perfil");
    });

    // "Mensagem": "@Dev " in the field, the card gone.
    let message = cx.debug_bounds("agent-card-message").unwrap();
    cx.simulate_click(message.center(), Modifiers::none());
    cx.run_until_parked();
    assert_eq!(
        room.read_with(cx, |room, cx| room.input.read(cx).text().to_string()),
        "@Dev "
    );
    assert!(cx.debug_bounds("agent-card").is_none());

    // Esc closes it.
    let face = cx.debug_bounds("room-face-1").unwrap();
    cx.simulate_click(face.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(cx.debug_bounds("agent-card").is_some());
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(cx.debug_bounds("agent-card").is_none());
    assert_eq!(open_on(&room, cx), None);

    // Away from the face, then a hover on the name: nothing before 500 ms.
    let away = point(px(1100.0), px(40.0));
    cx.simulate_mouse_move(away, None, Modifiers::none());
    cx.run_until_parked();
    let name = cx.debug_bounds("room-face-1").unwrap();
    cx.simulate_mouse_move(name.center(), None, Modifiers::none());
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(400));
    cx.run_until_parked();
    assert!(cx.debug_bounds("agent-card").is_none(), "not yet");
    cx.executor().advance_clock(Duration::from_millis(150));
    cx.run_until_parked();
    assert!(cx.debug_bounds("agent-card").is_some(), "after 500 ms");

    // The pointer away from the card and the face: it closes.
    cx.simulate_mouse_move(away, None, Modifiers::none());
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(350));
    cx.run_until_parked();
    assert!(cx.debug_bounds("agent-card").is_none());

    // A hover that leaves before 500 ms opens nothing, and no timer is left.
    cx.simulate_mouse_move(name.center(), None, Modifiers::none());
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(300));
    cx.simulate_mouse_move(away, None, Modifiers::none());
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(600));
    cx.run_until_parked();
    assert!(cx.debug_bounds("agent-card").is_none());

    // "Ver perfil": the agent's page on "Configuração".
    let face = cx.debug_bounds("room-face-1").unwrap();
    cx.simulate_click(face.center(), Modifiers::none());
    cx.run_until_parked();
    let profile = cx.debug_bounds("agent-card-profile").unwrap();
    cx.simulate_click(profile.center(), Modifiers::none());
    cx.run_until_parked();
    page.read_with(cx, |page, _| {
        assert!(!page.room_open);
        assert_eq!(page.agent_id, "dev");
        assert_eq!(page.tab, AgentTab::Settings);
    });
}

#[gpui::test]
fn the_team_columns_face_opens_the_card_too(cx: &mut TestAppContext) {
    let (page, _room, cx) = room_with_lines(cx, (0..2).map(line).collect(), AgentState::Idle);
    let face = cx.debug_bounds("team-face-0").unwrap();
    cx.simulate_click(face.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(cx.debug_bounds("agent-card").is_some());
    // The face opens the card, not the agent's conversation.
    assert!(page.read_with(cx, |page, _| page.room_open));
    let message = cx.debug_bounds("agent-card-message").unwrap();
    cx.simulate_click(message.center(), Modifiers::none());
    cx.run_until_parked();
    let room = page.read_with(cx, |page, _| page.room.as_ref().unwrap().0.clone());
    assert_eq!(
        room.read_with(cx, |room, cx| room.input.read(cx).text().to_string()),
        "@Dev "
    );
    assert!(cx.debug_bounds("agent-card").is_none());
}

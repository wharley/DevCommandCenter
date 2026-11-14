//! The agent card, headlessly: a hover on an answer's face opens it with the
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

type Room = gpui::Entity<super::room::TeamRoom>;

/// Somewhere with no agent under it.
fn away() -> gpui::Point<gpui::Pixels> {
    point(px(1100.0), px(40.0))
}

/// The pointer resting `at` (a face's centre) long enough for its card to
/// open. Each test hovers little: the lines these rooms are given are not
/// in a database, so they do not outlive the room reading it again.
fn hover(cx: &mut gpui::VisualTestContext, at: gpui::Point<gpui::Pixels>) {
    cx.simulate_mouse_move(at, None, Modifiers::none());
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(550));
    cx.run_until_parked();
}

fn open_on(room: &Room, cx: &mut gpui::VisualTestContext) -> Option<String> {
    room.read_with(cx, |room, _| {
        room.card.open.as_ref().map(|open| open.agent_id.clone())
    })
}

/// A room with one answer, and where its face is.
fn room_and_face(
    cx: &mut TestAppContext,
) -> (
    gpui::Entity<super::AgentPage>,
    Room,
    &mut gpui::VisualTestContext,
    gpui::Point<gpui::Pixels>,
) {
    let (page, room, cx) = room_with_lines(cx, (0..2).map(line).collect(), AgentState::Idle);
    let face = cx.debug_bounds("room-face-1").unwrap().center();
    (page, room, cx, face)
}

#[gpui::test]
fn a_face_shows_the_agents_card_after_half_a_second_of_hover(cx: &mut TestAppContext) {
    let (_page, room, cx, face) = room_and_face(cx);
    assert!(cx.debug_bounds("agent-card").is_none());

    // Nothing before 500 ms.
    cx.simulate_mouse_move(face, None, Modifiers::none());
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(400));
    cx.run_until_parked();
    assert_eq!(open_on(&room, cx), None, "not yet");
    cx.executor().advance_clock(Duration::from_millis(150));
    cx.run_until_parked();
    assert_eq!(open_on(&room, cx).as_deref(), Some("dev"), "after 500 ms");

    // The card, with what the team column says.
    assert!(cx.debug_bounds("agent-card").is_some());
    assert!(cx.debug_bounds("agent-card-name").is_some());
    assert!(cx.debug_bounds("agent-card-status").is_some());
    assert!(cx.debug_bounds("agent-card-rooms").is_some());
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
    assert_eq!(open_on(&room, cx), None);
}

#[gpui::test]
fn esc_or_the_pointer_away_closes_the_card(cx: &mut TestAppContext) {
    let (_page, room, cx, face) = room_and_face(cx);
    hover(cx, face);
    assert_eq!(open_on(&room, cx).as_deref(), Some("dev"));
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert_eq!(open_on(&room, cx), None);

    // Open again; the pointer away from the card and the face closes it.
    cx.simulate_mouse_move(away(), None, Modifiers::none());
    cx.run_until_parked();
    hover(cx, face);
    assert_eq!(open_on(&room, cx).as_deref(), Some("dev"));
    cx.simulate_mouse_move(away(), None, Modifiers::none());
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(350));
    cx.run_until_parked();
    assert_eq!(open_on(&room, cx), None);
}

#[gpui::test]
fn a_hover_that_leaves_early_opens_nothing(cx: &mut TestAppContext) {
    let (_page, room, cx, face) = room_and_face(cx);
    cx.simulate_mouse_move(face, None, Modifiers::none());
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(300));
    cx.simulate_mouse_move(away(), None, Modifiers::none());
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(600));
    cx.run_until_parked();
    assert_eq!(open_on(&room, cx), None);
}

#[gpui::test]
fn the_cards_profile_button_opens_the_agents_configuration(cx: &mut TestAppContext) {
    let (page, _room, cx, face) = room_and_face(cx);
    hover(cx, face);
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
fn a_click_on_an_agents_face_goes_to_its_conversation(cx: &mut TestAppContext) {
    let (page, _room, cx) = room_with_lines(cx, (0..2).map(line).collect(), AgentState::Idle);
    assert!(page.read_with(cx, |page, _| page.room_open));
    // In the room: the click is for talking to it, not for its card.
    let face = cx.debug_bounds("room-face-1").unwrap();
    cx.simulate_click(face.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(cx.debug_bounds("agent-card").is_none());
    page.read_with(cx, |page, _| {
        assert_eq!(page.agent_id, "dev");
        assert_eq!(page.tab, AgentTab::Conversation);
        assert!(!page.room_open);
    });
}

#[gpui::test]
fn the_team_columns_face_shows_the_card_on_hover_too(cx: &mut TestAppContext) {
    let (page, _room, cx) = room_with_lines(cx, (0..2).map(line).collect(), AgentState::Idle);
    let face = cx.debug_bounds("team-face-0").unwrap().center();
    hover(cx, face);
    assert!(cx.debug_bounds("agent-card").is_some());
    // A hover alone goes nowhere.
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

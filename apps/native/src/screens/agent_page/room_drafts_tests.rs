//! A draft per room, headlessly: what was left in Geral's field, a chip
//! with it, is there again after Checkout's room, whose field starts empty;
//! the agent's own "Conversa" keeps its own.

use gpui::TestAppContext;

use super::room_list_tests::room_with_lines;
use crate::data::agents::AgentState;
use crate::ui::text_area::Chip;

#[gpui::test]
fn each_room_keeps_its_own_draft(cx: &mut TestAppContext) {
    let (page, room, cx) = room_with_lines(cx, Vec::new(), AgentState::Idle);
    let field = |cx: &mut gpui::VisualTestContext| {
        room.update(cx, |room, cx| {
            let input = room.input.read(cx);
            let chips: Vec<Chip> = input.chips().into_iter().map(|(_, chip)| chip).collect();
            (input.text().replace(crate::ui::text_area::CHIP, ""), chips)
        })
    };
    room.update(cx, |room, cx| {
        room.input.update(cx, |area, cx| {
            area.set_text("olha o log ", cx);
            area.insert_chip(Chip::File("src/cache.rs".into()), None, cx);
        });
    });
    page.update(cx, |page, cx| {
        page.open_room_of(Some("checkout".into()), cx)
    });
    cx.run_until_parked();
    assert_eq!(
        field(cx),
        (String::new(), Vec::new()),
        "Checkout starts empty"
    );
    room.update(cx, |room, cx| {
        room.input
            .update(cx, |area, cx| area.set_text("só no checkout", cx));
    });

    page.update(cx, |page, cx| page.open_room_of(None, cx));
    cx.run_until_parked();
    assert_eq!(
        field(cx),
        (
            "olha o log ".to_string(),
            vec![Chip::File("src/cache.rs".into())]
        )
    );

    // The agent's own "Conversa" keeps its own; Geral's comes back after it.
    page.update(cx, |page, cx| page.show_conversation("dev".into(), cx));
    cx.run_until_parked();
    assert!(room.update(cx, |room, _| room.focus.is_some()));
    assert_eq!(field(cx), (String::new(), Vec::new()));
    room.update(cx, |room, cx| {
        room.input
            .update(cx, |area, cx| area.set_text("só pro Dev", cx));
    });
    page.update(cx, |page, cx| page.open_room_of(None, cx));
    cx.run_until_parked();
    assert_eq!(field(cx).0, "olha o log ");
    page.update(cx, |page, cx| page.show_conversation("dev".into(), cx));
    cx.run_until_parked();
    assert_eq!(field(cx).0, "só pro Dev");
    page.update(cx, |page, cx| page.open_room_of(None, cx));
    cx.run_until_parked();

    // Sent, the room keeps nothing.
    page.update(cx, |page, cx| {
        page.open_room_of(Some("checkout".into()), cx)
    });
    cx.run_until_parked();
    assert_eq!(field(cx).0, "só no checkout");
    room.update(cx, |room, cx| {
        room.input.update(cx, |area, cx| area.set_text("", cx));
    });
    page.update(cx, |page, cx| page.open_room_of(None, cx));
    cx.run_until_parked();
    page.update(cx, |page, cx| {
        page.open_room_of(Some("checkout".into()), cx)
    });
    cx.run_until_parked();
    assert_eq!(field(cx).0, "");
}

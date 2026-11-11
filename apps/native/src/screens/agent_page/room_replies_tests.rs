//! A reply an agent proposed is a card in its room that waits on the
//! person: it counts beside "Agentes", shows its text, and "Editar" opens
//! the text to change before sending.

use gpui::{AppContext, Modifiers, TestAppContext, WindowAppearance};

use super::AgentPage;
use super::room_tests::agent;
use crate::data::agent_replies::Replies;
use crate::data::loader::SidebarData;
use crate::screens::composer::Composer;
use crate::screens::sidebar::Sidebar;
use crate::theme::{Theme, ThemePreference};
use dcc_core::domain::agent_reply::{AgentReply, ReplyState};

fn reply(room: Option<&str>) -> AgentReply {
    AgentReply {
        id: "rp-1".into(),
        agent_id: "sup".into(),
        session_id: "s-sup".into(),
        connector_id: "slack".into(),
        connector: "Slack".into(),
        tool: "slack_send_message".into(),
        arguments: "{}".into(),
        text_argument: "message".into(),
        destination: "#suporte · thread do Guilherme".into(),
        text: "Eu e o Giovanni usamos a loja 374.".into(),
        state: ReplyState::Pending,
        error: None,
        created_at: "2026-11-11T22:00:00Z".into(),
        sent_at: None,
        room: room.map(String::from),
    }
}

#[gpui::test]
fn a_proposed_reply_waits_in_its_room_and_opens_to_edit(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::text_input::bind_keys(cx);
        crate::ui::text_area::bind_keys(cx);
    });
    let seen = dir.path().join("seen.json");
    let (page, cx) = cx.add_window_view(move |window, cx| {
        let sidebar = cx.new(|cx| Sidebar::new(false, false, false, seen, cx));
        let composer = cx.new(|cx| Composer::new(window, cx));
        AgentPage::new("sup".into(), sidebar, composer, cx)
    });
    cx.simulate_resize(gpui::size(gpui::px(1200.0), gpui::px(1400.0)));
    page.update(cx, |page, cx| {
        page.sidebar.update(cx, |sidebar, cx| {
            sidebar.apply_for_tests(
                SidebarData {
                    agents: vec![agent("sup", "Suporte", "reviewer", Vec::new())],
                    ..SidebarData::default()
                },
                cx,
            )
        });
        page.open_room(None, cx);
    });
    // Proposed in the Checkout room: Geral does not show it.
    cx.update(|_, cx| {
        cx.set_global(Replies {
            list: vec![reply(Some("checkout"))],
        })
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("room-reply-0").is_none());
    let waiting = page.update(cx, |page, cx| page.sidebar.read(cx).agents_waiting(cx));
    assert_eq!(waiting, (1, true));

    cx.update(|_, cx| {
        cx.set_global(Replies {
            list: vec![reply(None)],
        })
    });
    page.update(cx, |_, cx| cx.notify());
    cx.run_until_parked();
    assert!(cx.debug_bounds("room-reply-0").is_some());
    assert!(cx.debug_bounds("room-reply-text-0").is_some());
    let edit = cx.debug_bounds("room-reply-edit-0").unwrap();
    cx.simulate_click(edit.center(), Modifiers::none());
    cx.run_until_parked();
    // The text is a field now; "Editar" is gone while it is open.
    assert!(cx.debug_bounds("room-reply-text-0").is_none());
    assert!(cx.debug_bounds("room-reply-edit-0").is_none());
    assert!(cx.debug_bounds("room-reply-send-0").is_some());
}

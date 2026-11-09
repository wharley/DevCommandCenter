//! An agent proposed in the team room, driven headlessly: the card shows
//! who proposes it and the new agent; "Editar antes" opens the editor with
//! it; "Criar" makes it in the desktop and opens its conversation with the
//! first request in the field, not sent.

use dcc_core::domain::agent_proposal::{AgentProposal, proposal_draft};
use gpui::{AppContext, Modifiers, TestAppContext, WindowAppearance};

use super::AgentPage;
use super::page_tabs::AgentTab;
use crate::data::agents::AgentRecord;
use crate::data::loader::SidebarData;
use crate::screens::composer::Composer;
use crate::screens::sidebar::Sidebar;
use crate::test_support::fake_desktop_routes;
use crate::theme::{Theme, ThemePreference};

fn agent(id: &str, name: &str) -> AgentRecord {
    let mut record = super::editor_role::blank_agent();
    record.id = id.into();
    record.name = name.into();
    record.role = "r".into();
    record
}

#[gpui::test]
fn an_agent_proposed_is_created_or_edited_from_its_card(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::text_input::bind_keys(cx);
    });
    let dir = tempfile::tempdir().unwrap();
    let seen = dir.path().join("seen.json");
    let (port, desktop) = fake_desktop_routes(vec![(
        "/api/v1/agents/rev/proposals/p-1/accept",
        200,
        r#"{"agentId":"qa-1"}"#.into(),
    )]);
    let (page, cx) = cx.add_window_view(move |window, cx| {
        let sidebar = cx.new(|cx| Sidebar::new(false, false, false, seen, cx));
        let composer = cx.new(|cx| Composer::new(window, cx));
        AgentPage::new("rev".into(), sidebar, composer, cx)
    });
    cx.simulate_resize(gpui::size(gpui::px(1200.0), gpui::px(1400.0)));
    let proposal = AgentProposal {
        id: "p-1".into(),
        agent_id: "rev".into(),
        session_id: "s-home".into(),
        draft: proposal_draft(
            "QA do Checkout",
            "Teste o checkout de ponta a ponta.",
            "Rode o fluxo de compra no staging.",
            None,
            Vec::new(),
            Default::default(),
        )
        .unwrap(),
        created_at: "2026-11-09T12:00:00.000Z".into(),
    };
    page.update(cx, |page, cx| {
        page.sidebar.update(cx, |sidebar, cx| {
            sidebar.apply_for_tests(
                SidebarData {
                    agents: vec![agent("rev", "Revisor")],
                    agent_proposals: vec![proposal],
                    ..SidebarData::default()
                },
                cx,
            )
        });
        page.open_room(None, cx);
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("room-agent-proposal-0").is_some());

    // "Editar antes": the editor, filled with the proposal.
    let edit = cx.debug_bounds("room-proposal-edit-0").unwrap();
    cx.simulate_click(edit.center(), Modifiers::none());
    cx.run_until_parked();
    page.update(cx, |page, cx| {
        let (editor, _) = page.editor.as_ref().unwrap();
        let editor = editor.read(cx);
        assert!(editor.is_new());
        assert_eq!(editor.name.read(cx).text(), "QA do Checkout");
        assert!(page.editing_proposal.is_some());
        page.editor = None;
        page.editing_proposal = None;
        page.open_room(None, cx);
        let room = page.room.as_ref().unwrap().0.clone();
        room.update(cx, |room, _| {
            room.api = Some(crate::data::desktop_api::DesktopApi::new(port, "k"));
        });
    });
    cx.run_until_parked();

    // "Criar": made in the desktop; its conversation opens with the first
    // request waiting in the field.
    let create = cx.debug_bounds("room-proposal-create-0").unwrap();
    cx.simulate_click(create.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(cx.debug_bounds("room-agent-proposal-0").is_none());
    let seen = desktop.join().unwrap();
    assert_eq!(seen[0].0, "POST /api/v1/agents/rev/proposals/p-1/accept");
    page.update(cx, |page, _| {
        assert_eq!(page.agent_id, "qa-1");
        assert_eq!(page.tab, AgentTab::Conversation);
        assert_eq!(
            page.room_draft.as_deref(),
            Some("Rode o fluxo de compra no staging.")
        );
    });
}

//! A pass without the person, driven headlessly: an answer DCC already
//! passed on shows so, with no buttons; the editor offers the teammates to
//! call by itself, never the Reviewer.

use dcc_core::domain::agent_chain::auto_handoff_prompt;
use gpui::{AppContext, TestAppContext, WindowAppearance};

use super::AgentPage;
use crate::data::agents::AgentRecord;
use crate::data::loader::SidebarData;
use crate::data::team_room::{HomeFeed, RoomLine, Speaker};
use crate::screens::composer::Composer;
use crate::screens::sidebar::Sidebar;
use crate::theme::{Theme, ThemePreference};

fn agent(id: &str, name: &str, preset: Option<&str>) -> AgentRecord {
    let mut record = super::editor_role::blank_agent();
    record.id = id.into();
    record.name = name.into();
    record.preset = preset.map(str::to_string);
    record
}

fn line(key: &str, agent: &str, session: &str, speaker: Speaker, text: &str, at: &str) -> RoomLine {
    RoomLine {
        key: key.into(),
        agent_id: agent.into(),
        session_id: session.into(),
        speaker,
        text: text.into(),
        at: at.into(),
        receipt: None,
        tasks: Vec::new(),
    }
}

#[gpui::test]
fn a_pass_dcc_made_shows_as_done_and_the_editor_offers_teammates(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::text_input::bind_keys(cx);
    });
    let dir = tempfile::tempdir().unwrap();
    let seen = dir.path().join("seen.json");
    let (page, cx) = cx.add_window_view(move |window, cx| {
        let sidebar = cx.new(|cx| Sidebar::new(false, false, false, seen, cx));
        let composer = cx.new(|cx| Composer::new(window, cx));
        AgentPage::new("sup".into(), sidebar, composer, cx)
    });
    cx.simulate_resize(gpui::size(gpui::px(1200.0), gpui::px(1400.0)));
    let mut suporte = agent("sup", "Suporte", None);
    suporte.may_call = vec!["dev".into()];
    let agents = vec![
        suporte,
        agent("dev", "Dev", None),
        agent(
            "rev",
            "Revisor",
            Some(dcc_core::domain::agent::REVIEWER_PRESET),
        ),
    ];
    page.update(cx, |page, cx| {
        page.sidebar.update(cx, |sidebar, cx| {
            sidebar.apply_for_tests(
                SidebarData {
                    agents: agents.clone(),
                    ..SidebarData::default()
                },
                cx,
            )
        });
        page.open_room(None, cx);
    });
    cx.run_until_parked();
    page.update(cx, |page, cx| {
        let room = page.room.as_ref().unwrap().0.clone();
        room.update(cx, |room, cx| {
            let answer = "O seller pergunta do campo X; a API aceita.\n\n@Dev implementar o envio do campo X";
            let mut support = HomeFeed::new("sup", "s-sup");
            support.lines = vec![line(
                "s-sup:t-1",
                "sup",
                "s-sup",
                Speaker::Agent,
                answer,
                "2026-11-09T10:00:00Z",
            )];
            let mut dev = HomeFeed::new("dev", "s-dev");
            dev.lines = vec![line(
                "s-dev:m-1",
                "dev",
                "s-dev",
                Speaker::Person,
                &auto_handoff_prompt(
                    "Suporte",
                    "Dev",
                    "@Dev implementar o envio do campo X",
                    answer,
                    "s-sup:t-1",
                    1,
                ),
                "2026-11-09T10:00:05Z",
            )];
            room.take(vec![support, dev]);
            cx.notify();
        });
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("room-handoff-0").is_some());
    assert!(
        cx.debug_bounds("room-handoff-pass-0").is_none(),
        "already passed"
    );
    assert!(cx.debug_bounds("room-passed-1").is_some());

    // The editor offers Dev to call by itself, not the Reviewer.
    page.update_in(cx, |page, window, cx| {
        page.show("sup".into(), cx);
        page.open_editor(window, cx);
    });
    cx.run_until_parked();
    page.update(cx, |page, cx| {
        let (editor, _) = page.editor.as_ref().unwrap();
        let editor = editor.read(cx);
        let offered: Vec<&str> = editor
            .teammates
            .iter()
            .map(|(id, ..)| id.as_str())
            .collect();
        assert_eq!(offered, ["dev"]);
        assert_eq!(editor.may_call, ["dev"]);
    });
}

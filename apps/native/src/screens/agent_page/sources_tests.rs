//! Sources on screen, driven headlessly: a proposed one is a card in the
//! room, a source's news reads as one line, and the agent's page lists the
//! ones it watches.

use dcc_core::domain::agent_source::{AgentSource, SourceItem, source_prompt};
use gpui::{AppContext, TestAppContext, WindowAppearance};

use super::AgentPage;
use crate::data::agent_sources::Sources;
use crate::data::loader::SidebarData;
use crate::data::team_room::{HomeFeed, RoomLine, Speaker};
use crate::screens::composer::Composer;
use crate::screens::sidebar::Sidebar;
use crate::theme::{Theme, ThemePreference};

fn source(id: &str, watch: &str, proposed: bool) -> AgentSource {
    AgentSource {
        id: id.into(),
        agent_id: "sup".into(),
        connector_id: "slack".into(),
        connector: "Slack".into(),
        watch: watch.into(),
        tool: "slack_search_public".into(),
        arguments: "{}".into(),
        every_minutes: 15,
        enabled: true,
        proposed,
        last_checked_at: None,
        created_at: "2026-11-09T10:00:00Z".into(),
        room: None,
    }
}

#[gpui::test]
fn sources_show_as_cards_lines_and_on_the_page(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        cx.set_global(Sources {
            list: vec![
                source("s-1", "menções a mim", false),
                source("s-2", "#suporte-privado", true),
            ],
        });
    });
    let dir = tempfile::tempdir().unwrap();
    let seen = dir.path().join("seen.json");
    let (page, cx) = cx.add_window_view(move |window, cx| {
        let sidebar = cx.new(|cx| Sidebar::new(false, false, false, seen, cx));
        let composer = cx.new(|cx| Composer::new(window, cx));
        AgentPage::new("sup".into(), sidebar, composer, cx)
    });
    cx.simulate_resize(gpui::size(gpui::px(1200.0), gpui::px(2400.0)));
    let mut suporte = super::editor_role::blank_agent();
    suporte.id = "sup".into();
    suporte.name = "Suporte".into();
    page.update(cx, |page, cx| {
        page.sidebar.update(cx, |sidebar, cx| {
            sidebar.apply_for_tests(
                SidebarData {
                    agents: vec![suporte],
                    ..SidebarData::default()
                },
                cx,
            )
        });
        page.open_room(None, cx);
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("room-source-proposal-0").is_some());
    // One asked in the Checkout's room shows there, not in Geral.
    cx.update(|_, cx| {
        let mut asked_in_checkout = source("s-3", "#checkout-alertas", true);
        asked_in_checkout.room = Some("checkout".into());
        cx.set_global(Sources {
            list: vec![asked_in_checkout],
        });
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("room-source-proposal-0").is_none());
    page.update(cx, |page, cx| {
        page.open_room_of(Some("checkout".into()), cx)
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("room-source-proposal-0").is_some());
    // Back to Geral and to the sources it started with.
    cx.update(|_, cx| {
        cx.set_global(Sources {
            list: vec![
                source("s-1", "menções a mim", false),
                source("s-2", "#suporte-privado", true),
            ],
        });
    });
    page.update(cx, |page, cx| page.open_room(None, cx));
    cx.run_until_parked();
    page.update(cx, |page, cx| {
        let room = page.room.as_ref().unwrap().0.clone();
        room.update(cx, |room, cx| {
            let news = [SourceItem {
                id: "x".into(),
                text: "Giovanni: @Wharley o campo X?".into(),
            }];
            let mut feed = HomeFeed::new("sup", "s-home");
            feed.lines = vec![RoomLine {
                key: "s-home:m-1".into(),
                agent_id: "sup".into(),
                session_id: "s-home".into(),
                speaker: Speaker::Person,
                text: source_prompt(&source("s-1", "menções a mim", false), &news),
                at: "2026-11-09T10:00:00Z".into(),
                receipt: None,
                tasks: Vec::new(),
                steps: Default::default(),
            }];
            room.take(vec![feed]);
            cx.notify();
        });
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("room-source-0").is_some());

    // The page lists the one it watches, not the proposal.
    page.update(cx, |page, cx| page.show("sup".into(), cx));
    cx.run_until_parked();
    assert!(cx.debug_bounds("agent-sources").is_some());
    assert!(cx.debug_bounds("agent-source-0").is_some());
    assert!(cx.debug_bounds("agent-source-1").is_none());
}

#[gpui::test]
fn the_new_source_form_draws_the_tools_arguments_as_fields(cx: &mut TestAppContext) {
    use crate::data::agent_source_form::FormTool;
    use gpui::Modifiers;
    use serde_json::json;
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::text_input::bind_keys(cx);
        cx.set_global(Sources {
            list: vec![source("s-1", "menções a mim", false)],
        });
    });
    let dir = tempfile::tempdir().unwrap();
    let seen = dir.path().join("seen.json");
    let (page, cx) = cx.add_window_view(move |window, cx| {
        let sidebar = cx.new(|cx| Sidebar::new(false, false, false, seen, cx));
        let composer = cx.new(|cx| Composer::new(window, cx));
        AgentPage::new("sup".into(), sidebar, composer, cx)
    });
    cx.simulate_resize(gpui::size(gpui::px(1200.0), gpui::px(2400.0)));
    let mut suporte = super::editor_role::blank_agent();
    suporte.id = "sup".into();
    suporte.name = "Suporte".into();
    page.update(cx, |page, cx| {
        page.sidebar.update(cx, |sidebar, cx| {
            sidebar.apply_for_tests(
                SidebarData {
                    agents: vec![suporte],
                    ..SidebarData::default()
                },
                cx,
            )
        });
        page.show("sup".into(), cx);
    });
    cx.run_until_parked();
    // The source it has can be read now; "Nova fonte" opens the form.
    assert!(cx.debug_bounds("agent-source-read-0").is_some());
    let new = cx.debug_bounds("agent-source-new").unwrap();
    cx.simulate_click(new.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(cx.debug_bounds("agent-source-form").is_some());
    assert!(
        cx.debug_bounds("agent-source-new").is_none(),
        "one form at a time"
    );

    // The connectors and the first one's read-only tools, as the desktop
    // answers them (a test has no desktop).
    let tools = vec![
        FormTool {
            name: "slack_read_channel".into(),
            description: "Reads a channel".into(),
            input_schema: json!({"type": "object", "properties": {"channel": {"type": "string"}},
                "required": ["channel"]}),
        },
        FormTool {
            name: "slack_search_public".into(),
            description: "Finds messages".into(),
            input_schema: json!({"type": "object", "properties": {
                "query": {"type": "string", "description": "Search text."},
                "limit": {"type": "integer"},
                "include_bots": {"type": "boolean"}
            }, "required": ["query"]}),
        },
    ];
    page.update_in(cx, |page, window, cx| {
        page.take_source_connectors(vec![("slack".into(), "Slack".into())], window, cx);
        page.take_source_tools(Ok(tools), window, cx);
    });
    cx.run_until_parked();
    // It opens on the tool that searches, its required argument showing and
    // the optional ones under "Mais opções".
    assert_eq!(
        page.read_with(cx, |page, _| page.source_form.as_ref().unwrap().tool),
        1
    );
    assert!(cx.debug_bounds("source-field-query").is_some());
    assert!(cx.debug_bounds("source-field-limit").is_none());
    let more = cx.debug_bounds("source-form-more").unwrap();
    cx.simulate_click(more.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(cx.debug_bounds("source-field-limit").is_some());
    assert!(cx.debug_bounds("source-field-include_bots").is_some());

    // Another tool, other fields.
    let other = cx.debug_bounds("source-tool-0").unwrap();
    cx.simulate_click(other.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(cx.debug_bounds("source-field-channel").is_some());
    assert!(cx.debug_bounds("source-field-query").is_none());

    // The tools failing to read says why, in the form.
    page.update_in(cx, |page, window, cx| {
        page.take_source_tools(Err("the connector refused the call".into()), window, cx)
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("source-form-tools-failed").is_some());

    // "Cancelar" closes it.
    let cancel = cx.debug_bounds("agent-source-cancel").unwrap();
    cx.simulate_click(cancel.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(cx.debug_bounds("agent-source-form").is_none());
}

#[gpui::test]
fn editing_a_source_opens_the_form_on_what_it_has(cx: &mut TestAppContext) {
    use crate::data::agent_source_form::FormTool;
    use gpui::Modifiers;
    use serde_json::json;
    let mut watched = source("s-1", "menções a mim", false);
    watched.arguments = json!({"query": "to:me", "limit": 20}).to_string();
    watched.every_minutes = 30;
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::text_input::bind_keys(cx);
        cx.set_global(Sources {
            list: vec![watched],
        });
    });
    let dir = tempfile::tempdir().unwrap();
    let seen = dir.path().join("seen.json");
    let (page, cx) = cx.add_window_view(move |window, cx| {
        let sidebar = cx.new(|cx| Sidebar::new(false, false, false, seen, cx));
        let composer = cx.new(|cx| Composer::new(window, cx));
        AgentPage::new("sup".into(), sidebar, composer, cx)
    });
    cx.simulate_resize(gpui::size(gpui::px(1200.0), gpui::px(2400.0)));
    let mut suporte = super::editor_role::blank_agent();
    suporte.id = "sup".into();
    suporte.name = "Suporte".into();
    page.update(cx, |page, cx| {
        page.sidebar.update(cx, |sidebar, cx| {
            sidebar.apply_for_tests(
                SidebarData {
                    agents: vec![suporte],
                    ..SidebarData::default()
                },
                cx,
            )
        });
        page.show("sup".into(), cx);
    });
    cx.run_until_parked();
    let edit = cx.debug_bounds("agent-source-edit-0").unwrap();
    cx.simulate_click(edit.center(), Modifiers::none());
    cx.run_until_parked();
    // Its name, its pace; then its connector and tool as each is read.
    page.read_with(cx, |page, cx| {
        let form = page.source_form.as_ref().unwrap();
        assert_eq!(form.name.read(cx).text(), "menções a mim");
        assert_eq!(form.every, 30);
        assert_eq!(
            form.editing.as_ref().map(|source| source.id.as_str()),
            Some("s-1")
        );
    });
    let tools = vec![
        FormTool {
            name: "slack_read_channel".into(),
            description: String::new(),
            input_schema: json!({"type": "object", "properties": {"channel": {"type": "string"}}}),
        },
        FormTool {
            name: "slack_search_public".into(),
            description: String::new(),
            input_schema: json!({"type": "object", "properties": {
                "query": {"type": "string"}, "limit": {"type": "integer"}
            }, "required": ["query"]}),
        },
    ];
    page.update_in(cx, |page, window, cx| {
        page.take_source_connectors(
            vec![
                ("trello".into(), "Trello".into()),
                ("slack".into(), "Slack".into()),
            ],
            window,
            cx,
        );
        page.take_source_tools(Ok(tools), window, cx);
    });
    cx.run_until_parked();
    page.read_with(cx, |page, cx| {
        let form = page.source_form.as_ref().unwrap();
        assert_eq!(form.connector, 1, "its own connector, not the first");
        assert_eq!(form.tool, 1, "its own tool");
        let typed: Vec<(String, String)> = form
            .fields
            .iter()
            .map(|field| {
                (
                    field.field.name.clone(),
                    field.input.read(cx).text().to_string(),
                )
            })
            .collect();
        assert!(
            typed.contains(&("query".into(), "to:me".into())),
            "{typed:?}"
        );
        assert!(typed.contains(&("limit".into(), "20".into())), "{typed:?}");
    });
    // It saves over the source: the button says so.
    assert!(cx.debug_bounds("agent-source-create").is_some());
    cx.update(|_, cx| {
        use crate::i18n::Translate;
        assert_eq!(cx.t("agents.sources.form.save"), "Salvar")
    });
}

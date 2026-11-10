//! The report under an answer to tasks, driven headlessly: it shows the
//! tasks the status named, with their lines, and a row opens its task.

use gpui::{AppContext, Modifiers, TestAppContext, WindowAppearance};

use super::{AgentPage, AgentPageEvent};
use crate::data::loader::SidebarData;
use crate::data::task_reports::{ReportState, TaskReport};
use crate::data::team_room::{HomeFeed, RoomLine, Speaker};
use crate::screens::composer::Composer;
use crate::screens::sidebar::Sidebar;
use crate::theme::{Theme, ThemePreference};

#[gpui::test]
fn an_answer_to_tasks_carries_their_report(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
    });
    let dir = tempfile::tempdir().unwrap();
    let seen = dir.path().join("seen.json");
    let (page, cx) = cx.add_window_view(move |window, cx| {
        let sidebar = cx.new(|cx| Sidebar::new(false, false, false, seen, cx));
        let composer = cx.new(|cx| Composer::new(window, cx));
        AgentPage::new("dev".into(), sidebar, composer, cx)
    });
    cx.simulate_resize(gpui::size(gpui::px(1200.0), gpui::px(1400.0)));
    let mut dev = super::editor_role::blank_agent();
    dev.id = "dev".into();
    dev.name = "Dev".into();
    let report = TaskReport {
        title: "Campo X na Shopify".into(),
        state: ReportState::TurnEnded,
        task_id: Some("w-campo".into()),
        provider_id: "claude_code".into(),
        agent_id: Some("dev".into()),
        files: 2,
        additions: 30,
        deletions: 4,
        seconds: Some(360),
    };
    page.update(cx, |page, cx| {
        page.sidebar.update(cx, |sidebar, cx| {
            sidebar.apply_for_tests(
                SidebarData {
                    agents: vec![dev],
                    task_reports: [("t-1".to_string(), report)].into(),
                    ..SidebarData::default()
                },
                cx,
            )
        });
        page.open_room(None, cx);
    });
    cx.run_until_parked();
    let opened = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let sink = opened.clone();
    page.update(cx, |page, cx| {
        cx.subscribe_self(move |_, event: &AgentPageEvent, _| {
            if let AgentPageEvent::OpenTask(task) = event {
                sink.borrow_mut().push(task.clone());
            }
        })
        .detach();
        let room = page.room.as_ref().unwrap().0.clone();
        room.update(cx, |room, cx| {
            let mut feed = HomeFeed::new("dev", "s-home");
            feed.lines = vec![RoomLine {
                key: "s-home:t".into(),
                agent_id: "dev".into(),
                session_id: "s-home".into(),
                speaker: Speaker::Agent,
                text: "Implementei o envio do campo.".into(),
                at: "2026-11-09T10:06:00Z".into(),
                receipt: None,
                tasks: vec!["t-1".into(), "unknown".into()],
                steps: Default::default(),
            }];
            room.take(vec![feed]);
            cx.notify();
        });
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("room-report-0").is_some());
    let row = cx.debug_bounds("room-report-0-0").unwrap();
    assert!(
        cx.debug_bounds("room-report-0-1").is_none(),
        "an unknown task stays out"
    );
    cx.simulate_click(row.center(), Modifiers::none());
    cx.run_until_parked();
    assert_eq!(opened.borrow().as_slice(), ["w-campo"]);
}

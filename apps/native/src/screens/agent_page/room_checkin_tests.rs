//! A pull request's check-in opens it in DCC's Pull Requests; "GitHub ↗"
//! beside it still opens the page in the browser.

use gpui::{AppContext, Modifiers, TestAppContext, WindowAppearance};

use super::room_tests::agent;
use super::{AgentPage, AgentPageEvent};
use crate::data::check_ins::{CheckIn, CheckInKind, CheckIns};
use crate::data::loader::SidebarData;
use crate::screens::composer::Composer;
use crate::screens::sidebar::Sidebar;
use crate::theme::{Theme, ThemePreference};

const URL: &str = "https://github.com/acme/app/pull/771";

fn review(key: &str, url: &str) -> CheckIn {
    CheckIn {
        key: key.into(),
        kind: CheckInKind::ReviewRequested,
        title: "Fix/trackeamento ads".into(),
        place: "acme/app #771".into(),
        url: Some(url.into()),
        workspace_id: None,
        session_id: None,
        project_id: None,
        at: String::new(),
        unread: false,
    }
}

#[gpui::test]
fn a_pull_requests_check_in_opens_in_dcc_and_github_stays_beside_it(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::text_input::bind_keys(cx);
        let mut checks = CheckIns::load(None);
        let now = chrono::Utc::now();
        checks.take(CheckInKind::ReviewRequested, Vec::new(), now);
        checks.take(
            CheckInKind::ReviewRequested,
            vec![review("pr:771", URL)],
            now,
        );
        cx.set_global(checks);
    });
    let seen = dir.path().join("seen.json");
    let (page, cx) = cx.add_window_view(move |window, cx| {
        let sidebar = cx.new(|cx| Sidebar::new(false, false, false, seen, cx));
        let composer = cx.new(|cx| Composer::new(window, cx));
        AgentPage::new("rev".into(), sidebar, composer, cx)
    });
    cx.simulate_resize(gpui::size(gpui::px(1200.0), gpui::px(1400.0)));
    page.update(cx, |page, cx| {
        page.sidebar.update(cx, |sidebar, cx| {
            sidebar.apply_for_tests(
                SidebarData {
                    agents: vec![agent("rev", "Revisor", "reviewer", Vec::new())],
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
    page.update(cx, |_, cx| {
        cx.subscribe_self(move |_, event: &AgentPageEvent, _| {
            if let AgentPageEvent::OpenPullRequest(id) = event {
                sink.borrow_mut().push(id.clone());
            }
        })
        .detach();
    });

    // "GitHub ↗" opens the browser, not Pull Requests, and reads the card.
    let github = cx.debug_bounds("room-check-in-github-0").unwrap();
    cx.simulate_click(github.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(opened.borrow().is_empty());
    assert_eq!(cx.update(|_, cx| cx.global::<CheckIns>().unread()), 0);

    // The card itself: the pull request in DCC, found by its link.
    let card = cx.debug_bounds("room-check-in-0").unwrap();
    cx.simulate_click(
        card.left_center() + gpui::point(gpui::px(40.0), gpui::px(0.0)),
        Modifiers::none(),
    );
    cx.run_until_parked();
    assert_eq!(opened.borrow().as_slice(), [URL]);
}

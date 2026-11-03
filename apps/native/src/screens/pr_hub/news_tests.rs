//! A pull request the person reviewed that got commits since: it comes
//! first in "Revisando" with its tag, and opening it sees it.

use gpui::{Modifiers, TestAppContext, WindowAppearance, px, size};
use serde_json::json;

use super::PrHub;
use super::tests::item;
use crate::data::desktop_api::DesktopApi;
use crate::data::pr_news::PrNews;
use crate::theme::{Theme, ThemePreference};

#[gpui::test]
fn a_reviewed_pull_request_with_commits_since_comes_first_until_opened(cx: &mut TestAppContext) {
    use crate::test_support::fake_desktop_routes;
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::text_input::bind_keys(cx);
        cx.set_global(PrNews::in_memory());
    });
    // pr-1 waits on the review; pr-2 was reviewed and got two commits since.
    let list = json!({
        "items": [
            item("pr-1", 1, "Fix login", true, false),
            item("pr-2", 2, "Novas telas", false, false),
        ],
        "warnings": [],
    })
    .to_string();
    let reviewed = json!({
        "items": [{
            "url": "https://github.com/acme/app/pull/2", "number": 2, "repository": "acme/app",
            "title": "Novas telas", "author": "bob", "state": "APPROVED",
            "since": { "reviewedAt": "2026-11-03T12:00:00Z", "commits": 2, "replies": 0,
                       "lastActivity": "2026-11-03T14:00:00Z" },
        }],
        "warnings": [],
    })
    .to_string();
    let detail = json!({ "body": null, "comments": [], "checks": [] }).to_string();
    let (port, _desktop) = fake_desktop_routes(vec![
        ("/api/v1/pull-request-hub/reviewed", 200, reviewed),
        ("/api/v1/pull-request-hub/list", 200, list),
        ("/api/v1/pull-request-hub/detail", 200, detail.clone()),
        ("/api/v1/pull-request-hub/detail", 200, detail),
    ]);
    let (hub, cx) = cx.add_window_view(PrHub::new);
    cx.simulate_resize(size(px(1200.0), px(800.0)));
    hub.update(cx, |hub, cx| {
        hub.api = Some(DesktopApi::new(port, "k"));
        hub.refresh(cx);
    });
    cx.run_until_parked();
    let first = cx
        .debug_bounds("pr-card-pr-2")
        .expect("reviewed: in Revisando");
    let second = cx.debug_bounds("pr-card-pr-1").unwrap();
    assert!(first.top() < second.top(), "news first");
    assert!(cx.debug_bounds("pr-news-pr-2").is_some(), "its tag");
    assert!(cx.debug_bounds("pr-news-pr-1").is_none());
    assert_eq!(cx.update(|_, cx| cx.global::<PrNews>().count()), 1);
    // The person sees they reviewed it; pr-1 they did not.
    assert!(cx.debug_bounds("pr-my-review-pr-2").is_some());
    assert!(cx.debug_bounds("pr-my-review-pr-1").is_none());
    // Shown by itself it keeps the tag; opened, it is seen and stays put,
    // selected, until the hub opens again.
    cx.simulate_click(first.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(cx.debug_bounds("pr-news-pr-2").is_none());
    assert!(cx.debug_bounds("pr-my-review-pr-2").is_some());
    assert_eq!(cx.update(|_, cx| cx.global::<PrNews>().count()), 0);
    let still = cx.debug_bounds("pr-card-pr-2").unwrap();
    assert_eq!(still.top(), first.top(), "it did not move");
    assert_eq!(
        hub.read_with(cx, |hub, _| hub.selected.clone()).as_deref(),
        Some("pr-2")
    );
}

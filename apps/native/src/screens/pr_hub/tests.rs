//! The hub driven headlessly against a fake desktop: the list and its
//! filters and search, the first one chosen, the summary of another, a
//! comment, "Trabalhar nisto", and a merge after its confirmation.

use gpui::{Modifiers, TestAppContext, WindowAppearance, px, size};
use serde_json::json;

use super::{PrHub, PrHubEvent};
use crate::data::desktop_api::DesktopApi;
use crate::theme::{Theme, ThemePreference};

fn item(id: &str, number: u32, title: &str, reviewing: bool, mine: bool) -> serde_json::Value {
    json!({
        "id": id, "provider": "github", "host": "github.com", "repositoryId": "r", "projectId": "p-1",
        "repositoryName": "acme/app", "repositoryRoot": "/repo", "forgeLogin": "ana",
        "number": number, "title": title, "body": null, "url": format!("https://github.com/acme/app/pull/{number}"),
        "author": { "login": "bob", "name": "Bob" }, "headBranch": format!("feat/{number}"),
        "baseBranch": "main", "state": "open", "isDraft": false, "reviewDecision": null,
        "reviewRequestedForViewer": reviewing, "createdByViewer": mine,
        "reviewers": [{ "login": "ana" }], "additions": 12, "deletions": 3, "changedFiles": 2,
        "commentCount": 1, "checksState": "success", "updatedAt": "2026-10-27T10:00:00Z",
        "linkedWorkspaceId": null, "linkedWorkspaceName": null,
    })
}

#[gpui::test]
fn the_hub_lists_filters_and_reads_a_pull_request(cx: &mut TestAppContext) {
    use crate::test_support::fake_desktop_routes;
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::text_input::bind_keys(cx);
    });
    let list = json!({
        "items": [item("pr-1", 1, "Fix login", true, false), item("pr-2", 2, "Docs", false, true)],
        "warnings": [],
    })
    .to_string();
    let detail = json!({
        "body": "Corrige o login.", "comments": [{ "id": "c-1", "body": "**ok**", "author": { "login": "ana" } }],
        "checks": [{ "name": "ci", "state": "success" }], "files": [], "inlineComments": [],
        "reviewCapabilities": { "inlineComments": true, "approve": true, "requestChanges": true,
            "replyToThreads": true, "resolveThreads": true },
        "mergeCapabilities": null,
    })
    .to_string();
    let (port, desktop) = fake_desktop_routes(vec![
        ("/api/v1/pull-request-hub/list", 200, list.clone()),
        ("/api/v1/pull-request-hub/detail", 200, detail.clone()),
        ("/api/v1/pull-request-hub/detail", 200, detail.clone()),
        ("/api/v1/pull-request-hub/comment", 200, r#"{"comment":{"id":"c-2","body":"Valeu"}}"#.into()),
        ("/api/v1/pull-request-hub/detail", 200, detail),
        ("/api/v1/pull-request-hub/list", 200, list.clone()),
        (
            "/api/v1/workspaces/from-source-url",
            200,
            r#"{"workspaceId":"w-pr","setupReport":{"status":"pending","steps":[],"message":null},"setupHints":[]}"#.into(),
        ),
        ("/api/v1/pull-request-hub/list", 200, list),
    ]);
    let (hub, cx) = cx.add_window_view(PrHub::new);
    cx.simulate_resize(size(px(1200.0), px(800.0)));
    let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let sink = events.clone();
    hub.update(cx, |hub, cx| {
        hub.api = Some(DesktopApi::new(port, "k"));
        cx.subscribe_self(move |_, event: &PrHubEvent, _| {
            sink.borrow_mut().push(match event {
                PrHubEvent::OpenTask(id) => format!("task {id}"),
                PrHubEvent::Created(id) => format!("created {id}"),
            })
        })
        .detach();
        hub.refresh(cx);
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("pr-card-pr-1").is_some());
    assert!(cx.debug_bounds("pr-card-pr-2").is_some());
    assert_eq!(
        hub.read_with(cx, |hub, _| hub.selected.clone()).as_deref(),
        Some("pr-1"),
        "the first one"
    );
    assert!(cx.debug_bounds("pr-summary").is_some());
    // "Meus" keeps the one the person opened; the search narrows by branch.
    let bounds = cx.debug_bounds("pr-filter-mine").unwrap();
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(cx.debug_bounds("pr-card-pr-1").is_none());
    assert_eq!(
        hub.read_with(cx, |hub, _| hub.selected.clone()).as_deref(),
        Some("pr-2")
    );
    hub.update(cx, |hub, cx| {
        hub.set_filter(crate::data::pr_hub::Filter::All, cx);
        hub.search
            .update(cx, |input, cx| input.set_text("feat/9", cx));
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("pr-list-empty").is_some());
    hub.update(cx, |hub, cx| {
        hub.search.update(cx, |input, cx| input.set_text("", cx))
    });
    cx.run_until_parked();
    // A comment, then the task to work on it.
    hub.update(cx, |hub, cx| {
        hub.comment
            .update(cx, |area, cx| area.set_text("Valeu", cx));
        hub.send_comment(cx);
    });
    cx.run_until_parked();
    assert_eq!(
        hub.read_with(cx, |hub, cx| hub.comment.read(cx).text().to_string()),
        ""
    );
    hub.update(cx, |hub, cx| hub.work_on(cx));
    cx.run_until_parked();
    let seen = desktop.join().unwrap();
    cx.run_until_parked();
    assert_eq!(events.borrow().as_slice(), ["created w-pr"]);
    let body = |path: &str| {
        let (_, body) = seen
            .iter()
            .find(|(request, _)| request.ends_with(path))
            .unwrap();
        serde_json::from_str::<serde_json::Value>(body).unwrap()
    };
    assert_eq!(body("/comment")["body"], "Valeu");
    assert_eq!(body("/comment")["forgeLogin"], "ana");
    let created = body("/from-source-url");
    assert_eq!(created["url"], "https://github.com/acme/app/pull/2");
    assert_eq!(created["forgeLogin"], "ana");
}

#[gpui::test]
fn narrow_shows_the_list_or_the_detail(cx: &mut TestAppContext) {
    use crate::test_support::fake_desktop_routes;
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::text_input::bind_keys(cx);
    });
    let list = json!({ "items": [item("pr-1", 1, "Fix login", false, false)], "warnings": [] })
        .to_string();
    let detail = json!({ "body": null, "comments": [], "checks": [] }).to_string();
    let (port, _desktop) = fake_desktop_routes(vec![
        ("/api/v1/pull-request-hub/list", 200, list),
        ("/api/v1/pull-request-hub/detail", 200, detail),
    ]);
    let (hub, cx) = cx.add_window_view(PrHub::new);
    cx.simulate_resize(size(px(700.0), px(800.0)));
    hub.update(cx, |hub, cx| {
        hub.api = Some(DesktopApi::new(port, "k"));
        hub.refresh(cx);
    });
    cx.run_until_parked();
    // One paint measures the width, the next lays out for it.
    hub.update(cx, |_, cx| cx.notify());
    cx.run_until_parked();
    assert!(cx.debug_bounds("pr-list").is_some());
    assert!(cx.debug_bounds("pr-detail").is_none());
    let bounds = cx.debug_bounds("pr-card-pr-1").unwrap();
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(cx.debug_bounds("pr-list").is_none());
    assert!(cx.debug_bounds("pr-back").is_some());
    let bounds = cx.debug_bounds("pr-back").unwrap();
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(cx.debug_bounds("pr-list").is_some());
}

#[gpui::test]
fn a_github_pull_request_merges_after_its_confirmation(cx: &mut TestAppContext) {
    use crate::test_support::fake_desktop_routes;
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::text_input::bind_keys(cx);
    });
    let list =
        json!({ "items": [item("pr-1", 1, "Fix login", true, false)], "warnings": [] }).to_string();
    let detail = |mergeable: &str| {
        json!({
            "body": null, "comments": [], "checks": [], "files": [], "inlineComments": [],
            "reviewCapabilities": null,
            "mergeCapabilities": { "allowedMethods": ["squash", "rebase"], "viewerCanMerge": true,
                "mergeable": mergeable, "headSha": "0123456789abcdef0123" },
        })
        .to_string()
    };
    let (port, desktop) = fake_desktop_routes(vec![
        ("/api/v1/pull-request-hub/list", 200, list.clone()),
        ("/api/v1/pull-request-hub/detail", 200, detail("MERGEABLE")),
        (
            "/api/v1/pull-request-hub/merge",
            200,
            r#"{"status":"queued","url":null}"#.into(),
        ),
        (
            "/api/v1/pull-request-hub/detail",
            200,
            detail("CONFLICTING"),
        ),
        ("/api/v1/pull-request-hub/list", 200, list),
    ]);
    let (hub, cx) = cx.add_window_view(PrHub::new);
    cx.simulate_resize(size(px(1200.0), px(800.0)));
    hub.update(cx, |hub, cx| {
        hub.api = Some(DesktopApi::new(port, "k"));
        hub.refresh(cx);
    });
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("pr-merge").is_some(),
        "Mesclar, GitHub only"
    );
    assert!(cx.debug_bounds("pr-merge-dialog").is_none());
    hub.update(cx, |hub, cx| hub.ask_merge("squash", cx));
    cx.run_until_parked();
    assert!(cx.debug_bounds("pr-merge-dialog").is_some());
    hub.update(cx, |hub, cx| hub.cancel_merge(cx));
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("pr-merge-dialog").is_none(),
        "Cancelar closes it"
    );
    hub.update(cx, |hub, cx| {
        hub.ask_merge("squash", cx);
        hub.confirm_merge(cx);
    });
    cx.run_until_parked();
    let seen = desktop.join().unwrap();
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("pr-merge-dialog").is_none(),
        "closed once accepted"
    );
    let (pending, error) = hub.read_with(cx, |hub, _| (hub.pending_merge, hub.merge_error.clone()));
    assert_eq!((pending, error), (None, None));
    // Read again: now in conflict, so off with its reason.
    let blocked = hub.read_with(cx, |hub, cx| {
        let item = hub.selected_item(cx).unwrap();
        crate::data::pr_hub::merge_blocked(&item, hub.merge_detail(&item))
    });
    assert_eq!(blocked, Some("pullRequests.merge.conflictsBlocked"));
    let (_, body) = seen
        .iter()
        .find(|(path, _)| path.ends_with("/merge"))
        .unwrap();
    let body: serde_json::Value = serde_json::from_str(body).unwrap();
    assert_eq!(body["method"], "squash");
    assert_eq!(body["expectedHeadSha"], "0123456789abcdef0123");
    assert_eq!(body["repositoryRoot"], "/repo");
    assert_eq!(body["number"], 1);
    assert_eq!(body["forgeLogin"], "ana");
}

//! The hub driven headlessly against a fake desktop: the list and its
//! filters and search, the first one chosen, the summary of another, a
//! comment, "Trabalhar nisto", and a merge after its confirmation.

use gpui::{Modifiers, TestAppContext, WindowAppearance, px, size};
use serde_json::json;

use super::{PrHub, PrHubEvent};
use crate::data::desktop_api::DesktopApi;
use crate::theme::{Theme, ThemePreference};

pub(super) fn item(
    id: &str,
    number: u32,
    title: &str,
    reviewing: bool,
    mine: bool,
) -> serde_json::Value {
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
    // "Revisando" opens first: only what needs the person.
    assert!(cx.debug_bounds("pr-card-pr-1").is_some());
    assert!(cx.debug_bounds("pr-card-pr-2").is_none());
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
    let list =
        json!({ "items": [item("pr-1", 1, "Fix login", true, false)], "warnings": [] }).to_string();
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

/// The merge dialog for one pull request with these merge capabilities;
/// returns what the merge route received after the confirmation.
fn merge_with(
    capabilities: serde_json::Value,
    delete_branch: bool,
    cx: &mut TestAppContext,
) -> (serde_json::Value, bool, bool) {
    use crate::test_support::fake_desktop_routes;
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::text_input::bind_keys(cx);
    });
    let list =
        json!({ "items": [item("pr-1", 1, "Fix login", true, false)], "warnings": [] }).to_string();
    let detail = json!({
        "body": null, "comments": [], "checks": [], "files": [], "inlineComments": [],
        "reviewCapabilities": null, "mergeCapabilities": capabilities,
    })
    .to_string();
    let (port, desktop) = fake_desktop_routes(vec![
        ("/api/v1/pull-request-hub/list", 200, list.clone()),
        ("/api/v1/pull-request-hub/detail", 200, detail.clone()),
        (
            "/api/v1/pull-request-hub/merge",
            200,
            r#"{"status":"submitted","url":null}"#.into(),
        ),
        ("/api/v1/pull-request-hub/detail", 200, detail),
        ("/api/v1/pull-request-hub/list", 200, list),
    ]);
    let (hub, cx) = cx.add_window_view(PrHub::new);
    cx.simulate_resize(size(px(1200.0), px(800.0)));
    hub.update(cx, |hub, cx| {
        hub.api = Some(DesktopApi::new(port, "k"));
        hub.refresh(cx);
    });
    cx.run_until_parked();
    hub.update(cx, |hub, cx| {
        hub.ask_merge("squash", cx);
        hub.merge_delete_branch = delete_branch;
        cx.notify();
    });
    cx.run_until_parked();
    let waits = cx.debug_bounds("pr-merge-wait").is_some();
    let offers_delete = cx.debug_bounds("pr-merge-delete-branch").is_some();
    hub.update(cx, |hub, cx| hub.confirm_merge(cx));
    cx.run_until_parked();
    let seen = desktop.join().unwrap();
    cx.run_until_parked();
    let (_, body) = seen
        .iter()
        .find(|(path, _)| path.ends_with("/merge"))
        .expect("the merge was sent");
    (serde_json::from_str(body).unwrap(), waits, offers_delete)
}

#[gpui::test]
fn a_pull_request_that_waits_is_left_to_merge_when_ready(cx: &mut TestAppContext) {
    // GitHub blocks it (a rule of the repository) and the repository has auto-merge.
    let (sent, waits, offers_delete) = merge_with(
        json!({ "allowedMethods": ["squash"], "viewerCanMerge": true, "mergeable": "MERGEABLE",
            "headSha": "0123456789abcdef0123", "mergeStateStatus": "BLOCKED",
            "allowAutoMerge": true, "canDeleteBranch": true }),
        // Asked for, but the branch is never deleted before the merge happens.
        true,
        cx,
    );
    assert!(waits, "the dialog says what it waits on");
    assert!(!offers_delete, "no branch to delete yet");
    assert_eq!(sent["whenReady"], true);
    assert_eq!(sent["deleteBranch"], false);
}

#[gpui::test]
fn a_ready_pull_request_merges_now_and_takes_its_branch_when_asked(cx: &mut TestAppContext) {
    let (sent, waits, offers_delete) = merge_with(
        json!({ "allowedMethods": ["squash"], "viewerCanMerge": true, "mergeable": "MERGEABLE",
            "headSha": "0123456789abcdef0123", "mergeStateStatus": "CLEAN",
            "allowAutoMerge": true, "canDeleteBranch": true }),
        true,
        cx,
    );
    assert!(!waits && offers_delete);
    assert_eq!(sent["whenReady"], false);
    assert_eq!(sent["deleteBranch"], true);
}

#[gpui::test]
fn a_pull_request_is_closed_after_its_confirmation_and_stays_to_be_reopened(
    cx: &mut TestAppContext,
) {
    use crate::screens::pr_hub::actions::Ask;
    use crate::test_support::fake_desktop_routes;
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::text_input::bind_keys(cx);
    });
    let open =
        json!({ "items": [item("pr-1", 1, "Fix login", true, false)], "warnings": [] }).to_string();
    let none = json!({ "items": [], "warnings": [] }).to_string();
    let detail = |state: &str| {
        json!({
            "body": null, "comments": [], "checks": [], "files": [], "inlineComments": [],
            "reviewCapabilities": null, "state": state,
            "mergeCapabilities": { "allowedMethods": ["squash"], "viewerCanMerge": true,
                "mergeable": "MERGEABLE", "headSha": "0123456789abcdef0123",
                "mergeStateStatus": "CLEAN", "canDeleteBranch": true },
        })
        .to_string()
    };
    let (port, desktop) = fake_desktop_routes(vec![
        ("/api/v1/pull-request-hub/list", 200, open),
        ("/api/v1/pull-request-hub/detail", 200, detail("open")),
        (
            "/api/v1/pull-request-hub/action",
            200,
            r#"{"branchDeleted":true}"#.into(),
        ),
        // Read again: no longer among the open ones.
        ("/api/v1/pull-request-hub/list", 200, none),
        ("/api/v1/pull-request-hub/detail", 200, detail("closed")),
    ]);
    let (hub, cx) = cx.add_window_view(PrHub::new);
    cx.simulate_resize(size(px(1200.0), px(800.0)));
    hub.update(cx, |hub, cx| {
        hub.api = Some(DesktopApi::new(port, "k"));
        hub.refresh(cx);
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("pr-actions").is_some(), "the ⋯ menu");
    assert!(cx.debug_bounds("pr-action-dialog").is_none());
    // Picked in the menu: nothing is sent before the confirmation.
    hub.update(cx, |hub, cx| hub.ask_action(Ask::Close, cx));
    cx.run_until_parked();
    assert!(cx.debug_bounds("pr-action-dialog").is_some());
    assert!(cx.debug_bounds("pr-close-delete-branch").is_some());
    hub.update(cx, |hub, cx| {
        hub.actions.delete_branch = true;
        hub.actions
            .comment
            .update(cx, |area, cx| area.set_text("substituído pelo #9", cx));
        hub.confirm_action(cx);
    });
    cx.run_until_parked();
    let seen = desktop.join().unwrap();
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("pr-action-dialog").is_none(),
        "closed once done"
    );
    // No longer listed, but still on screen, closed, with its way back.
    let (listed, closed) = hub.read_with(cx, |hub, cx| {
        let item = hub
            .selected_item(cx)
            .expect("the closed one stays selected");
        (hub.items().len(), hub.is_closed(&item))
    });
    assert_eq!((listed, closed), (0, true));
    assert!(
        cx.debug_bounds("pr-merge").is_none(),
        "a closed one is not merged"
    );
    assert!(
        cx.debug_bounds("pr-actions").is_some(),
        "Reabrir is offered"
    );
    let (_, sent) = seen
        .iter()
        .find(|(path, _)| path.ends_with("/action"))
        .expect("the close was sent");
    let sent: serde_json::Value = serde_json::from_str(sent).unwrap();
    assert_eq!(sent["number"], 1);
    assert_eq!(
        sent["action"],
        json!({ "kind": "close", "comment": "substituído pelo #9", "deleteBranch": true })
    );
}

#[gpui::test]
fn a_failed_actions_check_shows_what_failed_and_runs_again(cx: &mut TestAppContext) {
    use crate::screens::pr_hub::checks::Log;
    use crate::test_support::fake_desktop_routes;
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::text_input::bind_keys(cx);
    });
    const JOB: &str = "https://github.com/acme/shop/actions/runs/11/job/22";
    let list =
        json!({ "items": [item("pr-1", 1, "Fix login", true, false)], "warnings": [] }).to_string();
    let detail = json!({
        "body": null, "comments": [], "files": [], "inlineComments": [],
        "reviewCapabilities": null, "mergeCapabilities": null,
        "checks": [
            { "name": "test", "state": "failure", "detailsUrl": JOB },
            { "name": "sonar", "state": "failure", "detailsUrl": "https://sonar.example.com/x" },
        ],
    })
    .to_string();
    let (port, desktop) = fake_desktop_routes(vec![
        ("/api/v1/pull-request-hub/list", 200, list),
        ("/api/v1/pull-request-hub/detail", 200, detail.clone()),
        (
            "/api/v1/pull-request-hub/check-log",
            200,
            json!({ "excerpt": "Run cargo test\nerror: test failed" }).to_string(),
        ),
        (
            "/api/v1/pull-request-hub/check-rerun",
            200,
            r#"{"ok":true}"#.into(),
        ),
        ("/api/v1/pull-request-hub/detail", 200, detail),
    ]);
    let (hub, cx) = cx.add_window_view(PrHub::new);
    cx.simulate_resize(size(px(1200.0), px(800.0)));
    hub.update(cx, |hub, cx| {
        hub.api = Some(DesktopApi::new(port, "k"));
        hub.refresh(cx);
    });
    cx.run_until_parked();
    // The Actions job has its log and its re-run; the other system only its page.
    assert!(cx.debug_bounds("pr-check-log-0").is_some());
    assert!(cx.debug_bounds("pr-check-rerun-0").is_some());
    assert!(cx.debug_bounds("pr-check-open-1").is_some());
    assert!(cx.debug_bounds("pr-check-log-1").is_none());
    assert!(
        cx.debug_bounds("pr-check-log").is_none(),
        "nothing read ahead"
    );
    let log = cx.debug_bounds("pr-check-log-0").unwrap();
    cx.simulate_click(log.center(), Modifiers::none());
    cx.run_until_parked();
    cx.background_executor
        .advance_clock(std::time::Duration::from_millis(50));
    for _ in 0..200 {
        cx.run_until_parked();
        if hub.read_with(cx, |hub, _| {
            matches!(hub.checks.logs.get(JOB), Some(Log::Read(_)))
        }) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    hub.read_with(cx, |hub, _| {
        assert_eq!(
            hub.checks.logs.get(JOB),
            Some(&Log::Read("Run cargo test\nerror: test failed".into()))
        );
    });
    assert!(cx.debug_bounds("pr-check-log").is_some());
    hub.update(cx, |hub, cx| hub.rerun_check(JOB.into(), cx));
    cx.run_until_parked();
    let seen = desktop.join().unwrap();
    cx.run_until_parked();
    let sent = |route: &str| -> serde_json::Value {
        let (_, body) = seen.iter().find(|(path, _)| path.ends_with(route)).unwrap();
        serde_json::from_str(body).unwrap()
    };
    assert_eq!(sent("/check-log")["detailsUrl"], JOB);
    assert_eq!(sent("/check-rerun")["detailsUrl"], JOB);
    // The log was of the run that failed: gone with the re-run.
    hub.read_with(cx, |hub, _| assert!(hub.checks.logs.is_empty()));
}

#[gpui::test]
fn the_summary_approves_with_its_text_without_opening_the_code(cx: &mut TestAppContext) {
    use crate::data::pr_review::ReviewEvent;
    use crate::test_support::fake_desktop_routes;
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::text_input::bind_keys(cx);
    });
    let list =
        json!({ "items": [item("pr-1", 1, "Fix login", true, false)], "warnings": [] }).to_string();
    let detail = json!({
        "body": null, "comments": [], "checks": [], "files": [], "inlineComments": [],
        "reviewCapabilities": { "approve": true, "requestChanges": true },
    })
    .to_string();
    let sent = json!({ "submitted": true, "url": null, "submittedCommentCount": 0,
        "bodySubmitted": true, "decisionSubmitted": true, "warning": null })
    .to_string();
    let (port, desktop) = fake_desktop_routes(vec![
        ("/api/v1/pull-request-hub/list", 200, list.clone()),
        ("/api/v1/pull-request-hub/detail", 200, detail.clone()),
        ("/api/v1/pull-request-hub/submit-review", 200, sent),
        ("/api/v1/pull-request-hub/detail", 200, detail),
        ("/api/v1/pull-request-hub/list", 200, list),
    ]);
    let (hub, cx) = cx.add_window_view(PrHub::new);
    cx.simulate_resize(size(px(1400.0), px(900.0)));
    hub.update(cx, |hub, cx| {
        hub.api = Some(DesktopApi::new(port, "k"));
        hub.refresh(cx);
    });
    cx.run_until_parked();
    // On the summary: the verdict, the text, then a button that says it.
    assert!(cx.debug_bounds("pr-summary-footer").is_some());
    let approve = cx.debug_bounds("pr-review-event-approve").unwrap();
    cx.simulate_click(approve.center(), Modifiers::none());
    cx.run_until_parked();
    hub.update(cx, |hub, cx| {
        assert_eq!(hub.review.event, ReviewEvent::Approve);
        let comment = hub.comment.clone();
        comment.update(cx, |area, cx| area.set_text("Pode entrar", cx));
    });
    cx.run_until_parked();
    let send = cx.debug_bounds("pr-send-comment").unwrap();
    cx.simulate_click(send.center(), Modifiers::none());
    cx.run_until_parked();
    let seen = desktop.join().unwrap();
    cx.run_until_parked();
    let (_, body) = seen
        .iter()
        .find(|(request, _)| request.ends_with("/submit-review"))
        .expect("the review was sent, not a comment");
    let review = serde_json::from_str::<serde_json::Value>(body).unwrap();
    assert_eq!(review["event"], "approve");
    assert_eq!(review["body"], "Pode entrar");
    assert!(
        !seen
            .iter()
            .any(|(request, _)| request.ends_with("/comment"))
    );
    hub.read_with(cx, |hub, cx| {
        assert!(hub.comment.read(cx).text().is_empty());
        assert_eq!(hub.review.event, ReviewEvent::Comment);
    });
}

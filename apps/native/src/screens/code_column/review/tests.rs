//! The PR sections over a fake desktop: who acts on the forge, the captured
//! delivery failure and its recoveries, the review state and its reviewers.

use dcc_core::domain::workspace_git::WorkspacePrStatusOutput;
use gpui::{Modifiers, TestAppContext, WindowAppearance, px, size};
use serde_json::{Value, json};

use super::super::{CodeColumn, CodeColumnEvent, CodeColumnTab};
use super::ReviewLoad;
use crate::data::desktop_api::DesktopApi;
use crate::data::forge::ReviewerAction;
use crate::test_support::{fake_desktop_answers, git_checkout, task_database};
use crate::theme::{Theme, ThemePreference};
use crate::ui::Toaster;

struct Setup {
    _db: tempfile::TempDir,
    _repo: tempfile::TempDir,
    server: std::thread::JoinHandle<Vec<(String, String)>>,
    events: std::rc::Rc<std::cell::RefCell<Vec<CodeColumnEvent>>>,
}

fn answer(value: Value) -> (u16, String) {
    (200, value.to_string())
}

fn open(
    cx: &mut TestAppContext,
    answers: Vec<(u16, String)>,
) -> (
    gpui::Entity<CodeColumn>,
    &mut gpui::VisualTestContext,
    Setup,
) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        Toaster::init(cx);
    });
    let repo = git_checkout(&[("a.txt", "um\n")]);
    let db = tempfile::tempdir().unwrap();
    let db_path = task_database(db.path(), "w-1", repo.path(), None);
    let (port, server) = fake_desktop_answers(answers);
    let root = repo.path().to_string_lossy().into_owned();
    let (column, cx) = cx.add_window_view(|_, cx| {
        let mut column = CodeColumn::new(CodeColumnTab::Changes, cx);
        column.db_path = db_path;
        column.endpoint = Some(DesktopApi::new(port, "k"));
        column
    });
    let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let seen = events.clone();
    column.update(cx, |column, cx| {
        cx.subscribe_self(move |_, event: &CodeColumnEvent, _| {
            seen.borrow_mut().push(event.clone())
        })
        .detach();
        column.set_root(Some(root), cx);
    });
    cx.run_until_parked();
    cx.simulate_resize(size(px(340.0), px(900.0)));
    cx.run_until_parked();
    (
        column,
        cx,
        Setup {
            _db: db,
            _repo: repo,
            server,
            events,
        },
    )
}

fn requests(server: std::thread::JoinHandle<Vec<(String, String)>>) -> Vec<(String, Value)> {
    server
        .join()
        .unwrap()
        .into_iter()
        .map(|(line, body)| {
            let path = line.split_whitespace().nth(1).unwrap().to_string();
            (path, serde_json::from_str(&body).unwrap_or(Value::Null))
        })
        .collect()
}

fn connected(login: &str, bound: &str) -> Value {
    json!({
        "provider": "github", "host": "github.com", "remoteState": "ok",
        "effectiveLogin": login, "boundLogin": bound,
    })
}

fn failure() -> Value {
    json!({
        "attemptToken": "t-1", "workspaceRoot": "/r", "branch": "feature", "headSha": "abcdef1234",
        "operation": "push", "classification": "authentication", "remote": "origin",
        "operationTarget": null, "pushTarget": { "remote": "origin", "branch": "feature", "url": null },
        "output": "fatal: Authentication failed", "outputTruncated": false,
        "changedFiles": ["src/main.rs"], "changedFilesTruncated": false, "externalUrl": null,
        "availableActions": ["retry", "send-to-agent"], "createdAt": "2026-10-24T12:00:00Z",
    })
}

#[gpui::test]
fn a_captured_failure_opens_and_goes_to_the_agent(cx: &mut TestAppContext) {
    let (column, cx, setup) = open(
        cx,
        vec![
            answer(connected("ana", "ana")),
            answer(json!({ "accounts": [{ "login": "ana", "name": "Ana Lima" }] })),
            answer(json!({ "snapshot": failure() })),
            answer(json!({ "snapshot": failure(), "refreshPipeline": false })),
            answer(json!({ "snapshot": null })),
        ],
    );
    assert!(
        cx.debug_bounds("forge-identity").is_some(),
        "who pushes, in the header"
    );
    assert!(cx.debug_bounds("delivery-failure").is_some());
    let toggle = cx.debug_bounds("failure-toggle").unwrap();
    cx.simulate_click(toggle.center(), Modifiers::none());
    column.update(cx, |column, cx| {
        assert!(column.review.failure_open);
        // Retrying asks first and sends nothing yet.
        column.recover("retry", false, cx);
        assert_eq!(column.review.confirm_recovery.as_deref(), Some("retry"));
        column.cancel_recovery(cx);
        column.recover("send-to-agent", false, cx);
    });
    cx.run_until_parked();
    let events = setup.events.borrow().clone();
    let prompt = events
        .iter()
        .find_map(|event| match event {
            CodeColumnEvent::AskAgent { prompt, .. } => Some(prompt.clone()),
            _ => None,
        })
        .expect("the request went to the composer");
    assert!(prompt.contains("Attempt token: t-1"));
    column.read_with(cx, |column, _| {
        assert!(column.review.failure.is_none(), "read again")
    });
    let requests = requests(setup.server);
    let paths: Vec<&str> = requests.iter().map(|(path, _)| path.as_str()).collect();
    assert_eq!(
        paths,
        vec![
            "/api/v1/forge/context",
            "/api/v1/forge/accounts",
            "/api/v1/git/delivery-failure",
            "/api/v1/git/delivery-recovery",
            "/api/v1/git/delivery-failure",
        ]
    );
    assert_eq!(requests[3].1["action"], "send-to-agent");
    assert_eq!(requests[3].1["attemptToken"], "t-1");
    assert_eq!(requests[3].1["forgeLogin"], "ana");
}

fn review(reviewers: Value) -> Value {
    json!({
        "provider": "github", "number": 12, "url": "https://github.com/o/r/pull/12",
        "reviewState": "pending", "reviewers": reviewers, "reviewersAvailable": true,
        "approvalsAvailable": true, "approvalsRequired": 1, "approvalsReceived": 0,
        "approvalsLeft": 1, "mergeable": "mergeable", "hasConflicts": false, "behindBy": 0,
        "discussionsResolved": true,
    })
}

#[gpui::test]
fn the_review_state_lists_reviewers_and_withdraws_a_request(cx: &mut TestAppContext) {
    let reviewers = json!([
        { "login": "bia", "state": "pending" },
        { "login": "caio", "state": "approved" },
    ]);
    let (column, cx, setup) = open(
        cx,
        vec![
            answer(connected("ana", "bob")),
            answer(json!({ "accounts": [] })),
            answer(json!({ "snapshot": null })),
            answer(json!({ "snapshot": null })),
            answer(review(reviewers.clone())),
            answer(json!({ "requested": [], "removed": ["bia"] })),
            answer(json!({ "snapshot": null })),
            answer(review(json!([{ "login": "caio", "state": "approved" }]))),
        ],
    );
    column.update(cx, |column, cx| {
        column.delivery.pr = Some(WorkspacePrStatusOutput {
            provider: Some("github".into()),
            host: None,
            number: Some(12),
            title: None,
            url: None,
            head_branch: None,
            base_branch: None,
            state: Some("open".into()),
            is_draft: false,
            mergeable: None,
            merge_state_status: None,
        });
        column.read_review(true, cx);
    });
    cx.run_until_parked();
    column.read_with(cx, |column, _| {
        let Some(ReviewLoad::Ready(state)) = &column.review.state else {
            panic!("the review state read: {:?}", column.review.state);
        };
        assert_eq!(state.reviewers.len(), 2);
    });
    let toggle = cx
        .debug_bounds("review-state-toggle")
        .expect("the review state");
    cx.simulate_click(toggle.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(cx.debug_bounds("request-review-trigger").is_some());
    column.update(cx, |column, cx| {
        column.change_reviewer("bia".into(), ReviewerAction::Remove, cx)
    });
    cx.run_until_parked();
    let toasts = cx.update(|_, cx| Toaster::messages(cx));
    assert!(
        toasts
            .iter()
            .any(|toast| toast == "Pedido de revisão a @bia cancelado"),
        "{toasts:?}"
    );
    column.read_with(cx, |column, _| {
        let Some(ReviewLoad::Ready(state)) = &column.review.state else {
            panic!("read again");
        };
        assert_eq!(state.reviewers.len(), 1);
    });
    let requests = requests(setup.server);
    let change = requests
        .iter()
        .find(|(path, _)| path == "/api/v1/forge/request-reviewers")
        .unwrap();
    assert_eq!(change.1["remove"], json!(["bia"]));
    assert_eq!(change.1["add"], json!([]));
    assert_eq!(change.1["number"], 12);
}

#[gpui::test]
fn the_picker_asks_someone_not_on_the_review(cx: &mut TestAppContext) {
    let (column, cx, setup) = open(
        cx,
        vec![
            answer(connected("ana", "ana")),
            answer(json!({ "accounts": [] })),
            answer(json!({ "snapshot": null })),
            answer(json!({ "snapshot": null })),
            answer(review(json!([{ "login": "caio", "state": "approved" }]))),
            answer(json!({ "provider": "github", "candidates": [
                { "login": "caio", "suggested": true },
                { "login": "dani", "name": "Dani", "suggested": true },
                { "login": "eva", "suggested": false },
            ] })),
            answer(json!({ "requested": ["dani"], "removed": [] })),
        ],
    );
    column.update(cx, |column, cx| {
        column.delivery.pr = Some(WorkspacePrStatusOutput {
            provider: Some("github".into()),
            host: None,
            number: Some(12),
            title: None,
            url: None,
            head_branch: None,
            base_branch: None,
            state: Some("open".into()),
            is_draft: false,
            mergeable: None,
            merge_state_status: None,
        });
        column.read_review(true, cx);
    });
    cx.run_until_parked();
    let toggle = cx.debug_bounds("review-state-toggle").unwrap();
    cx.simulate_click(toggle.center(), Modifiers::none());
    cx.run_until_parked();
    let trigger = cx.debug_bounds("request-review-trigger").unwrap();
    cx.simulate_click(trigger.center(), Modifiers::none());
    cx.run_until_parked();
    // Caio is on the review already: only Dani and Eva are offered.
    assert!(cx.debug_bounds("reviewer-candidate-dani").is_some());
    assert!(cx.debug_bounds("reviewer-candidate-eva").is_some());
    assert!(cx.debug_bounds("reviewer-candidate-caio").is_none());
    let dani = cx.debug_bounds("reviewer-candidate-dani").unwrap();
    cx.simulate_click(dani.center(), Modifiers::none());
    cx.run_until_parked();
    column.read_with(cx, |column, _| assert!(column.review.picker.is_none()));
    let requests = requests(setup.server);
    let ask = requests
        .iter()
        .find(|(path, _)| path == "/api/v1/forge/request-reviewers")
        .expect("Dani was asked");
    assert_eq!(ask.1["add"], json!(["dani"]));
    let search = requests
        .iter()
        .find(|(path, _)| path == "/api/v1/forge/reviewer-candidates")
        .unwrap();
    assert_eq!(search.1["number"], 12);
}

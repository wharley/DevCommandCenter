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
    let (column, cx, setup) = open_routed(
        cx,
        &[("a.txt", "um\n")],
        vec![
            (
                "/api/v1/forge/context",
                200,
                connected("ana", "bob").to_string(),
            ),
            (
                "/api/v1/forge/review-state",
                200,
                review(reviewers.clone()).to_string(),
            ),
            (
                "/api/v1/forge/request-reviewers",
                200,
                json!({ "requested": [], "removed": ["bia"] }).to_string(),
            ),
            (
                "/api/v1/forge/review-state",
                200,
                review(json!([{ "login": "caio", "state": "approved" }])).to_string(),
            ),
        ],
    );
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
    let candidates = json!({ "provider": "github", "candidates": [
        { "login": "caio", "suggested": true },
        { "login": "dani", "name": "Dani", "suggested": true },
        { "login": "eva", "suggested": false },
    ] });
    let (column, cx, setup) = open_routed(
        cx,
        &[("a.txt", "um\n")],
        vec![
            (
                "/api/v1/forge/context",
                200,
                connected("ana", "ana").to_string(),
            ),
            (
                "/api/v1/forge/review-state",
                200,
                review(json!([{ "login": "caio", "state": "approved" }])).to_string(),
            ),
            (
                "/api/v1/forge/reviewer-candidates",
                200,
                candidates.to_string(),
            ),
            (
                "/api/v1/forge/request-reviewers",
                200,
                json!({ "requested": ["dani"], "removed": [] }).to_string(),
            ),
        ],
    );
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

/// The column over a checkout with `files` changed, the desktop answering by
/// route, and an open pull request #12.
fn open_routed<'a>(
    cx: &'a mut TestAppContext,
    files: &[(&str, &str)],
    routes: Vec<(&'static str, u16, String)>,
) -> (
    gpui::Entity<CodeColumn>,
    &'a mut gpui::VisualTestContext,
    Setup,
) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        Toaster::init(cx);
    });
    let repo = git_checkout(files);
    for (path, _) in files {
        std::fs::write(repo.path().join(path), "changed\n").unwrap();
    }
    let db = tempfile::tempdir().unwrap();
    let db_path = task_database(db.path(), "w-1", repo.path(), None);
    let (port, server) = crate::test_support::fake_desktop_routes(routes);
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
    column.update(cx, |column, cx| {
        column.delivery.pr = Some(WorkspacePrStatusOutput {
            provider: Some("github".into()),
            host: None,
            number: Some(12),
            title: None,
            url: Some("https://github.com/o/r/pull/12".into()),
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

fn thread_comment(
    thread: &str,
    path: &str,
    line: Option<i64>,
    resolved: bool,
    outdated: bool,
) -> Value {
    json!({
        "id": format!("{thread}-1"), "parentId": null, "threadId": thread, "path": path,
        "body": format!("Look at {thread}\nmore"), "diffHunk": null, "htmlUrl": null, "side": "RIGHT",
        "line": line, "startLine": null, "originalLine": line, "originalStartLine": null,
        "author": { "login": "bia", "avatarUrl": null, "htmlUrl": null },
        "createdAt": "2026-10-24T10:00:00Z", "updatedAt": null,
        "resolvable": true, "resolved": resolved, "outdated": outdated,
    })
}

#[gpui::test]
fn conversations_show_their_badge_and_go_to_the_agent_as_picked(cx: &mut TestAppContext) {
    let comments = json!({
        "provider": "github", "host": "github.com", "number": 12,
        "url": "https://github.com/o/r/pull/12",
        "comments": [
            thread_comment("t-open", "src/a.rs", Some(3), false, false),
            thread_comment("t-done", "src/a.rs", Some(9), true, false),
            thread_comment("t-old", "src/b.rs", Some(4), false, true),
        ],
    });
    let (column, cx, setup) = open_routed(
        cx,
        &[("src/a.rs", "um\n"), ("src/b.rs", "um\n")],
        vec![
            (
                "/api/v1/forge/context",
                200,
                connected("ana", "ana").to_string(),
            ),
            ("/api/v1/forge/review-comments", 200, comments.to_string()),
        ],
    );
    column.read_with(cx, |column, _| {
        assert_eq!(column.review.open_by_path.get("src/a.rs"), Some(&1));
        assert_eq!(column.review.open_by_path.get("src/b.rs"), Some(&1));
        // Still asking for something: picked from the start.
        assert!(column.review.picked.contains("t-open"));
        assert!(column.review.picked.contains("t-old"));
        assert!(!column.review.picked.contains("t-done"));
    });
    assert!(cx.debug_bounds("review-threads").is_some());
    assert!(
        cx.debug_bounds("review-badge-src/a.rs").is_some(),
        "the card's badge"
    );
    let toggle = cx.debug_bounds("threads-toggle").unwrap();
    cx.simulate_click(toggle.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(cx.debug_bounds("thread-t-done").is_some());
    column.update(cx, |column, cx| {
        column.toggle_thread_pick("t-old", cx);
        column.ask_agent_about_threads(cx);
    });
    let prompt = setup
        .events
        .borrow()
        .iter()
        .find_map(|event| match event {
            CodeColumnEvent::AskAgent { prompt, .. } => Some(prompt.clone()),
            _ => None,
        })
        .expect("the request went to the composer");
    assert!(prompt.contains(
        "Address these 1 code review thread on the pull request https://github.com/o/r/pull/12."
    ));
    assert!(prompt.contains("### 1. src/a.rs:3"));
    assert!(!prompt.contains("src/b.rs"));
    let _ = requests(setup.server);
}

#[gpui::test]
fn a_gitlab_pipeline_opens_a_jobs_log_and_runs_it_again(cx: &mut TestAppContext) {
    let gitlab = json!({
        "provider": "gitlab", "host": "gitlab.com", "remoteState": "ok",
        "effectiveLogin": "ana", "boundLogin": "ana",
    });
    let pipeline = json!({ "pipeline": {
        "id": 77, "sha": "abcdef123456", "refName": "feature", "status": "failed",
        "webUrl": "https://gitlab.com/o/r/-/pipelines/77", "duration": 185.0,
        "jobs": [
            { "id": 1, "name": "lint", "stage": "test", "status": "success", "duration": 12.0,
              "webUrl": null, "allowFailure": false, "retryable": false },
            { "id": 2, "name": "unit", "stage": "test", "status": "failed", "duration": 40.0,
              "webUrl": null, "allowFailure": false, "retryable": true },
        ],
    }});
    let (column, cx, setup) = open_routed(
        cx,
        &[("a.txt", "um\n")],
        vec![
            ("/api/v1/forge/context", 200, gitlab.to_string()),
            ("/api/v1/forge/pipeline", 200, pipeline.to_string()),
            ("/api/v1/forge/pipeline", 200, pipeline.to_string()),
            (
                "/api/v1/forge/pipeline-job-log",
                200,
                json!({ "jobId": 2, "content": "boom", "truncated": false, "loadedBytes": 4 })
                    .to_string(),
            ),
            (
                "/api/v1/forge/pipeline-job-retry",
                200,
                json!({ "ok": true }).to_string(),
            ),
        ],
    );
    assert!(cx.debug_bounds("gitlab-pipeline").is_some());
    let toggle = cx.debug_bounds("pipeline-toggle").unwrap();
    cx.simulate_click(toggle.center(), Modifiers::none());
    cx.run_until_parked();
    let log = cx
        .debug_bounds("job-log-2")
        .expect("the failed job's log button");
    cx.simulate_click(log.center(), Modifiers::none());
    cx.run_until_parked();
    column.read_with(cx, |column, _| {
        assert_eq!(column.review.active_job, Some(2));
        assert_eq!(
            column
                .review
                .job_log
                .as_ref()
                .and_then(|log| log.as_ref().ok())
                .map(|log| log.content.as_str()),
            Some("boom")
        );
    });
    assert!(
        cx.debug_bounds("job-retry-1").is_none(),
        "lint can't run again"
    );
    let retry = cx.debug_bounds("job-retry-2").unwrap();
    cx.simulate_click(retry.center(), Modifiers::none());
    column.read_with(cx, |column, _| {
        assert_eq!(
            column.review.confirm_job_retry,
            Some((2, "unit".to_string()))
        )
    });
    column.update(cx, |column, cx| {
        let job = match &column.review.pipeline {
            Some(super::PipelineLoad::Ready(Some(pipeline))) => pipeline.jobs[1].clone(),
            other => panic!("pipeline: {other:?}"),
        };
        column.retry_job(&job, true, cx);
    });
    cx.run_until_parked();
    let toasts = cx.update(|_, cx| Toaster::messages(cx));
    assert!(
        toasts
            .iter()
            .any(|toast| toast == "Nova execução iniciada para unit"),
        "{toasts:?}"
    );
    let requests = requests(setup.server);
    let retried = requests
        .iter()
        .find(|(path, _)| path == "/api/v1/forge/pipeline-job-retry")
        .unwrap();
    assert_eq!(
        (retried.1["pipelineId"].clone(), retried.1["jobId"].clone()),
        (json!(77), json!(2))
    );
}

#[gpui::test]
fn the_badge_a_listed_conversation_and_the_tree_open_the_stage_on_it(cx: &mut TestAppContext) {
    use crate::screens::stage::DiffRequest;
    use dcc_core::domain::workspace_git::{WorkspaceGitBranchDiffOutput, WorkspaceGitPreviewScope};

    let comments = json!({
        "provider": "github", "host": "github.com", "number": 12,
        "comments": [
            thread_comment("t-open", "src/a.rs", Some(3), false, false),
            thread_comment("t-done", "src/a.rs", Some(9), true, false),
            thread_comment("t-old", "src/b.rs", Some(4), false, true),
        ],
    });
    let (column, cx, setup) = open_routed(
        cx,
        &[("src/a.rs", "um\n"), ("src/b.rs", "um\n")],
        vec![
            (
                "/api/v1/forge/context",
                200,
                connected("ana", "ana").to_string(),
            ),
            ("/api/v1/forge/review-comments", 200, comments.to_string()),
        ],
    );
    let opened = |setup: &Setup| -> Vec<DiffRequest> {
        setup
            .events
            .borrow()
            .iter()
            .filter_map(|event| match event {
                CodeColumnEvent::OpenDiff(request) => Some(request.clone()),
                _ => None,
            })
            .collect()
    };
    // The card's badge: its diff, landing on the first open conversation.
    let badge = cx.debug_bounds("review-badge-src/a.rs").unwrap();
    cx.simulate_click(badge.center(), Modifiers::none());
    let request = opened(&setup).pop().expect("the card's diff");
    let review = request.review.expect("with its conversations");
    assert_eq!(request.path, "src/a.rs");
    assert!(review.focus && review.thread_id.is_none());
    assert_eq!(review.comments.len(), 2, "only this file's");
    assert_eq!(review.forge_login.as_deref(), Some("ana"));
    // A listed conversation opens on itself: in the PR's diff when the
    // branch's base is known, else in the local one.
    let toggle = cx.debug_bounds("threads-toggle").unwrap();
    cx.simulate_click(toggle.center(), Modifiers::none());
    cx.run_until_parked();
    let row = cx.debug_bounds("thread-open-t-old").unwrap();
    cx.simulate_click(row.center(), Modifiers::none());
    let request = opened(&setup).pop().unwrap();
    let base = column.read_with(cx, |column, _| column.changes.base_branch());
    assert_eq!(request.path, "src/b.rs");
    assert_eq!(
        request.scope,
        if base.is_some() {
            WorkspaceGitPreviewScope::Committed
        } else {
            WorkspaceGitPreviewScope::Unstaged
        }
    );
    assert_eq!(request.base, base);
    assert_eq!(request.review.unwrap().thread_id.as_deref(), Some("t-old"));
    // With the base known, a tree file with conversations opens in the PR's
    // diff; one without opens as a file.
    column.update(cx, |column, cx| {
        column.changes.branch = super::super::Loaded::Ready(WorkspaceGitBranchDiffOutput {
            changes: Vec::new(),
            base_branch: Some("main".into()),
        });
        column.open_file("src/a.rs", cx);
        column.open_file("README.md", cx);
    });
    let request = opened(&setup).pop().unwrap();
    assert_eq!(
        (request.scope, request.base.as_deref()),
        (WorkspaceGitPreviewScope::Committed, Some("main"))
    );
    assert!(
        setup
            .events
            .borrow()
            .iter()
            .any(|event| *event == CodeColumnEvent::OpenFile("README.md".into()))
    );
    let _ = requests(setup.server);
}

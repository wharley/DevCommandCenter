//! The pull request's conversations on the stage's diff: landing on the
//! first open one, the marks, the drawer's reply and resolve, Esc, the list,
//! handing a thread to the agent, and the switch between the local diff and
//! the pull request's.

use dcc_core::domain::review_threads::{WorkspacePrReviewComment, WorkspacePrReviewCommentAuthor};
use dcc_core::domain::workspace_git::WorkspaceGitPreviewScope;
use gpui::{Modifiers, TestAppContext, WindowAppearance, px, size};

use super::{DiffLoad, DiffRequest, ReviewRequest, ShowChat, Stage, StageEvent};
use crate::data::desktop_api::DesktopApi;
use crate::test_support::{fake_desktop_routes, git, git_checkout};
use crate::theme::{Theme, ThemePreference};

fn comment(id: &str, thread: &str, line: i64, resolved: bool) -> WorkspacePrReviewComment {
    WorkspacePrReviewComment {
        id: id.into(),
        thread_id: thread.into(),
        path: "src/main.rs".into(),
        body: format!("Look at line {line}"),
        side: Some("RIGHT".into()),
        line: Some(line),
        author: Some(WorkspacePrReviewCommentAuthor {
            login: Some("ana".into()),
            ..Default::default()
        }),
        created_at: Some("2026-10-24T12:00:00Z".into()),
        resolvable: Some(true),
        resolved: Some(resolved),
        html_url: Some("https://github.com/o/r/pull/7#r11".into()),
        ..Default::default()
    }
}

fn comments() -> Vec<WorkspacePrReviewComment> {
    vec![comment("11", "t1", 2, false), comment("12", "t2", 1, true)]
}

type Events = std::rc::Rc<std::cell::RefCell<Vec<StageEvent>>>;

/// A branch off `main` that changed the file, its PR with two
/// conversations, and the desktop answering `routes`.
#[allow(clippy::type_complexity)]
fn open<'a>(
    cx: &'a mut TestAppContext,
    scope: WorkspaceGitPreviewScope,
    routes: Vec<(&'static str, u16, String)>,
) -> (
    gpui::Entity<Stage>,
    &'a mut gpui::VisualTestContext,
    tempfile::TempDir,
    Events,
    std::thread::JoinHandle<Vec<(String, String)>>,
) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::text_area::bind_keys(cx);
        super::bind_keys(cx);
    });
    let repo = git_checkout(&[("src/main.rs", "fn main() {}\n")]);
    git(repo.path(), &["checkout", "-q", "-b", "feature"]);
    std::fs::write(
        repo.path().join("src/main.rs"),
        "fn main() {\n    run();\n}\n",
    )
    .unwrap();
    git(repo.path(), &["commit", "-qam", "run"]);
    let (port, server) = fake_desktop_routes(routes);
    let (stage, cx) = cx.add_window_view(|_, cx| Stage::new(cx));
    let events: Events = Default::default();
    let seen = events.clone();
    let db = repo.path().join("dcc.db");
    stage.update(cx, |stage, cx| {
        stage.endpoint = Some(DesktopApi::new(port, "k"));
        stage.db_path = db;
        cx.subscribe_self(move |_, event: &StageEvent, _| seen.borrow_mut().push(event.clone()))
            .detach();
    });
    cx.simulate_resize(size(px(1100.0), px(800.0)));
    let request = DiffRequest {
        root: repo.path().to_string_lossy().into_owned(),
        path: "src/main.rs".into(),
        name: "main.rs".into(),
        status: "M".into(),
        base: (scope == WorkspaceGitPreviewScope::Committed).then(|| "main".to_string()),
        scope,
        from_changes: true,
        review: Some(ReviewRequest {
            root: repo.path().to_string_lossy().into_owned(),
            branch: Some("feature".into()),
            forge_login: None,
            comments: comments(),
            focus: true,
            thread_id: None,
        }),
    };
    stage.update_in(cx, |stage, window, cx| stage.open_diff(request, window, cx));
    cx.run_until_parked();
    (stage, cx, repo, events, server)
}

fn read_comments() -> (&'static str, u16, String) {
    let output = serde_json::json!({ "number": 7, "comments": comments() });
    ("/api/v1/forge/review-comments", 200, output.to_string())
}

fn active(stage: &gpui::Entity<Stage>, cx: &mut gpui::VisualTestContext) -> Option<String> {
    stage.read_with(cx, |stage, _| {
        stage.review().and_then(|review| review.active.clone())
    })
}

#[gpui::test]
fn a_badge_lands_on_the_open_conversation_and_the_drawer_answers(cx: &mut TestAppContext) {
    let (stage, cx, _repo, events, server) = open(
        cx,
        WorkspaceGitPreviewScope::Committed,
        vec![
            read_comments(),
            (
                "/api/v1/forge/reply-thread",
                200,
                r#"{"submitted":true}"#.into(),
            ),
            read_comments(),
            (
                "/api/v1/forge/resolve-thread",
                200,
                r#"{"resolved":true}"#.into(),
            ),
            read_comments(),
        ],
    );
    // The open conversation, not the resolved one, and only it is marked.
    assert_eq!(active(&stage, cx).as_deref(), Some("t1"));
    stage.read_with(cx, |stage, cx| {
        let Some(DiffLoad::Ready(view)) = stage.diff.as_ref().map(|diff| &diff.load) else {
            panic!("the diff");
        };
        let marks = &view.read(cx).marks;
        assert_eq!(marks.len(), 1);
        assert_eq!(marks[0].title, "@ana: Look at line 2");
        assert_eq!(stage.review().unwrap().number, Some(7));
    });
    assert!(cx.debug_bounds("review-bar").is_some());
    assert!(cx.debug_bounds("thread-drawer").is_some());
    assert!(cx.debug_bounds("review-mark-t1").is_some(), "the callout");
    // A reply goes to the forge, then the comments are read again.
    stage.update_in(cx, |stage, window, cx| {
        let reply = stage.review().unwrap().reply.clone();
        reply.update(cx, |area, cx| {
            area.set_text("Feito", cx);
            gpui::Focusable::focus_handle(area, cx).focus(window, cx);
        });
    });
    cx.run_until_parked();
    let send = cx.debug_bounds("thread-reply-send").unwrap();
    cx.simulate_click(send.center(), Modifiers::none());
    cx.run_until_parked();
    stage.read_with(cx, |stage, cx| {
        assert_eq!(stage.review().unwrap().reply.read(cx).text(), "", "cleared");
    });
    let resolve = cx.debug_bounds("thread-resolve").unwrap();
    cx.simulate_click(resolve.center(), Modifiers::none());
    cx.run_until_parked();
    let calls = server.join().unwrap();
    let reply = calls
        .iter()
        .find(|(line, _)| line.contains("reply-thread"))
        .expect("the reply");
    let body: serde_json::Value = serde_json::from_str(&reply.1).unwrap();
    assert_eq!(body["number"], 7);
    assert_eq!(body["commentId"], "11");
    assert_eq!(body["threadId"], "t1");
    assert_eq!(body["body"], "Feito");
    let resolve = calls
        .iter()
        .find(|(line, _)| line.contains("resolve-thread"))
        .expect("the resolve");
    let body: serde_json::Value = serde_json::from_str(&resolve.1).unwrap();
    assert_eq!(body["resolved"], true);
    // Esc closes the conversation before anything else.
    stage.update_in(cx, |stage, window, cx| window.focus(&stage.focus, cx));
    cx.dispatch_action(ShowChat);
    assert_eq!(active(&stage, cx), None);
    stage.read_with(cx, |stage, _| assert!(!stage.chat_active()));
    // The list opens the other one; "Enviar ao agente" hands it over.
    cx.run_until_parked();
    let row = cx.debug_bounds("review-row-t2").expect("listed");
    cx.simulate_click(row.center(), Modifiers::none());
    assert_eq!(active(&stage, cx).as_deref(), Some("t2"));
    cx.run_until_parked();
    let send = cx.debug_bounds("thread-send-to-agent").unwrap();
    cx.simulate_click(send.center(), Modifiers::none());
    let events = events.borrow();
    let Some(StageEvent::ThreadToAgent {
        annotation,
        instruction,
        send,
    }) = events.last()
    else {
        panic!("the thread for the agent");
    };
    assert!(*send);
    assert!(instruction.starts_with("Analyze this code review thread"));
    assert!(instruction.contains("Base branch: main"));
    assert_eq!((annotation.start_line, annotation.end_line), (1, 1));
    stage.read_with(cx, |stage, _| {
        assert!(stage.chat_active(), "the Chat comes forward")
    });
}

#[gpui::test]
fn a_local_diff_switches_to_the_pull_requests(cx: &mut TestAppContext) {
    let (stage, cx, _repo, _, _server) = open(
        cx,
        WorkspaceGitPreviewScope::Unstaged,
        vec![read_comments()],
    );
    // The branch's base came from the checkout (or not, in a bare test
    // repository): either way the badge's landing waited for it.
    stage.update(cx, |stage, cx| {
        let review = stage.diff.as_mut().unwrap().review.as_mut().unwrap();
        review.pr_base = Some("main".into());
        review.prefer_pr = false;
        stage.reload_diff(cx);
    });
    cx.run_until_parked();
    stage.read_with(cx, |stage, _| assert!(!stage.showing_pr_diff()));
    let show = cx
        .debug_bounds("review-show-pr-diff")
        .expect("Ver no diff do PR");
    cx.simulate_click(show.center(), Modifiers::none());
    cx.run_until_parked();
    stage.read_with(cx, |stage, cx| {
        assert!(stage.showing_pr_diff());
        assert_eq!(
            stage.content_params().map(|(scope, _, base)| (scope, base)),
            Some((
                WorkspaceGitPreviewScope::Committed,
                Some("main".to_string())
            ))
        );
        let Some(DiffLoad::Ready(view)) = stage.diff.as_ref().map(|diff| &diff.load) else {
            panic!("the PR's diff");
        };
        assert_eq!(view.read(cx).marks.len(), 1, "marked once it shows");
    });
    let back = cx
        .debug_bounds("review-show-local-diff")
        .expect("Mudanças locais");
    cx.simulate_click(back.center(), Modifiers::none());
    cx.run_until_parked();
    stage.read_with(cx, |stage, cx| {
        assert!(!stage.showing_pr_diff());
        let Some(DiffLoad::Ready(view)) = stage.diff.as_ref().map(|diff| &diff.load) else {
            panic!("the local diff");
        };
        assert!(
            view.read(cx).marks.is_empty(),
            "the local diff can't place them"
        );
    });
}

#[gpui::test]
fn a_comments_markdown_shows_clean_with_its_marker_as_a_chip(cx: &mut TestAppContext) {
    let (stage, cx, _repo, _, _server) = open(
        cx,
        WorkspaceGitPreviewScope::Committed,
        vec![read_comments()],
    );
    let marked = "♻️ **Reuso.** Re-implementa `parse_ms`, que já existe.\n\n- um\n- dois";
    let mut reply = comment("13", "t1", 2, false);
    reply.body = "🔴 **Bloqueante** — e o `id` vazio?".into();
    stage.update(cx, |stage, cx| {
        let review = stage.diff.as_mut().unwrap().review.as_mut().unwrap();
        for comment in review.comments.iter_mut().filter(|c| c.thread_id == "t1") {
            comment.body = marked.into();
        }
        review.comments.push(reply);
        stage.review_changed(cx);
    });
    cx.run_until_parked();
    assert_eq!(active(&stage, cx).as_deref(), Some("t1"));
    // The drawer: the comment and its reply as markdown, each marker a chip.
    let drawer = cx.debug_bounds("thread-drawer").expect("the drawer");
    let body = cx.debug_bounds("review-comment-body").expect("markdown");
    assert!(drawer.contains(&body.origin));
    let chip = cx.debug_bounds("review-severity-chip").expect("the chip");
    assert!(drawer.contains(&chip.center()), "the drawer's chip");
    // The bar's row and the diff's callout show one plain line, the marker
    // a chip before it.
    let row = cx.debug_bounds("review-row-t1").expect("listed");
    let callout = cx.debug_bounds("review-mark-t1").expect("the callout");
    let chip = cx
        .debug_bounds("review-preview-chip")
        .expect("the row's chip");
    assert!(
        [row, callout]
            .iter()
            .any(|area| area.contains(&chip.center())),
        "the chip sits in the row or the callout"
    );
    let preview = crate::data::review_comment_text::comment_preview(marked);
    assert_eq!(preview.label.as_deref(), Some("Reuso"));
    assert_eq!(preview.text, "Re-implementa parse_ms, que já existe.");
    assert!(!preview.text.contains("**") && !preview.text.contains('`'));
}

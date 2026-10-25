//! Reviewing a delegation's worktree from "Mudanças": the session's notice,
//! the worktree's changes under the bar, apply / discard through a fake
//! desktop, and the child's own conversation opening (and leaving) it.

use gpui::{Modifiers, TestAppContext, WindowAppearance, px, size};

use super::delegation::Reviewable;
use super::{CodeColumn, CodeColumnEvent, CodeColumnTab};
use crate::data::desktop_api::DesktopApi;
use crate::test_support::{fake_desktop_routes, git_checkout, task_database};
use crate::theme::{Theme, ThemePreference};
use crate::ui::Toaster;

struct Setup {
    _db: tempfile::TempDir,
    parent: tempfile::TempDir,
    worktree: tempfile::TempDir,
    server: std::thread::JoinHandle<Vec<(String, String)>>,
    events: std::rc::Rc<std::cell::RefCell<Vec<CodeColumnEvent>>>,
}

impl Setup {
    fn parent_root(&self) -> String {
        self.parent.path().to_string_lossy().into_owned()
    }

    fn reviewable(&self) -> Reviewable {
        Reviewable {
            id: "d-1".into(),
            provider_id: "codex".into(),
            parent_session_id: "s-parent".into(),
            child_session_id: Some("s-child".into()),
            touched_files: 1,
            worktree: self.worktree.path().to_string_lossy().into_owned(),
        }
    }
}

fn open<'a>(
    cx: &'a mut TestAppContext,
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
    let parent = git_checkout(&[("main.rs", "fn main() {}\n")]);
    // The child's worktree, with an edit waiting.
    let worktree = git_checkout(&[("main.rs", "fn main() {}\n")]);
    std::fs::write(worktree.path().join("main.rs"), "fn main() { run(); }\n").unwrap();
    let db = tempfile::tempdir().unwrap();
    let db_path = task_database(db.path(), "w-1", parent.path(), None);
    let (port, server) = fake_desktop_routes(routes);
    let (column, cx) = cx.add_window_view(|_, cx| {
        let mut column = CodeColumn::new(CodeColumnTab::Changes, cx);
        column.db_path = db_path;
        column.endpoint = Some(DesktopApi::new(port, "k"));
        column
    });
    let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let seen = events.clone();
    let root = parent.path().to_string_lossy().into_owned();
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
            parent,
            worktree,
            server,
            events,
        },
    )
}

#[gpui::test]
fn a_waiting_delegation_is_reviewed_and_applied(cx: &mut TestAppContext) {
    let (column, cx, setup) = open(
        cx,
        vec![(
            "/api/v1/delegations/d-1/apply",
            200,
            r#"{"changedFiles":["main.rs"]}"#.to_string(),
        )],
    );
    let reviewable = setup.reviewable();
    column.update(cx, |column, cx| {
        column.set_reviews(Some("s-parent".into()), vec![reviewable.clone()], cx)
    });
    cx.run_until_parked();
    let show = cx
        .debug_bounds("delegation-show-d-1")
        .expect("the session's notice");
    cx.simulate_click(show.center(), Modifiers::none());
    cx.run_until_parked();
    column.read_with(cx, |column, _| {
        assert_eq!(column.root.as_deref(), Some(reviewable.worktree.as_str()));
        assert_eq!(column.tab, CodeColumnTab::Changes);
        // The worktree's edit is what the list shows.
        let paths: Vec<&str> = column
            .changes
            .cards
            .iter()
            .map(|card| card.entry.path.as_str())
            .collect();
        assert_eq!(paths, vec!["main.rs"]);
    });
    assert!(cx.debug_bounds("delegation-review").is_some());
    assert!(
        cx.debug_bounds("delivery-commit").is_none(),
        "no delivery from the worktree"
    );
    // The conversation keeps telling the task's checkout: the review stays.
    let parent = setup.parent_root();
    column.update(cx, |column, cx| {
        column.set_root(Some(parent.clone()), cx);
        column.set_reviews(Some("s-parent".into()), vec![reviewable.clone()], cx);
    });
    column.read_with(cx, |column, _| assert!(column.delegation_review.is_some()));
    let apply = cx.debug_bounds("delegation-apply").unwrap();
    cx.simulate_click(apply.center(), Modifiers::none());
    cx.run_until_parked();
    column.read_with(cx, |column, _| {
        assert!(column.delegation_review.is_none());
        assert_eq!(
            column.root.as_deref(),
            Some(parent.as_str()),
            "back on the task's checkout"
        );
    });
    let toasts = cx.update(|_, cx| Toaster::messages(cx));
    assert!(
        toasts
            .iter()
            .any(|toast| toast == "1 arquivo(s) aplicado(s)"),
        "{toasts:?}"
    );
    let requests = setup.server.join().unwrap();
    let body: serde_json::Value = serde_json::from_str(&requests[0].1).unwrap();
    assert_eq!(body["workspaceRoot"], parent.as_str());
}

#[gpui::test]
fn the_childs_conversation_reviews_it_and_discarding_goes_back(cx: &mut TestAppContext) {
    let (column, cx, setup) = open(
        cx,
        vec![(
            "/api/v1/delegations/d-1/discard",
            200,
            r#"{"ok":true}"#.to_string(),
        )],
    );
    let reviewable = setup.reviewable();
    column.update(cx, |column, cx| {
        column.set_reviews(Some("s-child".into()), vec![reviewable.clone()], cx)
    });
    cx.run_until_parked();
    column.read_with(cx, |column, _| {
        let review = column
            .delegation_review
            .as_ref()
            .expect("the child's review");
        assert!(review.from_child);
    });
    let discard = cx.debug_bounds("delegation-discard").unwrap();
    cx.simulate_click(discard.center(), Modifiers::none());
    cx.run_until_parked();
    let toasts = cx.update(|_, cx| Toaster::messages(cx));
    assert!(
        toasts
            .iter()
            .any(|toast| toast == "Mudanças da delegação descartadas"),
        "{toasts:?}"
    );
    assert!(
        setup
            .events
            .borrow()
            .contains(&CodeColumnEvent::OpenSession("s-parent".into())),
        "the parent's conversation opens again"
    );
    let _ = setup.server.join();
}

#[gpui::test]
fn a_file_chip_opens_its_diff_in_the_delegations_worktree(cx: &mut TestAppContext) {
    use dcc_core::domain::workspace_git::WorkspaceGitPreviewScope;

    let (column, cx, setup) = open(cx, Vec::new());
    let reviewable = setup.reviewable();
    column.update(cx, |column, cx| {
        column.set_reviews(Some("s-parent".into()), vec![reviewable.clone()], cx);
        // Not under review yet: no diff to open.
        assert!(column.delegation_file_request("d-1", "main.rs").is_none());
        column.review_delegation("d-1", cx);
        let request = column
            .delegation_file_request("d-1", " main.rs ")
            .expect("the file's diff");
        assert_eq!(request.root, reviewable.worktree);
        assert_eq!(request.path, "main.rs");
        assert_eq!(request.scope, WorkspaceGitPreviewScope::Unstaged);
        assert_eq!(request.status, "M");
        // Another delegation's chip, or an empty path, opens nothing.
        assert!(column.delegation_file_request("d-2", "main.rs").is_none());
        assert!(column.delegation_file_request("d-1", " ").is_none());
    });
    drop(setup);
}

//! The conversation header's delivery over a real checkout and a fake
//! desktop, with the column on "Arquivos" (its dialogs come from the
//! overlay): the counts and the next step, a commit through its preview,
//! a push the remote is ahead of retried from its toast, "Criar PR" with
//! the local changes and as a draft.

use dcc_core::domain::commit_mode::CommitMode;
use dcc_core::domain::execution_dock::{
    DockAction, DockActionId, DockActionKind, DockRunMode, DockStatus,
};
use gpui::{
    AppContext, Context, Entity, Modifiers, Render, TestAppContext, VisualTestContext, Window,
    WindowAppearance, div, prelude::*, px, size,
};
use serde_json::Value;

use super::super::{CodeColumn, CodeColumnTab};
use super::{DeliveryOverlay, PreviewOrigin};
use crate::data::desktop_api::DesktopApi;
use crate::test_support::{fake_desktop_routes, git_checkout, task_database};
use crate::theme::{Theme, ThemePreference};
use crate::ui::Toaster;

/// The column, its overlay and the toasts, as the shell draws them.
struct Root {
    column: Entity<CodeColumn>,
    overlay: Entity<DeliveryOverlay>,
}

impl Render for Root {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .child(self.column.clone())
            .child(self.overlay.clone())
            .children(Toaster::handle(cx))
    }
}

type Server = std::thread::JoinHandle<Vec<(String, String)>>;

fn open<'a>(
    cx: &'a mut TestAppContext,
    repo: &std::path::Path,
    db: &std::path::Path,
    answers: Vec<(&'static str, u16, String)>,
) -> (Entity<CodeColumn>, &'a mut VisualTestContext, Server) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::text_input::bind_keys(cx);
        crate::ui::text_area::bind_keys(cx);
        Toaster::init(cx);
    });
    let db_path = task_database(db, "w-1", repo, None);
    let (port, server) = fake_desktop_routes(answers);
    let (root, cx) = cx.add_window_view(|_, cx| {
        let column = cx.new(|cx| {
            let mut column = CodeColumn::new(CodeColumnTab::Files, cx);
            column.db_path = db_path;
            column.endpoint = Some(DesktopApi::new(port, "k"));
            column
        });
        let overlay = cx.new(|cx| DeliveryOverlay::new(column.clone(), cx));
        Root { column, overlay }
    });
    let column = cx.update(|_, cx| root.read(cx).column.clone());
    let checkout = repo.to_string_lossy().into_owned();
    column.update(cx, |column, cx| column.set_root(Some(checkout), cx));
    cx.simulate_resize(size(px(900.0), px(800.0)));
    cx.run_until_parked();
    (column, cx, server)
}

fn requests(server: Server) -> Vec<(String, Value)> {
    server
        .join()
        .unwrap()
        .into_iter()
        .map(|(line, body)| (line, serde_json::from_str(&body).unwrap_or(Value::Null)))
        .collect()
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    cx.run_until_parked();
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} on screen"));
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
}

fn step(id: DockActionId, mode: Option<DockRunMode>, kind: DockActionKind) -> DockAction {
    DockAction {
        id,
        mode,
        kind,
        disabled: false,
        primary: false,
    }
}

fn toasts(cx: &mut VisualTestContext) -> Vec<String> {
    cx.update(|_, cx| Toaster::messages(cx))
}

#[gpui::test]
fn a_commit_without_a_remote_goes_through_its_preview(cx: &mut TestAppContext) {
    let repo = git_checkout(&[("a.txt", "um\n")]);
    std::fs::write(repo.path().join("a.txt"), "dois\ntres\n").unwrap();
    let db = tempfile::tempdir().unwrap();
    let suggestion = r#"{"subject":"chore: update a","body":null,"stagedFileCount":1,"stagedFingerprint":"fp-1","source":"heuristic-git-staged"}"#;
    let (column, cx, server) = open(
        cx,
        repo.path(),
        db.path(),
        vec![
            (
                "/api/v1/workspaces/w-1/delivery/prepare-commit",
                200,
                suggestion.into(),
            ),
            (
                "/api/v1/workspaces/w-1/delivery/commit-reviewed",
                200,
                r#"{"ok":true}"#.into(),
            ),
        ],
    );
    let dock = column.read_with(cx, |column, _| column.dock().expect("a checkout"));
    assert_eq!(
        dock.status,
        DockStatus::Local {
            files: 1,
            additions: 2,
            deletions: 1
        }
    );
    // No remote: delivery stops at the commit, the push is off.
    let first = dock.actions[0];
    assert_eq!(first.mode, Some(DockRunMode::Commit));
    assert!(first.primary && !first.disabled);
    let commit_and_push = dock
        .actions
        .iter()
        .find(|action| action.id == DockActionId::CommitAndPush)
        .unwrap();
    assert!(commit_and_push.disabled);
    cx.update(|window, cx| {
        column.update(cx, |column, cx| column.run_dock_action(first, window, cx))
    });
    assert_eq!(
        column.read_with(cx, |column, _| column.delivery.preparing),
        Some(false),
        "the preview opens while it is prepared"
    );
    cx.run_until_parked();
    column.read_with(cx, |column, cx| {
        let preview = column.delivery.preview.as_ref().expect("the preview");
        assert_eq!(preview.origin, PreviewOrigin::Dock { push: false });
        assert_eq!(preview.subject.read(cx).text(), "chore: update a");
    });
    for sparkles in ["draft-commit-subject", "draft-commit-body"] {
        assert!(cx.debug_bounds(sparkles).is_some(), "{sparkles} drawn");
    }
    click(cx, "commit-preview-confirm");
    let sent = requests(server);
    assert_eq!(
        sent[1].0,
        "POST /api/v1/workspaces/w-1/delivery/commit-reviewed"
    );
    assert_eq!(sent[1].1["push"], false);
    assert_eq!(sent[1].1["message"], "chore: update a");
    assert!(toasts(cx).contains(&"Commit criado".to_string()));
}

#[gpui::test]
fn a_push_behind_the_remote_is_retried_from_its_toast(cx: &mut TestAppContext) {
    let repo = git_checkout(&[("a.txt", "um\n")]);
    let db = tempfile::tempdir().unwrap();
    let behind = r#"{"ok":false,"error":{"code":"behind_remote","message":"recusado","outcome":{"target":"origin/feat"}}}"#;
    let (column, cx, server) = open(
        cx,
        repo.path(),
        db.path(),
        vec![
            ("/api/v1/workspaces/w-1/delivery/push", 409, behind.into()),
            (
                "/api/v1/workspaces/w-1/delivery/sync-and-push",
                200,
                r#"{"ok":true}"#.into(),
            ),
        ],
    );
    let push = step(
        DockActionId::Push,
        Some(DockRunMode::Mode(CommitMode::Push)),
        DockActionKind::Execute,
    );
    cx.update(|window, cx| {
        column.update(cx, |column, cx| column.run_dock_action(push, window, cx))
    });
    cx.run_until_parked();
    assert!(
        toasts(cx).contains(&"origin/feat tem commits que você não tem localmente".to_string())
    );
    click(cx, "toast-action-0");
    let sent = requests(server);
    assert_eq!(
        sent[1].0,
        "POST /api/v1/workspaces/w-1/delivery/sync-and-push"
    );
    assert!(toasts(cx).contains(&"Atualizado de origin/feat e enviado".to_string()));
}

#[gpui::test]
fn create_request_takes_the_local_changes_as_a_draft(cx: &mut TestAppContext) {
    let repo = git_checkout(&[("a.txt", "um\n")]);
    std::fs::write(repo.path().join("a.txt"), "dois\n").unwrap();
    let db = tempfile::tempdir().unwrap();
    let forge = r#"{"provider":"github","host":"github.com","namespace":"o","repo":"r","status":"ready","remoteState":"ok","effectiveLogin":"wharley"}"#;
    let context = r#"{"headBranch":"feat/guia","baseBranch":"main","title":"Guia do checkout","requestLabel":"PR"}"#;
    let (column, cx, server) = open(
        cx,
        repo.path(),
        db.path(),
        vec![
            ("/api/v1/forge/context", 200, forge.into()),
            ("/api/v1/forge/change-request-context", 200, context.into()),
            (
                "/api/v1/workspaces/w-1/delivery",
                200,
                r#"{"changesToken":"tok-1"}"#.into(),
            ),
            (
                "/api/v1/workspaces/w-1/delivery/request",
                200,
                r#"{"ok":true}"#.into(),
            ),
        ],
    );
    let draft = step(
        DockActionId::CreateDraftPr,
        Some(DockRunMode::Mode(CommitMode::CreatePr)),
        DockActionKind::CreateRequest,
    );
    cx.update(|window, cx| {
        column.update(cx, |column, cx| column.run_dock_action(draft, window, cx))
    });
    cx.run_until_parked();
    column.read_with(cx, |column, cx| {
        let request = column.delivery.request.as_ref().expect("the dialog");
        assert!(request.draft && request.include_local);
        assert_eq!(request.title.read(cx).text(), "Guia do checkout");
    });
    assert!(
        cx.debug_bounds("request-submit").is_some(),
        "the overlay draws it"
    );
    for sparkles in ["draft-request-title", "draft-request-body"] {
        assert!(cx.debug_bounds(sparkles).is_some(), "{sparkles} drawn");
    }
    click(cx, "request-submit");
    let sent = requests(server);
    let lines: Vec<&str> = sent.iter().map(|(line, _)| line.as_str()).collect();
    assert_eq!(
        lines,
        [
            "POST /api/v1/forge/context",
            "POST /api/v1/forge/change-request-context",
            "GET /api/v1/workspaces/w-1/delivery",
            "POST /api/v1/workspaces/w-1/delivery/request",
        ]
    );
    let body = &sent[3].1;
    assert_eq!(body["title"], "Guia do checkout");
    assert_eq!(body["draft"], true);
    assert_eq!(body["includeLocalChanges"], true);
    assert_eq!(body["changesToken"], "tok-1");
    assert!(column.read_with(cx, |column, _| column.delivery.request.is_none()));
    assert!(toasts(cx).contains(&"PR criado".to_string()));
}

/// A long ✨ description scrolls inside its field: the dialog keeps its size.
#[gpui::test]
fn a_long_description_scrolls_inside_its_field(cx: &mut TestAppContext) {
    let repo = git_checkout(&[("a.txt", "um\n")]);
    std::fs::write(repo.path().join("a.txt"), "dois\n").unwrap();
    let db = tempfile::tempdir().unwrap();
    let long = (1..=80)
        .map(|line| format!("- linha {line}"))
        .collect::<Vec<_>>()
        .join("\\n");
    let answer = format!(r#"{{"text":"{long}"}}"#);
    let draft = "/api/v1/workspaces/w-1/delivery/draft";
    let (column, cx, _server) = open(cx, repo.path(), db.path(), vec![(draft, 200, answer)]);
    column.update_in(cx, |column, window, cx| {
        column.open_request_dialog(false, window, cx)
    });
    cx.run_until_parked();
    column.update(cx, |column, cx| {
        column.toggle_draft(super::draft::DraftField::RequestBody, cx)
    });
    cx.run_until_parked();
    column.read_with(cx, |column, cx| {
        let request = column.delivery.request.as_ref().unwrap();
        assert!(request.body.read(cx).text().contains("linha 80"));
    });
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    let field = cx.debug_bounds("request-body").expect("the description");
    assert!(
        field.size.height <= px(super::BODY_MAX_HEIGHT + 1.0),
        "{:?}",
        field.size
    );
}

//! The delivery header over real checkouts and a fake desktop: the
//! button's mode and label, the commit preview, the merge and the sync
//! asked first, a push stopped by the checks, a merged task completed.

use dcc_core::domain::commit_mode::CommitMode;
use dcc_core::domain::workspace_git::WorkspacePrStatusOutput;
use gpui::{Modifiers, TestAppContext, WindowAppearance, px, size};
use serde_json::Value;

use super::super::{CodeColumn, CodeColumnTab};
use super::Confirm;
use crate::data::desktop_api::DesktopApi;
use crate::test_support::{fake_desktop_answers, git, git_checkout, task_database};
use crate::theme::{Theme, ThemePreference};
use crate::ui::Toaster;

struct Setup {
    _db: tempfile::TempDir,
    server: std::thread::JoinHandle<Vec<(String, String)>>,
}

/// The column on "Mudanças" over `root`, its task in a database of its
/// own, the desktop answering `answers` in turn.
fn open<'a>(
    cx: &'a mut TestAppContext,
    root: &std::path::Path,
    worktree: Option<&std::path::Path>,
    answers: Vec<(u16, String)>,
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
    let db = tempfile::tempdir().unwrap();
    let db_path = task_database(db.path(), "w-1", root, worktree);
    let (port, server) = fake_desktop_answers(answers);
    let checkout = worktree.unwrap_or(root).to_string_lossy().into_owned();
    let (column, cx) = cx.add_window_view(|_, cx| {
        let mut column = CodeColumn::new(CodeColumnTab::Changes, cx);
        column.db_path = db_path;
        column.endpoint = Some(DesktopApi::new(port, "k"));
        column
    });
    column.update(cx, |column, cx| column.set_root(Some(checkout), cx));
    cx.run_until_parked();
    cx.simulate_resize(size(px(340.0), px(800.0)));
    cx.run_until_parked();
    (column, cx, Setup { _db: db, server })
}

fn ok() -> (u16, String) {
    (200, r#"{"ok":true}"#.to_string())
}

fn requests(setup: Setup) -> Vec<(String, Value)> {
    setup
        .server
        .join()
        .unwrap()
        .into_iter()
        .map(|(line, body)| {
            let path = line.split_whitespace().nth(1).unwrap().to_string();
            (path, serde_json::from_str(&body).unwrap_or(Value::Null))
        })
        .collect()
}

fn toasts(cx: &mut gpui::VisualTestContext) -> Vec<String> {
    cx.update(|_, cx| Toaster::messages(cx))
}

fn open_pr(state: &str) -> WorkspacePrStatusOutput {
    WorkspacePrStatusOutput {
        provider: Some("github".into()),
        host: Some("github.com".into()),
        number: Some(12),
        title: Some("Guia".into()),
        url: Some("https://github.com/o/r/pull/12".into()),
        head_branch: None,
        base_branch: Some("main".into()),
        state: Some(state.into()),
        is_draft: false,
        mergeable: Some("MERGEABLE".into()),
        merge_state_status: Some("CLEAN".into()),
    }
}

#[gpui::test]
fn a_local_commit_goes_through_its_preview(cx: &mut TestAppContext) {
    let repo = git_checkout(&[("a.txt", "um\n")]);
    std::fs::write(repo.path().join("a.txt"), "dois\n").unwrap();
    let suggestion = r#"{"subject":"chore: update a","body":null,"stagedFileCount":1,"stagedFingerprint":"fp-1","source":"heuristic-git-staged"}"#;
    let (column, cx, setup) = open(cx, repo.path(), None, vec![(200, suggestion.into()), ok()]);
    column.read_with(cx, |column, _| {
        assert_eq!(column.commit_mode(), CommitMode::CommitAndPush);
        assert_eq!(column.delivery.task.as_ref().unwrap().id.0, "w-1");
    });
    // No remote: the button says "Commitar".
    assert!(cx.debug_bounds("delivery-header").is_some());
    let button = cx
        .debug_bounds("delivery-commit")
        .expect("the delivery button");
    cx.simulate_click(button.center(), Modifiers::none());
    cx.run_until_parked();
    column.update(cx, |column, cx| {
        let preview = column
            .delivery
            .preview
            .as_ref()
            .expect("the preview opened");
        assert_eq!(preview.subject.read(cx).text(), "chore: update a");
        assert_eq!(preview.staged_files, 1);
        preview
            .subject
            .update(cx, |area, cx| area.set_text("chore: tweak a", cx));
        column.confirm_preview(cx);
    });
    cx.run_until_parked();
    column.read_with(cx, |column, _| {
        assert!(column.delivery.preview.is_none());
        assert!(!column.delivery.busy);
    });
    assert!(
        toasts(cx).iter().any(|toast| toast == "Commit criado"),
        "{:?}",
        toasts(cx)
    );
    let requests = requests(setup);
    assert_eq!(
        requests[0].0,
        "/api/v1/workspaces/w-1/delivery/prepare-commit"
    );
    assert_eq!(
        requests[1].0,
        "/api/v1/workspaces/w-1/delivery/commit-reviewed"
    );
    assert_eq!(requests[1].1["message"], "chore: tweak a");
    assert_eq!(requests[1].1["stagedFingerprint"], "fp-1");
    assert_eq!(requests[1].1["push"], false, "nowhere to push");
    assert_eq!(requests[1].1["body"], Value::Null);
}

#[gpui::test]
fn a_merge_asks_first_then_completes_the_task(cx: &mut TestAppContext) {
    let repo = git_checkout(&[("a.txt", "um\n")]);
    let (column, cx, setup) = open(cx, repo.path(), None, vec![ok(), ok()]);
    column.update(cx, |column, cx| {
        column.delivery.pr = Some(open_pr("open"));
        cx.notify();
    });
    cx.run_until_parked();
    column.read_with(cx, |column, _| {
        assert_eq!(column.commit_mode(), CommitMode::Merge)
    });
    assert!(cx.debug_bounds("delivery-request-link").is_some(), "PR #12");
    let button = cx.debug_bounds("delivery-commit").unwrap();
    cx.simulate_click(button.center(), Modifiers::none());
    column.read_with(cx, |column, _| {
        assert_eq!(column.delivery.confirm, Some(Confirm::Merge))
    });
    column.update(cx, |column, cx| column.run_confirmed(cx));
    cx.run_until_parked();
    assert!(
        toasts(cx).iter().any(|toast| toast == "PR mesclado"),
        "{:?}",
        toasts(cx)
    );
    let paths: Vec<String> = requests(setup).into_iter().map(|(path, _)| path).collect();
    assert_eq!(
        paths,
        vec![
            "/api/v1/workspaces/w-1/delivery/merge",
            "/api/v1/workspaces/w-1/delivery/complete",
        ]
    );
}

/// A clone on `feature` with one commit not pushed (`ahead`), or behind
/// its upstream by one (`behind`).
fn tracked_checkout(ahead: bool) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let remote = dir.path().join("remote.git");
    let repo = dir.path().join("repo");
    git(
        dir.path(),
        &[
            "init",
            "-q",
            "--bare",
            "-b",
            "main",
            remote.to_str().unwrap(),
        ],
    );
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "feature"]);
    std::fs::write(repo.join("a.txt"), "um\n").unwrap();
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "init"]);
    git(
        &repo,
        &["remote", "add", "origin", remote.to_str().unwrap()],
    );
    git(&repo, &["push", "-q", "-u", "origin", "feature"]);
    std::fs::write(repo.join("b.txt"), "dois\n").unwrap();
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "b"]);
    if !ahead {
        git(&repo, &["push", "-q"]);
        git(&repo, &["reset", "-q", "--hard", "HEAD~1"]);
    }
    dir
}

#[gpui::test]
fn a_push_the_checks_stop_says_so(cx: &mut TestAppContext) {
    let dir = tracked_checkout(true);
    let refused = (
        409,
        r#"{"error":{"code":"checks_failed","message":"Os checks configurados para antes do push falharam."}}"#
            .to_string(),
    );
    let (column, cx, setup) = open(cx, &dir.path().join("repo"), None, vec![refused]);
    column.read_with(cx, |column, _| {
        assert_eq!(column.commit_mode(), CommitMode::Push)
    });
    let button = cx.debug_bounds("delivery-commit").unwrap();
    cx.simulate_click(button.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(
        toasts(cx)
            .iter()
            .any(|toast| toast == "Push bloqueado: revise os checks do projeto."),
        "{:?}",
        toasts(cx)
    );
    assert_eq!(requests(setup)[0].0, "/api/v1/workspaces/w-1/delivery/push");
}

#[gpui::test]
fn a_branch_behind_offers_to_sync_with_the_base(cx: &mut TestAppContext) {
    let dir = tracked_checkout(false);
    let synced = r#"{"branch":"feature","baseBranch":"main","remote":"origin","updated":true,"conflictCount":0}"#;
    let (column, cx, setup) = open(
        cx,
        &dir.path().join("repo"),
        None,
        vec![(200, synced.into())],
    );
    column.read_with(cx, |column, _| {
        assert_eq!(column.status.as_ref().unwrap().behind_of_remote_count, 1)
    });
    let sync = cx
        .debug_bounds("branch-toolbar-sync")
        .expect("the sync bar");
    cx.simulate_click(sync.center(), Modifiers::none());
    column.read_with(cx, |column, _| {
        assert_eq!(column.delivery.confirm, Some(Confirm::SyncBase))
    });
    column.update(cx, |column, cx| column.run_confirmed(cx));
    cx.run_until_parked();
    assert!(
        toasts(cx)
            .iter()
            .any(|toast| toast == "Worktree atualizado com origin/main"),
        "{:?}",
        toasts(cx)
    );
    let requests = requests(setup);
    assert_eq!(requests[0].0, "/api/v1/workspaces/w-1/delivery/sync-base");
}

#[gpui::test]
fn a_merged_task_with_its_own_worktree_completes_once(cx: &mut TestAppContext) {
    let repo = git_checkout(&[("a.txt", "um\n")]);
    let parent = tempfile::tempdir().unwrap();
    let (column, cx, setup) = open(cx, parent.path(), Some(repo.path()), vec![ok()]);
    column.update(cx, |column, cx| {
        column.delivery.pr = Some(open_pr("merged"));
        assert_eq!(column.commit_mode(), CommitMode::Merged);
        column.complete_if_merged(cx);
        // A second read does not complete it again.
        column.complete_if_merged(cx);
    });
    cx.run_until_parked();
    // Merged: no button.
    assert!(cx.debug_bounds("delivery-commit").is_none());
    let paths: Vec<String> = requests(setup).into_iter().map(|(path, _)| path).collect();
    assert_eq!(paths, vec!["/api/v1/workspaces/w-1/delivery/complete"]);
}

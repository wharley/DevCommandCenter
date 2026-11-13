//! The delivery header over real checkouts and a fake desktop: the
//! button's mode and label, the commit preview, the merge and the sync
//! asked first, a push stopped by the checks, a merged task completed.

use dcc_core::domain::commit_mode::CommitMode;
use dcc_core::domain::execution_dock::DockRunMode;
use dcc_core::domain::workspace_git::WorkspacePrStatusOutput;
use gpui::{Modifiers, TestAppContext, WindowAppearance, px, size};
use serde_json::Value;

use super::super::{CodeColumn, CodeColumnTab};
use super::Confirm;
use crate::data::desktop_api::DesktopApi;
use crate::test_support::{fake_desktop_routes, git, git_checkout, task_database};
use crate::theme::{Theme, ThemePreference};
use crate::ui::Toaster;

pub(super) struct Setup {
    _db: tempfile::TempDir,
    server: std::thread::JoinHandle<Vec<(String, String)>>,
}

/// The column on "Mudanças" over `root`, its task in a database of its
/// own, the desktop answering each step's route in turn.
pub(super) fn open<'a>(
    cx: &'a mut TestAppContext,
    root: &std::path::Path,
    worktree: Option<&std::path::Path>,
    answers: Vec<(&'static str, u16, String)>,
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
    let (port, server) = fake_desktop_routes(answers);
    let checkout = worktree.unwrap_or(root).to_string_lossy().into_owned();
    let (column, cx) = cx.add_window_view(|_, cx| {
        let mut column = CodeColumn::new(CodeColumnTab::Changes, cx);
        column.db_path = db_path;
        column.endpoint = Some(DesktopApi::new(port, "k"));
        column
    });
    column.update(cx, |column, cx| column.set_root(Some(checkout), cx));
    cx.run_until_parked();
    cx.simulate_resize(size(px(380.0), px(800.0)));
    cx.run_until_parked();
    (column, cx, Setup { _db: db, server })
}

fn ok(path: &'static str) -> (&'static str, u16, String) {
    (path, 200, r#"{"ok":true}"#.to_string())
}

pub(super) const PREPARE: &str = "/api/v1/workspaces/w-1/delivery/prepare-commit";
const COMMIT: &str = "/api/v1/workspaces/w-1/delivery/commit-reviewed";
const MERGE: &str = "/api/v1/workspaces/w-1/delivery/merge";
const COMPLETE: &str = "/api/v1/workspaces/w-1/delivery/complete";
const PUSH: &str = "/api/v1/workspaces/w-1/delivery/push";
const SYNC: &str = "/api/v1/workspaces/w-1/delivery/sync-base";
const PULL: &str = "/api/v1/workspaces/w-1/delivery/pull";

pub(super) fn requests(setup: Setup) -> Vec<(String, Value)> {
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

pub(super) fn toasts(cx: &mut gpui::VisualTestContext) -> Vec<String> {
    cx.update(|_, cx| Toaster::messages(cx))
}

fn open_pr(state: &str) -> WorkspacePrStatusOutput {
    WorkspacePrStatusOutput {
        checks_failing: None,
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
    let (column, cx, setup) = open(
        cx,
        repo.path(),
        None,
        vec![(PREPARE, 200, suggestion.into()), ok(COMMIT)],
    );
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
    let (column, cx, setup) = open(cx, repo.path(), None, vec![ok(MERGE), ok(COMPLETE)]);
    column.update(cx, |column, cx| {
        column.delivery.pr = Some(open_pr("open"));
        cx.notify();
    });
    cx.run_until_parked();
    column.read_with(cx, |column, _| {
        assert_eq!(column.commit_mode(), CommitMode::Merge)
    });
    assert!(cx.debug_bounds("delivery-rail-request").is_some(), "PR #12");
    assert!(cx.debug_bounds("delivery-standing").is_some());
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
        PUSH,
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
fn a_push_the_checks_stop_shows_their_report_and_hands_it_to_the_agent(cx: &mut TestAppContext) {
    let dir = tracked_checkout(true);
    let refused = (
        PUSH,
        409,
        r#"{"error":{"code":"checks_failed","message":"Os checks falharam.","outcome":{"report":{"status":"failed","sourcePath":null,"steps":[{"command":"npm test","success":false,"exitCode":1,"output":"1 failing","timedOut":false,"durationMs":900,"truncated":false}]}}}}"#
            .to_string(),
    );
    let (column, cx, _setup) = open(cx, &dir.path().join("repo"), None, vec![refused]);
    let button = cx.debug_bounds("delivery-commit").unwrap();
    cx.simulate_click(button.center(), Modifiers::none());
    cx.run_until_parked();
    column.read_with(cx, |column, _| {
        let report = column.delivery.blocked.as_ref().expect("the report");
        assert_eq!(report.steps[0].command, "npm test");
    });
    // The dialog is the shell's overlay; its "Pedir ao agente" hands the
    // failures to the composer and closes it.
    let asked = std::rc::Rc::new(std::cell::RefCell::new(None));
    let seen = asked.clone();
    let _listen = cx.update(|_, cx| {
        cx.subscribe(&column, move |_, event, _| {
            if let super::super::CodeColumnEvent::AskAgent { prompt, .. } = event {
                *seen.borrow_mut() = Some(prompt.clone());
            }
        })
    });
    column.update(cx, |column, cx| column.ask_agent_to_fix_checks(cx));
    let prompt = asked.borrow().clone().expect("asked the agent");
    assert!(prompt.contains("`npm test` (exit 1):"), "{prompt}");
    assert!(prompt.contains("1 failing"));
    column.read_with(cx, |column, _| assert!(column.delivery.blocked.is_none()));
}

#[gpui::test]
fn a_branch_behind_its_remote_offers_to_pull(cx: &mut TestAppContext) {
    let dir = tracked_checkout(false);
    let pulled = r#"{"branch":"feature","baseBranch":"feature","remote":"origin","updated":true,"conflictCount":0}"#;
    let (column, cx, setup) = open(
        cx,
        &dir.path().join("repo"),
        None,
        vec![(PULL, 200, pulled.into())],
    );
    column.read_with(cx, |column, _| {
        assert_eq!(column.status.as_ref().unwrap().behind_of_remote_count, 1)
    });
    column.read_with(cx, |column, _| {
        let first = column.dock().unwrap().actions[0];
        assert_eq!(first.mode, Some(DockRunMode::Pull), "pulling comes first");
    });
    let sync = cx
        .debug_bounds("branch-toolbar-sync")
        .expect("the pull bar");
    cx.simulate_click(sync.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(
        toasts(cx)
            .iter()
            .any(|toast| toast == "Alterações de origin/feature trazidas para a tarefa."),
        "{:?}",
        toasts(cx)
    );
    let requests = requests(setup);
    assert_eq!(requests[0].0, PULL);
}

#[gpui::test]
fn updating_from_the_base_asks_then_merges_it_in(cx: &mut TestAppContext) {
    let dir = tracked_checkout(false);
    let synced = r#"{"branch":"feature","baseBranch":"main","remote":"origin","updated":true,"conflictCount":0}"#;
    let (column, cx, setup) = open(
        cx,
        &dir.path().join("repo"),
        None,
        vec![(SYNC, 200, synced.into())],
    );
    column.update(cx, |column, cx| column.ask_sync_base(cx));
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
    assert_eq!(requests(setup)[0].0, SYNC);
}

#[gpui::test]
fn a_merged_task_with_its_own_worktree_completes_once(cx: &mut TestAppContext) {
    let repo = git_checkout(&[("a.txt", "um\n")]);
    let parent = tempfile::tempdir().unwrap();
    let (column, cx, setup) = open(cx, parent.path(), Some(repo.path()), vec![ok(COMPLETE)]);
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

/// A checkout on `feature` in the middle of merging `main`, `a.txt` conflicted.
fn conflicted_checkout() -> tempfile::TempDir {
    let repo = git_checkout(&[("a.txt", "one\n")]);
    git(repo.path(), &["checkout", "-q", "-b", "feature"]);
    std::fs::write(repo.path().join("a.txt"), "feature\n").unwrap();
    git(repo.path(), &["commit", "-qam", "feature"]);
    git(repo.path(), &["checkout", "-q", "main"]);
    std::fs::write(repo.path().join("a.txt"), "main\n").unwrap();
    git(repo.path(), &["commit", "-qam", "main"]);
    git(repo.path(), &["checkout", "-q", "feature"]);
    let _ = std::process::Command::new("git")
        .args(["merge", "main"])
        .current_dir(repo.path())
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .output();
    repo
}

fn conflict_state(result: &str) -> String {
    let side = |text: &str| {
        serde_json::json!({
            "exists": true, "binary": false, "truncated": false,
            "byteCount": text.len(), "mode": "100644", "text": text,
        })
    };
    serde_json::json!({
        "operation": "merge", "currentBranch": "feature", "incomingRef": "main",
        "conflicts": [{
            "path": "a.txt", "kind": "both-modified",
            "base": side("one\n"), "current": side("feature\n"), "incoming": side("main\n"),
            "result": side(result),
        }],
    })
    .to_string()
}

#[gpui::test]
fn conflicts_open_the_resolver_until_their_results_are_clean(cx: &mut TestAppContext) {
    let repo = conflicted_checkout();
    let (column, cx, setup) = open(
        cx,
        repo.path(),
        None,
        vec![(
            "/api/v1/git/conflict-state",
            200,
            conflict_state("<<<<<<< HEAD\nfeature\n=======\nmain\n>>>>>>> main\n"),
        )],
    );
    column.read_with(cx, |column, _| {
        assert!(column.delivery.conflicts.is_some(), "read with the status");
        assert_eq!(column.commit_mode(), CommitMode::ResolveConflicts);
    });
    let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let sink = events.clone();
    column.update(cx, |_, cx| {
        cx.subscribe_self(move |_, event: &super::super::CodeColumnEvent, _| {
            sink.borrow_mut().push(event.clone())
        })
        .detach()
    });
    let button = cx.debug_bounds("delivery-commit").unwrap();
    cx.simulate_click(button.center(), Modifiers::none());
    let opened = events.borrow().iter().find_map(|event| match event {
        super::super::CodeColumnEvent::OpenMergeResolver(request) => Some(request.clone()),
        _ => None,
    });
    let request = opened.expect("the resolver");
    assert_eq!(request.root, repo.path().to_string_lossy());
    assert_eq!(request.task_id.as_deref(), Some("w-1"));
    let _ = requests(setup);
}

#[gpui::test]
fn clean_results_complete_the_merge_once_confirmed(cx: &mut TestAppContext) {
    let repo = conflicted_checkout();
    let (column, cx, setup) = open(
        cx,
        repo.path(),
        None,
        vec![
            ("/api/v1/git/conflict-state", 200, conflict_state("feature\nmain\n")),
            ("/api/v1/git/conflict-state", 200, conflict_state("feature\nmain\n")),
            ok("/api/v1/git/mark-conflict-resolved"),
            (
                "/api/v1/git/validation-config",
                200,
                r#"{"commands":[],"sourcePath":null,"timeoutSeconds":600,"configHash":null}"#
                    .into(),
            ),
            (
                "/api/v1/git/complete-merge",
                200,
                r#"{"completed":true,"validation":{"status":"not-configured","sourcePath":null,"steps":[]}}"#
                    .into(),
            ),
        ],
    );
    column.read_with(cx, |column, _| {
        assert_eq!(column.commit_mode(), CommitMode::CompleteMerge)
    });
    assert!(cx.debug_bounds("delivery-review-resolution").is_some());
    let button = cx.debug_bounds("delivery-commit").unwrap();
    cx.simulate_click(button.center(), Modifiers::none());
    column.read_with(cx, |column, _| {
        assert_eq!(column.delivery.confirm, Some(Confirm::CompleteMerge))
    });
    column.update(cx, |column, cx| column.run_confirmed(cx));
    cx.run_until_parked();
    assert!(
        toasts(cx)
            .iter()
            .any(|toast| toast == "Merge concluído e enviado"),
        "{:?}",
        toasts(cx)
    );
    let calls = requests(setup);
    let paths: Vec<&str> = calls.iter().map(|(path, _)| path.as_str()).collect();
    assert_eq!(
        paths,
        vec![
            "/api/v1/git/conflict-state",
            "/api/v1/git/conflict-state",
            "/api/v1/git/mark-conflict-resolved",
            "/api/v1/git/validation-config",
            "/api/v1/git/complete-merge",
        ]
    );
    assert_eq!(calls[2].1["relativePath"], "a.txt");
}

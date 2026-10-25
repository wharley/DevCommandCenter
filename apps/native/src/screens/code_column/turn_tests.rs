//! "Último turno" over a fake desktop: the chip and its count, the turn's
//! files with their patch, Guarded Undo prepared, confirmed and run, a
//! preparation that is refused, and Undo across a task's two projects.

use gpui::{Modifiers, TestAppContext, WindowAppearance, px, size};
use serde_json::{Value, json};

use super::changes::Scope;
use super::turn::TurnDiff;
use super::{CodeColumn, CodeColumnTab};
use crate::data::desktop_api::DesktopApi;
use crate::test_support::{fake_desktop_routes, git_checkout, task_database};
use crate::theme::{Theme, ThemePreference};
use crate::ui::Toaster;

struct Setup {
    _db: tempfile::TempDir,
    _repo: tempfile::TempDir,
    server: std::thread::JoinHandle<Vec<(String, String)>>,
}

fn open<'a>(
    cx: &'a mut TestAppContext,
    routes: Vec<(&'static str, u16, String)>,
    two_projects: bool,
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
    let repo = git_checkout(&[("a.rs", "fn a() {}\n")]);
    let db = tempfile::tempdir().unwrap();
    let db_path = task_database(db.path(), "w-1", repo.path(), None);
    if two_projects {
        bundle(&db_path, db.path());
    }
    let (port, server) = fake_desktop_routes(routes);
    let root = repo.path().to_string_lossy().into_owned();
    let (column, cx) = cx.add_window_view(|_, cx| {
        let mut column = CodeColumn::new(CodeColumnTab::Changes, cx);
        column.db_path = db_path;
        column.endpoint = Some(DesktopApi::new(port, "k"));
        column
    });
    column.update(cx, |column, cx| column.set_root(Some(root), cx));
    cx.run_until_parked();
    // The conversation tells its session once the task is known.
    column.update(cx, |column, cx| {
        column.set_reviews(Some("s-1".into()), Vec::new(), cx)
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
        },
    )
}

/// A second project "LP" with the task as its bundle's primary.
fn bundle(db_path: &std::path::Path, dir: &std::path::Path) {
    use dcc_core::domain::workspace::WorkspaceId;
    use dcc_core::domain::workspace_bundle::{
        WorkspaceBundle, WorkspaceBundleId, WorkspaceBundleIsolation, WorkspaceBundleMember,
        WorkspaceBundleState,
    };
    let other = dir.join("lp");
    std::fs::create_dir_all(&other).unwrap();
    task_database(dir, "w-2", &other, None);
    let repo = dcc_infra::db::SqliteWorkspaceRepo::open(db_path).unwrap();
    let id = WorkspaceBundleId("b-1".into());
    let member = |workspace: &str, position| WorkspaceBundleMember {
        bundle_id: id.clone(),
        workspace_id: WorkspaceId(workspace.into()),
        created_for_bundle: true,
        position,
    };
    futures::executor::block_on(dcc_core::ports::WorkspaceBundleRepo::save_workspace_bundle(
        &repo,
        &WorkspaceBundle {
            id: id.clone(),
            name: "Duas frentes".into(),
            primary_workspace_id: WorkspaceId("w-1".into()),
            state: WorkspaceBundleState::Ready,
            isolation: WorkspaceBundleIsolation::Local,
            created_at: "2026-10-24T00:00:00Z".into(),
            updated_at: "2026-10-24T00:00:00Z".into(),
        },
        &[member("w-1", 0), member("w-2", 1)],
    ))
    .unwrap();
}

fn file(path: &str, insertions: u32) -> Value {
    json!({
        "path": path, "oldPath": null, "status": "M", "insertions": insertions,
        "deletions": 1, "untracked": false, "binary": false, "previewUnavailable": false,
    })
}

fn review(workspace: &str, capture: &str) -> Value {
    json!({
        "snapshotId": format!("snap-{workspace}"), "sessionId": "s-1", "turnId": "t-1",
        "workspaceId": workspace, "state": "completed", "compatibility": "matches_result",
        "baseFingerprint": null, "resultFingerprint": null,
        "files": [file("src/a.rs", 2), file("b.rs", 3)],
        "insertions": 5, "deletions": 2, "diffTruncated": false,
        "excludedPreexistingUntrackedCount": 0, "observedValidations": [],
        "turnOutcome": "completed", "outcomeReason": null, "error": null, "completedAt": null,
        "guardedUndo": {
            "state": capture, "reasonCode": null, "fileCount": 2, "artifactBytes": 10,
            "completedAt": null, "expiresAt": null,
        },
        "activeUndo": null,
    })
}

fn patch() -> String {
    json!({
        "snapshotId": "snap-w-1", "path": "src/a.rs", "previewUnavailable": false,
        "diff": "--- a/src/a.rs\n+++ b/src/a.rs\n@@ -1,1 +1,2 @@\n-fn a() {}\n+fn a() { b(); }\n+fn b() {}\n",
    })
    .to_string()
}

fn ready(snapshot: &str) -> String {
    json!({
        "status": "ready", "snapshotId": snapshot,
        "previewToken": format!("token-{snapshot}"), "expiresAt": "2099-01-01T00:00:00Z",
        "fileCount": 2, "totalBytes": 1234,
        "files": [{ "displayPath": "src/a.rs", "size": 9, "binary": false, "preview": "fn a() {}" }],
        "unrelatedPathsAreNotTargets": true,
    })
    .to_string()
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

#[gpui::test]
fn the_last_turn_shows_its_files_and_undo_restores_it(cx: &mut TestAppContext) {
    let (column, cx, setup) = open(
        cx,
        vec![
            (
                "/api/v1/sessions/s-1/last-turn-review",
                200,
                review("w-1", "eligible").to_string(),
            ),
            // Both cards start open: each reads its patch.
            ("/api/v1/turn-review/file-diff", 200, patch()),
            ("/api/v1/turn-review/file-diff", 200, patch()),
            ("/api/v1/guarded-undo/prepare", 200, ready("snap-w-1")),
            (
                "/api/v1/guarded-undo/execute",
                200,
                json!({ "status": "completed", "operationId": "op-1", "reasonCode": null })
                    .to_string(),
            ),
            // Read again after the restore: the capture is spent.
            (
                "/api/v1/sessions/s-1/last-turn-review",
                200,
                review("w-1", "consumed").to_string(),
            ),
        ],
        false,
    );
    column.read_with(cx, |column, _| {
        assert_eq!(column.turn.count(), Some(2), "the chip's count");
        assert_eq!(column.review_scope(), Scope::Working);
    });
    let chip = cx
        .debug_bounds("changes-scope-last-turn")
        .expect("the chip with a session");
    cx.simulate_click(chip.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(cx.debug_bounds("turn-summary").is_some());
    assert!(cx.debug_bounds("turn-shield").is_some());
    // Two files: both cards start open; the first one's patch is read.
    column.read_with(cx, |column, _| {
        assert_eq!(column.review_scope(), Scope::LastTurn);
        assert!(column.turn.cards.iter().all(|card| card.open));
        assert!(
            column
                .turn
                .cards
                .iter()
                .all(|card| matches!(card.diff, TurnDiff::Ready(_)))
        );
    });
    let undo = cx.debug_bounds("turn-undo").expect("Desfazer");
    cx.simulate_click(undo.center(), Modifiers::none());
    cx.run_until_parked();
    column.read_with(cx, |column, _| {
        let single = &column.turn.single;
        assert!(single.dialog);
        assert_eq!(single.prepared.as_ref().unwrap().total_bytes, 1234);
        assert!(!single.expired);
    });
    column.update(cx, |column, cx| column.execute_undo(cx));
    cx.run_until_parked();
    column.read_with(cx, |column, _| {
        assert_eq!(column.turn.single.outcome, Some(("completed", None)));
        assert!(column.turn.summary().is_some());
    });
    assert!(cx.debug_bounds("turn-undo-result").is_some());
    assert!(
        cx.debug_bounds("turn-undo").is_none(),
        "nothing left to undo"
    );
    column.update(cx, |column, cx| column.close_undo(cx));
    column.read_with(cx, |column, _| {
        assert!(!column.turn.single.dialog);
        assert_eq!(column.turn.single.outcome, None);
    });
    let requests = requests(setup.server);
    let execute = requests
        .iter()
        .find(|(path, _)| path == "/api/v1/guarded-undo/execute")
        .unwrap();
    assert_eq!(execute.1["previewToken"], "token-snap-w-1");
    let read = &requests[0];
    assert_eq!(read.1["workspaceId"], "w-1");
}

#[gpui::test]
fn a_refused_preparation_says_why_and_opens_nothing(cx: &mut TestAppContext) {
    let (column, cx, setup) = open(
        cx,
        vec![
            (
                "/api/v1/sessions/s-1/last-turn-review",
                200,
                review("w-1", "eligible").to_string(),
            ),
            (
                "/api/v1/guarded-undo/prepare",
                200,
                json!({
                    "status": "blocked", "snapshotId": "snap-w-1",
                    "reasonCode": "head_changed", "detailsAvailable": false,
                })
                .to_string(),
            ),
        ],
        false,
    );
    column.update(cx, |column, cx| {
        column.set_scope(Scope::LastTurn, cx);
        column.prepare_undo(cx);
    });
    cx.run_until_parked();
    column.read_with(cx, |column, _| {
        let single = &column.turn.single;
        assert!(!single.dialog);
        assert_eq!(
            single.problem,
            Some(("blocked", "head_changed".to_string()))
        );
    });
    let _ = setup.server.join();
}

#[gpui::test]
fn a_two_project_turn_is_undone_project_after_project(cx: &mut TestAppContext) {
    let (column, cx, setup) = open(
        cx,
        vec![
            (
                "/api/v1/sessions/s-1/last-turn-review",
                200,
                review("w-1", "eligible").to_string(),
            ),
            // Each project's review of the same turn.
            (
                "/api/v1/sessions/s-1/last-turn-review",
                200,
                review("w-1", "eligible").to_string(),
            ),
            (
                "/api/v1/sessions/s-1/last-turn-review",
                200,
                review("w-2", "eligible").to_string(),
            ),
            ("/api/v1/guarded-undo/prepare", 200, ready("snap-w-1")),
            ("/api/v1/guarded-undo/prepare", 200, ready("snap-w-2")),
            (
                "/api/v1/guarded-undo/execute",
                200,
                json!({ "status": "completed", "operationId": "op-1", "reasonCode": null })
                    .to_string(),
            ),
            (
                "/api/v1/guarded-undo/execute",
                200,
                json!({ "status": "rolled_back", "operationId": "op-2", "reasonCode": "head_changed" })
                    .to_string(),
            ),
        ],
        true,
    );
    column.update(cx, |column, cx| column.set_scope(Scope::LastTurn, cx));
    cx.run_until_parked();
    column.read_with(cx, |column, _| {
        assert_eq!(column.turn.members.len(), 2);
        assert_eq!(column.turn.member_reviews.len(), 2);
    });
    assert!(cx.debug_bounds("turn-undo").is_none());
    let undo = cx
        .debug_bounds("turn-undo-projects")
        .expect("Desfazer o turno · 2 projetos");
    cx.simulate_click(undo.center(), Modifiers::none());
    cx.run_until_parked();
    column.update(cx, |column, cx| {
        assert_eq!(column.turn.projects.prepared.as_ref().unwrap().len(), 2);
        column.execute_projects_undo(cx);
    });
    cx.run_until_parked();
    column.read_with(cx, |column, _| {
        let statuses: Vec<&str> = column
            .turn
            .projects
            .results
            .as_ref()
            .unwrap()
            .iter()
            .map(|result| result.status)
            .collect();
        assert_eq!(statuses, vec!["completed", "rolled_back"]);
    });
    let requests = requests(setup.server);
    let member_reads: Vec<&Value> = requests
        .iter()
        .filter(|(path, body)| path.ends_with("/last-turn-review") && body["turnId"] == "t-1")
        .map(|(_, body)| &body["workspaceId"])
        .collect();
    assert_eq!(member_reads, vec!["w-1", "w-2"]);
}

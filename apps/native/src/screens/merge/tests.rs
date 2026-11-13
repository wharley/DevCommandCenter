//! The resolver driven headlessly against a fake desktop: a block taken
//! and the file saved, a binary file taken whole by git, the checks asked
//! about and the merge completed; the questions before a doubtful save or
//! a lost edit; starting a merge; an operation it doesn't handle.

use std::cell::RefCell;
use std::rc::Rc;

use dcc_core::domain::merge_conflict::{
    WorkspaceGitConflictContent, WorkspaceGitConflictEntry, WorkspaceGitConflictKind,
    WorkspaceGitConflictOperation, WorkspaceGitConflictStateOutput,
};
use gpui::{Modifiers, TestAppContext, WindowAppearance, px, size};
use serde_json::json;

use super::{MergeResolver, MergeResolverEvent, MergeResolverRequest, Pending};
use crate::data::desktop_api::DesktopApi;
use crate::test_support::fake_desktop_routes;
use crate::theme::{Theme, ThemePreference};

pub(super) const CONFLICTED: &str =
    "before\n<<<<<<< HEAD\nmine\n=======\ntheirs\n>>>>>>> origin/main\nafter\n";

pub(super) fn text(value: Option<&str>) -> WorkspaceGitConflictContent {
    WorkspaceGitConflictContent {
        exists: true,
        byte_count: value.map_or(0, str::len) as u64,
        mode: Some("100644".into()),
        text: value.map(str::to_string),
        ..Default::default()
    }
}

pub(super) fn source() -> WorkspaceGitConflictEntry {
    WorkspaceGitConflictEntry {
        path: "src/a.rs".into(),
        kind: WorkspaceGitConflictKind::BothModified,
        base: text(Some("before\nbase\nafter\n")),
        current: text(Some("before\nmine\nafter\n")),
        incoming: text(Some("before\ntheirs\nafter\n")),
        result: WorkspaceGitConflictContent {
            mode: None,
            ..text(Some(CONFLICTED))
        },
    }
}

fn image() -> WorkspaceGitConflictEntry {
    let binary = WorkspaceGitConflictContent {
        exists: true,
        binary: true,
        byte_count: 10,
        mode: Some("100644".into()),
        text: None,
        ..Default::default()
    };
    WorkspaceGitConflictEntry {
        path: "logo.png".into(),
        kind: WorkspaceGitConflictKind::BothModified,
        base: binary.clone(),
        current: binary.clone(),
        incoming: binary.clone(),
        result: binary,
    }
}

pub(super) fn state(
    operation: WorkspaceGitConflictOperation,
    conflicts: Vec<WorkspaceGitConflictEntry>,
) -> String {
    serde_json::to_string(&WorkspaceGitConflictStateOutput {
        operation,
        current_branch: Some("refs/heads/feature".into()),
        incoming_ref: Some("origin/main".into()),
        conflicts,
    })
    .unwrap()
}

pub(super) fn checks(commands: &[&str]) -> String {
    json!({ "commands": commands, "sourcePath": null, "timeoutSeconds": 600, "configHash": "h1" })
        .to_string()
}

type Events = Rc<RefCell<Vec<MergeResolverEvent>>>;

#[allow(clippy::type_complexity)]
fn open<'a>(
    cx: &'a mut TestAppContext,
    routes: Vec<(&'static str, u16, String)>,
) -> (
    gpui::Entity<MergeResolver>,
    &'a mut gpui::VisualTestContext,
    Events,
    std::thread::JoinHandle<Vec<(String, String)>>,
) {
    open_asked(cx, routes, false)
}

/// `start`: opened on a conflict the forge reports, to bring the base in.
#[allow(clippy::type_complexity)]
fn open_asked<'a>(
    cx: &'a mut TestAppContext,
    routes: Vec<(&'static str, u16, String)>,
    start: bool,
) -> (
    gpui::Entity<MergeResolver>,
    &'a mut gpui::VisualTestContext,
    Events,
    std::thread::JoinHandle<Vec<(String, String)>>,
) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::Toaster::init(cx);
        crate::screens::file_view::bind_keys(cx);
    });
    let (port, server) = fake_desktop_routes(routes);
    let request = MergeResolverRequest {
        start,
        root: "/tmp/checkout".into(),
        task_id: Some("w-1".into()),
        base_branch: Some("main".into()),
        forge_login: Some("ana".into()),
        workspace_label: Some("Pagamentos".into()),
        agent: None,
    };
    let (resolver, cx) = cx.add_window_view(move |_, cx| {
        MergeResolver::new(request, cx).with_endpoint(DesktopApi::new(port, "k"), cx)
    });
    cx.simulate_resize(size(px(1400.0), px(900.0)));
    let events: Events = Rc::default();
    let sink = events.clone();
    cx.update(|_, cx| {
        cx.subscribe(&resolver, move |_, event: &MergeResolverEvent, _| {
            sink.borrow_mut().push(event.clone())
        })
        .detach();
    });
    cx.run_until_parked();
    (resolver, cx, events, server)
}

pub(super) fn click(cx: &mut gpui::VisualTestContext, selector: &'static str) {
    cx.run_until_parked();
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} on screen"));
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
}

#[gpui::test]
fn blocks_files_and_the_merge_get_resolved(cx: &mut TestAppContext) {
    let merge = WorkspaceGitConflictOperation::Merge;
    let (resolver, cx, events, server) = open(
        cx,
        vec![
            // The first read, the open merge's checks.
            (
                "/api/v1/git/conflict-state",
                200,
                state(merge, vec![source(), image()]),
            ),
            ("/api/v1/git/validation-config", 200, checks(&["yarn test"])),
            // Saving the text file.
            (
                "/api/v1/files/write",
                200,
                json!({ "conflicted": false }).to_string(),
            ),
            ("/api/v1/git/mark-conflict-resolved", 200, "{}".into()),
            (
                "/api/v1/git/conflict-state",
                200,
                state(merge, vec![image()]),
            ),
            ("/api/v1/git/validation-config", 200, checks(&["yarn test"])),
            // The image taken whole by git.
            ("/api/v1/git/accept-conflict", 200, "{}".into()),
            ("/api/v1/git/conflict-state", 200, state(merge, vec![])),
            ("/api/v1/git/validation-config", 200, checks(&["yarn test"])),
            (
                "/api/v1/git/complete-merge",
                200,
                json!({
                    "completed": true,
                    "validation": { "status": "passed", "sourcePath": null, "steps": [] },
                })
                .to_string(),
            ),
        ],
    );
    // The first file opens on its only block, labelled by the workspace.
    resolver.read_with(cx, |resolver, cx| {
        let open = resolver.open.as_ref().expect("the first file");
        assert_eq!(open.entry.path, "src/a.rs");
        assert_eq!(open.hunks().len(), 1);
        let ((current, current_ref), (incoming, _)) = resolver.labels(cx);
        assert_eq!(current, "Pagamentos · workspace atual");
        assert_eq!(current_ref, "feature");
        assert_eq!(incoming, "origin/main · branch base");
    });
    assert!(cx.debug_bounds("merge-hunk-position").is_some());
    click(cx, "merge-accept-both");
    resolver.read_with(cx, |resolver, cx| {
        let open = resolver.open.as_ref().unwrap();
        assert_eq!(open.buffer, "before\nmine\ntheirs\nafter\n");
        assert_eq!(
            open.editor.read(cx).text(),
            open.buffer,
            "in the editor too"
        );
        assert!(open.hunks().is_empty());
    });
    assert!(cx.debug_bounds("merge-hunks-done").is_some());
    click(cx, "merge-save");
    // The binary file is next: only whole sides can resolve it.
    resolver.read_with(cx, |resolver, _| {
        assert_eq!(resolver.open.as_ref().unwrap().entry.path, "logo.png");
        assert_eq!(resolver.resolved(), (1, 2));
    });
    click(cx, "merge-use-current");
    // Every file resolved: the checks are asked about, then the merge goes.
    assert!(cx.debug_bounds("merge-validation-commands").is_some());
    click(cx, "merge-complete");
    resolver.read_with(cx, |resolver, _| {
        assert_eq!(resolver.pending, Some(Pending::ValidationCommands))
    });
    click(cx, "merge-confirm-ok");
    assert!(events.borrow().contains(&MergeResolverEvent::Closed));
    let calls = server.join().unwrap();
    let body = |path: &str| -> serde_json::Value {
        let (_, body) = calls
            .iter()
            .find(|(line, _)| line.contains(path))
            .unwrap_or_else(|| panic!("{path} called"));
        serde_json::from_str(body).unwrap()
    };
    let write = body("/api/v1/files/write");
    assert_eq!(write["content"], "before\nmine\ntheirs\nafter\n");
    assert_eq!(write["expectedPrevious"], CONFLICTED);
    assert_eq!(body("mark-conflict-resolved")["delete"], false);
    assert_eq!(body("accept-conflict")["side"], "current");
    let complete = body("complete-merge");
    assert_eq!(complete["validationConfigHash"], "h1");
    assert_eq!(complete["validationCommands"], json!(["yarn test"]));
    assert_eq!(complete["forgeLogin"], "ana");
}

#[gpui::test]
fn a_doubtful_result_or_a_lost_edit_asks_first(cx: &mut TestAppContext) {
    let merge = WorkspaceGitConflictOperation::Merge;
    let mut other = source();
    other.path = "src/b.rs".into();
    let (resolver, cx, events, _server) = open(
        cx,
        vec![
            (
                "/api/v1/git/conflict-state",
                200,
                state(merge, vec![source(), other]),
            ),
            ("/api/v1/git/validation-config", 200, checks(&[])),
        ],
    );
    // Blocks left: nothing is saved.
    resolver.update(cx, |resolver, cx| resolver.save(false, false, cx));
    resolver.read_with(cx, |resolver, _| {
        assert_eq!(resolver.pending, None);
        assert_eq!(resolver.busy, None);
    });
    // A stray marker, then an empty file where a side had code.
    resolver.update(cx, |resolver, cx| {
        resolver.set_result("kept\n=======\n".into(), cx);
        resolver.save(false, false, cx);
        assert_eq!(resolver.pending, Some(Pending::RemainingMarkers));
        resolver.cancel_pending(cx);
        resolver.set_result("  \n".into(), cx);
        resolver.save(false, false, cx);
        assert_eq!(resolver.pending, Some(Pending::EmptyResult));
        resolver.cancel_pending(cx);
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("merge-confirm-text").is_none(), "answered");
    // The edit is unsaved: another file, or closing, asks.
    click(cx, "merge-file-src/b.rs");
    resolver.read_with(cx, |resolver, _| {
        assert_eq!(
            resolver.pending,
            Some(Pending::DiscardForFile("src/b.rs".into()))
        );
        assert_eq!(resolver.open.as_ref().unwrap().entry.path, "src/a.rs");
    });
    click(cx, "merge-confirm-ok");
    resolver.read_with(cx, |resolver, _| {
        assert_eq!(resolver.open.as_ref().unwrap().entry.path, "src/b.rs")
    });
    resolver.update(cx, |resolver, cx| {
        resolver.set_result("changed\n".into(), cx);
        resolver.request_close(cx);
        assert_eq!(resolver.pending, Some(Pending::DiscardToClose));
        resolver.restore(cx);
        resolver.cancel_pending(cx);
        resolver.request_close(cx);
    });
    assert_eq!(events.borrow().as_slice(), &[MergeResolverEvent::Closed]);
}

#[gpui::test]
fn with_no_merge_it_starts_one(cx: &mut TestAppContext) {
    let none = WorkspaceGitConflictOperation::None;
    let merge = WorkspaceGitConflictOperation::Merge;
    let (resolver, cx, events, server) = open(
        cx,
        vec![
            ("/api/v1/git/conflict-state", 200, state(none, vec![])),
            (
                "/api/v1/workspaces/w-1/delivery/sync-base",
                200,
                json!({
                    "branch": "feature", "baseBranch": "main", "remote": "origin",
                    "updated": false, "conflictCount": 1,
                })
                .to_string(),
            ),
            (
                "/api/v1/git/conflict-state",
                200,
                state(merge, vec![source()]),
            ),
            (
                "/api/v1/git/conflict-state",
                200,
                state(merge, vec![source()]),
            ),
            ("/api/v1/git/validation-config", 200, checks(&[])),
        ],
    );
    click(cx, "merge-start");
    resolver.read_with(cx, |resolver, _| {
        assert_eq!(resolver.open.as_ref().unwrap().entry.path, "src/a.rs");
    });
    assert!(events.borrow().contains(&MergeResolverEvent::StateChanged));
    assert!(
        !events.borrow().contains(&MergeResolverEvent::Closed),
        "conflicts to resolve"
    );
    let calls = server.join().unwrap();
    let sync = calls
        .iter()
        .find(|(line, _)| line.contains("sync-base"))
        .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&sync.1).unwrap()["baseBranch"],
        "main"
    );
}

fn synced() -> (&'static str, u16, String) {
    (
        "/api/v1/workspaces/w-1/delivery/sync-base",
        200,
        json!({
            "branch": "feature", "baseBranch": "main", "remote": "origin",
            "updated": false, "conflictCount": 1,
        })
        .to_string(),
    )
}

#[gpui::test]
fn asked_to_resolve_with_no_merge_here_it_brings_the_base_in(cx: &mut TestAppContext) {
    let none = WorkspaceGitConflictOperation::None;
    let merge = WorkspaceGitConflictOperation::Merge;
    let conflicts = || {
        (
            "/api/v1/git/conflict-state",
            200,
            state(merge, vec![source()]),
        )
    };
    let (resolver, cx, _, server) = open_asked(
        cx,
        vec![
            ("/api/v1/git/conflict-state", 200, state(none, vec![])),
            synced(),
            conflicts(),
            conflicts(),
            ("/api/v1/git/validation-config", 200, checks(&[])),
        ],
        true,
    );
    let calls = server.join().unwrap();
    cx.run_until_parked();
    // No screen to start it from: the conflicts are what shows.
    assert!(cx.debug_bounds("merge-start").is_none());
    resolver.read_with(cx, |resolver, _| {
        assert_eq!(resolver.open.as_ref().unwrap().entry.path, "src/a.rs");
        assert!(!resolver.start_pending, "only once");
    });
    assert_eq!(
        calls
            .iter()
            .filter(|(line, _)| line.contains("sync-base"))
            .count(),
        1
    );
}

#[gpui::test]
fn a_merge_under_way_can_be_dropped_for_the_current_base(cx: &mut TestAppContext) {
    let merge = WorkspaceGitConflictOperation::Merge;
    let conflicts = || {
        (
            "/api/v1/git/conflict-state",
            200,
            state(merge, vec![source()]),
        )
    };
    let (resolver, cx, events, server) = open(
        cx,
        vec![
            conflicts(),
            ("/api/v1/git/validation-config", 200, checks(&[])),
            ("/api/v1/git/abort-merge", 200, r#"{"ok":true}"#.into()),
            synced(),
            conflicts(),
            conflicts(),
            ("/api/v1/git/validation-config", 200, checks(&[])),
        ],
    );
    // The abort's question has a second answer.
    click(cx, "merge-abort");
    assert!(cx.debug_bounds("merge-confirm-restart").is_some());
    resolver.update(cx, |resolver, cx| resolver.confirm_restart(cx));
    cx.run_until_parked();
    let calls = server.join().unwrap();
    cx.run_until_parked();
    let order: Vec<&str> = calls
        .iter()
        .filter_map(|(line, _)| {
            ["abort-merge", "sync-base"]
                .into_iter()
                .find(|route| line.contains(route))
        })
        .collect();
    assert_eq!(order, ["abort-merge", "sync-base"]);
    // It stays open, on what the current base left.
    assert!(!events.borrow().contains(&MergeResolverEvent::Closed));
    resolver.read_with(cx, |resolver, _| {
        assert_eq!(resolver.open.as_ref().unwrap().entry.path, "src/a.rs");
    });
}

#[gpui::test]
fn another_operation_is_left_to_the_terminal(cx: &mut TestAppContext) {
    let (resolver, cx, _, _server) = open(
        cx,
        vec![(
            "/api/v1/git/conflict-state",
            200,
            state(WorkspaceGitConflictOperation::Rebase, vec![source()]),
        )],
    );
    resolver.read_with(cx, |resolver, _| assert!(resolver.open.is_some()));
    assert!(
        cx.debug_bounds("merge-file-src/a.rs").is_none(),
        "not the files"
    );
    assert!(cx.debug_bounds("merge-resolver").is_some());
}

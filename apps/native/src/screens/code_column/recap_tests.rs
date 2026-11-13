//! The recap strip over "Mudanças": shown only when the task needs the
//! person or is blocked, its action opening the section it names, its
//! signals, and the delegations waiting for review without "Ver atividade".

use dcc_core::domain::workspace_recap::{DeliveryState, RecapAction};
use gpui::{Modifiers, TestAppContext, WindowAppearance, px, size};
use serde_json::json;

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
    let repo = git_checkout(&[("a.txt", "um\n")]);
    std::fs::write(repo.path().join("a.txt"), "dois\n").unwrap();
    let db = tempfile::tempdir().unwrap();
    let db_path = task_database(db.path(), "w-1", repo.path(), None);
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
    cx.simulate_resize(size(px(380.0), px(900.0)));
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

fn context() -> String {
    json!({
        "provider": "github", "host": "github.com", "remoteState": "ok",
        "effectiveLogin": "ana", "boundLogin": "ana",
    })
    .to_string()
}

fn failure() -> String {
    json!({ "snapshot": {
        "attemptToken": "t-1", "workspaceRoot": "/r", "branch": "main", "headSha": "abcdef1234",
        "operation": "push", "classification": "rejected", "remote": "origin",
        "operationTarget": null, "pushTarget": null,
        "output": "! [rejected] main -> main (fetch first)", "outputTruncated": false,
        "changedFiles": [], "changedFilesTruncated": false, "externalUrl": null,
        "availableActions": ["synchronize"], "createdAt": "2026-10-24T12:00:00Z",
    }})
    .to_string()
}

#[gpui::test]
fn a_rejected_push_blocks_and_its_action_opens_the_problem(cx: &mut TestAppContext) {
    let (column, cx, setup) = open(
        cx,
        vec![
            ("/api/v1/forge/context", 200, context()),
            ("/api/v1/git/delivery-failure", 200, failure()),
        ],
    );
    column.read_with(cx, |column, _| {
        let recap = column.workspace_recap().expect("the status answered");
        assert_eq!(recap.state, DeliveryState::Blocked);
        assert_eq!(recap.message_key, "deliveryFailure.push");
        assert!(!column.review.failure_open);
    });
    assert!(cx.debug_bounds("recap-strip").is_some());
    let action = cx.debug_bounds("recap-action").expect("Ver problema");
    cx.simulate_click(action.center(), Modifiers::none());
    column.read_with(cx, |column, _| assert!(column.review.failure_open));
    assert!(cx.debug_bounds("recap-signals").is_none());
    let details = cx.debug_bounds("recap-details").unwrap();
    cx.simulate_click(details.center(), Modifiers::none());
    assert!(
        cx.debug_bounds("recap-signals").is_some(),
        "the six signals open"
    );
    let _ = setup.server.join();
}

#[gpui::test]
fn delegations_waiting_show_without_an_action_and_a_turn_hides_them(cx: &mut TestAppContext) {
    let (column, cx, setup) = open(cx, vec![("/api/v1/forge/context", 200, context())]);
    // Uncommitted work alone is development: no strip.
    assert!(cx.debug_bounds("recap-strip").is_none());
    column.update(cx, |column, cx| column.set_conversation_state(false, 2, cx));
    cx.run_until_parked();
    column.read_with(cx, |column, _| {
        let recap = column.workspace_recap().unwrap();
        assert_eq!(recap.message_key, "delegationResults");
        assert_eq!(recap.action, Some(RecapAction::Activity));
    });
    assert!(cx.debug_bounds("recap-strip").is_some());
    assert!(
        cx.debug_bounds("recap-action").is_none(),
        "no \"Ver atividade\" in the column"
    );
    // The agent at work comes first.
    column.update(cx, |column, cx| column.set_conversation_state(true, 2, cx));
    cx.run_until_parked();
    assert!(cx.debug_bounds("recap-strip").is_none());
    let _ = setup.server.join();
}

#[gpui::test]
fn the_sync_action_asks_first(cx: &mut TestAppContext) {
    let (column, cx, setup) = open(cx, vec![("/api/v1/forge/context", 200, context())]);
    column.update(cx, |column, cx| {
        column.run_recap_action(RecapAction::Sync, cx);
        assert_eq!(
            column.delivery.confirm,
            Some(super::delivery::Confirm::SyncBase)
        );
    });
    let _ = setup.server.join();
}

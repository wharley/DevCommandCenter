//! The local-direct branch picker over a fake desktop: a pick moves the
//! checkout from the branch it was read on, a new branch goes by name, and
//! a started conversation leaves it locked.

use gpui::{Modifiers, TestAppContext, WindowAppearance};
use serde_json::{Value, json};

use super::LocalBranchPicker;
use crate::data::desktop_api::DesktopApi;
use crate::test_support::fake_desktop_routes;
use crate::theme::{Theme, ThemePreference};

const LIST: &str = "/api/v1/workspaces/local-branches";
const SWITCH: &str = "/api/v1/workspaces/local-branches/switch";

/// The fake desktop's thread: each request it answered, with its body.
type Server = std::thread::JoinHandle<Vec<(String, String)>>;

fn branches(current: &str, has_conversation: bool) -> String {
    json!({
        "currentBranch": current,
        "taskBranch": current,
        "hasConversation": has_conversation,
        "agentRunning": false,
        "branches": [
            { "name": "feature", "reference": "refs/heads/feature", "remote": false },
            { "name": "main", "reference": "refs/heads/main", "remote": false },
            { "name": "origin/release", "reference": "refs/remotes/origin/release", "remote": true },
        ],
        "refreshError": null,
    })
    .to_string()
}

fn open<'a>(
    cx: &'a mut TestAppContext,
    routes: Vec<(&'static str, u16, String)>,
) -> (
    gpui::Entity<LocalBranchPicker>,
    &'a mut gpui::VisualTestContext,
    Server,
) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::text_input::bind_keys(cx);
    });
    let (port, server) = fake_desktop_routes(routes);
    let (picker, cx) = cx.add_window_view(|_, cx| {
        LocalBranchPicker::new("w-1".into(), Some(DesktopApi::new(port, "k")), cx)
    });
    cx.run_until_parked();
    (picker, cx, server)
}

fn click(cx: &mut gpui::VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector}"));
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
}

fn bodies(server: Server) -> Vec<(String, Value)> {
    server
        .join()
        .unwrap()
        .into_iter()
        .map(|(line, body)| (line, serde_json::from_str(&body).unwrap()))
        .collect()
}

#[gpui::test]
fn a_picked_branch_moves_the_checkout_from_the_one_it_was_read_on(cx: &mut TestAppContext) {
    let (picker, cx, server) = open(
        cx,
        vec![
            (LIST, 200, branches("main", false)),
            (LIST, 200, branches("main", false)),
            (SWITCH, 200, branches("feature", false)),
        ],
    );
    click(cx, "local-branch-trigger");
    assert!(picker.read_with(cx, |picker, _| picker.open));
    click(cx, "local-branch-refs/heads/feature");
    picker.read_with(cx, |picker, _| {
        assert!(!picker.open);
        let data = picker.data.as_ref().unwrap();
        assert_eq!(data.current_branch.as_deref(), Some("feature"));
    });
    let requests = bodies(server);
    assert_eq!(
        requests[1].1["refresh"], true,
        "opening fetches the remotes"
    );
    assert_eq!(
        requests[2],
        (
            format!("POST {SWITCH}"),
            json!({
                "workspaceId": "w-1",
                "expectedBranch": "main",
                "reference": "refs/heads/feature",
                "newBranch": null,
            })
        )
    );
}

#[gpui::test]
fn a_new_branch_goes_by_its_name(cx: &mut TestAppContext) {
    let (picker, cx, server) = open(
        cx,
        vec![
            (LIST, 200, branches("main", false)),
            (LIST, 200, branches("main", false)),
            (SWITCH, 200, branches("feature/x", false)),
        ],
    );
    click(cx, "local-branch-trigger");
    click(cx, "local-branch-new");
    cx.simulate_input("feature/x");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    picker.read_with(cx, |picker, _| assert!(!picker.open));
    let requests = bodies(server);
    assert_eq!(requests[2].1["newBranch"], "feature/x");
    assert_eq!(requests[2].1["reference"], Value::Null);
}

#[gpui::test]
fn a_started_conversation_leaves_the_branch_locked(cx: &mut TestAppContext) {
    let (picker, cx, server) = open(
        cx,
        vec![
            (LIST, 200, branches("main", false)),
            (LIST, 200, branches("main", true)),
        ],
    );
    picker.update(cx, |picker, cx| {
        picker.set_state(Some("main".into()), true, cx)
    });
    cx.run_until_parked();
    click(cx, "local-branch-trigger");
    picker.read_with(cx, |picker, _| {
        assert!(picker.locked());
        assert!(!picker.open, "a locked branch does not open");
    });
    assert_eq!(bodies(server).len(), 2);
}

/// The rail's branch is the picker only on the project's own checkout.
#[gpui::test]
fn only_a_local_task_picks_its_branch_under_the_composer(cx: &mut TestAppContext) {
    use crate::data::conversation::ExecutionContext;
    use crate::screens::composer::tests::{open, session};
    let (composer, cx) = open(cx);
    let context = |isolated| ExecutionContext {
        project_label: Some("rosa-app".into()),
        project_icon: None,
        project_color: None,
        project_seed: None,
        base_branch: Some("main".into()),
        current_branch: Some("main".into()),
        isolated,
        setup_report: None,
    };
    let task = |isolated| crate::screens::composer::ComposerSession {
        task_id: Some("w-1".into()),
        context: Some(context(isolated)),
        ..session("a")
    };
    composer.update(cx, |composer, cx| composer.set_session(task(false), cx));
    cx.run_until_parked();
    assert!(cx.debug_bounds("local-branch-trigger").is_some());
    composer.update(cx, |composer, cx| composer.set_session(task(true), cx));
    cx.run_until_parked();
    assert!(composer.read_with(cx, |composer, _| composer.local_branch.is_none()));
    assert!(cx.debug_bounds("local-branch-trigger").is_none());
}

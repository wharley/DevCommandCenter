//! The open-project dialog driven headlessly: a real git checkout for the
//! branches, a fake desktop for the creation.

use std::collections::HashMap;
use std::process::Command;

use dcc_core::application::linked_task::LinkedFolders;
use gpui::{TestAppContext, WindowAppearance};

use super::{OpenProjectDialog, OpenProjectEvent, OpenRequest};
use crate::data::desktop_api::DesktopApi;
use crate::data::tasks::Project;
use crate::test_support::fake_desktop;
use crate::theme::{Theme, ThemePreference};

/// A checkout with `main` and `release`.
fn checkout() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let git = |args: &[&str]| {
        let status = Command::new("git")
            .args(args)
            .current_dir(dir.path())
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .output()
            .unwrap();
        assert!(status.status.success(), "{args:?}");
    };
    git(&["init", "-q", "-b", "main"]);
    git(&["commit", "-q", "--allow-empty", "-m", "first"]);
    git(&["branch", "release"]);
    dir
}

fn project(root: &str, id: &str, branch: &str) -> Project {
    Project {
        id: root.into(),
        root_path: root.into(),
        display_name: id.to_uppercase(),
        icon: None,
        color: None,
        pinned_at: None,
        project_id: id.into(),
        name: id.into(),
        base_branch: branch.into(),
    }
}

fn open(
    cx: &mut TestAppContext,
    projects: Vec<Project>,
    linked: HashMap<String, LinkedFolders>,
    request: OpenRequest,
) -> (
    gpui::Entity<OpenProjectDialog>,
    &mut gpui::VisualTestContext,
) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
    });
    cx.add_window_view(move |window, cx| {
        OpenProjectDialog::new(&projects, &linked, request, window, cx)
    })
}

const CREATED: &str = r#"{"workspaceId":"ws-9","setupReport":{"status":"pending","steps":[],"message":null},"setupHints":[]}"#;

#[gpui::test]
fn one_project_creates_a_task_with_its_branch_and_name(cx: &mut TestAppContext) {
    let repo = checkout();
    let root = repo.path().to_string_lossy().into_owned();
    let (port, server) = fake_desktop(CREATED.into());
    let request = OpenRequest {
        multi: false,
        worktree: true,
        pick_folder: false,
    };
    let (dialog, cx) = open(
        cx,
        vec![project(&root, "app", "release")],
        HashMap::new(),
        request,
    );
    let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let seen = events.clone();
    dialog.update(cx, |dialog, cx| {
        cx.subscribe_self(move |_, event: &OpenProjectEvent, _| {
            seen.borrow_mut().push(event.clone())
        })
        .detach();
        dialog.endpoint = Some(DesktopApi::new(port, "k"));
    });
    cx.run_until_parked();
    dialog.update(cx, |dialog, cx| {
        assert_eq!(
            dialog.model.branches,
            vec!["main".to_string(), "release".to_string()]
        );
        assert_eq!(
            dialog.model.base_branch, "release",
            "the project's base stays"
        );
        dialog.model.worktree = false;
        dialog
            .name
            .update(cx, |name, cx| name.set_text("Pagamentos", cx));
        dialog.submit(cx);
        assert!(dialog.busy);
    });
    cx.run_until_parked();
    let (request, body) = server.join().unwrap();
    assert_eq!(request, "POST /api/v1/workspaces");
    let body: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(body["projectId"], "app");
    assert_eq!(body["workspaceRoot"], root);
    assert_eq!(body["baseBranch"], "release");
    assert_eq!(body["name"], "Pagamentos");
    assert_eq!(body["isolationMode"], "localDirect");
    assert_eq!(
        *events.borrow(),
        vec![OpenProjectEvent::Created("ws-9".into())]
    );
    // The dialog lays out.
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert!(cx.debug_bounds("open-project").is_some());
}

#[gpui::test]
fn linked_projects_make_a_multi_project_task(cx: &mut TestAppContext) {
    let repo = checkout();
    let root = repo.path().to_string_lossy().into_owned();
    let (port, server) = fake_desktop(r#"{"workspaceId":"ws-1","count":2}"#.into());
    let linked = HashMap::from([(
        root.clone(),
        LinkedFolders {
            paths: vec!["/code/api".into()],
            context_paths: Vec::new(),
        },
    )]);
    let request = OpenRequest {
        multi: true,
        worktree: true,
        pick_folder: false,
    };
    let (dialog, cx) = open(
        cx,
        vec![
            project(&root, "app", "main"),
            project("/code/api", "api", "develop"),
        ],
        linked,
        request,
    );
    dialog.update(cx, |dialog, _| {
        dialog.endpoint = Some(DesktopApi::new(port, "k"));
        assert!(dialog.model.adding, "the list starts open");
        assert_eq!(dialog.model.extras, vec!["/code/api".to_string()]);
    });
    cx.run_until_parked();
    // The chips and the open project list lay out.
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert!(cx.debug_bounds("open-project").is_some());
    dialog.update(cx, |dialog, cx| {
        assert!(dialog.model.brings_projects());
        dialog.submit(cx);
    });
    cx.run_until_parked();
    let (request, body) = server.join().unwrap();
    assert_eq!(request, "POST /api/v1/workspace-bundles");
    let body: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(body["name"], "Nova tarefa");
    assert_eq!(body["projects"][0]["workspaceRoot"], root);
    assert_eq!(body["projects"][1]["baseBranch"], "develop");
}

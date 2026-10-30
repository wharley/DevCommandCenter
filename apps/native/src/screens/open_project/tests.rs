//! The open-project dialog driven headlessly: a real git checkout for the
//! branches, a fake desktop for the creation.

use std::collections::HashMap;

use dcc_core::application::linked_task::LinkedFolders;
use gpui::{TestAppContext, WindowAppearance};

use super::{OpenProjectDialog, OpenProjectEvent, OpenRequest};
use crate::data::desktop_api::DesktopApi;
use crate::data::tasks::Project;
use crate::test_support::fake_desktop;
use crate::theme::{Theme, ThemePreference};

/// A checkout with `main` and `release`.
fn checkout() -> tempfile::TempDir {
    let dir = crate::test_support::git_checkout(&[]);
    crate::test_support::git(dir.path(), &["branch", "release"]);
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
        ..OpenRequest::default()
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
        ..OpenRequest::default()
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

#[gpui::test]
fn clone_takes_the_url_and_a_destination(cx: &mut TestAppContext) {
    use super::DialogMode;
    let (port, server) = fake_desktop(CREATED.into());
    let request = OpenRequest {
        mode: DialogMode::Clone,
        worktree: true,
        ..OpenRequest::default()
    };
    let (dialog, cx) = open(
        cx,
        vec![project("/code/app", "app", "main")],
        HashMap::new(),
        request,
    );
    dialog.update(cx, |dialog, cx| {
        dialog.endpoint = Some(DesktopApi::new(port, "k"));
        assert!(!dialog.can_submit(cx));
        dialog.repository_url.update(cx, |input, cx| {
            input.set_text("https://github.com/org/meu-repo.git", cx)
        });
        dialog.clone_destination("/work/Meu Repo".into(), cx);
        assert_eq!(dialog.project_id.read(cx).text(), "meu-repo");
        assert!(dialog.can_submit(cx));
        dialog.submit(cx);
    });
    cx.run_until_parked();
    let (request, body) = server.join().unwrap();
    assert_eq!(request, "POST /api/v1/workspaces/clone");
    let body: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(body["repositoryUrl"], "https://github.com/org/meu-repo.git");
    assert_eq!(body["workspaceRoot"], "/work/Meu Repo");
    assert_eq!(body["projectId"], "meu-repo");
    assert_eq!(body["baseBranch"], "");
    assert_eq!(body["name"], serde_json::Value::Null);
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert!(cx.debug_bounds("open-project").is_some());
}

#[gpui::test]
fn a_project_opens_an_existing_branch_but_not_a_pr(cx: &mut TestAppContext) {
    use super::{RepositoryContext, Start};
    use crate::test_support::fake_desktop_answers;
    let resolution = |kind: &str| {
        serde_json::json!({
            "kind": kind, "url": "u", "provider": "github", "host": "github.com",
            "repository": "org/app", "headBranch": "feat/x", "headSha": "abc",
            "baseBranch": "main", "changeRequestNumber": null, "title": "Feature X",
            "author": null, "state": null, "sourceRepository": null, "isCrossRepository": false
        })
        .to_string()
    };
    let (port, server) = fake_desktop_answers(vec![
        (200, resolution("pull_request")),
        (200, resolution("branch")),
        (200, CREATED.into()),
    ]);
    let request = OpenRequest {
        worktree: true,
        context: Some(RepositoryContext {
            project_id: "app".into(),
            workspace_root: "/code/app".into(),
            label: "Checkout".into(),
        }),
        ..OpenRequest::default()
    };
    let (dialog, cx) = open(
        cx,
        vec![project("/code/app", "app", "main")],
        HashMap::new(),
        request,
    );
    let url = "https://github.com/org/app/tree/feat/x";
    dialog.update(cx, |dialog, cx| {
        dialog.endpoint = Some(DesktopApi::new(port, "k"));
        assert_eq!(
            dialog.start,
            Start::Branch,
            "opened from a project: a branch first"
        );
        assert!(dialog.is_source());
        dialog
            .source_url
            .update(cx, |input, cx| input.set_text(url, cx));
        assert!(!dialog.can_submit(cx), "validate first");
        dialog.validate_source(cx);
    });
    cx.run_until_parked();
    dialog.update(cx, |dialog, cx| {
        assert!(dialog.resolution.is_none(), "a PR goes to the PR hub");
        dialog.validate_source(cx);
    });
    cx.run_until_parked();
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert!(cx.debug_bounds("open-project").is_some());
    dialog.update(cx, |dialog, cx| {
        assert!(dialog.can_submit(cx));
        dialog
            .name
            .update(cx, |input, cx| input.set_text("Feature X", cx));
        dialog.submit(cx);
    });
    cx.run_until_parked();
    let calls = server.join().unwrap();
    assert_eq!(calls[0].0, "POST /api/v1/workspaces/source-url/resolve");
    assert!(calls[0].1.contains(r#""workspaceRoot":"/code/app""#));
    assert_eq!(calls[2].0, "POST /api/v1/workspaces/from-source-url");
    let body: serde_json::Value = serde_json::from_str(&calls[2].1).unwrap();
    assert_eq!(body["projectId"], "app");
    assert_eq!(body["url"], url);
    assert_eq!(body["name"], "Feature X");
}

/// "Também nesta tarefa" keeps its width beside "Adicionar projeto" (it
/// once collapsed to a sliver, one letter per line).
#[gpui::test]
fn the_also_in_task_title_keeps_its_width(cx: &mut TestAppContext) {
    let repo = checkout();
    let root = repo.path().to_string_lossy().into_owned();
    let mut projects = vec![project(&root, "rosa-app", "main")];
    for name in ["meu-lme-app", "checkout", "cmdori", "lme", "painel"] {
        projects.push(project(&format!("/code/{name}"), name, "main"));
    }
    let (_dialog, cx) = open(cx, projects, HashMap::new(), OpenRequest::default());
    cx.run_until_parked();
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    let title = cx.debug_bounds("open-project-also-in-task").unwrap();
    assert!(title.size.width > gpui::px(200.0), "{title:?}");
    assert!(title.size.height < gpui::px(40.0), "{title:?}");
}

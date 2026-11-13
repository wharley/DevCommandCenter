//! The new-task screen driven headlessly against a fake desktop.

use std::collections::HashMap;

use gpui::{TestAppContext, WindowAppearance};

use super::{NewTaskEvent, NewTaskView};
use crate::data::desktop_api::DesktopApi;
use crate::data::tasks::Project;
use crate::test_support::fake_desktop;
use crate::theme::{Theme, ThemePreference};

fn project(id: &str, name: &str) -> Project {
    Project {
        id: format!("/code/{id}"),
        root_path: format!("/code/{id}"),
        display_name: name.into(),
        icon: None,
        color: None,
        pinned_at: None,
        project_id: id.into(),
        name: id.into(),
        base_branch: "main".into(),
        missing: false,
    }
}

fn open(cx: &mut TestAppContext) -> (gpui::Entity<NewTaskView>, &mut gpui::VisualTestContext) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
    });
    cx.add_window_view(|_, _| NewTaskView::new())
}

const CREATED: &str = r#"{"workspaceId":"ws-1","setupReport":{"status":"pending","steps":[],"message":null},"setupHints":[]}"#;

#[gpui::test]
fn picking_a_project_creates_the_task_with_the_choices(cx: &mut TestAppContext) {
    let (port, server) = fake_desktop(CREATED.into());
    let (view, cx) = open(cx);
    let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let seen = events.clone();
    let projects = vec![project("app", "Checkout"), project("api", "API")];
    let counts = HashMap::from([(
        "/code/app".to_string(),
        dcc_core::application::linked_task::LinkedFolders {
            paths: vec!["/code/api".into(), "/notes".into()],
            context_paths: vec!["/notes".into()],
        },
    )]);
    view.update(cx, |view, cx| {
        cx.subscribe_self(move |_, event: &NewTaskEvent, _| seen.borrow_mut().push(event.clone()))
            .detach();
        view.endpoint = Some(DesktopApi::new(port, "k"));
        view.set_projects(&projects, &counts, cx);
        assert!(view.any_linked());
        assert_eq!(view.linked_badge(&projects[0]), Some(2));
        assert_eq!(view.linked_badge(&projects[1]), None);
        // Unticking the worktree works on the checkout itself.
        view.worktree = false;
        view.select_project(&projects[0], cx);
        assert!(view.busy());
        // One creation at a time.
        view.select_project(&projects[1], cx);
    });
    cx.run_until_parked();
    let (request, body) = server.join().unwrap();
    assert_eq!(request, "POST /api/v1/workspaces/create");
    let body: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(body["projectId"], "app");
    assert_eq!(body["workspaceRoot"], "/code/app");
    assert_eq!(body["isolationMode"], "localDirect");
    assert_eq!(body["includeLinkedFolders"], true);
    assert_eq!(body["bundleName"], "Nova tarefa");
    view.read_with(cx, |view, _| assert!(!view.busy()));
    assert_eq!(*events.borrow(), vec![NewTaskEvent::Created("ws-1".into())]);
}

#[gpui::test]
fn the_screen_lays_out_with_its_shortcuts(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |view, cx| {
        let projects: Vec<Project> = (0..6)
            .map(|index| project(&format!("p{index}"), "P"))
            .collect();
        view.set_projects(&projects, &HashMap::new(), cx);
        assert!(!view.any_linked(), "no linked folders: no second checkbox");
    });
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert!(cx.debug_bounds("new-task").is_some());
}

#[gpui::test]
fn a_gone_folder_starts_nothing_and_offers_its_removal(cx: &mut TestAppContext) {
    use gpui::Modifiers;
    let (view, cx) = open(cx);
    let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let seen = events.clone();
    let gone = Project {
        missing: true,
        ..project("gone", "Apagado")
    };
    view.update(cx, |view, cx| {
        cx.subscribe_self(move |_, event: &NewTaskEvent, _| seen.borrow_mut().push(event.clone()))
            .detach();
        view.set_projects(&[gone.clone(), project("app", "App")], &HashMap::new(), cx);
        // No task starts in a folder that is gone.
        view.select_project(&gone, cx);
        assert!(!view.busy());
        view.menu_open = true;
    });
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    // Its ✕ is there without hovering; it asks the shell to confirm.
    let bounds = cx
        .debug_bounds("new-task-remove-project-0")
        .expect("the gone folder's ✕");
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
    assert_eq!(
        *events.borrow(),
        vec![NewTaskEvent::RemoveProject("/code/gone".into())]
    );
    view.read_with(cx, |view, _| assert!(!view.menu_open));
}

/// Opening the screen fetches each project's base once, in the background;
/// opening it again right away fetches nothing.
#[gpui::test]
fn opening_the_screen_fetches_each_projects_base_once(cx: &mut TestAppContext) {
    let ok = (200, r#"{"ok":true}"#.to_string());
    let (port, server) = crate::test_support::fake_desktop_answers(vec![ok.clone(), ok]);
    let (view, cx) = open(cx);
    let mut gone = project("old", "Antigo");
    gone.missing = true;
    let projects = vec![project("app", "Checkout"), project("api", "API"), gone];
    view.update(cx, |view, cx| {
        view.endpoint = Some(DesktopApi::new(port, "k"));
        view.set_projects(&projects, &HashMap::new(), cx);
        view.fetch_bases(cx);
        view.fetch_bases(cx);
    });
    cx.run_until_parked();
    let requests = server.join().unwrap();
    let bodies: Vec<serde_json::Value> = requests
        .iter()
        .map(|(line, body)| {
            assert_eq!(line, "POST /api/v1/git/fetch-base");
            serde_json::from_str(body).unwrap()
        })
        .collect();
    assert_eq!(
        bodies,
        vec![
            serde_json::json!({ "repositoryRoot": "/code/app", "baseBranch": "main" }),
            serde_json::json!({ "repositoryRoot": "/code/api", "baseBranch": "main" }),
        ]
    );
}

#[gpui::test]
fn a_task_on_a_machine_starts_in_one_of_its_projects(cx: &mut TestAppContext) {
    use crate::test_support::fake_desktop_answers;
    let machines = r#"{"machines":[{"id":"m-1","name":"Servidor","fingerprint":"AAAA-BBBB-CCCC","createdAt":"2026-11-13","connected":true}]}"#;
    let catalog = r#"{"providers":[],"workspaces":[],"repositories":[{"id":"r-9","projectId":"p-9","name":"vendeagora-app","displayName":null,"icon":null,"color":null,"logo":null,"pinnedAt":null,"rootPath":"/srv/vendeagora-app","baseBranch":"main","remote":null,"remoteUrl":null,"forgeProvider":null,"forgeLogin":null,"createdAt":"2026-11-01","updatedAt":"2026-11-01"}]}"#;
    let (port, server) = fake_desktop_answers(vec![
        (200, machines.into()),
        (200, catalog.into()),
        (200, CREATED.into()),
    ]);
    let (view, cx) = open(cx);
    let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let seen = events.clone();
    view.update(cx, |view, cx| {
        cx.subscribe_self(move |_, event: &NewTaskEvent, _| seen.borrow_mut().push(event.clone()))
            .detach();
        view.endpoint = Some(DesktopApi::new(port, "k"));
        view.set_projects(&[project("app", "Checkout")], &HashMap::new(), cx);
        view.load_machines(cx);
    });
    cx.run_until_parked();
    // "Onde" shows with a machine paired; picking it lists its projects.
    assert!(cx.debug_bounds("new-task-where").is_some());
    view.update(cx, |view, cx| {
        assert_eq!(
            view.projects().len(),
            1,
            "this computer's until a machine is picked"
        );
        view.set_place(Some("m-1".into()), cx);
        assert!(view.projects().is_empty(), "the machine's are being read");
    });
    cx.run_until_parked();
    let remote = view.read_with(cx, |view, _| view.projects().to_vec());
    assert_eq!(remote.len(), 1);
    assert_eq!(remote[0].root_path, "/srv/vendeagora-app");
    assert!(
        !remote[0].missing,
        "a machine's folder is not looked for here"
    );
    view.update(cx, |view, cx| view.select_project(&remote[0], cx));
    cx.run_until_parked();
    let requests = server.join().unwrap();
    let asked: Vec<&str> = requests
        .iter()
        .map(|(request, _)| request.as_str())
        .collect();
    assert_eq!(
        asked,
        [
            "GET /api/v1/machines",
            "GET /api/v1/machines/m-1/proxy/api/v1/mobile/catalog",
            "POST /api/v1/machines/m-1/tasks",
        ]
    );
    let body: serde_json::Value = serde_json::from_str(&requests[2].1).unwrap();
    assert_eq!(body["workspaceRoot"], "/srv/vendeagora-app");
    assert_eq!(body["projectId"], "p-9");
    assert_eq!(body["isolationMode"], "protectedWorktree");
    assert_eq!(*events.borrow(), vec![NewTaskEvent::Created("ws-1".into())]);
}

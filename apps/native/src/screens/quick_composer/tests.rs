//! The quick composer against a fake desktop and a real database: no
//! projects, the project and its environment, a launch another run already
//! holds showing the prompt kept, and releasing it after the check.

use gpui::{TestAppContext, VisualTestContext, WindowAppearance, px, size};
use serde_json::json;

use super::QuickComposer;
use crate::data::desktop_api::DesktopApi;
use crate::theme::{Theme, ThemePreference};

fn provider() -> dcc_core::domain::provider::ProviderDescriptor {
    serde_json::from_value(json!({
        "id": "codex", "label": "Codex", "description": "",
        "models": [{ "id": "gpt", "label": "GPT", "description": "", "recommended": true,
            "effortLevels": ["low", "high"] }],
        "capabilities": { "streaming": true, "mcpSupport": "unsupported", "tools": true, "vision": true,
            "resumable": true, "experimental": false, "canBeDelegationTarget": true,
            "canRequestDelegation": true, "supportsReadOnlyDelegation": true,
            "supportsEditDelegation": true },
        "health": "Healthy", "stable": true
    }))
    .unwrap()
}

/// A database holding the project `project` at `root`.
fn database(dir: &std::path::Path) -> std::path::PathBuf {
    let db = crate::test_support::task_database(dir, "w-old", dir, None);
    drop(dcc_infra::db::SqliteSessionRepo::open(&db).unwrap());
    let repo = dcc_infra::db::SqliteWorkspaceRepo::open(&db).unwrap();
    futures::executor::block_on(dcc_core::ports::RepositoryRepo::save_repository(
        &repo,
        &dcc_core::domain::repository::Repository {
            id: dcc_core::domain::repository::RepositoryId("r-1".into()),
            project_id: dcc_core::domain::project::ProjectId("project".into()),
            name: "app".into(),
            display_name: Some("Meu app".into()),
            icon: None,
            color: None,
            logo: None,
            pinned_at: None,
            root_path: dir.to_string_lossy().into_owned(),
            base_branch: "main".into(),
            remote: None,
            remote_url: None,
            forge_provider: None,
            forge_login: None,
            created_at: "2026-10-28T00:00:00Z".into(),
            updated_at: "2026-10-28T00:00:00Z".into(),
        },
    ))
    .unwrap();
    db
}

fn open(cx: &mut TestAppContext) -> (gpui::Entity<QuickComposer>, &mut VisualTestContext) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::text_area::bind_keys(cx);
        crate::ui::text_input::bind_keys(cx);
    });
    let (quick, cx) = cx.add_window_view(QuickComposer::new);
    cx.simulate_resize(size(px(720.0), px(440.0)));
    (quick, cx)
}

#[gpui::test]
fn without_projects_it_points_to_the_main_window(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let db = crate::test_support::task_database(dir.path(), "w-old", dir.path(), None);
    drop(dcc_infra::db::SqliteSessionRepo::open(&db).unwrap());
    let (quick, cx) = open(cx);
    quick.update_in(cx, |quick, window, cx| {
        quick.db_path = db;
        quick.refresh(window, cx);
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("quick-no-projects").is_some());
    assert!(quick.read_with(cx, |quick, cx| quick.blocked(cx)));
}

#[gpui::test]
fn a_held_launch_keeps_its_prompt_until_released(cx: &mut TestAppContext) {
    use crate::test_support::fake_desktop_routes;
    let dir = tempfile::tempdir().unwrap();
    let db = database(dir.path());
    let held = json!({
        "id": "l-1", "revision": 4, "phase": "sending",
        "request": { "projectId": "project", "rootPath": "/repo", "baseBranch": "main",
            "isolationMode": "protectedWorktree", "providerId": "codex", "modelId": "gpt",
            "providerRuntime": null, "turn": { "rawPrompt": "Corrija o login", "envelope": {} } },
        "workspaceId": "w-1", "sessionId": null, "error": null
    });
    let released = {
        let mut released = held.clone();
        released["revision"] = json!(5);
        released["phase"] = json!("failed");
        released["error"] = json!("interrompido");
        released
    };
    let (port, desktop) = fake_desktop_routes(vec![
        (
            "/api/v1/quick-composer/status",
            200,
            json!({ "supported": true, "shortcut": null, "shortcutError": false, "launch": null })
                .to_string(),
        ),
        ("/api/v1/quick-composer/begin", 200, held.to_string()),
        (
            "/api/v1/quick-composer/checkpoint",
            200,
            released.to_string(),
        ),
    ]);
    let (quick, cx) = open(cx);
    quick.update_in(cx, |quick, window, cx| {
        quick.db_path = db;
        quick.api = Some(DesktopApi::new(port, "k"));
        quick.composer.update(cx, |composer, _| {
            composer.catalog = Some(std::sync::Arc::new(vec![provider()]));
            composer.catalog_at = Some(std::time::Instant::now());
        });
        quick.refresh(window, cx);
    });
    cx.run_until_parked();
    // The project, Worktree by default, and something to run it on.
    let (project, mode, blocked) = quick.read_with(cx, |quick, cx| {
        (
            quick.project().map(|project| project.display_name.clone()),
            quick.mode(),
            quick.blocked(cx),
        )
    });
    assert_eq!(
        (project.as_deref(), mode, blocked),
        (Some("Meu app"), "protectedWorktree", false)
    );
    assert!(cx.debug_bounds("quick-no-projects").is_none());
    // Local for this project.
    let bounds = cx.debug_bounds("quick-local").unwrap();
    cx.simulate_click(bounds.center(), gpui::Modifiers::none());
    assert_eq!(quick.read_with(cx, |quick, _| quick.mode()), "localDirect");
    // Another run holds a launch: the send stops there, the prompt kept.
    quick.update_in(cx, |quick, window, cx| {
        quick.composer.update(cx, |composer, cx| {
            composer
                .input
                .update(cx, |input, cx| input.set_text("Outra coisa", cx));
            composer.send(window, cx);
        });
    });
    cx.run_until_parked();
    let (busy, unfinished) = quick.read_with(cx, |quick, _| (quick.busy, quick.unfinished()));
    assert!(!busy && unfinished);
    assert!(cx.debug_bounds("quick-launch").is_some());
    assert!(cx.debug_bounds("quick-recover").is_some());
    // "Já conferi a tarefa": released, a new one may go.
    let bounds = cx.debug_bounds("quick-recover").unwrap();
    cx.simulate_click(bounds.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    let seen = desktop.join().unwrap();
    assert!(!quick.read_with(cx, |quick, _| quick.unfinished()));
    let (_, begin) = seen
        .iter()
        .find(|(path, _)| path.ends_with("/begin"))
        .unwrap();
    let begin: serde_json::Value = serde_json::from_str(begin).unwrap();
    assert_eq!(begin["request"]["turn"]["rawPrompt"], "Outra coisa");
    assert_eq!(begin["request"]["isolationMode"], "localDirect");
    assert_eq!(begin["request"]["providerId"], "codex");
    let (_, checkpoint) = seen
        .iter()
        .find(|(path, _)| path.ends_with("/checkpoint"))
        .unwrap();
    let checkpoint: serde_json::Value = serde_json::from_str(checkpoint).unwrap();
    assert_eq!(
        (
            checkpoint["phase"].as_str(),
            checkpoint["revision"].as_u64()
        ),
        (Some("failed"), Some(5))
    );
}

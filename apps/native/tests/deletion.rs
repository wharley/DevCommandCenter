//! Deleting from the native app, end to end on a throwaway repository: the
//! desktop's lock is respected, and a completed task loses its worktree and
//! its row.

use std::path::Path;
use std::process::Command;

use dcc_core::domain::project::ProjectId;
use dcc_core::domain::workspace::{Workspace, WorkspaceId, WorkspaceState};
use dcc_core::ports::WorkspaceRepo;
use dcc_infra::db::SqliteWorkspaceRepo;
use dcc_infra::runtime_owner::RuntimeOwner;
use dcc_native::data::deletion::{self, DeleteError, DeleteTask};
use futures::executor::block_on;

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(["-c", "user.name=DCC", "-c", "user.email=dcc@example.com"])
        .args(args)
        .current_dir(dir)
        .status()
        .expect("git");
    assert!(status.success(), "git {args:?}");
}

#[test]
fn a_completed_task_is_deleted_only_while_the_desktop_is_closed() {
    let root = tempfile::tempdir().unwrap();
    let repo_dir = root.path().join("repo");
    std::fs::create_dir_all(&repo_dir).unwrap();
    git(&repo_dir, &["init", "-q", "-b", "main"]);
    std::fs::write(repo_dir.join("README.md"), "hi\n").unwrap();
    git(&repo_dir, &["add", "."]);
    git(&repo_dir, &["commit", "-q", "-m", "init"]);
    let worktree = root.path().join(".dcc-worktrees").join("task");
    git(
        &repo_dir,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "task",
            worktree.to_str().unwrap(),
        ],
    );

    let app = root.path().join("app");
    std::fs::create_dir_all(&app).unwrap();
    std::fs::write(
        app.join("ai-memory-settings.json"),
        r#"{"mode":"disabled"}"#,
    )
    .unwrap();
    let db = app.join("database.sqlite");
    let repo = SqliteWorkspaceRepo::open(&db).unwrap();
    let id = WorkspaceId("t1".into());
    let now = "2026-10-23T12:00:00Z".to_string();
    block_on(repo.save_workspace(&Workspace {
        id: id.clone(),
        project_id: ProjectId("p".into()),
        name: Some("Task".into()),
        root_path: repo_dir.to_string_lossy().into(),
        base_branch: "main".into(),
        worktree_path: Some(worktree.to_string_lossy().into()),
        source: None,
        state: WorkspaceState::Completed,
        setup_report: None,
        pinned_at: None,
        snoozed_until: None,
        created_at: now.clone(),
        updated_at: now,
    }))
    .unwrap();
    let request = DeleteTask {
        task_id: "t1".into(),
        ..DeleteTask::default()
    };

    let desktop = RuntimeOwner::try_acquire(&app).unwrap().expect("lock");
    assert_eq!(
        deletion::delete_task(&db, &request),
        Err(DeleteError::DesktopOpen)
    );
    assert!(worktree.exists());
    drop(desktop);

    deletion::delete_task(&db, &request).expect("delete");
    assert!(!worktree.exists());
    assert!(block_on(repo.get_workspace(&id)).unwrap().is_none());
    // The lock is free again for the desktop.
    assert!(RuntimeOwner::try_acquire(&app).unwrap().is_some());
}

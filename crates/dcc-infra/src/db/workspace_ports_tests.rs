use std::cell::RefCell;

use dcc_core::domain::workspace::{Workspace, WorkspaceState};

use super::*;

fn repo() -> SqliteWorkspaceRepo {
    let conn = Connection::open_in_memory().expect("memory");
    SqliteWorkspaceRepo::from_connection(Arc::new(Mutex::new(conn))).expect("repo")
}

fn task(repo: &SqliteWorkspaceRepo, id: &str, state: WorkspaceState) -> WorkspaceId {
    let workspace = Workspace {
        id: WorkspaceId(id.to_string()),
        project_id: ProjectId("p".to_string()),
        name: Some(id.to_string()),
        root_path: "/tmp/p".to_string(),
        base_branch: "main".to_string(),
        worktree_path: Some(format!("/tmp/p-{id}")),
        source: None,
        state,
        setup_report: None,
        pinned_at: None,
        snoozed_until: None,
        created_at: "2026-11-04T10:00:00Z".to_string(),
        updated_at: "2026-11-04T10:00:00Z".to_string(),
    };
    futures::executor::block_on(repo.save_workspace(&workspace)).unwrap();
    workspace.id
}

#[test]
fn each_task_keeps_its_own_block() {
    let repo = repo();
    let a = task(&repo, "a", WorkspaceState::Ready);
    let b = task(&repo, "b", WorkspaceState::Ready);

    let first = repo.workspace_port_base_with(&a, |_| true).unwrap();
    let second = repo.workspace_port_base_with(&b, |_| true).unwrap();

    assert_eq!(first, PORT_BLOCK_START);
    assert_eq!(second, PORT_BLOCK_START + PORT_BLOCK_SIZE);
    // Asked again, even with every port busy (its own server is running).
    assert_eq!(repo.workspace_port_base_with(&a, |_| false).unwrap(), first);
}

#[test]
fn a_busy_block_is_skipped_and_a_deleted_task_gives_its_block_back() {
    let repo = repo();
    let a = task(&repo, "a", WorkspaceState::Ready);
    let tried = RefCell::new(Vec::new());
    let base = repo
        .workspace_port_base_with(&a, |base| {
            tried.borrow_mut().push(base);
            base != PORT_BLOCK_START
        })
        .unwrap();
    assert_eq!(base, PORT_BLOCK_START + PORT_BLOCK_SIZE);
    assert_eq!(tried.borrow()[0], PORT_BLOCK_START);

    futures::executor::block_on(repo.delete_workspace(&a)).unwrap();
    let b = task(&repo, "b", WorkspaceState::Ready);
    let c = task(&repo, "c", WorkspaceState::Ready);
    repo.workspace_port_base_with(&b, |_| true).unwrap();
    assert_eq!(
        repo.workspace_port_base_with(&c, |_| true).unwrap(),
        PORT_BLOCK_START + PORT_BLOCK_SIZE
    );
}

#[test]
fn a_finished_task_block_goes_to_the_next_task() {
    let repo = repo();
    let done = task(&repo, "done", WorkspaceState::Completed);
    repo.workspace_port_base_with(&done, |_| true).unwrap();
    let next = task(&repo, "next", WorkspaceState::Ready);

    assert_eq!(
        repo.workspace_port_base_with(&next, |_| true).unwrap(),
        PORT_BLOCK_START
    );
    // Reopened, the finished task gets a fresh block.
    assert_eq!(
        repo.workspace_port_base_with(&done, |_| true).unwrap(),
        PORT_BLOCK_START + PORT_BLOCK_SIZE
    );
}

//! Tasks a resident agent opens from its own conversation (`dcc_open_task`):
//! the person names a project, the agent writes the brief, and the task is a
//! task of its own there — in the task list like one made with "Nova tarefa",
//! a protected worktree unless the person wants the project's checkout. It
//! rides on the task proposal (`delegation_runtime::propose_task`), so the
//! card, starting it and the `[DCC]` status back to the conversation work as
//! for any proposal; only where the task lives is different.

use std::path::Path;

use dcc_core::application::{CreateWorkspaceForRepoInput, WorkspaceIsolationMode};
use dcc_core::domain::agent_conversation::NewTaskTarget;
use dcc_core::domain::delegation::Delegation;
use dcc_core::domain::project::ProjectId;
use dcc_core::domain::session::SessionId;
use dcc_core::domain::workspace::Workspace;
use dcc_infra::db::SqliteWorkspaceRepo;
use rusqlite::{Connection, OpenFlags};

use crate::app_events::AppEvents;
use crate::delegation_runtime::{propose_task, ProposeTaskInput};
use crate::state::SessionCommandState;

/// The efforts a task's first turn may run with.
pub const EFFORTS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];

#[derive(Clone, Debug)]
pub struct OpenTaskInput {
    pub parent_session_id: SessionId,
    /// The project's name as `dcc_overview` shows it, or its id.
    pub project: String,
    pub title: String,
    pub instruction: String,
    pub worktree: bool,
    pub target_provider_id: Option<String>,
    pub target_model_id: Option<String>,
    pub effort: Option<String>,
    /// Start without a card; only for a turn the person started.
    pub start_now: bool,
}

/// The person's project called `project` (its name, any case, or its id);
/// homes and ideas are not projects. A project with several repositories
/// opens in the first one added.
pub fn find_project(
    db_path: &Path,
    project: &str,
    worktree: bool,
    effort: Option<String>,
) -> Result<NewTaskTarget, String> {
    let conn = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|error| error.to_string())?;
    let mut statement = conn
        .prepare(
            "SELECT project_id, COALESCE(NULLIF(display_name, ''), name), root_path, base_branch
             FROM dcc_repositories
             WHERE project_id NOT LIKE 'dcc-agent-%'
             AND root_path NOT IN (SELECT root_path FROM dcc_ideas)
             ORDER BY created_at",
        )
        .map_err(|error| error.to_string())?;
    let projects = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .and_then(Iterator::collect::<Result<Vec<_>, _>>)
        .map_err(|error| error.to_string())?;
    let wanted = project.trim();
    let found = projects.iter().find(|(id, ..)| id == wanted).or_else(|| {
        projects
            .iter()
            .find(|(_, label, ..)| label.trim().eq_ignore_ascii_case(wanted))
    });
    let Some((project_id, label, root_path, base_branch)) = found else {
        let mut names: Vec<&str> = projects
            .iter()
            .map(|(_, label, ..)| label.as_str())
            .collect();
        names.sort_unstable_by_key(|name| name.to_lowercase());
        names.dedup();
        return Err(format!(
            "no project named \"{wanted}\"; the person's projects are: {}",
            names.join(", ")
        ));
    };
    Ok(NewTaskTarget {
        project_id: project_id.clone(),
        project: label.clone(),
        root_path: root_path.clone(),
        base_branch: base_branch.clone(),
        worktree,
        effort,
        workspace_id: None,
    })
}

/// Proposes (or, with `start_now`, starts) a task in one of the person's
/// projects. Only a resident agent's own conversation opens tasks this way.
pub async fn open_task(
    state: &SessionCommandState,
    input: OpenTaskInput,
) -> Result<Delegation, String> {
    if !state
        .is_agent_conversation(&input.parent_session_id)
        .map_err(|error| error.to_string())?
    {
        return Err("dcc_open_task only works in a resident agent's own conversation; in a task, use dcc_propose_task".to_string());
    }
    let effort = input
        .effort
        .map(|effort| effort.trim().to_lowercase())
        .filter(|effort| !effort.is_empty());
    if let Some(effort) = &effort {
        if !EFFORTS.contains(&effort.as_str()) {
            return Err(format!("effort must be one of {}", EFFORTS.join(", ")));
        }
    }
    let db_path = state.db_path().to_path_buf();
    let project = input.project.clone();
    let worktree = input.worktree;
    let target =
        tokio::task::spawn_blocking(move || find_project(&db_path, &project, worktree, effort))
            .await
            .map_err(|error| error.to_string())??;
    propose_task(
        state,
        ProposeTaskInput {
            parent_session_id: input.parent_session_id,
            title: input.title,
            instruction: input.instruction,
            edits: true,
            target_provider_id: input.target_provider_id,
            target_model_id: input.target_model_id,
            start_now: input.start_now,
            new_task: Some(target),
        },
    )
    .await
}

/// The task a started proposal becomes, made like "Nova tarefa" makes one:
/// on the project's base branch, in a protected worktree or its checkout.
pub async fn create_task_workspace(
    state: &SessionCommandState,
    target: &NewTaskTarget,
    name: &str,
) -> Result<Workspace, String> {
    let repo = SqliteWorkspaceRepo::open(state.db_path()).map_err(|error| error.to_string())?;
    let input = CreateWorkspaceForRepoInput {
        project_id: ProjectId(target.project_id.clone()),
        workspace_root: target.root_path.clone(),
        base_branch: target.base_branch.clone(),
        name: Some(name.trim().to_string()).filter(|name| !name.is_empty()),
        isolation_mode: Some(if target.worktree {
            WorkspaceIsolationMode::ProtectedWorktree
        } else {
            WorkspaceIsolationMode::LocalDirect
        }),
        context_folders: Vec::new(),
    };
    // The app reads tasks from the API, so nothing listens for these events.
    crate::commands::workspace_commands::create_workspace_for_repo_with_repo(
        &repo,
        &AppEvents::silent(),
        input,
    )
    .await
    .map(|output| output.workspace)
}

/// A task made for a proposal that then failed to start goes to the
/// archive instead of sitting empty in the list; archiving keeps it whole.
pub async fn set_aside_unstarted_task(state: &SessionCommandState, workspace_id: &str) {
    let workspace = crate::state::WorkspaceCommandState::from_session(state);
    let input = crate::commands::workspace_commands::WorkspaceIdInput {
        workspace_id: workspace_id.to_string(),
    };
    if let Err(error) =
        crate::commands::workspace_commands::archive_workspace_with_state(&workspace, input).await
    {
        eprintln!("[DCC] could not archive the task that failed to start: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(root: &Path, args: &[&str]) {
        let output = std::process::Command::new("git")
            .args(["-c", "user.name=t", "-c", "user.email=t@t"])
            .args(args)
            .current_dir(root)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn input(project: &str) -> OpenTaskInput {
        OpenTaskInput {
            parent_session_id: SessionId("parent".into()),
            project: project.into(),
            title: "Fase 2".into(),
            instruction: "Implement it.".into(),
            worktree: true,
            target_provider_id: None,
            target_model_id: None,
            effort: None,
            start_now: false,
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn only_an_agents_own_conversation_opens_tasks_and_they_are_real_tasks() {
        let root = tempfile::tempdir().unwrap();
        let physical = std::fs::canonicalize(root.path()).unwrap();
        let state = SessionCommandState::new_headless(
            physical.join("state.sqlite"),
            physical.join("app-data"),
        );
        let error = open_task(&state, input("Checkout")).await.unwrap_err();
        assert!(error.contains("own conversation"), "{error}");

        let repo = physical.join("loja");
        std::fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        git(&repo, &["commit", "-q", "--allow-empty", "-m", "first"]);
        let repo = repo.to_string_lossy().into_owned();
        let mut target = NewTaskTarget {
            project_id: "p-loja".into(),
            project: "Checkout".into(),
            root_path: repo.clone(),
            base_branch: "main".into(),
            worktree: true,
            effort: None,
            workspace_id: None,
        };
        let task = create_task_workspace(&state, &target, " Fase 2 ")
            .await
            .unwrap();
        assert_eq!(task.project_id.0, "p-loja");
        assert_eq!(task.name.as_deref(), Some("Fase 2"));
        let worktree = task.worktree_path.expect("a protected worktree");
        assert_ne!(worktree, repo);
        assert!(Path::new(&worktree).is_dir());

        target.worktree = false;
        let local = create_task_workspace(&state, &target, "Direto")
            .await
            .unwrap();
        assert_eq!(local.worktree_path, None);
        assert_eq!(local.root_path, repo);
    }

    #[test]
    fn a_project_is_found_by_name_or_id_and_never_a_home_or_an_idea() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("db.sqlite");
        let conn = Connection::open(&db).unwrap();
        conn.execute_batch(
            "CREATE TABLE dcc_repositories (project_id TEXT, display_name TEXT, name TEXT, root_path TEXT, base_branch TEXT, created_at TEXT);
            CREATE TABLE dcc_ideas (root_path TEXT);
            INSERT INTO dcc_repositories VALUES ('p-loja', 'Checkout', 'loja', '/loja', 'main', '1');
            INSERT INTO dcc_repositories VALUES ('p-loja', NULL, 'loja-api', '/loja-api', 'develop', '2');
            INSERT INTO dcc_repositories VALUES ('p-painel', NULL, 'painel', '/painel', 'main', '3');
            INSERT INTO dcc_repositories VALUES ('dcc-agent-a', NULL, 'Revisor', '/home/a', 'main', '4');
            INSERT INTO dcc_repositories VALUES ('idea', NULL, 'ideia', '/ideia', 'main', '5');
            INSERT INTO dcc_ideas VALUES ('/ideia');",
        )
        .unwrap();
        let found = find_project(&db, " checkout ", true, Some("high".into())).unwrap();
        assert_eq!(found.project_id, "p-loja");
        assert_eq!(found.project, "Checkout");
        assert_eq!(found.root_path, "/loja");
        assert_eq!(found.base_branch, "main");
        assert!(found.worktree);
        assert_eq!(found.effort.as_deref(), Some("high"));
        assert_eq!(
            find_project(&db, "p-painel", false, None).unwrap().project,
            "painel"
        );
        for missing in ["Revisor", "ideia", "nada"] {
            let error = find_project(&db, missing, true, None).unwrap_err();
            assert!(error.contains("Checkout, loja-api, painel"), "{error}");
        }
    }
}

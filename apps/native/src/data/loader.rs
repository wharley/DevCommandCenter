//! Reads everything the sidebar shows from the database the desktop app
//! writes, through the same `dcc-infra` repositories. Blocking: call it off
//! the UI thread.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use dcc_core::domain::delegation::DelegationStatus;
use dcc_core::domain::workspace::WorkspaceId;
use dcc_core::ports::{DelegationRepo, RepositoryRepo, WorkspaceBundleRepo, WorkspaceRepo};
use dcc_infra::db::{SqliteSessionRepo, SqliteWorkspaceRepo};
use futures::executor::block_on;

use dcc_core::domain::agent_conversation::home_agent_id;

use super::activity::{TaskActivity, provider_id, task_activity};
use super::tasks::{Project, TaskSummary, fold_bundles};

/// Same folder as the desktop: `DCC_APP_DATA_DIR`, else the app's data dir
/// (`com.devcommandcenter.app`, `.dev` in debug builds like `yarn dev`).
pub fn database_path() -> PathBuf {
    if let Some(dir) = std::env::var_os("DCC_APP_DATA_DIR") {
        return PathBuf::from(dir).join("database.sqlite");
    }
    let identifier = if cfg!(debug_assertions) {
        "com.devcommandcenter.app.dev"
    } else {
        "com.devcommandcenter.app"
    };
    dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join(identifier)
        .join("database.sqlite")
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SidebarData {
    /// Tasks as the rail lists them, newest update first; idea tasks excluded.
    pub tasks: Vec<TaskSummary>,
    /// Projects, the agents' homes excluded.
    pub projects: Vec<Project>,
    pub activities: HashMap<String, TaskActivity>,
    pub provider_ids: HashMap<String, String>,
    /// Notes still open, across projects (the rail's Notes count).
    pub open_notes: usize,
    /// Delegated edits waiting for the person to apply or discard, by task.
    pub review_pending: HashMap<String, usize>,
    /// Resident agents with their sessions' state.
    pub agents: Vec<super::agents::AgentRecord>,
    /// Each agent's own conversation task (its home), by agent id; not in
    /// `tasks` nor `projects`.
    pub agent_homes: HashMap<String, String>,
    /// Tasks an agent proposed or started, by the task they hang under.
    pub child_tasks: HashMap<String, Vec<super::child_tasks::ChildTask>>,
    /// The folders each project links, by repository id, and which of them
    /// have no Git (they ride along as context).
    pub linked_folders: HashMap<String, dcc_core::application::linked_task::LinkedFolders>,
    /// Pull requests an agent asked to review that the app has not taken.
    pub review_requests: Vec<dcc_core::domain::pr_review_request::PrReviewRequest>,
    /// The agent each task belongs to (`task_agents`), by task id.
    pub task_agents: HashMap<String, String>,
    /// Agents an agent proposed that the person has not answered.
    pub agent_proposals: Vec<dcc_core::domain::agent_proposal::AgentProposal>,
}

/// One row per project, by name (`listRepositories` folded by
/// `projectId`): for the connectors switched off per project.
pub fn project_names(db_path: &Path) -> Vec<(String, String)> {
    let Ok(workspaces) = (db_path.exists())
        .then(|| SqliteWorkspaceRepo::open(db_path))
        .transpose()
    else {
        return Vec::new();
    };
    let Some(workspaces) = workspaces else {
        return Vec::new();
    };
    let mut projects: Vec<(String, String)> = Vec::new();
    for repository in block_on(workspaces.list_repositories()).unwrap_or_default() {
        if home_agent_id(&repository.project_id.0).is_some() {
            continue;
        }
        if !projects
            .iter()
            .any(|(id, _)| *id == repository.project_id.0)
        {
            let name = repository
                .display_name
                .clone()
                .filter(|name| !name.trim().is_empty())
                .unwrap_or_else(|| repository.name.clone());
            projects.push((repository.project_id.0.clone(), name));
        }
    }
    projects.sort_by_key(|(_, name)| name.to_lowercase());
    projects
}

pub fn load(db_path: &Path, untitled: &str) -> anyhow::Result<SidebarData> {
    if !db_path.exists() {
        return Ok(SidebarData::default());
    }
    let workspaces = SqliteWorkspaceRepo::open(db_path)?;
    let sessions = SqliteSessionRepo::open_read_only(db_path)?;

    let projects: Vec<Project> = block_on(workspaces.list_repositories())?
        .iter()
        .filter(|repository| home_agent_id(&repository.project_id.0).is_none())
        .map(|repository| Project {
            missing: !std::path::Path::new(&repository.root_path).is_dir(),
            ..Project::from_repository(repository)
        })
        .collect();
    let mut context_folders: HashMap<String, Vec<String>> = workspaces
        .list_workspace_context_folders()?
        .into_iter()
        .map(|(id, folders)| (id.0, folders))
        .collect();
    let tasks: Vec<TaskSummary> = block_on(workspaces.list_workspaces())?
        .iter()
        .map(|workspace| {
            let folders = context_folders.remove(&workspace.id.0).unwrap_or_default();
            TaskSummary::from_workspace(workspace, folders, untitled)
        })
        .collect();
    let agent_homes: HashMap<String, String> = tasks
        .iter()
        .filter(|task| task.status.is_selectable())
        .filter_map(|task| {
            Some((
                home_agent_id(&task.project_id)?.to_string(),
                task.id.clone(),
            ))
        })
        .collect();
    let bundles = block_on(workspaces.list_workspace_bundles())?;
    let tasks: Vec<TaskSummary> = fold_bundles(tasks, &bundles, &projects)
        .into_iter()
        .filter(|task| home_agent_id(&task.project_id).is_none())
        .collect();

    let delegations = block_on(sessions.list_delegations(None, None))?;
    // A task an agent opened in a project is that task's own conversation.
    let delegated: HashSet<String> = delegations
        .iter()
        .filter(|delegation| delegation.budget.new_task.is_none())
        .filter_map(|delegation| delegation.child_session_id.as_ref().map(|id| id.0.clone()))
        .collect();
    let child_tasks = super::child_tasks::by_workspace(&delegations);
    let mut review_pending: HashMap<String, usize> = HashMap::new();
    for delegation in &delegations {
        if delegation.status == DelegationStatus::ReviewPending {
            *review_pending
                .entry(delegation.workspace_id.0.clone())
                .or_default() += 1;
        }
    }
    let mut activities = HashMap::new();
    let mut provider_ids = HashMap::new();
    for task in &tasks {
        let summaries = sessions.list_workspace_sessions(&WorkspaceId(task.id.clone()))?;
        if let Some(provider) = provider_id(&summaries, &delegated) {
            provider_ids.insert(task.id.clone(), provider);
        }
        // Like the desktop, only open tasks are followed live.
        if !task.status.is_selectable() {
            continue;
        }
        if let Some(activity) = task_activity(&summaries) {
            activities.insert(task.id.clone(), activity);
        }
    }

    let open_notes = sessions
        .list_project_notes()
        .map_err(anyhow::Error::msg)?
        .iter()
        .filter(|note| note.status == "open")
        .count();

    let agents = super::agents::load(&sessions)?;
    let task_agents = super::task_agents::by_task(&agents, &delegations);
    // A small table, empty but for a moment after an agent asks.
    let review_requests = sessions.waiting_pr_review_requests().unwrap_or_default();
    let agent_proposals = sessions.waiting_agent_proposals().unwrap_or_default();

    let linked_folders = workspaces
        .list_repository_linked_folders()?
        .into_iter()
        .filter(|(_, paths)| !paths.is_empty())
        .map(|(id, paths)| {
            let linked = dcc_core::application::linked_task::LinkedFolders {
                context_paths: dcc_infra::project_actions::linked_context_paths(&paths),
                paths,
            };
            (id.0, linked)
        })
        .collect();
    Ok(SidebarData {
        linked_folders,
        tasks,
        projects,
        activities,
        provider_ids,
        open_notes,
        review_pending,
        agents,
        agent_homes,
        child_tasks,
        review_requests,
        agent_proposals,
        task_agents,
    })
}

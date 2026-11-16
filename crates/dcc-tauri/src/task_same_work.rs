//! Looks up, before an agent's task is opened, whether a branch or pull
//! request its brief names already belongs to an open task of that project
//! (`dcc_core::domain::task_same_work`). The branches come from one `git
//! worktree list` of the project; a linked pull request costs one read of
//! GitHub for its branch. A brief that names neither costs nothing. What
//! was asked is then brought to that task's own conversation (`forward`),
//! so the work is done where it belongs instead of only being refused.

use std::path::Path;

use dcc_core::domain::agent_conversation::NewTaskTarget;
use dcc_core::domain::task_same_work::{named_branches, named_pull_requests, task_on_branch};
use dcc_infra::forge::github_head::pull_request_head;
use dcc_infra::forge::github_read::RepoSlug;
use dcc_infra::forge::github_read_run::GithubTarget;
use rusqlite::{Connection, OpenFlags};

/// Pull requests of one brief looked up at most.
const MAX_PULL_REQUESTS: usize = 2;

/// `(path, branch)` of each worktree of the repository at `root`.
fn worktrees(root: &str) -> Vec<(String, String)> {
    let Ok(output) = std::process::Command::new("git")
        .args(["-C", root, "worktree", "list", "--porcelain"])
        .output()
    else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let mut found = Vec::new();
    let mut path: Option<String> = None;
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("worktree ") {
            path = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("branch refs/heads/") {
            if let Some(path) = path.take() {
                found.push((path, value.to_string()));
            }
        }
    }
    found
}

/// The project's open tasks as `(id, name, where its branch is checked out)`.
fn open_tasks(db_path: &Path, project_id: &str) -> Vec<(String, String, String)> {
    let Ok(conn) = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY) else {
        return Vec::new();
    };
    let Ok(mut statement) = conn.prepare(
        "SELECT id, COALESCE(NULLIF(name, ''), id), COALESCE(NULLIF(worktree_path, ''), root_path)
         FROM dcc_workspaces WHERE project_id = ?1 AND state = 'ready'",
    ) else {
        return Vec::new();
    };
    statement
        .query_map([project_id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .and_then(Iterator::collect::<Result<Vec<(String, String, String)>, _>>)
        .unwrap_or_default()
}

fn same_path(left: &str, right: &str) -> bool {
    let clean = |path: &str| {
        std::fs::canonicalize(path)
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_else(|_| path.trim_end_matches('/').to_string())
    };
    clean(left) == clean(right)
}

/// The branch of the pull request `number` of the project's repository.
fn head_of(db_path: &Path, root: &str, number: u64) -> Option<String> {
    let context =
        crate::commands::forge::context::resolve_workspace_forge_context(db_path, root, None)
            .ok()??;
    if context.provider != crate::commands::forge_commands::ForgeCliProvider::Github {
        return None;
    }
    let target = GithubTarget {
        host: context.host,
        repo: RepoSlug {
            owner: context.namespace,
            name: context.repo,
        },
        login: context.effective_login,
    };
    pull_request_head(&target, number).ok().flatten()
}

/// An open task that already has what a brief names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExistingTask {
    /// "branch `x`" or "pull request #7".
    pub what: String,
    pub name: String,
    pub workspace_id: String,
}

/// What `text` names that an open task of the project already has.
/// Blocking.
pub fn existing_task(db_path: &Path, target: &NewTaskTarget, text: &str) -> Option<ExistingTask> {
    let mut named: Vec<(String, String)> = named_branches(text)
        .into_iter()
        .map(|branch| (format!("branch `{branch}`"), branch))
        .collect();
    let requests = named_pull_requests(text);
    if named.is_empty() && requests.is_empty() {
        return None;
    }
    let checked_out = worktrees(&target.root_path);
    // Known by its id, shown by its name.
    let open = open_tasks(db_path, &target.project_id);
    let tasks: Vec<(String, String)> = open
        .iter()
        .filter_map(|(id, _, path)| {
            let (_, branch) = checked_out
                .iter()
                .find(|(worktree, _)| same_path(worktree, path))?;
            Some((id.clone(), branch.clone()))
        })
        .collect();
    if tasks.is_empty() {
        return None;
    }
    let find = |named: &[(String, String)]| {
        named.iter().find_map(|(what, branch)| {
            let id = task_on_branch(branch, &target.base_branch, &tasks)?;
            let (_, name, _) = open.iter().find(|(task, ..)| task == id)?;
            Some(ExistingTask {
                what: what.clone(),
                name: name.clone(),
                workspace_id: id.to_string(),
            })
        })
    };
    if let Some(found) = find(&named) {
        return Some(found);
    }
    // No branch named matched: a linked pull request says its own.
    named.clear();
    for number in requests.into_iter().take(MAX_PULL_REQUESTS) {
        if let Some(branch) = head_of(db_path, &target.root_path, number) {
            named.push((format!("pull request #{number}"), branch));
        }
    }
    find(&named)
}

/// The task's own conversation, newest first: not archived, not a
/// delegated child's and not one bound to an agent. None when it has none.
async fn own_session(
    state: &crate::state::SessionCommandState,
    workspace_id: &str,
) -> Option<dcc_core::domain::session::SessionId> {
    use dcc_core::ports::DelegationRepo;
    let workspace = dcc_core::domain::workspace::WorkspaceId(workspace_id.to_string());
    let delegated: Vec<String> = DelegationRepo::list_delegations(state, Some(&workspace), None)
        .await
        .ok()?
        .into_iter()
        .filter_map(|delegation| delegation.child_session_id.map(|id| id.0))
        .collect();
    state
        .list_workspace_sessions(&workspace)
        .ok()?
        .into_iter()
        .filter(|summary| summary.thread.archived_at.is_none())
        .map(|summary| summary.session.id)
        .filter(|id| !delegated.contains(&id.0))
        .find(|id| state.session_agent(id).ok().flatten().is_none())
}

/// Brings what `agent` asked to the task that owns the work, as a turn of
/// DCC's in its own conversation.
pub async fn forward(
    state: &crate::state::SessionCommandState,
    existing: &ExistingTask,
    agent: &str,
    title: &str,
    brief: &str,
) -> Result<(), String> {
    let session = own_session(state, &existing.workspace_id)
        .await
        .ok_or_else(|| "the task has no conversation of its own".to_string())?;
    let prompt =
        dcc_core::domain::task_same_work::forwarded_prompt(agent, &existing.what, title, brief);
    crate::delegation_runtime::send_dcc_turn(state, &session, prompt).await
}

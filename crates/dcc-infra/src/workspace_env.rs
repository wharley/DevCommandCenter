//! What a task's programs (its terminals, its project tasks, its agents) are
//! told about where they run. The names follow Conductor's (`CONDUCTOR_PORT`,
//! `CONDUCTOR_WORKSPACE_PATH`…), so a script written for it only changes the
//! prefix: `pnpm dev --port $DCC_PORT`, `--port $((DCC_PORT + 1))`.

use std::path::Path;

use dcc_core::domain::workspace::Workspace;

use crate::db::SqliteWorkspaceRepo;
use crate::workspace_git::resolve_workspace_active_root;

/// `DCC_PORT` is left out only when no block could be taken; the rest is
/// always there.
pub fn workspace_run_env(
    repo: &SqliteWorkspaceRepo,
    workspace: &Workspace,
) -> Vec<(String, String)> {
    let path = resolve_workspace_active_root(workspace).to_string();
    let name = workspace
        .name
        .clone()
        .filter(|name| !name.trim().is_empty())
        .or_else(|| {
            Path::new(&path)
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
        })
        .unwrap_or_default();
    let mut env = vec![
        ("DCC_WORKSPACE_ID".to_string(), workspace.id.0.clone()),
        ("DCC_WORKSPACE_NAME".to_string(), name),
        ("DCC_WORKSPACE_PATH".to_string(), path),
        ("DCC_ROOT_PATH".to_string(), workspace.root_path.clone()),
        (
            "DCC_DEFAULT_BRANCH".to_string(),
            workspace.base_branch.clone(),
        ),
    ];
    match repo.workspace_port_base(&workspace.id) {
        Ok(port) => env.push(("DCC_PORT".to_string(), port.to_string())),
        Err(error) => eprintln!("[dcc] no port block for task {}: {error}", workspace.id.0),
    }
    env
}

/// The env of the task whose id an owner key or request names, when that
/// task runs at `cwd`; nothing for another folder.
pub async fn workspace_run_env_at(
    repo: &SqliteWorkspaceRepo,
    workspace_id: &str,
    cwd: &str,
) -> Vec<(String, String)> {
    use dcc_core::ports::WorkspaceRepo;
    let id = dcc_core::domain::workspace::WorkspaceId(workspace_id.to_string());
    match repo.get_workspace(&id).await {
        Ok(Some(workspace)) if resolve_workspace_active_root(&workspace) == cwd.trim() => {
            workspace_run_env(repo, &workspace)
        }
        _ => Vec::new(),
    }
}

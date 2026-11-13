//! The whole file list of a task's checkout, for another DCC that shows
//! this machine's task: what the local tree reads from git here (tracked
//! and new files, and the ignored ones outside ignored folders), found by
//! the task's id only, so no path of this computer is ever asked for.
use super::*;
use dcc_core::{domain::workspace::WorkspaceId, ports::WorkspaceRepo};
use dcc_infra::{db::SqliteWorkspaceRepo, workspace_git};

fn listing(tracked: Vec<String>, ignored: Vec<String>) -> Value {
    json!({ "tracked": tracked, "ignored": ignored })
}

pub(super) async fn tree(
    State(config): State<Arc<RwLock<HttpConfig>>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, HttpApiError> {
    let repo = SqliteWorkspaceRepo::open(&config.read().await.db_path)
        .map_err(|error| HttpApiError::internal(error.to_string()))?;
    let workspace = repo
        .get_workspace(&WorkspaceId(id))
        .await
        .map_err(|error| HttpApiError::internal(error.to_string()))?
        .ok_or_else(|| HttpApiError::NotFound("Workspace não encontrado.".into()))?;
    let root = workspace.worktree_path.unwrap_or(workspace.root_path);
    if !std::path::Path::new(&root).is_dir() {
        return Err(HttpApiError::NotFound(
            "A pasta do workspace não existe mais.".into(),
        ));
    }
    tokio::task::spawn_blocking(move || {
        let tracked = workspace_git::git_tracked_files(&root)?;
        let ignored = workspace_git::git_ignored_files(&root)?;
        Ok::<_, String>(listing(tracked, ignored))
    })
    .await
    .map_err(|error| HttpApiError::internal(error.to_string()))?
    .map(Json)
    .map_err(HttpApiError::internal)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_listing_keeps_tracked_and_ignored_apart() {
        let value = listing(vec!["src/main.rs".into()], vec![".env".into()]);
        assert_eq!(value["tracked"][0], "src/main.rs");
        assert_eq!(value["ignored"][0], ".env");
    }
}

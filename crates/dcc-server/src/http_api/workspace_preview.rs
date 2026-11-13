//! The two versions of a changed file of a task's checkout, for another
//! DCC that draws this machine's task: what the local diff reads from git
//! here (`file_preview_content`), found by the task's id only. A change not
//! committed yet is the file against HEAD; one of the branch is HEAD
//! against the base the caller names, which must read as a branch name.
use super::*;
use dcc_core::{
    domain::{workspace::WorkspaceId, workspace_git::WorkspaceGitPreviewScope},
    ports::WorkspaceRepo,
};
use dcc_infra::{db::SqliteWorkspaceRepo, workspace_file_preview::file_preview_content};

/// A side past this is not sent: a diff that size is read on the machine.
const MAX_SIDE: usize = 1_000_000;

#[derive(Deserialize)]
pub(super) struct PreviewQuery {
    path: String,
    #[serde(default)]
    status: String,
    scope: WorkspaceGitPreviewScope,
    base: Option<String>,
}

/// A branch or a remote's branch, never an option or a revision expression.
fn branch_name(base: &str) -> bool {
    !base.is_empty()
        && base.len() <= 200
        && !base.starts_with('-')
        && !base.contains("..")
        && base
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '-' | '_' | '.'))
}

fn answer(original: String, modified: String, inline: bool) -> Result<Value, HttpApiError> {
    if original.len() > MAX_SIDE || modified.len() > MAX_SIDE {
        return Err(HttpApiError::BadRequest(
            "Arquivo grande demais para mostrar o diff daqui.".into(),
        ));
    }
    Ok(json!({ "original": original, "modified": modified, "inline": inline }))
}

pub(super) async fn preview(
    State(config): State<Arc<RwLock<HttpConfig>>>,
    Path(id): Path<String>,
    Query(query): Query<PreviewQuery>,
) -> Result<Json<Value>, HttpApiError> {
    if query.base.as_deref().is_some_and(|base| !branch_name(base)) {
        return Err(HttpApiError::BadRequest("Base inválida.".into()));
    }
    let repo = SqliteWorkspaceRepo::open(&config.read().await.db_path)
        .map_err(|error| HttpApiError::internal(error.to_string()))?;
    let workspace = repo
        .get_workspace(&WorkspaceId(id))
        .await
        .map_err(|error| HttpApiError::internal(error.to_string()))?
        .ok_or_else(|| HttpApiError::NotFound("Workspace não encontrado.".into()))?;
    let root = workspace.worktree_path.unwrap_or(workspace.root_path);
    let (original, modified, inline) = tokio::task::spawn_blocking(move || {
        // The path is checked there: relative, inside the checkout.
        file_preview_content(
            &root,
            &query.path,
            &query.status,
            &query.scope,
            query.base.as_deref(),
        )
    })
    .await
    .map_err(|error| HttpApiError::internal(error.to_string()))?
    .map_err(HttpApiError::BadRequest)?;
    answer(original, modified, inline).map(Json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_base_is_a_branch_name_and_nothing_else() {
        for good in ["main", "origin/main", "release/1.2", "feat_x-1"] {
            assert!(branch_name(good), "{good}");
        }
        for bad in [
            "",
            "-c",
            "--output=x",
            "main..dev",
            "HEAD~1",
            "a b",
            "x:y",
            "@{u}",
        ] {
            assert!(!branch_name(bad), "{bad}");
        }
    }

    #[test]
    fn a_side_too_big_is_not_sent() {
        assert!(answer("a".into(), "b".into(), false).is_ok());
        assert!(answer("x".repeat(MAX_SIDE + 1), String::new(), true).is_err());
    }
}

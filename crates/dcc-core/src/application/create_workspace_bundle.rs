use std::collections::BTreeSet;

use chrono::Utc;
use uuid::Uuid;

use crate::{
    domain::{
        workspace::Workspace,
        workspace_bundle::{
            WorkspaceBundle, WorkspaceBundleId, WorkspaceBundleMember, WorkspaceBundleState,
            WorkspaceBundleSummary,
        },
    },
    ports::WorkspaceBundleRepo,
    CoreError, Result,
};

pub async fn create_workspace_bundle<R>(
    repo: &R,
    name: &str,
    workspaces: &[Workspace],
) -> Result<WorkspaceBundleSummary>
where
    R: WorkspaceBundleRepo + Sync,
{
    let name = name.trim();
    if name.is_empty() {
        return Err(CoreError::InvalidInput(
            "workspace bundle name cannot be empty".to_string(),
        ));
    }
    if workspaces.len() < 2 {
        return Err(CoreError::InvalidInput(
            "workspace bundle requires at least two workspaces".to_string(),
        ));
    }

    let mut workspace_ids = BTreeSet::new();
    let mut roots = BTreeSet::new();
    for workspace in workspaces {
        if !workspace_ids.insert(workspace.id.0.clone()) {
            return Err(CoreError::InvalidInput(format!(
                "duplicate workspace in bundle: {}",
                workspace.id.0
            )));
        }
        let normalized_root = workspace.root_path.trim().replace('\\', "/");
        if normalized_root.is_empty() || !roots.insert(normalized_root.clone()) {
            return Err(CoreError::InvalidInput(format!(
                "workspace bundle requires distinct repository roots: {normalized_root}"
            )));
        }
    }

    let now = Utc::now().to_rfc3339();
    let bundle = WorkspaceBundle {
        id: WorkspaceBundleId(Uuid::new_v4().to_string()),
        name: name.to_string(),
        primary_workspace_id: workspaces[0].id.clone(),
        state: WorkspaceBundleState::Ready,
        created_at: now.clone(),
        updated_at: now,
    };
    let members = workspaces
        .iter()
        .enumerate()
        .map(|(position, workspace)| WorkspaceBundleMember {
            bundle_id: bundle.id.clone(),
            workspace_id: workspace.id.clone(),
            created_for_bundle: true,
            position: position as u32,
        })
        .collect::<Vec<_>>();

    repo.save_workspace_bundle(&bundle, &members).await?;
    Ok(WorkspaceBundleSummary { bundle, members })
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;

    use super::*;
    use crate::{
        domain::{project::ProjectId, workspace::WorkspaceId},
        ports::WorkspaceBundleRepo,
    };

    #[derive(Clone, Default)]
    struct FakeRepo {
        saved: Arc<Mutex<Vec<WorkspaceBundleSummary>>>,
    }

    #[async_trait]
    impl WorkspaceBundleRepo for FakeRepo {
        async fn save_workspace_bundle(
            &self,
            bundle: &WorkspaceBundle,
            members: &[WorkspaceBundleMember],
        ) -> Result<()> {
            self.saved
                .lock()
                .expect("saved lock")
                .push(WorkspaceBundleSummary {
                    bundle: bundle.clone(),
                    members: members.to_vec(),
                });
            Ok(())
        }

        async fn get_workspace_bundle(
            &self,
            _id: &WorkspaceBundleId,
        ) -> Result<Option<WorkspaceBundleSummary>> {
            Ok(None)
        }

        async fn get_workspace_bundle_for_workspace(
            &self,
            _workspace_id: &WorkspaceId,
        ) -> Result<Option<WorkspaceBundleSummary>> {
            Ok(None)
        }

        async fn list_workspace_bundles(&self) -> Result<Vec<WorkspaceBundleSummary>> {
            Ok(self.saved.lock().expect("saved lock").clone())
        }

        async fn set_workspace_bundle_state(
            &self,
            _id: &WorkspaceBundleId,
            _state: WorkspaceBundleState,
            _updated_at: String,
        ) -> Result<Option<WorkspaceBundleSummary>> {
            Ok(None)
        }

        async fn delete_workspace_bundle(&self, _id: &WorkspaceBundleId) -> Result<()> {
            Ok(())
        }
    }

    fn workspace(id: &str, root: &str) -> Workspace {
        Workspace {
            id: WorkspaceId(id.to_string()),
            project_id: ProjectId(id.to_string()),
            name: Some(id.to_string()),
            root_path: root.to_string(),
            base_branch: "main".to_string(),
            worktree_path: Some(format!("{root}/.dcc-worktrees/main-{id}")),
            state: crate::domain::workspace::WorkspaceState::Ready,
            setup_report: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn creates_bundle_for_distinct_workspaces() {
        let repo = FakeRepo::default();
        let result = futures::executor::block_on(create_workspace_bundle(
            &repo,
            "Checkout",
            &[
                workspace("backend", "/tmp/backend"),
                workspace("frontend", "/tmp/frontend"),
            ],
        ))
        .expect("create bundle");

        assert_eq!(result.bundle.name, "Checkout");
        assert_eq!(result.members.len(), 2);
        assert_eq!(result.bundle.primary_workspace_id.0, "backend");
    }

    #[test]
    fn rejects_single_workspace_without_persisting() {
        let repo = FakeRepo::default();
        let error = futures::executor::block_on(create_workspace_bundle(
            &repo,
            "Single",
            &[workspace("backend", "/tmp/backend")],
        ))
        .expect_err("single workspace must fail");

        assert!(error.to_string().contains("at least two"));
        assert!(repo.saved.lock().expect("saved lock").is_empty());
    }
}

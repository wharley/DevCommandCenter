//! How a task started in a project begins (`linked-folders.ts`): the
//! project's linked Git folders make it a multi-project task, every folder in
//! the mode the person chose; folders without Git only ride along as context
//! on the project itself.

use crate::application::{CreateWorkspaceForRepoInput, WorkspaceIsolationMode};
use crate::domain::project::ProjectId;
use crate::domain::repository::{path_leaf, repository_display_name, Repository};

/// The project the task starts in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkedTaskPrimary {
    pub project_id: String,
    pub workspace_root: String,
    pub base_branch: String,
}

/// A project's linked folders, and which of them have no Git.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LinkedFolders {
    pub paths: Vec<String>,
    pub context_paths: Vec<String>,
}

#[derive(Clone, Debug)]
pub enum LinkedTaskPlan {
    /// One project, with the folders without Git as context.
    Single { context_folders: Vec<String> },
    /// The project first (the primary), then each linked Git folder.
    Bundle {
        projects: Vec<CreateWorkspaceForRepoInput>,
    },
}

/// `normalizeWorkspaceRoot`: forward slashes, no trailing one.
pub fn normalize_workspace_root(root: &str) -> String {
    root.replace('\\', "/").trim_end_matches('/').to_string()
}

/// `sanitizeProjectIdSegment`
fn sanitize_project_id_segment(value: &str) -> String {
    let mut sanitized = String::new();
    for character in value.trim().to_lowercase().chars() {
        let keep = character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-');
        let next = if keep { character } else { '-' };
        if next == '-' && sanitized.ends_with('-') {
            continue;
        }
        sanitized.push(next);
    }
    let sanitized = sanitized.trim_matches(['-', '_', '.']).to_string();
    if sanitized.is_empty() {
        "project".to_string()
    } else {
        sanitized
    }
}

/// `inferProjectIdFromWorkspaceRoot`: a folder that is not a DCC project yet.
pub fn infer_project_id_from_workspace_root(root: &str) -> String {
    sanitize_project_id_segment(&path_leaf(&normalize_workspace_root(root)).unwrap_or_default())
}

fn find<'a>(repositories: &'a [Repository], path: &str) -> Option<&'a Repository> {
    let root = normalize_workspace_root(path.trim());
    repositories
        .iter()
        .find(|candidate| normalize_workspace_root(candidate.root_path.trim()) == root)
}

/// `planLinkedTask`. `linked` is the primary project's linked folders (empty
/// when it links none); `include_linked_folders: false` leaves them all out.
pub fn plan_linked_task(
    primary: &LinkedTaskPrimary,
    linked: &LinkedFolders,
    repositories: &[Repository],
    isolation_mode: Option<WorkspaceIsolationMode>,
    include_linked_folders: bool,
) -> LinkedTaskPlan {
    if !include_linked_folders {
        return LinkedTaskPlan::Single {
            context_folders: Vec::new(),
        };
    }
    let context_folders = linked.context_paths.clone();
    let project_folders: Vec<&String> = linked
        .paths
        .iter()
        .filter(|path| !context_folders.contains(path))
        .collect();
    if project_folders.is_empty() {
        return LinkedTaskPlan::Single { context_folders };
    }
    // Every folder of a multi-project task follows the person's mode; a
    // protected one is the backend default.
    let isolation = match isolation_mode {
        Some(WorkspaceIsolationMode::LocalDirect) => Some(WorkspaceIsolationMode::LocalDirect),
        _ => None,
    };
    let primary_repository = find(repositories, &primary.workspace_root);
    let mut projects = vec![CreateWorkspaceForRepoInput {
        project_id: ProjectId(primary.project_id.clone()),
        workspace_root: primary.workspace_root.clone(),
        base_branch: primary.base_branch.clone(),
        name: primary_repository.map(repository_display_name),
        isolation_mode: isolation.clone(),
        context_folders,
    }];
    for path in project_folders {
        let repository = find(repositories, path);
        projects.push(CreateWorkspaceForRepoInput {
            project_id: ProjectId(
                repository
                    .map(|repository| repository.project_id.0.clone())
                    .unwrap_or_else(|| infer_project_id_from_workspace_root(path)),
            ),
            workspace_root: path.clone(),
            // Unknown base: the backend starts from that checkout's branch.
            base_branch: repository
                .map(|repository| repository.base_branch.clone())
                .unwrap_or_default(),
            name: Some(
                repository
                    .map(repository_display_name)
                    .or_else(|| path_leaf(&normalize_workspace_root(path)))
                    .unwrap_or_else(|| "Project".to_string()),
            ),
            isolation_mode: isolation.clone(),
            context_folders: Vec::new(),
        });
    }
    LinkedTaskPlan::Bundle { projects }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::repository::RepositoryId;

    fn repository(root: &str, project: &str, display: Option<&str>) -> Repository {
        Repository {
            id: RepositoryId(root.into()),
            project_id: ProjectId(project.into()),
            name: project.into(),
            display_name: display.map(str::to_string),
            icon: None,
            color: None,
            logo: None,
            pinned_at: None,
            root_path: root.into(),
            base_branch: "main".into(),
            remote: None,
            remote_url: None,
            forge_provider: None,
            forge_login: None,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    fn primary() -> LinkedTaskPrimary {
        LinkedTaskPrimary {
            project_id: "app".into(),
            workspace_root: "/code/app".into(),
            base_branch: "develop".into(),
        }
    }

    #[test]
    fn folders_without_git_ride_along_as_context() {
        let linked = LinkedFolders {
            paths: vec!["/notes".into()],
            context_paths: vec!["/notes".into()],
        };
        let LinkedTaskPlan::Single { context_folders } =
            plan_linked_task(&primary(), &linked, &[], None, true)
        else {
            panic!("one project");
        };
        assert_eq!(context_folders, vec!["/notes".to_string()]);
        let LinkedTaskPlan::Single { context_folders } =
            plan_linked_task(&primary(), &linked, &[], None, false)
        else {
            panic!("one project");
        };
        assert!(context_folders.is_empty(), "left out on request");
    }

    #[test]
    fn linked_git_folders_make_a_multi_project_task() {
        let repositories = vec![
            repository("/code/app", "app", Some("Checkout")),
            repository("/code/api/", "api", None),
        ];
        let linked = LinkedFolders {
            paths: vec!["/code/api".into(), "/code/New Lib".into(), "/notes".into()],
            context_paths: vec!["/notes".into()],
        };
        let LinkedTaskPlan::Bundle { projects } = plan_linked_task(
            &primary(),
            &linked,
            &repositories,
            Some(WorkspaceIsolationMode::LocalDirect),
            true,
        ) else {
            panic!("several projects");
        };
        assert_eq!(projects.len(), 3);
        assert_eq!(projects[0].name.as_deref(), Some("Checkout"));
        assert_eq!(projects[0].base_branch, "develop");
        assert_eq!(projects[0].context_folders, vec!["/notes".to_string()]);
        assert_eq!(projects[1].project_id.0, "api");
        assert_eq!(projects[1].base_branch, "main");
        assert_eq!(projects[2].project_id.0, "new-lib");
        assert_eq!(projects[2].name.as_deref(), Some("New Lib"));
        assert_eq!(projects[2].base_branch, "", "its own branch, decided later");
        assert!(projects.iter().all(|project| matches!(
            project.isolation_mode,
            Some(WorkspaceIsolationMode::LocalDirect)
        )));
        let LinkedTaskPlan::Bundle { projects } =
            plan_linked_task(&primary(), &linked, &repositories, None, true)
        else {
            panic!("several projects");
        };
        assert!(projects
            .iter()
            .all(|project| project.isolation_mode.is_none()));
    }

    #[test]
    fn project_ids_follow_the_desktop_rule() {
        assert_eq!(
            infer_project_id_from_workspace_root("C:\\code\\Meu App\\"),
            "meu-app"
        );
        assert_eq!(infer_project_id_from_workspace_root("/x/__"), "project");
        assert_eq!(infer_project_id_from_workspace_root("/x/a..b"), "a..b");
    }
}

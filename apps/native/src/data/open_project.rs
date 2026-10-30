//! The open-project dialog's rules (`create-workspace-dialog.tsx` in "open"
//! mode and `create-workspace-dialog.logic.ts`): which project the task
//! starts in (a listed one or a folder picked on disk), its base branch,
//! the other projects it brings (a project's linked Git folders come
//! preselected), the folders without Git it takes as context, and what the
//! desktop is asked to create.

use std::collections::HashMap;

use dcc_core::application::linked_task::{
    LinkedFolders, infer_project_id_from_workspace_root, normalize_workspace_root,
};
use dcc_core::application::{CreateWorkspaceForRepoInput, WorkspaceIsolationMode};
use dcc_core::domain::project::ProjectId;
use dcc_core::domain::repository::path_leaf;

use super::tasks::Project;

/// A project the dialog offers: a DCC project, a linked folder that is not
/// one yet, or a folder picked on disk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectOption {
    pub id: String,
    pub project_id: String,
    pub label: String,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub root_path: String,
    /// Empty: the task starts from that checkout's current branch.
    pub base_branch: String,
    /// A DCC project (its linked folders can be remembered).
    pub registered: bool,
}

impl ProjectOption {
    fn from_project(project: &Project) -> Self {
        Self {
            id: project.id.clone(),
            project_id: project.project_id.clone(),
            label: project.display_name.clone(),
            icon: project.icon.clone(),
            color: project.color.clone(),
            root_path: project.root_path.clone(),
            base_branch: project.base_branch.clone(),
            registered: true,
        }
    }

    /// `repositoryNameFromWorkspaceRoot` and `inferProjectIdFromWorkspaceRoot`.
    fn from_folder(id: String, path: &str, base_branch: String) -> Self {
        Self {
            id,
            project_id: infer_project_id_from_workspace_root(path),
            label: path_leaf(&normalize_workspace_root(path)).unwrap_or_else(|| "Project".into()),
            icon: None,
            color: None,
            root_path: path.to_string(),
            base_branch,
            registered: false,
        }
    }
}

/// What the desktop is asked to create.
#[derive(Clone, Debug)]
pub enum Submission {
    Single(CreateWorkspaceForRepoInput),
    Bundle {
        name: String,
        projects: Vec<CreateWorkspaceForRepoInput>,
    },
}

#[derive(Clone, Debug)]
pub struct OpenProjectModel {
    projects: Vec<ProjectOption>,
    linked: HashMap<String, LinkedFolders>,
    picked: Option<ProjectOption>,
    pub selected: Option<String>,
    /// The other projects of the task, as option ids, in the picked order.
    pub extras: Vec<String>,
    /// The primary's folders without Git this task brings.
    pub included_context: Vec<String>,
    pub adding: bool,
    pub worktree: bool,
    pub remember: bool,
    pub project_id: String,
    pub workspace_root: String,
    pub base_branch: String,
    pub branches: Vec<String>,
    pub loading_branches: bool,
}

fn same_root(left: &str, right: &str) -> bool {
    normalize_workspace_root(left.trim()) == normalize_workspace_root(right.trim())
}

impl OpenProjectModel {
    /// The dialog as it opens: the project it was asked for, else the last
    /// one a task started in, else the first (`initialTaskRepository`);
    /// none when it opens straight to the folder picker.
    pub fn new(
        projects: &[Project],
        linked: &HashMap<String, LinkedFolders>,
        multi: bool,
        worktree: bool,
        last_root: Option<&str>,
        pick_folder: bool,
    ) -> Self {
        let mut model = Self {
            projects: projects.iter().map(ProjectOption::from_project).collect(),
            linked: linked.clone(),
            picked: None,
            selected: None,
            extras: Vec::new(),
            included_context: Vec::new(),
            adding: multi,
            worktree,
            remember: false,
            project_id: String::new(),
            workspace_root: String::new(),
            base_branch: String::new(),
            branches: Vec::new(),
            loading_branches: false,
        };
        if !pick_folder {
            let initial = last_root
                .and_then(|root| {
                    model
                        .projects
                        .iter()
                        .find(|option| same_root(&option.root_path, root))
                })
                .or_else(|| model.projects.first())
                .map(|option| option.id.clone());
            if let Some(id) = initial {
                model.select(&id);
            }
        }
        model
    }

    /// The linked folders of each project that can be projects (Git).
    fn linked_project_folders(&self, repository_id: &str) -> Vec<String> {
        self.linked
            .get(repository_id)
            .map(|linked| {
                linked
                    .paths
                    .iter()
                    .filter(|path| !linked.context_paths.contains(path))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Projects plus the linked Git folders that are not projects yet
    /// (`withLinkedFolderOptions`).
    pub fn multi_options(&self) -> Vec<ProjectOption> {
        let mut options = self.projects.clone();
        let mut ids: Vec<&String> = self.linked.keys().collect();
        ids.sort();
        for repository_id in ids {
            for path in self.linked_project_folders(repository_id) {
                if options
                    .iter()
                    .any(|option| same_root(&option.root_path, &path))
                {
                    continue;
                }
                let id = format!("linked:{}", normalize_workspace_root(&path));
                options.push(ProjectOption::from_folder(id, &path, String::new()));
            }
        }
        options
    }

    /// The listed projects, a picked folder first (`includePickedRepository`).
    pub fn single_options(&self) -> Vec<ProjectOption> {
        match &self.picked {
            Some(picked)
                if !self
                    .projects
                    .iter()
                    .any(|project| same_root(&project.root_path, &picked.root_path)) =>
            {
                std::iter::once(picked.clone())
                    .chain(self.projects.iter().cloned())
                    .collect()
            }
            _ => self.projects.clone(),
        }
    }

    pub fn primary(&self) -> Option<ProjectOption> {
        let id = self.selected.as_deref()?;
        self.single_options()
            .into_iter()
            .find(|option| option.id == id)
    }

    /// The context folders the primary links (all offered, some included).
    pub fn primary_context_folders(&self) -> Vec<String> {
        self.primary()
            .and_then(|primary| self.linked.get(&primary.id))
            .map(|linked| linked.context_paths.clone())
            .unwrap_or_default()
    }

    /// A new primary brings its linked folders along, already selected.
    fn preset_linked(&mut self, primary: Option<&str>) {
        let Some(primary) = primary else {
            self.extras.clear();
            self.included_context.clear();
            return;
        };
        let options = self.multi_options();
        self.extras = self
            .linked
            .get(primary)
            .map(|linked| {
                linked
                    .paths
                    .iter()
                    .filter(|path| !linked.context_paths.contains(path))
                    .filter_map(|path| {
                        options
                            .iter()
                            .find(|option| same_root(&option.root_path, path))
                            .filter(|option| option.id != primary)
                            .map(|option| option.id.clone())
                    })
                    .collect()
            })
            .unwrap_or_default();
        self.included_context = self
            .linked
            .get(primary)
            .map(|linked| linked.context_paths.clone())
            .unwrap_or_default();
    }

    /// Picks a listed project; answers the root whose branches to load.
    pub fn select(&mut self, id: &str) -> Option<String> {
        let option = self
            .single_options()
            .into_iter()
            .find(|option| option.id == id)?;
        if self.selected.as_deref() != Some(id) {
            self.preset_linked(Some(id));
        }
        self.selected = Some(option.id.clone());
        self.project_id = option.project_id.clone();
        self.workspace_root = option.root_path.clone();
        self.base_branch = option.base_branch.clone();
        self.loading_branches = true;
        Some(option.root_path)
    }

    /// "Outra pasta": a project already listed is selected; any other folder
    /// becomes the task's project, its base branch read next.
    pub fn pick_folder(&mut self, path: &str) -> Option<String> {
        if let Some(listed) = self
            .projects
            .iter()
            .find(|option| same_root(&option.root_path, path))
        {
            let id = listed.id.clone();
            self.picked = None;
            return self.select(&id);
        }
        let id = format!("picked:{}", normalize_workspace_root(path));
        let option = ProjectOption::from_folder(id.clone(), path, String::new());
        self.project_id = option.project_id.clone();
        self.workspace_root = option.root_path.clone();
        self.base_branch = String::new();
        self.picked = Some(option);
        self.preset_linked(None);
        self.selected = Some(id);
        self.loading_branches = true;
        Some(path.to_string())
    }

    /// The branches of `root`, once read; a stale answer is dropped. The
    /// base stays when it exists there, else the first branch.
    pub fn branches_loaded(&mut self, root: &str, branches: Option<Vec<String>>) {
        if !same_root(root, &self.workspace_root) {
            return;
        }
        self.loading_branches = false;
        let branches = branches.unwrap_or_default();
        if self.base_branch.trim().is_empty() || !branches.contains(&self.base_branch) {
            self.base_branch = branches.first().cloned().unwrap_or_default();
        }
        if let Some(picked) = self.picked.as_mut()
            && same_root(&picked.root_path, root)
            && picked.base_branch.is_empty()
        {
            picked.base_branch = self.base_branch.clone();
        }
        self.branches = branches;
    }

    pub fn toggle_extra(&mut self, id: &str) {
        match self.extras.iter().position(|extra| extra == id) {
            Some(index) => {
                self.extras.remove(index);
            }
            None => self.extras.push(id.to_string()),
        }
    }

    pub fn toggle_context(&mut self, path: &str) {
        match self
            .included_context
            .iter()
            .position(|included| included == path)
        {
            Some(index) => {
                self.included_context.remove(index);
            }
            None => self.included_context.push(path.to_string()),
        }
    }

    /// `orderMultiProjectSelection`
    pub fn extra_options(&self) -> Vec<ProjectOption> {
        let options = self.multi_options();
        self.extras
            .iter()
            .filter_map(|id| options.iter().find(|option| option.id == *id).cloned())
            .collect()
    }

    pub fn brings_projects(&self) -> bool {
        !self.extra_options().is_empty()
    }

    /// Worktrees the task creates (`protectedWorktreeCount`).
    pub fn worktree_count(&self) -> usize {
        1 + self.extra_options().len()
    }

    pub fn can_submit(&self) -> bool {
        !self.project_id.trim().is_empty()
            && !self.workspace_root.trim().is_empty()
            && !self.base_branch.trim().is_empty()
    }

    /// The folders this task brings besides its project.
    pub fn task_folders(&self) -> Vec<String> {
        self.extra_options()
            .into_iter()
            .map(|option| option.root_path)
            .chain(self.included_context.iter().cloned())
            .collect()
    }

    /// "Lembrar estas pastas": a DCC project whose folders here differ from
    /// the ones it saved (`linkedFoldersChanged`).
    pub fn can_remember(&self) -> bool {
        let Some(primary) = self.primary().filter(|primary| primary.registered) else {
            return false;
        };
        let saved: Vec<String> = self
            .linked
            .get(&primary.id)
            .map(|linked| linked.paths.clone())
            .unwrap_or_default();
        let normalize = |paths: &[String]| {
            let mut paths: Vec<String> = paths
                .iter()
                .map(|path| normalize_workspace_root(path))
                .collect();
            paths.sort();
            paths.dedup();
            paths
        };
        normalize(&saved) != normalize(&self.task_folders())
    }

    /// `handleSubmit`: a multi-project task when it brings other projects,
    /// else one task with the context folders.
    pub fn submission(&self, name: &str, default_name: &str) -> Option<Submission> {
        if !self.can_submit() {
            return None;
        }
        let isolation = (!self.worktree).then_some(WorkspaceIsolationMode::LocalDirect);
        let name = name.trim();
        let primary = self.primary()?;
        if self.brings_projects() {
            let mut projects = vec![CreateWorkspaceForRepoInput {
                project_id: ProjectId(self.project_id.trim().to_string()),
                workspace_root: self.workspace_root.trim().to_string(),
                base_branch: self.base_branch.trim().to_string(),
                name: Some(primary.label.clone()),
                isolation_mode: isolation.clone(),
                context_folders: self.included_context.clone(),
            }];
            projects.extend(self.extra_options().into_iter().map(|option| {
                CreateWorkspaceForRepoInput {
                    project_id: ProjectId(option.project_id),
                    workspace_root: option.root_path,
                    base_branch: option.base_branch,
                    name: Some(option.label),
                    isolation_mode: isolation.clone(),
                    context_folders: Vec::new(),
                }
            }));
            return Some(Submission::Bundle {
                name: if name.is_empty() {
                    default_name.to_string()
                } else {
                    name.to_string()
                },
                projects,
            });
        }
        Some(Submission::Single(CreateWorkspaceForRepoInput {
            project_id: ProjectId(self.project_id.trim().to_string()),
            workspace_root: self.workspace_root.trim().to_string(),
            base_branch: self.base_branch.trim().to_string(),
            name: (!name.is_empty()).then(|| name.to_string()),
            isolation_mode: isolation,
            context_folders: self.included_context.clone(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(id: &str, branch: &str) -> Project {
        Project {
            id: format!("/code/{id}"),
            root_path: format!("/code/{id}"),
            display_name: id.to_uppercase(),
            icon: None,
            color: None,
            pinned_at: None,
            project_id: id.into(),
            name: id.into(),
            base_branch: branch.into(),
            missing: false,
        }
    }

    fn linked() -> HashMap<String, LinkedFolders> {
        HashMap::from([(
            "/code/app".to_string(),
            LinkedFolders {
                paths: vec!["/code/api".into(), "/code/lib".into(), "/notes".into()],
                context_paths: vec!["/notes".into()],
            },
        )])
    }

    #[test]
    fn it_opens_on_the_last_project_with_its_linked_folders_selected() {
        let projects = [project("api", "main"), project("app", "develop")];
        let model =
            OpenProjectModel::new(&projects, &linked(), false, true, Some("/code/app/"), false);
        assert_eq!(model.selected.as_deref(), Some("/code/app"));
        assert_eq!(model.base_branch, "develop");
        assert_eq!(
            model.extras,
            vec!["/code/api".to_string(), "linked:/code/lib".to_string()]
        );
        assert_eq!(model.included_context, vec!["/notes".to_string()]);
        assert_eq!(model.worktree_count(), 3);
        assert!(!model.can_remember(), "the saved folders, unchanged");
        let first = OpenProjectModel::new(&projects, &linked(), false, true, None, false);
        assert_eq!(first.selected.as_deref(), Some("/code/api"));
        assert!(first.extras.is_empty());
        let picker = OpenProjectModel::new(&projects, &linked(), true, true, None, true);
        assert_eq!(picker.selected, None);
        assert!(picker.adding);
    }

    #[test]
    fn a_picked_folder_becomes_the_project_and_reads_its_branches() {
        let projects = [project("api", "main")];
        let mut model = OpenProjectModel::new(&projects, &HashMap::new(), false, true, None, false);
        assert_eq!(
            model.pick_folder("/work/New Thing").as_deref(),
            Some("/work/New Thing")
        );
        assert_eq!(model.project_id, "new-thing");
        assert!(!model.can_submit(), "no base yet");
        // A stale answer for another root is dropped.
        model.branches_loaded("/code/api", Some(vec!["x".into()]));
        assert!(model.branches.is_empty());
        model.branches_loaded("/work/New Thing", Some(vec!["trunk".into(), "dev".into()]));
        assert_eq!(model.base_branch, "trunk");
        assert_eq!(model.single_options()[0].base_branch, "trunk");
        assert!(model.can_submit());
        let Some(Submission::Single(input)) = model.submission("  ", "Nova tarefa") else {
            panic!("one project");
        };
        assert_eq!(
            (input.workspace_root.as_str(), input.name),
            ("/work/New Thing", None)
        );
        // Picking a listed project's folder selects it.
        model.pick_folder("/code/api/");
        assert_eq!(model.selected.as_deref(), Some("/code/api"));
        assert_eq!(model.single_options().len(), 1);
    }

    #[test]
    fn other_projects_make_a_multi_project_task() {
        let projects = [project("api", "main"), project("app", "develop")];
        let mut model =
            OpenProjectModel::new(&projects, &linked(), false, false, Some("/code/app"), false);
        model.branches_loaded("/code/app", Some(vec!["develop".into(), "main".into()]));
        model.toggle_extra("linked:/code/lib");
        model.toggle_context("/notes");
        assert!(model.can_remember(), "lib and notes left out");
        let Some(Submission::Bundle { name, projects }) = model.submission("", "Nova tarefa")
        else {
            panic!("several projects");
        };
        assert_eq!(name, "Nova tarefa");
        assert_eq!(projects.len(), 2);
        assert_eq!(projects[0].name.as_deref(), Some("APP"));
        assert!(projects[0].context_folders.is_empty());
        assert_eq!(projects[1].project_id.0, "api");
        assert!(projects.iter().all(|project| matches!(
            project.isolation_mode,
            Some(WorkspaceIsolationMode::LocalDirect)
        )));
        model.toggle_extra("/code/api");
        assert!(
            matches!(model.submission("Pagamentos", "x"), Some(Submission::Single(input)) if input.name.as_deref() == Some("Pagamentos"))
        );
    }
}

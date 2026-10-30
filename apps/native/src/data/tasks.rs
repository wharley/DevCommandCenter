//! A task as the sidebar reads it: the desktop's `WorkspaceSummary`
//! (`workspaceToSummary` in `use-workspaces.ts`), with multi-project tasks
//! folded into their primary workspace (`navigationWorkspaces` in `App.tsx`).

use std::collections::{HashMap, HashSet};

use dcc_core::domain::attention::AttentionWorkspace;
use dcc_core::domain::repository::Repository;
use dcc_core::domain::workspace::Workspace;
use dcc_core::domain::workspace_bundle::WorkspaceBundleSummary;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskStatus {
    Ready,
    SetupPending,
    Initializing,
    Archived,
    Completed,
}

impl TaskStatus {
    fn parse(status: &str) -> Self {
        match status {
            "setup_pending" => Self::SetupPending,
            "initializing" => Self::Initializing,
            "archived" => Self::Archived,
            "completed" => Self::Completed,
            _ => Self::Ready,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::SetupPending => "setup_pending",
            Self::Initializing => "initializing",
            Self::Archived => "archived",
            Self::Completed => "completed",
        }
    }

    /// Archived and completed tasks cannot be opened from the list.
    pub fn is_selectable(self) -> bool {
        !matches!(self, Self::Archived | Self::Completed)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TaskSummary {
    pub id: String,
    pub name: String,
    pub branch: String,
    pub base_branch: String,
    pub status: TaskStatus,
    pub project_id: String,
    pub root_path: String,
    pub worktree_path: Option<String>,
    pub pinned_at: Option<String>,
    pub snoozed_until: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub bundle_id: Option<String>,
    pub member_workspace_ids: Vec<String>,
    pub member_names: Vec<String>,
    pub member_project_names: Vec<String>,
    pub context_folders: Vec<String>,
}

impl TaskSummary {
    /// A ready single-project task named after its id, for tests.
    #[cfg(test)]
    pub fn test(id: &str) -> Self {
        Self {
            id: id.into(),
            name: id.into(),
            branch: "main".into(),
            base_branch: "main".into(),
            status: TaskStatus::Ready,
            project_id: "p".into(),
            root_path: "/repo".into(),
            worktree_path: None,
            pinned_at: None,
            snoozed_until: None,
            created_at: "2026-10-23T15:00:00Z".into(),
            updated_at: "2026-10-23T15:00:00Z".into(),
            bundle_id: None,
            member_workspace_ids: vec![],
            member_names: vec![],
            member_project_names: vec![],
            context_folders: vec![],
        }
    }

    /// `untitled` is the localized "New task" used when nothing names the task.
    pub fn from_workspace(
        workspace: &Workspace,
        context_folders: Vec<String>,
        untitled: &str,
    ) -> Self {
        // The setup rule (a `setup_pending` task with no observed problem is
        // ready) lives in dcc-core, shared with the desktop and the phone.
        let status = TaskStatus::parse(&AttentionWorkspace::from_workspace(workspace).status);
        let explicit = workspace
            .name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty());
        let from_source = workspace.source.as_ref().and_then(|source| {
            source
                .title
                .as_deref()
                .map(str::trim)
                .filter(|title| !title.is_empty())
                .or_else(|| Some(source.head_branch.trim()).filter(|branch| !branch.is_empty()))
        });
        let branch = workspace
            .source
            .as_ref()
            .map(|source| source.head_branch.clone())
            .unwrap_or_else(|| workspace.base_branch.clone());
        Self {
            id: workspace.id.0.clone(),
            name: explicit.or(from_source).unwrap_or(untitled).to_string(),
            branch,
            base_branch: workspace.base_branch.clone(),
            status,
            project_id: workspace.project_id.0.clone(),
            root_path: workspace.root_path.clone(),
            worktree_path: workspace.worktree_path.clone(),
            pinned_at: workspace.pinned_at.clone(),
            snoozed_until: workspace.snoozed_until.clone(),
            created_at: workspace.created_at.clone(),
            updated_at: workspace.updated_at.clone(),
            bundle_id: None,
            member_workspace_ids: Vec::new(),
            member_names: Vec::new(),
            member_project_names: Vec::new(),
            context_folders,
        }
    }

    /// `workspaceRailDisplayTitle`: the name, or the humanized branch when
    /// the name is just the branch (imported branches).
    pub fn display_title(&self) -> String {
        let name = self.name.trim();
        let branch = self.branch.trim();
        if !name.is_empty() && name != branch {
            return name.to_string();
        }
        if branch.is_empty() {
            name.to_string()
        } else {
            humanize_branch(branch)
        }
    }

    /// `projectGroupingKey`
    pub fn project_key(&self) -> &str {
        let root = self.root_path.trim();
        if !root.is_empty() {
            return root;
        }
        self.worktree_path
            .as_deref()
            .map(str::trim)
            .filter(|path| !path.is_empty())
            .unwrap_or(self.project_id.trim())
    }
}

/// `humanizeWorkspaceBranchLabel`: `feat/add-login` → `Add Login`.
pub fn humanize_branch(branch: &str) -> String {
    let slug = branch.split_once('/').map_or(branch, |(_, rest)| rest);
    let spaced = slug.replace(['-', '_'], " ");
    let mut out = String::with_capacity(spaced.len());
    let mut at_word_start = true;
    for character in spaced.chars() {
        if at_word_start && character.is_ascii_alphanumeric() {
            out.extend(character.to_uppercase());
        } else {
            out.push(character);
        }
        // JavaScript's `\b\w`: ASCII letters, digits and `_` are word characters.
        at_word_start = !(character.is_ascii_alphanumeric() || character == '_');
    }
    out
}

/// A project as the rail draws it (`Repository` with its display name).
#[derive(Clone, Debug, PartialEq)]
pub struct Project {
    pub id: String,
    pub root_path: String,
    pub display_name: String,
    /// `projectIconValue`: mascot, symbol or logo data URI.
    pub icon: Option<String>,
    pub color: Option<String>,
    pub pinned_at: Option<String>,
    /// For starting a task from the picker (`repository.projectId`, `name`, `baseBranch`).
    pub project_id: String,
    pub name: String,
    pub base_branch: String,
    /// Its folder is gone from disk (deleted or moved in Finder): no task
    /// starts there, and removing it from DCC is offered.
    pub missing: bool,
}

impl Project {
    pub fn from_repository(repository: &Repository) -> Self {
        Self {
            id: repository.id.0.clone(),
            root_path: repository.root_path.clone(),
            display_name: repository_display_name(repository),
            icon: crate::project::icon_value(
                repository.icon.as_deref(),
                repository.logo.as_deref(),
            )
            .map(str::to_string),
            color: repository.color.clone(),
            pinned_at: repository.pinned_at.clone(),
            project_id: repository.project_id.0.clone(),
            name: repository.name.clone(),
            base_branch: repository.base_branch.clone(),
            missing: false,
        }
    }
}

pub use dcc_core::domain::repository::{path_leaf, repository_display_name};

/// Hides the extra members of multi-project tasks and gives the primary one
/// the bundle's name and member lists.
pub fn fold_bundles(
    tasks: Vec<TaskSummary>,
    bundles: &[WorkspaceBundleSummary],
    projects: &[Project],
) -> Vec<TaskSummary> {
    let secondary: HashSet<&str> = bundles
        .iter()
        .flat_map(|summary| {
            summary
                .members
                .iter()
                .map(|member| member.workspace_id.0.as_str())
                .filter(|id| *id != summary.bundle.primary_workspace_id.0)
        })
        .collect();
    let by_id: HashMap<&str, &TaskSummary> =
        tasks.iter().map(|task| (task.id.as_str(), task)).collect();
    let mut folded = Vec::with_capacity(tasks.len());
    for task in &tasks {
        if secondary.contains(task.id.as_str()) {
            continue;
        }
        let mut task = task.clone();
        if let Some(bundle) = bundles
            .iter()
            .find(|summary| summary.bundle.primary_workspace_id.0 == task.id)
        {
            let members: Vec<String> = bundle
                .members
                .iter()
                .map(|m| m.workspace_id.0.clone())
                .collect();
            task.name = bundle.bundle.name.clone();
            task.bundle_id = Some(bundle.bundle.id.0.clone());
            task.member_names = members
                .iter()
                .filter_map(|id| by_id.get(id.as_str()).map(|member| member.name.clone()))
                .collect();
            task.member_project_names = members
                .iter()
                .filter_map(|id| by_id.get(id.as_str()))
                .filter_map(|member| {
                    projects
                        .iter()
                        .find(|project| project.root_path == member.root_path)
                        .map(|project| project.display_name.clone())
                        .or_else(|| path_leaf(&member.root_path))
                })
                .collect();
            task.member_workspace_ids = members;
        }
        folded.push(task);
    }
    folded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branch_names_are_humanized() {
        assert_eq!(humanize_branch("feat/add-login_page"), "Add Login Page");
        assert_eq!(humanize_branch("main"), "Main");
    }

    #[test]
    fn path_leaf_ignores_trailing_slashes() {
        assert_eq!(path_leaf("/Users/me/projetos/dcc/").as_deref(), Some("dcc"));
        assert_eq!(path_leaf("   "), None);
    }
}

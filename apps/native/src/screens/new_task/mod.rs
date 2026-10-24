//! "Nova tarefa" (`NewTaskLaunchState`): pick a project and the task starts
//! there, in a protected worktree unless the person unticks it, bringing the
//! project's linked folders unless they say no. The desktop creates it (its
//! own creation, so its window shows it too); the new task then opens.

#[cfg(test)]
mod tests;
mod view;

use std::collections::HashMap;

use dcc_core::application::linked_task::LinkedFolders;
use gpui::{Context, EventEmitter, SharedString};

use crate::data::desktop_api::{ApiError, CreateTaskRequest, CreatedTask, DesktopApi};
use crate::data::setup_report::{Description, Tone, creation_notice};
use crate::data::tasks::Project;
use crate::i18n::{Args, Translate};
use crate::ui::Toaster;

/// What the shell does after the screen acted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NewTaskEvent {
    /// The desktop created this task: open it with the composer focused.
    Created(String),
    /// "Abrir outro projeto…" (straight to the folder picker) or
    /// "Selecionar vários projetos…", in the screen's worktree choice.
    OpenProject { multi: bool, worktree: bool },
}

pub struct NewTaskView {
    projects: Vec<Project>,
    /// Linked folders per project (by repository id).
    linked: HashMap<String, LinkedFolders>,
    /// Per task: a protected worktree unless unticked (`useWorktree`).
    pub(crate) worktree: bool,
    pub(crate) include_linked: bool,
    /// The project whose task is being created.
    pub(crate) pending: Option<String>,
    pub(crate) menu_open: bool,
    /// Tests point the screen at a fake desktop.
    pub(crate) endpoint: Option<DesktopApi>,
}

impl EventEmitter<NewTaskEvent> for NewTaskView {}

impl NewTaskView {
    pub fn new() -> Self {
        Self {
            projects: Vec::new(),
            linked: HashMap::new(),
            worktree: true,
            include_linked: true,
            pending: None,
            menu_open: false,
            endpoint: None,
        }
    }

    /// The projects the sidebar lists, idea folders already out.
    pub fn set_projects(
        &mut self,
        projects: &[Project],
        linked: &HashMap<String, LinkedFolders>,
        cx: &mut Context<Self>,
    ) {
        if self.projects == projects && &self.linked == linked {
            return;
        }
        self.projects = projects.to_vec();
        self.linked = linked.clone();
        cx.notify();
    }

    pub(crate) fn projects(&self) -> &[Project] {
        &self.projects
    }

    /// "+N" when the project links folders and they come along.
    pub(crate) fn linked_badge(&self, project: &Project) -> Option<usize> {
        let count = self.linked_count(project);
        (count > 0 && self.include_linked).then_some(count)
    }

    pub(crate) fn any_linked(&self) -> bool {
        self.projects
            .iter()
            .any(|project| self.linked_count(project) > 0)
    }

    fn linked_count(&self, project: &Project) -> usize {
        self.linked
            .get(&project.id)
            .map_or(0, |linked| linked.paths.len())
    }

    pub(crate) fn open_project(&mut self, multi: bool, cx: &mut Context<Self>) {
        self.menu_open = false;
        cx.emit(NewTaskEvent::OpenProject {
            multi,
            worktree: self.worktree,
        });
        cx.notify();
    }

    pub(crate) fn busy(&self) -> bool {
        self.pending.is_some()
    }

    /// Starts a task in `project` with the screen's choices.
    pub fn select_project(&mut self, project: &Project, cx: &mut Context<Self>) {
        let (worktree, include_linked) = (self.worktree, self.include_linked);
        self.create(project, worktree, include_linked, cx);
    }

    /// `handleQuickCreateTask`: also the sidebar's "+" on a project, which
    /// starts protected with the linked folders.
    pub fn create(
        &mut self,
        project: &Project,
        worktree: bool,
        include_linked_folders: bool,
        cx: &mut Context<Self>,
    ) {
        if self.busy() {
            return;
        }
        self.pending = Some(project.id.clone());
        self.menu_open = false;
        let request = CreateTaskRequest {
            project_id: project.project_id.clone(),
            workspace_root: project.root_path.clone(),
            base_branch: project.base_branch.clone(),
            worktree,
            include_linked_folders,
            bundle_name: cx.t("sidebar.newWorkspace").to_string(),
        };
        let endpoint = self.endpoint.clone();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    endpoint
                        .or_else(DesktopApi::load)
                        .ok_or(ApiError::DesktopClosed)?
                        .create_task(&request)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.pending = None;
                match result {
                    Ok(created) => {
                        notify(&created, false, cx);
                        cx.emit(NewTaskEvent::Created(created.workspace_id));
                    }
                    Err(ApiError::DesktopClosed) => {
                        Toaster::error(cx.t("native.desktopClosed"), cx)
                    }
                    Err(error) => Toaster::error_with(
                        cx.t("workspaceDialog.toastCreateError"),
                        error.to_string(),
                        cx,
                    ),
                }
                cx.notify();
            });
        })
        .detach();
    }
}

impl Default for NewTaskView {
    fn default() -> Self {
        Self::new()
    }
}

/// `notifyWorkspaceCreationResult`: created, with how its setup went.
pub(crate) fn notify(created: &CreatedTask, clone: bool, cx: &mut gpui::App) {
    let notice = creation_notice(&created.setup_report, &created.setup_hints, clone);
    let title = cx.t(notice.title_key);
    let description: Option<SharedString> =
        notice.description.map(|description| match description {
            Description::Key { key, commands } => {
                cx.t_with(key, &Args::default().with("commands", commands))
            }
            Description::Text(text) => text.into(),
        });
    match (notice.tone, description) {
        (Tone::Success, None) => Toaster::success(title, cx),
        (Tone::Success, Some(description)) => Toaster::success_with(title, description, cx),
        (Tone::Warning, description) => {
            Toaster::warning_with(title, description.unwrap_or_default(), cx)
        }
        (Tone::Error, Some(description)) => Toaster::error_with(title, description, cx),
        (Tone::Error, None) => Toaster::error(title, cx),
    }
}

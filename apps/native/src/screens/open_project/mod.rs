//! "Abrir projeto" (`CreateWorkspaceDialog` in "open" mode): the task's
//! project (listed, or any folder on disk), its base branch, the other
//! projects it brings, the worktree choice and an optional name. The
//! desktop creates the task; the dialog then closes and the task opens.

mod sources;
#[cfg(test)]
mod tests;
mod view;

use std::collections::HashMap;
use std::path::PathBuf;

use dcc_core::application::linked_task::LinkedFolders;
use gpui::{AppContext, Context, Entity, EventEmitter, PathPromptOptions, Subscription, Window};

use crate::data::desktop_api::{ApiError, DesktopApi};
use crate::data::open_project::{OpenProjectModel, Submission};
use crate::data::tasks::Project;
use crate::i18n::{Args, Translate};
use crate::ui::{TextInput, TextInputEvent, Toaster};

/// `dcc-last-task-repository-root-v1`
const LAST_ROOT_FILE: &str = "last-task-repository-root";

fn last_root_path() -> PathBuf {
    crate::prefs::native_file(LAST_ROOT_FILE)
}

fn read_last_root() -> Option<String> {
    if cfg!(test) {
        return None;
    }
    std::fs::read_to_string(last_root_path())
        .ok()
        .map(|root| root.trim().to_string())
        .filter(|root| !root.is_empty())
}

/// Best effort, like the desktop's storage.
fn remember_last_root(root: &str) {
    if cfg!(test) || root.trim().is_empty() {
        return;
    }
    let path = last_root_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, root.trim());
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpenProjectEvent {
    /// The desktop created this task: open it.
    Created(String),
    Closed,
}

/// `WorkspaceCreationMode`
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DialogMode {
    #[default]
    Open,
    /// "Clone from URL".
    Clone,
}

/// The project the dialog was opened from ("Abrir branch" on its menu).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepositoryContext {
    pub project_id: String,
    pub workspace_root: String,
    pub label: String,
}

/// `WorkspaceStart`: from a base, or on an existing branch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Start {
    New,
    Branch,
}

/// How the dialog was asked for.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OpenRequest {
    pub mode: DialogMode,
    /// "Selecionar vários projetos…": the project list starts open.
    pub multi: bool,
    pub worktree: bool,
    /// "Abrir outro projeto…": straight to the folder picker.
    pub pick_folder: bool,
    pub context: Option<RepositoryContext>,
}

pub struct OpenProjectDialog {
    pub(crate) model: OpenProjectModel,
    pub(crate) mode: DialogMode,
    pub(crate) context: Option<RepositoryContext>,
    pub(crate) start: Start,
    pub(crate) multi: bool,
    /// Clone: the repository URL, the project id and the optional base.
    pub(crate) repository_url: Entity<TextInput>,
    pub(crate) project_id: Entity<TextInput>,
    pub(crate) clone_base: Entity<TextInput>,
    /// Branch: the URL and, once validated, what it points at.
    pub(crate) source_url: Entity<TextInput>,
    pub(crate) resolution: Option<(String, crate::data::desktop_api::SourceResolution)>,
    pub(crate) resolving: bool,
    pub(crate) name: Entity<TextInput>,
    /// The repository path typed by hand when no project is listed.
    pub(crate) root: Entity<TextInput>,
    pub(crate) busy: bool,
    /// Tests point the dialog at a fake desktop.
    pub(crate) endpoint: Option<DesktopApi>,
    _events: Vec<Subscription>,
}

impl EventEmitter<OpenProjectEvent> for OpenProjectDialog {}

impl OpenProjectDialog {
    pub fn new(
        projects: &[Project],
        linked: &HashMap<String, LinkedFolders>,
        request: OpenRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let open = request.mode == DialogMode::Open;
        // `initialTaskRepository(repositories, repositoryContext.workspaceRoot, last)`.
        let first_root = request
            .context
            .as_ref()
            .map(|context| context.workspace_root.clone())
            .or_else(read_last_root);
        let model = OpenProjectModel::new(
            projects,
            linked,
            open && request.multi,
            request.worktree,
            first_root.as_deref(),
            request.pick_folder || !open,
        );
        let placeholder = cx.t("workspaceDialog.defaultsFromBranch");
        let name = cx.new(|cx| TextInput::new(window, cx).with_placeholder(placeholder));
        let root_hint = if open {
            "/path/to/git/repo"
        } else {
            "/path/to/clone/destination"
        };
        let root = cx.new(|cx| TextInput::new(window, cx).with_placeholder(root_hint));
        let repository_url = cx.new(|cx| {
            TextInput::new(window, cx).with_placeholder("https://github.com/org/repo.git")
        });
        let project_id = cx.new(|cx| TextInput::new(window, cx).with_placeholder("dcc-demo"));
        let base_hint = cx.t("workspaceDialog.autoDetectPlaceholder");
        let clone_base = cx.new(|cx| TextInput::new(window, cx).with_placeholder(base_hint));
        let source_hint = cx.t("workspaceDialog.sourceUrlPlaceholder");
        let source_url = cx.new(|cx| TextInput::new(window, cx).with_placeholder(source_hint));
        let refresh = |_: &mut Self,
                       _: &Entity<TextInput>,
                       event: &TextInputEvent,
                       _: &mut Window,
                       cx: &mut Context<Self>| {
            if *event == TextInputEvent::Changed {
                cx.notify();
            }
        };
        let events = vec![
            cx.subscribe_in(&repository_url, window, refresh),
            cx.subscribe_in(&project_id, window, refresh),
            cx.subscribe_in(&clone_base, window, refresh),
            // A changed URL needs validating again.
            cx.subscribe_in(
                &source_url,
                window,
                |dialog, _, event: &TextInputEvent, _, cx| {
                    if *event == TextInputEvent::Changed {
                        dialog.resolution = None;
                        cx.notify();
                    }
                },
            ),
            cx.subscribe_in(
                &root,
                window,
                |dialog, input, event: &TextInputEvent, _, cx| {
                    if dialog.mode == DialogMode::Clone {
                        if *event == TextInputEvent::Changed {
                            cx.notify();
                        }
                        return;
                    }
                    if matches!(event, TextInputEvent::Submitted | TextInputEvent::Blurred) {
                        let path = input.read(cx).text().trim().to_string();
                        if !path.is_empty() && path != dialog.model.workspace_root {
                            dialog.use_folder(path, cx);
                        }
                    }
                },
            ),
            cx.subscribe_in(&name, window, |dialog, _, event: &TextInputEvent, _, cx| {
                if *event == TextInputEvent::Submitted {
                    dialog.submit(cx);
                }
            }),
        ];
        let start = if request.context.is_some() && open {
            Start::Branch
        } else {
            Start::New
        };
        let mut dialog = Self {
            model,
            mode: request.mode,
            context: request.context.clone().filter(|_| open),
            start,
            multi: request.multi,
            repository_url,
            project_id,
            clone_base,
            source_url,
            resolution: None,
            resolving: false,
            name,
            root,
            busy: false,
            endpoint: None,
            _events: events,
        };
        let root = dialog.model.workspace_root.clone();
        if open && !root.is_empty() {
            dialog.load_branches(root, cx);
        }
        if open && request.pick_folder {
            dialog.pick_folder(cx);
        }
        dialog
    }

    pub(crate) fn close(&mut self, cx: &mut Context<Self>) {
        if !self.busy {
            cx.emit(OpenProjectEvent::Closed);
        }
    }

    pub(crate) fn select(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(root) = self.model.select(id) {
            remember_last_root(&root);
            self.load_branches(root, cx);
        }
        cx.notify();
    }

    fn use_folder(&mut self, path: String, cx: &mut Context<Self>) {
        if let Some(root) = self.model.pick_folder(&path) {
            remember_last_root(&root);
            self.load_branches(root, cx);
        }
        cx.notify();
    }

    /// "Outra pasta" / "Escolher pasta".
    pub(crate) fn pick_folder(&mut self, cx: &mut Context<Self>) {
        let clone = self.mode == DialogMode::Clone;
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(cx.t(if clone {
                "workspaceDialog.pickFolderClone"
            } else {
                "workspaceDialog.pickFolderRepo"
            })),
        });
        cx.spawn(async move |this, cx| {
            let picked = paths.await;
            let _ = this.update(cx, |dialog, cx| match picked {
                Ok(Ok(Some(paths))) => {
                    if let Some(path) = paths.into_iter().next() {
                        let path = path.to_string_lossy().into_owned();
                        if clone {
                            dialog.clone_destination(path, cx);
                        } else {
                            dialog.use_folder(path, cx);
                        }
                    }
                }
                Ok(Ok(None)) => {}
                Ok(Err(error)) => Toaster::error_with(
                    cx.t("workspaceDialog.toastPickerError"),
                    error.to_string(),
                    cx,
                ),
                Err(_) => {}
            });
        })
        .detach();
    }

    /// `listLocalBranches`, read here (git only, nothing changes).
    fn load_branches(&mut self, root: String, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let path = root.clone();
            let branches = cx
                .background_executor()
                .spawn(async move { dcc_infra::git::list_local_branch_names(&path) })
                .await;
            let _ = this.update(cx, |dialog, cx| {
                match branches {
                    Ok(branches) => dialog.model.branches_loaded(&root, Some(branches)),
                    Err(error) => {
                        dialog.model.branches_loaded(&root, None);
                        Toaster::error_with(
                            cx.t("workspaceDialog.toastLoadBranchesError"),
                            error.to_string(),
                            cx,
                        );
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn submit(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        if self.mode == DialogMode::Clone {
            return self.submit_clone(cx);
        }
        if self.is_source() {
            return self.submit_source(cx);
        }
        let name = self.name.read(cx).text().to_string();
        let Some(submission) = self.model.submission(&name, &cx.t("sidebar.newWorkspace")) else {
            return;
        };
        // "Lembrar estas pastas": the folders this task brings, saved on its project.
        let remember = (self.model.remember && self.model.can_remember())
            .then(|| {
                self.model
                    .primary()
                    .map(|primary| (primary.id, self.model.task_folders()))
            })
            .flatten();
        let root = self.model.workspace_root.clone();
        let local = !self.model.worktree;
        let endpoint = self.endpoint.clone();
        self.busy = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let api = endpoint
                        .or_else(DesktopApi::load)
                        .ok_or(ApiError::DesktopClosed)?;
                    let created = match &submission {
                        Submission::Single(input) => Created::Single(api.create_workspace(input)?),
                        Submission::Bundle { name, projects } => {
                            let (id, count) = api.create_workspace_bundle(name, projects)?;
                            Created::Bundle { id, count }
                        }
                    };
                    let remembered =
                        remember.map(|(project, folders)| save_linked_folders(&project, &folders));
                    Ok::<_, ApiError>((created, remembered))
                })
                .await;
            let _ = this.update(cx, |dialog, cx| {
                dialog.busy = false;
                match result {
                    Ok((created, remembered)) => {
                        let id = match created {
                            Created::Single(task) => {
                                crate::screens::new_task::notify(&task, false, cx);
                                task.workspace_id
                            }
                            Created::Bundle { id, count } => {
                                let key = if local {
                                    "workspaceDialog.multiLocalToastDescription"
                                } else {
                                    "workspaceDialog.multiToastDescription"
                                };
                                Toaster::success_with(
                                    cx.t("workspaceDialog.multiToastSuccess"),
                                    cx.t_with(key, &Args::count(count as i64)),
                                    cx,
                                );
                                id
                            }
                        };
                        if let Some(Err(error)) = remembered {
                            Toaster::error_with(
                                cx.t("workspaceDialog.rememberLinkedFoldersError"),
                                error,
                                cx,
                            );
                        }
                        remember_last_root(&root);
                        cx.emit(OpenProjectEvent::Created(id));
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

enum Created {
    Single(crate::data::desktop_api::CreatedTask),
    Bundle { id: String, count: usize },
}

/// `setRepositoryLinkedFolders`: a project setting, written here.
fn save_linked_folders(project: &str, folders: &[String]) -> Result<(), String> {
    if cfg!(test) {
        return Ok(());
    }
    let repo = dcc_infra::db::SqliteWorkspaceRepo::open(crate::data::loader::database_path())
        .map_err(|error| error.to_string())?;
    futures::executor::block_on(dcc_infra::project_actions::set_repository_linked_folders(
        &repo, project, folders,
    ))
    .map(|_| ())
}

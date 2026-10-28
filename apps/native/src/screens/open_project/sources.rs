//! The dialog's other two ways in: "Clonar" (a remote repository into a
//! folder, with a task in it) and "Abrir branch" (a task on an existing
//! branch of the project it was opened from; a PR URL goes to the PR hub).

use dcc_core::application::CreateWorkspaceFromUrlInput;
use dcc_core::application::linked_task::infer_project_id_from_workspace_root;
use dcc_core::domain::project::ProjectId;
use gpui::{App, Context};

use super::{DialogMode, OpenProjectDialog, OpenProjectEvent, Start, remember_last_root};
use crate::data::desktop_api::{ApiError, CreatedTask, DesktopApi};
use crate::i18n::Translate;
use crate::ui::Toaster;

fn text(input: &gpui::Entity<crate::ui::TextInput>, cx: &App) -> String {
    input.read(cx).text().trim().to_string()
}

impl OpenProjectDialog {
    /// `isSourceWorkspace`: opened from a project, on an existing branch.
    pub(crate) fn is_source(&self) -> bool {
        self.mode == DialogMode::Open && self.context.is_some() && self.start == Start::Branch
    }

    /// `canSubmit` for each way in.
    pub(crate) fn can_submit(&self, cx: &App) -> bool {
        if self.busy {
            return false;
        }
        match self.mode {
            DialogMode::Clone => {
                !text(&self.project_id, cx).is_empty()
                    && !text(&self.root, cx).is_empty()
                    && !text(&self.repository_url, cx).is_empty()
            }
            DialogMode::Open if self.is_source() => {
                !self.resolving
                    && self
                        .resolution
                        .as_ref()
                        .is_some_and(|(url, _)| *url == text(&self.source_url, cx))
            }
            DialogMode::Open => self.model.can_submit(),
        }
    }

    pub(crate) fn set_start(&mut self, start: Start, cx: &mut Context<Self>) {
        if self.busy || self.resolving {
            return;
        }
        self.start = start;
        cx.notify();
    }

    /// The clone's destination; the project id follows the folder unless
    /// the person typed one.
    pub(crate) fn clone_destination(&mut self, path: String, cx: &mut Context<Self>) {
        self.root
            .update(cx, |root, cx| root.set_text(path.clone(), cx));
        if text(&self.project_id, cx).is_empty() {
            let id = infer_project_id_from_workspace_root(&path);
            self.project_id
                .update(cx, |input, cx| input.set_text(id, cx));
        }
        cx.notify();
    }

    /// "Validar": only a branch is opened here.
    pub(crate) fn validate_source(&mut self, cx: &mut Context<Self>) {
        let Some(context) = self.context.clone() else {
            return;
        };
        let url = text(&self.source_url, cx);
        if url.is_empty() || self.resolving {
            return;
        }
        self.resolving = true;
        self.resolution = None;
        let endpoint = self.endpoint.clone();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let asked = url.clone();
            let result = cx
                .background_executor()
                .spawn(async move {
                    endpoint
                        .or_else(DesktopApi::load)
                        .ok_or(ApiError::DesktopClosed)?
                        .resolve_source_url(&context.workspace_root, &asked)
                })
                .await;
            let _ = this.update(cx, |dialog, cx| {
                dialog.resolving = false;
                match result {
                    Ok(resolution) if resolution.kind == "branch" => {
                        dialog.resolution = Some((url, resolution));
                    }
                    Ok(_) => Toaster::error_with(
                        cx.t("workspaceDialog.pullRequestUrlTitle"),
                        cx.t("workspaceDialog.pullRequestUrlDescription"),
                        cx,
                    ),
                    Err(ApiError::DesktopClosed) => {
                        Toaster::error(cx.t("native.desktopClosed"), cx)
                    }
                    Err(error) => Toaster::error_with(
                        cx.t("workspaceDialog.sourceValidationError"),
                        error.to_string(),
                        cx,
                    ),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn submit_clone(&mut self, cx: &mut Context<Self>) {
        if !self.can_submit(cx) {
            return;
        }
        let name = text(&self.name, cx);
        let input = CreateWorkspaceFromUrlInput {
            project_id: ProjectId(text(&self.project_id, cx)),
            repository_url: text(&self.repository_url, cx),
            workspace_root: text(&self.root, cx),
            base_branch: text(&self.clone_base, cx),
            name: (!name.is_empty()).then_some(name),
        };
        self.run_single(true, move |api| api.clone_workspace(&input), cx);
    }

    pub(super) fn submit_source(&mut self, cx: &mut Context<Self>) {
        let Some(context) = self.context.clone() else {
            return;
        };
        if !self.can_submit(cx) {
            return;
        }
        let (url, name) = (text(&self.source_url, cx), text(&self.name, cx));
        self.run_single(
            false,
            move |api| {
                api.create_from_source_url(
                    &context.project_id,
                    &context.workspace_root,
                    &url,
                    (!name.is_empty()).then_some(name.as_str()),
                    None,
                )
            },
            cx,
        );
    }

    /// One task from the desktop, its notice, then it opens.
    fn run_single(
        &mut self,
        clone: bool,
        job: impl FnOnce(&DesktopApi) -> Result<CreatedTask, ApiError> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        let endpoint = self.endpoint.clone();
        self.busy = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    job(&endpoint
                        .or_else(DesktopApi::load)
                        .ok_or(ApiError::DesktopClosed)?)
                })
                .await;
            let _ = this.update(cx, |dialog, cx| {
                dialog.busy = false;
                match result {
                    Ok(task) => {
                        crate::screens::new_task::notify(&task, clone, cx);
                        if !clone && let Some(context) = &dialog.context {
                            remember_last_root(&context.workspace_root);
                        }
                        cx.emit(OpenProjectEvent::Created(task.workspace_id));
                    }
                    Err(ApiError::DesktopClosed) => {
                        Toaster::error(cx.t("native.desktopClosed"), cx)
                    }
                    Err(error) => Toaster::error_with(
                        cx.t(if clone {
                            "workspaceDialog.toastCloneError"
                        } else {
                            "workspaceDialog.toastCreateError"
                        }),
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

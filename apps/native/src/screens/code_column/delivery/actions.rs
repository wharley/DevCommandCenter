//! What the header's button does in each mode (`handleInspectorCommit`),
//! the confirmed merge and sync (`executeConfirmed…`), and the task's
//! completion once its pull request is merged. The toasts say what the
//! desktop's say, in its words.

use std::time::Duration;

use dcc_core::domain::commit_mode::{CommitButtonStatus, CommitMode, should_auto_complete_merged};
use gpui::{App, AppContext, Context, Focusable, SharedString, Window};

use super::super::CodeColumn;
use super::{Confirm, Preview, PreviewOrigin};
use crate::data::desktop_api::{ApiError, DesktopApi};
use crate::i18n::{Args, Translate};
use crate::ui::text_area::TextArea;
use crate::ui::{ToastKind, Toaster};

/// `inspectorActionTitle`
fn action_title(mode: CommitMode, label: &str, cx: &App) -> SharedString {
    let args = Args::default().with("requestLabel", label);
    let key = match mode {
        CommitMode::CreatePr => "inspector.gitAction.createRequest",
        CommitMode::OpenPr => "inspector.gitAction.openRequest",
        CommitMode::CommitAndPush => "inspector.gitAction.commitAndPush",
        CommitMode::Push => "inspector.gitAction.push",
        CommitMode::Fix => "inspector.gitAction.fixCi",
        CommitMode::ResolveConflicts => "inspector.gitAction.resolveConflicts",
        CommitMode::CompleteMerge => "inspector.gitAction.completeMerge",
        CommitMode::Merge => "inspector.gitAction.merge",
        CommitMode::Merged => "inspector.gitAction.merged",
        CommitMode::Closed => "inspector.gitAction.closed",
    };
    cx.t_with(key, &args)
}

/// What the step's toast ends on.
enum Outcome {
    Done(ToastKind, SharedString),
    /// The before-push checks failed: the push did not run.
    Blocked,
}

fn checks_failed(error: &ApiError) -> bool {
    matches!(error, ApiError::Refused { code, .. } if code == "checks_failed")
}

impl CodeColumn {
    pub(super) fn api(&self) -> Result<DesktopApi, ApiError> {
        self.endpoint
            .clone()
            .or_else(DesktopApi::load)
            .ok_or(ApiError::DesktopClosed)
    }

    pub(super) fn task_id(&self) -> Option<String> {
        self.delivery.task.as_ref().map(|task| task.id.0.clone())
    }

    /// The button's status: busy while it runs, then "done" for 900ms or
    /// "error" for 1.2s.
    fn settle_button(&mut self, status: CommitButtonStatus, cx: &mut Context<Self>) {
        self.delivery.button = Some(status);
        cx.notify();
        let wait = match status {
            CommitButtonStatus::Done => Duration::from_millis(900),
            CommitButtonStatus::Error => Duration::from_millis(1200),
            _ => return,
        };
        self._button = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(wait).await;
            let _ = this.update(cx, |column, cx| {
                column.delivery.button = None;
                cx.notify();
            });
        }));
    }

    /// The button clicked (`WorkspaceCommitButton`'s `onClick`).
    pub(crate) fn press_commit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mode = self.commit_mode();
        if matches!(mode, CommitMode::Merged | CommitMode::Closed)
            || self.delivery.button() == CommitButtonStatus::Busy
        {
            return;
        }
        match mode {
            CommitMode::CommitAndPush => self.open_preview(window, cx),
            CommitMode::Merge => {
                // `handleInspectorCommit` only asks; the button says "done".
                self.delivery.confirm = Some(Confirm::Merge);
                self.settle_button(CommitButtonStatus::Done, cx);
            }
            CommitMode::OpenPr | CommitMode::Fix => self.open_request(mode, cx),
            CommitMode::Push | CommitMode::CreatePr => self.run_step(mode, None, cx),
            CommitMode::ResolveConflicts => self.open_merge_resolver(cx),
            CommitMode::CompleteMerge => {
                self.delivery.confirm = Some(Confirm::CompleteMerge);
                self.settle_button(CommitButtonStatus::Done, cx);
            }
            CommitMode::Merged | CommitMode::Closed => {}
        }
    }

    /// "Commitar e enviar" first shows the message (`onPrepareCommitMessage`).
    fn open_preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.task_id() else {
            return;
        };
        let api = self.api();
        self.delivery.button = Some(CommitButtonStatus::Busy);
        cx.notify();
        self._delivery = Some(cx.spawn_in(window, async move |this, cx| {
            let suggestion = cx
                .background_executor()
                .spawn(async move { api?.prepare_commit(&id) })
                .await;
            let _ = this.update_in(cx, |column, window, cx| match suggestion {
                Ok(suggestion) => {
                    let subject = cx.new(|cx| {
                        let mut area = TextArea::new(window, cx);
                        area.set_text(&suggestion.subject, cx);
                        area
                    });
                    let body = cx.new(|cx| {
                        let mut area = TextArea::new(window, cx);
                        area.set_text(suggestion.body.as_deref().unwrap_or_default(), cx);
                        area
                    });
                    let focus = subject.read(cx).focus_handle(cx);
                    focus.focus(window, cx);
                    column.delivery.preview = Some(Preview {
                        origin: PreviewOrigin::Review,
                        subject,
                        body,
                        staged_files: suggestion.staged_file_count,
                        fingerprint: suggestion.staged_fingerprint,
                    });
                    column.delivery.button = None;
                    column.refresh_git(cx);
                    cx.notify();
                }
                Err(error) => {
                    report(
                        error,
                        CommitMode::CommitAndPush,
                        column.delivery.request_label(),
                        cx,
                    );
                    column.settle_button(CommitButtonStatus::Error, cx);
                }
            });
        }));
    }

    pub(crate) fn close_preview(&mut self, cx: &mut Context<Self>) {
        if self.delivery.button() != CommitButtonStatus::Busy && !self.delivery.busy {
            self.delivery.preview = None;
            self.delivery.preparing = None;
            self.delivery.drafting = None;
            cx.notify();
        }
    }

    /// "Commitar e enviar" in the preview: the message as edited.
    pub(crate) fn confirm_preview(&mut self, cx: &mut Context<Self>) {
        let Some(preview) = &self.delivery.preview else {
            return;
        };
        let message = preview.subject.read(cx).text().trim().to_string();
        if message.is_empty() {
            return;
        }
        let body = preview.body.read(cx).text().to_string();
        let fingerprint = preview.fingerprint.clone();
        if let PreviewOrigin::Dock { push } = preview.origin {
            self.delivery.preview = None;
            self.delivery.drafting = None;
            self.dock_commit(message, body, fingerprint, push, cx);
            return;
        }
        self.run_step(
            CommitMode::CommitAndPush,
            Some((message, body, fingerprint)),
            cx,
        );
    }

    /// "Abrir PR" / "Corrigir CI": the pull request in the browser.
    fn open_request(&mut self, mode: CommitMode, cx: &mut Context<Self>) {
        let label = self.delivery.request_label();
        match self.delivery.pr.as_ref().and_then(|pr| pr.url.clone()) {
            Some(url) => {
                cx.open_url(&url);
                if mode == CommitMode::Fix {
                    Toaster::info(
                        format!("Abra o {label} para inspecionar checks e conflitos."),
                        cx,
                    );
                } else {
                    Toaster::success(format!("{label} aberto no navegador"), cx);
                }
                self.settle_button(CommitButtonStatus::Done, cx);
            }
            None => {
                let message = cx.t_with(
                    "inspector.gitAction.failed",
                    &Args::default()
                        .with("action", action_title(mode, label, cx).to_string())
                        .with("message", format!("no {label} for this branch")),
                );
                Toaster::error(message, cx);
                self.settle_button(CommitButtonStatus::Error, cx);
            }
        }
    }

    /// A step in the desktop with its loading toast: push, create the pull
    /// request, or the reviewed commit (and its push).
    fn run_step(
        &mut self,
        mode: CommitMode,
        reviewed: Option<(String, String, String)>,
        cx: &mut Context<Self>,
    ) {
        let Some(id) = self.task_id() else {
            return;
        };
        if self.delivery.busy {
            return;
        }
        let label = self.delivery.request_label();
        let has_changes = self
            .status
            .as_ref()
            .is_some_and(|status| !status.staged.is_empty() || !status.unstaged.is_empty());
        let local_only = self
            .status
            .as_ref()
            .is_some_and(|status| !status.has_remote);
        let title = action_title(mode, label, cx);
        let toast = Toaster::loading(format!("{title}..."), cx);
        if mode == CommitMode::Push || (mode == CommitMode::CommitAndPush && !local_only) {
            Toaster::settle(
                toast,
                ToastKind::Loading,
                cx.t("automation.beforePushRunning"),
                None,
                cx,
            );
        }
        let committed = cx.t("composer.executionDock.actions.committed");
        let api = self.api();
        self.delivery.busy = true;
        self.delivery.button = Some(CommitButtonStatus::Busy);
        cx.notify();
        self._delivery = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let api = api?;
                    let outcome = match (mode, reviewed) {
                        (CommitMode::Push, _) => api
                            .deliver(&id, "push")
                            .map(|()| Outcome::Done(ToastKind::Success, "Pushed".into())),
                        (CommitMode::CreatePr, _) if has_changes => Err(ApiError::Failed(format!(
                            "Commit local changes before creating a {label}."
                        ))),
                        (CommitMode::CreatePr, _) => api.create_request(&id).map(|()| {
                            Outcome::Done(ToastKind::Success, format!("{label} criado").into())
                        }),
                        (_, Some((message, body, fingerprint))) => {
                            let body = Some(body.as_str()).filter(|body| !body.trim().is_empty());
                            api.commit_reviewed(&id, &message, body, &fingerprint, !local_only)
                                .map(|()| {
                                    let text: SharedString = if local_only {
                                        committed.clone()
                                    } else {
                                        "Committed and pushed".into()
                                    };
                                    Outcome::Done(ToastKind::Success, text)
                                })
                        }
                        _ => return Ok(None),
                    };
                    match outcome {
                        Err(error) if checks_failed(&error) => Ok(Some(Outcome::Blocked)),
                        other => other.map(Some),
                    }
                })
                .await;
            let _ = this.update(cx, |column, cx| {
                column.delivery.busy = false;
                match result {
                    Ok(Some(Outcome::Done(kind, text))) => {
                        Toaster::settle(toast, kind, text, None, cx);
                        column.delivery.preview = None;
                        column.delivery.drafting = None;
                        column.settle_button(CommitButtonStatus::Done, cx);
                    }
                    Ok(Some(Outcome::Blocked)) => {
                        Toaster::settle(
                            toast,
                            ToastKind::Error,
                            cx.t("automation.beforePushBlocked"),
                            None,
                            cx,
                        );
                        column.delivery.preview = None;
                        column.delivery.drafting = None;
                        column.settle_button(CommitButtonStatus::Done, cx);
                    }
                    Ok(None) => {
                        Toaster::dismiss_id(toast, cx);
                        column.delivery.button = None;
                    }
                    Err(error) => {
                        let message = failure(error, mode, label, cx);
                        Toaster::settle(toast, ToastKind::Error, message, None, cx);
                        column.settle_button(CommitButtonStatus::Error, cx);
                    }
                }
                column.refresh_git(cx);
                cx.notify();
            });
        }));
    }

    pub(crate) fn cancel_confirm(&mut self, cx: &mut Context<Self>) {
        self.delivery.confirm = None;
        cx.notify();
    }

    /// The confirmation's button: merge, or bring the base in.
    pub(crate) fn run_confirmed(&mut self, cx: &mut Context<Self>) {
        match self.delivery.confirm.take() {
            Some(Confirm::Merge) => self.merge(cx),
            Some(Confirm::SyncBase) => self.sync_base(cx),
            Some(Confirm::CompleteMerge) => self.complete_merge(cx),
            None => {}
        }
        cx.notify();
    }

    pub(crate) fn ask_sync_base(&mut self, cx: &mut Context<Self>) {
        self.delivery.confirm = Some(Confirm::SyncBase);
        cx.notify();
    }

    /// `executeConfirmedMerge`: the merge, then the task completed.
    fn merge(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.task_id() else {
            return;
        };
        let label = self.delivery.request_label();
        let args = Args::default().with("requestLabel", label);
        let toast = Toaster::loading(
            cx.t_with("inspector.gitConfirmation.mergeLoading", &args),
            cx,
        );
        let api = self.api();
        self.delivery.busy = true;
        cx.notify();
        self._delivery = Some(cx.spawn(async move |this, cx| {
            let (merged, completed) = cx
                .background_executor()
                .spawn(async move {
                    let api = api?;
                    api.deliver(&id, "merge")?;
                    Ok::<_, ApiError>(api.deliver(&id, "complete"))
                })
                .await
                .map_or_else(
                    |error| (Err(error), Ok(())),
                    |completed| (Ok(()), completed),
                );
            let _ = this.update(cx, |column, cx| {
                column.delivery.busy = false;
                match merged {
                    Ok(()) => {
                        let text = cx.t_with("inspector.gitConfirmation.mergeSuccess", &args);
                        Toaster::settle(toast, ToastKind::Success, text, None, cx);
                        if let Err(error) = completed {
                            Toaster::error(complete_error(error, cx), cx);
                        } else if let Some(id) = column.task_id() {
                            column.delivery.completed = Some(id);
                        }
                    }
                    Err(error) => {
                        let text = cx.t_with(
                            "inspector.gitConfirmation.mergeFailed",
                            &args.clone().with("message", message_of(error, cx)),
                        );
                        Toaster::settle(toast, ToastKind::Error, text, None, cx);
                    }
                }
                column.refresh_git(cx);
                cx.notify();
            });
        }));
    }

    /// `executeConfirmedSyncBase`: the base merged in (the PR's base when it
    /// has one).
    fn sync_base(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.task_id() else {
            return;
        };
        let base = self
            .delivery
            .pr
            .as_ref()
            .and_then(|pr| pr.base_branch.clone());
        let toast = Toaster::loading(cx.t("inspector.gitConfirmation.syncLoading"), cx);
        let api = self.api();
        self.delivery.syncing = true;
        cx.notify();
        self._delivery = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api?.sync_base(&id, base.as_deref()) })
                .await;
            let _ = this.update(cx, |column, cx| {
                column.delivery.syncing = false;
                match result {
                    Ok(synced) => {
                        let base_ref = format!("{}/{}", synced.remote, synced.base_branch);
                        let args = Args::default().with("baseRef", base_ref);
                        let (kind, key) = if synced.updated {
                            (ToastKind::Success, "inspector.gitConfirmation.syncSuccess")
                        } else {
                            (
                                ToastKind::Info,
                                "inspector.gitConfirmation.syncAlreadyCurrent",
                            )
                        };
                        Toaster::settle(toast, kind, cx.t_with(key, &args), None, cx);
                    }
                    Err(error) => {
                        let text = cx.t_with(
                            "inspector.gitConfirmation.syncFailed",
                            &Args::default().with("message", message_of(error, cx)),
                        );
                        Toaster::settle(toast, ToastKind::Error, text, None, cx);
                    }
                }
                column.refresh_git(cx);
                cx.notify();
            });
        }));
    }

    /// A merged pull request completes the task that has its own worktree
    /// (`shouldAutoCompleteMergedWorkspace`), once.
    pub(crate) fn complete_if_merged(&mut self, cx: &mut Context<Self>) {
        let Some(task) = self.delivery.task.clone() else {
            return;
        };
        if self.delivery.completed.as_deref() == Some(task.id.0.as_str())
            || !should_auto_complete_merged(
                self.commit_mode(),
                &task.state,
                task.worktree_path.is_some(),
            )
        {
            return;
        }
        self.delivery.completed = Some(task.id.0.clone());
        let api = self.api();
        let id = task.id.0;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api?.deliver(&id, "complete") })
                .await;
            let _ = this.update(cx, |column, cx| {
                if let Err(error) = result {
                    // Let a later read try again.
                    column.delivery.completed = None;
                    Toaster::error(complete_error(error, cx), cx);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// The status, branch and pull request read again after a step.
    pub(crate) fn refresh_git(&mut self, cx: &mut Context<Self>) {
        let Some(root) = self.root.clone() else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let path = root.clone();
            let status = cx
                .background_executor()
                .spawn(async move { dcc_infra::workspace_git::read_workspace_git_status(&path) })
                .await;
            let _ = this.update(cx, |column, cx| {
                if column.root.as_deref() != Some(root.as_str()) {
                    return;
                }
                if let Ok(status) = status {
                    column.apply_status(status, cx);
                }
                column.read_branch(cx);
                column.read_pull_request(true, cx);
                column.read_review(true, cx);
            });
        })
        .detach();
    }
}

pub(crate) fn message_of(error: ApiError, cx: &App) -> String {
    error.text(cx)
}

/// `inspector.gitAction.failed`: "{action} falhou: {message}".
fn failure(error: ApiError, mode: CommitMode, label: &str, cx: &App) -> SharedString {
    cx.t_with(
        "inspector.gitAction.failed",
        &Args::default()
            .with("action", action_title(mode, label, cx).to_string())
            .with("message", message_of(error, cx)),
    )
}

fn report(error: ApiError, mode: CommitMode, label: &str, cx: &mut App) {
    let message = failure(error, mode, label, cx);
    Toaster::error(message, cx);
}

fn complete_error(error: ApiError, cx: &App) -> SharedString {
    cx.t_with(
        "sidebar.completeWorkspaceError",
        &Args::default().with("message", message_of(error, cx)),
    )
}

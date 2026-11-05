//! The conversation header's delivery (`WorkspaceDeliveryControls` run by
//! `useWorkspaceDelivery`): what its two buttons show, from this column's
//! status, branch and pull request (read whether the column shows or not,
//! as the desktop's panel hoists them), and what each step of its menu
//! does. Its toasts are the dock's own: no loading toast for a step, "A
//! ação git falhou" with the reason, and a push the remote is ahead of
//! offers "Atualizar e enviar".

use std::collections::HashSet;
use std::time::Duration;

use dcc_core::domain::commit_mode::CommitMode;
use dcc_core::domain::execution_dock::{
    DockAction, DockActionContext, DockActionId, DockActionKind, DockChangeSummary, DockGitState,
    DockRunMode, DockStatus, resolve_dock_actions, resolve_dock_status,
};
use dcc_core::domain::workspace_git::WorkspaceGitChangeEntry;
use gpui::{App, AppContext, Context, Focusable, SharedString, Window};

use super::super::{CodeColumn, CodeColumnEvent, Loaded};
use super::actions::message_of;
use super::{Confirm, Preview, PreviewOrigin};
use crate::data::desktop_api::ApiError;
use crate::i18n::{Args, Translate};
use crate::ui::text_area::TextArea;
use crate::ui::{ToastKind, Toaster};

/// How long "tem commits que você não tem" stays (`duration: 15_000`).
const BEHIND_REMOTE: Duration = Duration::from_secs(15);

/// What the header's delivery controls draw.
#[derive(Clone, Debug, PartialEq)]
pub struct DockControls {
    pub status: DockStatus,
    /// The menu's steps, the next one first.
    pub actions: Vec<DockAction>,
    /// A step or its preview's preparation runs (`deliveryBusy`).
    pub busy: bool,
    /// The menu waits (`actionLoading`): busy, or git still read.
    pub loading: bool,
    /// "PR", or "MR" on GitLab.
    pub request_label: &'static str,
}

fn totals<'a>(
    entries: impl Iterator<Item = &'a WorkspaceGitChangeEntry> + Clone,
) -> (u32, u32, u32) {
    let files = entries
        .clone()
        .map(|entry| entry.path.as_str())
        .collect::<HashSet<_>>()
        .len() as u32;
    let (additions, deletions) = entries.fold((0, 0), |(add, del), entry| {
        (add + entry.insertions, del + entry.deletions)
    });
    (files, additions, deletions)
}

impl CodeColumn {
    /// `gitChangeSummary`: the working tree, the branch, the pull request.
    pub(crate) fn dock_summary(&self) -> Option<DockChangeSummary> {
        let status = self.status.as_ref();
        let branch = match &self.changes.branch {
            Loaded::Ready(diff) => Some(diff),
            _ => None,
        };
        let pr = self.delivery.pr.as_ref();
        if status.is_none() && branch.is_none() && pr.is_none() {
            return None;
        }
        let (files, additions, deletions) = status
            .map(|status| totals(status.staged.iter().chain(&status.unstaged)))
            .unwrap_or_default();
        let (branch_files, branch_additions, branch_deletions) = branch
            .map(|diff| totals(diff.changes.iter()))
            .unwrap_or_default();
        Some(DockChangeSummary {
            files,
            additions,
            deletions,
            branch_files,
            branch_additions,
            branch_deletions,
            ahead_of_remote_count: status.map_or(0, |status| status.ahead_of_remote_count),
            has_remote: status.map(|status| status.has_remote),
            pull_request_state: pr.and_then(|pr| pr.state.clone()),
            pull_request_number: pr.and_then(|pr| pr.number),
        })
    }

    /// `gitStatusState`: loading until the status and the branch are read.
    fn dock_git_state(&self) -> DockGitState {
        let branch_pending = matches!(self.changes.branch, Loaded::Pending);
        if (self.status.is_none() && self.status_error.is_none()) || branch_pending {
            DockGitState::Loading
        } else if self.status_error.is_some() || matches!(self.changes.branch, Loaded::Failed(_)) {
            DockGitState::Error
        } else {
            DockGitState::Ready
        }
    }

    /// The header's controls; none without a task's checkout.
    pub fn dock(&self) -> Option<DockControls> {
        self.root.as_ref()?;
        let git = self.dock_git_state();
        let summary = self.dock_summary();
        let busy = self.delivery.busy || self.delivery.syncing || self.delivery.preparing.is_some();
        let loading = busy || git != DockGitState::Ready;
        let context = DockActionContext::from_summary(
            Some(self.commit_mode()),
            loading,
            false,
            summary.as_ref(),
        );
        Some(DockControls {
            status: resolve_dock_status(summary.as_ref(), git),
            actions: resolve_dock_actions(&context),
            busy,
            loading,
            request_label: self.delivery.request_label(),
        })
    }

    /// A step picked in the menu (`runAction` → `runDeliveryAction`).
    pub fn run_dock_action(
        &mut self,
        action: DockAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if action.disabled {
            return;
        }
        if action.kind == DockActionKind::CreateRequest {
            self.open_request_dialog(action.id == DockActionId::CreateDraftPr, window, cx);
            return;
        }
        let Some(mode) = action.mode else {
            return;
        };
        if self.delivery.busy {
            return;
        }
        match mode {
            DockRunMode::Commit => self.prepare_dock_commit(false, window, cx),
            DockRunMode::Mode(CommitMode::CommitAndPush) => {
                self.prepare_dock_commit(true, window, cx)
            }
            DockRunMode::SyncBase => self.ask_sync_base(cx),
            DockRunMode::Mode(CommitMode::CreatePr) => self.open_request_dialog(false, window, cx),
            DockRunMode::Mode(CommitMode::Merge) => {
                self.delivery.confirm = Some(Confirm::Merge);
                cx.notify();
            }
            DockRunMode::Mode(
                CommitMode::Fix | CommitMode::ResolveConflicts | CommitMode::CompleteMerge,
            ) => cx.emit(CodeColumnEvent::ReviewChanges),
            DockRunMode::Mode(CommitMode::Push) => self.dock_push(cx),
            DockRunMode::Mode(CommitMode::OpenPr | CommitMode::Closed) => {
                self.dock_open_request(cx)
            }
            DockRunMode::Mode(CommitMode::Merged) => {}
        }
    }

    /// The preview opens at once, "Preparando…" until the staged message
    /// is read (everything staged first when nothing is).
    fn prepare_dock_commit(&mut self, push: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.delivery.preparing.is_some() {
            return;
        }
        let Some(id) = self.task_id() else {
            Toaster::error(cx.t("composer.executionDock.actions.unavailable"), cx);
            return;
        };
        let api = self.api();
        self.delivery.preparing = Some(push);
        cx.notify();
        self._delivery = Some(cx.spawn_in(window, async move |this, cx| {
            let suggestion = cx
                .background_executor()
                .spawn(async move { api?.prepare_commit(&id) })
                .await;
            let _ = this.update_in(cx, |column, window, cx| {
                if column.delivery.preparing.take().is_none() {
                    // Cancelled while it was prepared.
                    return;
                }
                match suggestion {
                    Ok(suggestion) => {
                        let subject = cx.new(|cx| {
                            let mut area = TextArea::new(window, cx)
                                .with_max_height(super::SUBJECT_MAX_HEIGHT);
                            area.set_text(&suggestion.subject, cx);
                            area
                        });
                        let body = cx.new(|cx| {
                            let mut area =
                                TextArea::new(window, cx).with_max_height(super::BODY_MAX_HEIGHT);
                            area.set_text(suggestion.body.as_deref().unwrap_or_default(), cx);
                            area
                        });
                        subject.read(cx).focus_handle(cx).focus(window, cx);
                        column.delivery.preview = Some(Preview {
                            origin: PreviewOrigin::Dock { push },
                            subject,
                            body,
                            staged_files: suggestion.staged_file_count,
                            fingerprint: suggestion.staged_fingerprint,
                        });
                        column.refresh_git(cx);
                    }
                    Err(error) => {
                        let reason = message_of(error, cx);
                        Toaster::error_with(cx.t("commit.preview.preparationFailed"), reason, cx);
                    }
                }
                cx.notify();
            });
        }));
    }

    /// The previewed commit, pushed or not.
    pub(super) fn dock_commit(
        &mut self,
        message: String,
        body: String,
        fingerprint: String,
        push: bool,
        cx: &mut Context<Self>,
    ) {
        let done = if push {
            "composer.executionDock.actions.committedAndPushed"
        } else {
            "composer.executionDock.actions.committed"
        };
        self.dock_step(done, push, cx, move |api, id| {
            let body = Some(body.as_str()).filter(|body| !body.trim().is_empty());
            api.commit_reviewed(id, &message, body, &fingerprint, push)
        });
    }

    fn dock_push(&mut self, cx: &mut Context<Self>) {
        self.dock_step(
            "composer.executionDock.actions.pushed",
            true,
            cx,
            |api, id| api.deliver(id, "push"),
        );
    }

    /// One step in the desktop; its toast once it ends.
    fn dock_step(
        &mut self,
        done: &'static str,
        pushes: bool,
        cx: &mut Context<Self>,
        step: impl FnOnce(&crate::data::desktop_api::DesktopApi, &str) -> Result<(), ApiError>
        + Send
        + 'static,
    ) {
        let Some(id) = self.task_id() else {
            Toaster::error(cx.t("composer.executionDock.actions.unavailable"), cx);
            return;
        };
        if self.delivery.busy {
            return;
        }
        let api = self.api();
        self.delivery.busy = true;
        cx.notify();
        self._delivery = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { step(&api?, &id) })
                .await;
            let _ = this.update(cx, |column, cx| {
                column.delivery.busy = false;
                match result {
                    Ok(()) => Toaster::success(cx.t(done), cx),
                    Err(ApiError::Refused { code, target, .. })
                        if pushes && code == "behind_remote" =>
                    {
                        column.behind_remote(target.unwrap_or_default(), cx)
                    }
                    Err(error) => dock_failed(error, cx),
                }
                column.refresh_git(cx);
                cx.notify();
            });
        }));
    }

    /// "tem commits que você não tem", with "Atualizar e enviar".
    fn behind_remote(&mut self, target: String, cx: &mut Context<Self>) {
        let args = Args::default().with("target", target.clone());
        let column = cx.entity().downgrade();
        Toaster::error_action(
            cx.t_with("composer.executionDock.actions.pushBehindRemote", &args),
            cx.t("composer.executionDock.actions.pushBehindRemoteHint"),
            cx.t("composer.executionDock.actions.syncAndPush"),
            BEHIND_REMOTE,
            move |cx| {
                let target = target.clone();
                let _ = column.update(cx, |column, cx| column.sync_and_push(target, cx));
            },
            cx,
        );
    }

    /// `syncAndPush`: the remote's commits brought in, the checks, the push.
    pub(crate) fn sync_and_push(&mut self, target: String, cx: &mut Context<Self>) {
        let Some(id) = self.task_id() else {
            return;
        };
        if self.delivery.busy {
            return;
        }
        let args = Args::default().with("target", target);
        let toast = Toaster::loading(
            cx.t_with("composer.executionDock.actions.syncAndPushLoading", &args),
            cx,
        );
        let api = self.api();
        self.delivery.busy = true;
        cx.notify();
        self._delivery = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api?.deliver(&id, "sync-and-push") })
                .await;
            let _ = this.update(cx, |column, cx| {
                column.delivery.busy = false;
                match result {
                    Ok(()) => {
                        let text =
                            cx.t_with("composer.executionDock.actions.syncAndPushSuccess", &args);
                        Toaster::settle(toast, ToastKind::Success, text, None, cx);
                    }
                    Err(error) => {
                        let reason: SharedString = failure_reason(error, cx).into();
                        let text = cx.t("composer.executionDock.actions.failed");
                        Toaster::settle(toast, ToastKind::Error, text, Some(reason), cx);
                    }
                }
                column.refresh_git(cx);
                cx.notify();
            });
        }));
    }

    /// "Abrir PR": the pull request in the browser.
    fn dock_open_request(&mut self, cx: &mut Context<Self>) {
        match self.delivery.pr.as_ref().and_then(|pr| pr.url.clone()) {
            Some(url) => {
                cx.open_url(&url);
                Toaster::success(cx.t("composer.executionDock.actions.opened"), cx);
            }
            None => {
                let label = self.delivery.request_label();
                dock_failed(ApiError::Failed(format!("no {label} for this branch")), cx);
            }
        }
    }
}

/// The reason under "A ação git falhou" (the checks' own words when they
/// stopped the push).
pub(super) fn failure_reason(error: ApiError, cx: &App) -> String {
    match error {
        ApiError::Refused { code, .. } if code == "checks_failed" => cx
            .t("composer.executionDock.actions.beforePushBlocked")
            .to_string(),
        error => message_of(error, cx),
    }
}

pub(super) fn dock_failed(error: ApiError, cx: &mut App) {
    let reason = failure_reason(error, cx);
    Toaster::error_with(cx.t("composer.executionDock.actions.failed"), reason, cx);
}

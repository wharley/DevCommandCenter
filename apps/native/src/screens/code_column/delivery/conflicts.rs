//! A merge with conflicts in "Mudanças" (`conflictStateQuery`): its state
//! read every 5 seconds while git reports one, which tells "Resolver
//! conflitos" from "Concluir merge" (the results already free of markers);
//! the resolver opened on the stage's place; and the confirmed completion
//! (`executeConfirmedCompleteMerge`): each result staged, the project's
//! checks, the merge commit and the push.

use std::time::Duration;

use dcc_core::domain::merge_conflict::{WorkspaceGitConflictOperation, conflict_resolution_ready};
use gpui::Context;

use super::super::{CodeColumn, CodeColumnEvent};
use crate::data::desktop_api::ApiError;
use crate::i18n::{Args, Translate};
use crate::screens::merge::MergeResolverRequest;
use crate::ui::{ToastKind, Toaster};

/// `refetchInterval: 5_000`
const EVERY: Duration = Duration::from_secs(5);

impl CodeColumn {
    /// A merge with conflicts is going on, as the status says.
    fn merge_conflicted(&self) -> bool {
        self.status
            .as_ref()
            .is_some_and(|status| status.merge_in_progress && status.conflict_count > 0)
    }

    /// `isMergeConflictResolutionReady`
    pub(crate) fn conflicts_ready(&self) -> bool {
        conflict_resolution_ready(self.delivery.conflicts.as_ref())
    }

    /// Starts (or stops) reading the conflict state after a status read.
    pub(crate) fn follow_conflicts(&mut self, cx: &mut Context<Self>) {
        if !self.merge_conflicted() {
            self.delivery.conflicts = None;
            self.delivery._conflicts = None;
            return;
        }
        if self.delivery._conflicts.is_some() {
            return;
        }
        let Some(root) = self.root.clone() else {
            return;
        };
        let api = self.api();
        self.delivery._conflicts = Some(cx.spawn(async move |this, cx| {
            loop {
                let (path, api) = (root.clone(), api.clone());
                let read = cx
                    .background_executor()
                    .spawn(async move { api?.conflict_state(&path) })
                    .await;
                let going = this.update(cx, |column, cx| {
                    if column.root.as_deref() != Some(root.as_str()) || !column.merge_conflicted() {
                        column.delivery.conflicts = None;
                        column.delivery._conflicts = None;
                        return false;
                    }
                    if let Ok(state) = read
                        && column.delivery.conflicts.as_ref() != Some(&state)
                    {
                        column.delivery.conflicts = Some(state);
                        cx.notify();
                    }
                    true
                });
                if !matches!(going, Ok(true)) {
                    return;
                }
                cx.background_executor().timer(EVERY).await;
            }
        }));
    }

    /// "Resolver conflitos" / "Revisar resolução": the resolver opens.
    pub(crate) fn open_merge_resolver(&mut self, cx: &mut Context<Self>) {
        let Some(root) = self.root.clone() else {
            return;
        };
        let workspace_label = self
            .delivery
            .task
            .as_ref()
            .and_then(|task| task.name.clone())
            .filter(|name| !name.trim().is_empty())
            .or_else(|| self.current_branch());
        cx.emit(CodeColumnEvent::OpenMergeResolver(MergeResolverRequest {
            root,
            task_id: self.task_id(),
            base_branch: self
                .delivery
                .pr
                .as_ref()
                .and_then(|pr| pr.base_branch.clone()),
            forge_login: self.review.login(),
            workspace_label,
            agent: None,
        }));
    }

    /// How many results the confirmation names.
    pub(crate) fn conflicts_to_complete(&self) -> usize {
        self.delivery
            .conflicts
            .as_ref()
            .map(|state| state.conflicts.len())
            .or_else(|| {
                self.status
                    .as_ref()
                    .map(|status| status.conflict_count as usize)
            })
            .unwrap_or(0)
    }

    /// `executeConfirmedCompleteMerge`
    pub(crate) fn complete_merge(&mut self, cx: &mut Context<Self>) {
        let Some(root) = self.root.clone() else {
            return;
        };
        let toast = Toaster::loading(cx.t("inspector.gitConfirmation.completeMergeLoading"), cx);
        let api = self.api();
        let login = self.review.login();
        let (changed, failed_checks) = (
            cx.t("inspector.gitConfirmation.completeMergeStateChanged")
                .to_string(),
            cx.t("inspector.gitConfirmation.completeMergeValidationFailed")
                .to_string(),
        );
        self.delivery.busy = true;
        cx.notify();
        self._delivery = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let api = api?;
                    let state = api.conflict_state(&root)?;
                    if state.operation != WorkspaceGitConflictOperation::Merge
                        || (!state.conflicts.is_empty() && !conflict_resolution_ready(Some(&state)))
                    {
                        return Err(ApiError::Failed(changed));
                    }
                    for conflict in &state.conflicts {
                        api.mark_conflict_resolved(&root, &conflict.path, false)?;
                    }
                    let config = api.validation_config(&root)?;
                    let completed = api.complete_merge(&root, login.as_deref(), &config)?;
                    if !completed.completed {
                        return Err(ApiError::Failed(failed_checks));
                    }
                    Ok(())
                })
                .await;
            let _ = this.update(cx, |column, cx| {
                column.delivery.busy = false;
                match result {
                    Ok(()) => {
                        let text = cx.t("inspector.gitConfirmation.completeMergeSuccess");
                        Toaster::settle(toast, ToastKind::Success, text, None, cx);
                    }
                    Err(error) => {
                        let text = cx.t_with(
                            "inspector.gitConfirmation.completeMergeFailed",
                            &Args::default().with("message", super::actions::message_of(error, cx)),
                        );
                        Toaster::settle(toast, ToastKind::Error, text, None, cx);
                    }
                }
                column.delivery.conflicts = None;
                column.refresh_git(cx);
                cx.notify();
            });
        }));
    }
}

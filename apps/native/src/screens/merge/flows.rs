//! What the resolver does, as the desktop does it: read the state (and the
//! checks once a merge is going), start the merge, take a block or a whole
//! side, save the result and mark it resolved, delete it, abort, and
//! complete with the checks. Questions come first where the desktop asks.

use dcc_core::domain::merge_conflict::{
    ConflictResolution, WorkspaceGitConflictOperation, WorkspaceGitConflictSide,
    apply_conflict_resolution, conflict_result_suspiciously_empty, has_conflict_marker_fragments,
    parse_conflict_hunks,
};
use gpui::Context;

use super::{Busy, Load, MergeResolver, MergeResolverEvent, Pending};
use crate::data::desktop_api::ApiError;
use crate::i18n::{Args, Translate};
use crate::ui::Toaster;

fn message(error: ApiError, cx: &gpui::App) -> String {
    match error {
        ApiError::DesktopClosed => cx.t("native.desktopClosed").to_string(),
        error => error.to_string(),
    }
}

impl MergeResolver {
    /// The conflict state (`query.refetch()`), and the checks with a merge.
    pub(crate) fn read(&mut self, cx: &mut Context<Self>) {
        let root = self.request.root.clone();
        let api = self.api();
        self._read = Some(cx.spawn(async move |this, cx| {
            let read = cx
                .background_executor()
                .spawn(async move {
                    let api = api?;
                    let state = api.conflict_state(&root)?;
                    let validation = (state.operation == WorkspaceGitConflictOperation::Merge)
                        .then(|| api.validation_config(&root));
                    Ok::<_, ApiError>((state, validation))
                })
                .await;
            let _ = this.update(cx, |resolver, cx| {
                match read {
                    Ok((state, validation)) => {
                        resolver.state = Load::Ready(state);
                        resolver.validation = validation.map(|config| match config {
                            Ok(config) => Load::Ready(config),
                            Err(error) => Load::Failed(message(error, cx)),
                        });
                        resolver.follow_conflicts(cx);
                    }
                    Err(error) => resolver.state = Load::Failed(message(error, cx)),
                }
                cx.notify();
            });
        }));
    }

    /// Git moved: read again here, and the code column too.
    fn changed(&mut self, cx: &mut Context<Self>) {
        self.read(cx);
        cx.emit(MergeResolverEvent::StateChanged);
    }

    /// `startMerge`: the base brought in; the conflicts it leaves show here.
    pub(crate) fn start_merge(&mut self, cx: &mut Context<Self>) {
        let Some(task) = self.request.task_id.clone() else {
            return;
        };
        let (root, base) = (self.request.root.clone(), self.request.base_branch.clone());
        let api = self.api();
        self.busy = Some(Busy::Start);
        cx.notify();
        self._action = Some(cx.spawn(async move |this, cx| {
            let (synced, state) = cx
                .background_executor()
                .spawn(async move {
                    let Ok(api) = api else {
                        return (Err(ApiError::DesktopClosed), None);
                    };
                    let synced = api.sync_base(&task, base.as_deref()).map(|_| ());
                    (synced, api.conflict_state(&root).ok())
                })
                .await;
            let _ = this.update(cx, |resolver, cx| {
                resolver.busy = None;
                let conflicted = state
                    .as_ref()
                    .is_some_and(|state| !state.conflicts.is_empty());
                if let Some(state) = state {
                    resolver.state = Load::Ready(state);
                    resolver.follow_conflicts(cx);
                }
                cx.emit(MergeResolverEvent::StateChanged);
                if conflicted {
                    Toaster::info(cx.t("mergeConflict.toast.mergeStarted"), cx);
                } else if let Err(error) = synced {
                    Toaster::error(message(error, cx), cx);
                } else {
                    Toaster::success(cx.t("mergeConflict.toast.updatedWithoutConflicts"), cx);
                    cx.emit(MergeResolverEvent::Closed);
                }
                resolver.read(cx);
                cx.notify();
            });
        }));
    }

    /// `applyHunk`: the block on screen, by where it starts.
    pub(crate) fn apply_hunk(&mut self, resolution: ConflictResolution, cx: &mut Context<Self>) {
        let Some(open) = self.open.as_ref() else {
            return;
        };
        let Some(hunk) = open.hunk() else {
            return;
        };
        let source = open.buffer.clone();
        let Some(current) = parse_conflict_hunks(&source)
            .into_iter()
            .find(|candidate| candidate.start_offset == hunk.start_offset)
        else {
            return;
        };
        let next = apply_conflict_resolution(&source, &current, resolution);
        self.set_result(next, cx);
    }

    /// `useWholeSide`: a text side goes into the result to review; one that
    /// can't be shown (deleted, binary, too big) is taken by git directly.
    pub(crate) fn use_whole_side(
        &mut self,
        side: WorkspaceGitConflictSide,
        cx: &mut Context<Self>,
    ) {
        let Some(open) = self.open.as_ref() else {
            return;
        };
        let text = match side {
            WorkspaceGitConflictSide::Current => open.entry.current.text.clone(),
            WorkspaceGitConflictSide::Incoming => open.entry.incoming.text.clone(),
        };
        let ((current, _), (incoming, _)) = self.labels(cx);
        let branch = match side {
            WorkspaceGitConflictSide::Current => current,
            WorkspaceGitConflictSide::Incoming => incoming,
        };
        if let Some(text) = text {
            self.set_result(text, cx);
            if let Some(open) = self.open.as_mut() {
                open.active_hunk = 0;
            }
            Toaster::info(
                cx.t_with(
                    "mergeConflict.toast.sideLoadedForReview",
                    &Args::default().with("branch", branch),
                ),
                cx,
            );
            return;
        }
        let (root, path) = (self.request.root.clone(), open.entry.path.clone());
        let api = self.api();
        self.busy = Some(match side {
            WorkspaceGitConflictSide::Current => Busy::Current,
            WorkspaceGitConflictSide::Incoming => Busy::Incoming,
        });
        cx.notify();
        let done = cx.t_with(
            "mergeConflict.toast.resolvedWith",
            &Args::default()
                .with("path", path.clone())
                .with("branch", branch),
        );
        self._action = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api?.accept_conflict(&root, &path, side) })
                .await;
            let _ = this.update(cx, |resolver, cx| {
                resolver.busy = None;
                match result {
                    Ok(()) => {
                        Toaster::success(done, cx);
                        resolver.changed(cx);
                    }
                    Err(error) => Toaster::error(message(error, cx), cx),
                }
                cx.notify();
            });
        }));
    }

    /// `saveAndResolve`: no block left, then the questions, then the write
    /// (only when it changed) and the file staged.
    pub(crate) fn save(&mut self, markers_ok: bool, empty_ok: bool, cx: &mut Context<Self>) {
        let Some(open) = self.open.as_ref() else {
            return;
        };
        let content = open.buffer.clone();
        if !parse_conflict_hunks(&content).is_empty() {
            Toaster::warning(cx.t("mergeConflict.toast.resolveAllHunks"), cx);
            return;
        }
        if !markers_ok && has_conflict_marker_fragments(&content) {
            self.pending = Some(Pending::RemainingMarkers);
            cx.notify();
            return;
        }
        if !empty_ok && conflict_result_suspiciously_empty(&open.entry, &content) {
            self.pending = Some(Pending::EmptyResult);
            cx.notify();
            return;
        }
        let (root, path) = (self.request.root.clone(), open.entry.path.clone());
        let exists = open.entry.result.exists;
        let initial = open.initial_result.clone();
        let write = content != initial.clone().unwrap_or_default() || !exists;
        let api = self.api();
        let changed = cx.t("mergeConflict.toast.fileChanged").to_string();
        let done = cx.t_with(
            "mergeConflict.toast.markedResolved",
            &Args::default().with("path", path.clone()),
        );
        self.busy = Some(Busy::Save);
        cx.notify();
        self._action = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let api = api?;
                    if write {
                        let expected = if exists { initial.as_deref() } else { None };
                        if api.write_file(&root, &path, &content, expected)?.is_err() {
                            return Err(ApiError::Failed(changed));
                        }
                    }
                    api.mark_conflict_resolved(&root, &path, false)
                })
                .await;
            let _ = this.update(cx, |resolver, cx| {
                resolver.busy = None;
                match result {
                    Ok(()) => {
                        Toaster::success(done, cx);
                        resolver.changed(cx);
                    }
                    Err(error) => Toaster::error(message(error, cx), cx),
                }
                cx.notify();
            });
        }));
    }

    /// `deleteResult`, once confirmed.
    fn delete_result(&mut self, cx: &mut Context<Self>) {
        let Some(open) = self.open.as_ref() else {
            return;
        };
        let (root, path) = (self.request.root.clone(), open.entry.path.clone());
        let api = self.api();
        let done = cx.t_with(
            "mergeConflict.toast.removedResolved",
            &Args::default().with("path", path.clone()),
        );
        self.busy = Some(Busy::Delete);
        cx.notify();
        self._action = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api?.mark_conflict_resolved(&root, &path, true) })
                .await;
            let _ = this.update(cx, |resolver, cx| {
                resolver.busy = None;
                match result {
                    Ok(()) => {
                        Toaster::success(done, cx);
                        resolver.changed(cx);
                    }
                    Err(error) => Toaster::error(message(error, cx), cx),
                }
                cx.notify();
            });
        }));
    }

    /// `abortMerge`, once confirmed: the resolver closes.
    fn abort(&mut self, cx: &mut Context<Self>) {
        let root = self.request.root.clone();
        let api = self.api();
        self.busy = Some(Busy::Abort);
        cx.notify();
        self._action = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api?.abort_merge(&root) })
                .await;
            let _ = this.update(cx, |resolver, cx| {
                resolver.busy = None;
                match result {
                    Ok(()) => {
                        cx.emit(MergeResolverEvent::StateChanged);
                        Toaster::success(cx.t("mergeConflict.toast.mergeAborted"), cx);
                        cx.emit(MergeResolverEvent::Closed);
                    }
                    Err(error) => Toaster::error(message(error, cx), cx),
                }
                cx.notify();
            });
        }));
    }

    /// "Validar, concluir e enviar": the commands asked about first.
    pub(crate) fn request_complete(&mut self, cx: &mut Context<Self>) {
        let commands = match &self.validation {
            Some(Load::Ready(config)) => config.commands.len(),
            _ => 0,
        };
        if commands > 0 {
            self.pending = Some(Pending::ValidationCommands);
            cx.notify();
            return;
        }
        self.complete(cx);
    }

    /// `completeMerge`: a failed check stays here with its report.
    fn complete(&mut self, cx: &mut Context<Self>) {
        let Some(Load::Ready(config)) = self.validation.clone() else {
            return;
        };
        let (root, login) = (self.request.root.clone(), self.request.forge_login.clone());
        let api = self.api();
        self.busy = Some(Busy::Complete);
        self.report = None;
        cx.notify();
        self._action = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api?.complete_merge(&root, login.as_deref(), &config) })
                .await;
            let _ = this.update(cx, |resolver, cx| {
                resolver.busy = None;
                match result {
                    Ok(output) => {
                        resolver.report = Some(output.validation);
                        cx.emit(MergeResolverEvent::StateChanged);
                        if output.completed {
                            Toaster::success(cx.t("mergeConflict.toast.mergeCompleted"), cx);
                            cx.emit(MergeResolverEvent::Closed);
                        } else {
                            Toaster::error(cx.t("mergeConflict.toast.validationFailed"), cx);
                        }
                    }
                    Err(error) => Toaster::error(message(error, cx), cx),
                }
                cx.notify();
            });
        }));
    }

    pub(crate) fn request_delete(&mut self, cx: &mut Context<Self>) {
        self.pending = Some(Pending::DeleteResult);
        cx.notify();
    }

    pub(crate) fn request_abort(&mut self, cx: &mut Context<Self>) {
        self.pending = Some(Pending::Abort);
        cx.notify();
    }

    /// The question's "OK".
    pub(crate) fn confirm(&mut self, cx: &mut Context<Self>) {
        match self.pending.take() {
            Some(Pending::DiscardForFile(path)) => self.open_path(&path, cx),
            Some(Pending::DiscardToClose) => cx.emit(MergeResolverEvent::Closed),
            Some(Pending::RemainingMarkers) => self.save(true, false, cx),
            Some(Pending::EmptyResult) => self.save(true, true, cx),
            Some(Pending::DeleteResult) => self.delete_result(cx),
            Some(Pending::Abort) => self.abort(cx),
            Some(Pending::ValidationCommands) => self.complete(cx),
            None => {}
        }
        cx.notify();
    }

    pub(crate) fn cancel_pending(&mut self, cx: &mut Context<Self>) {
        self.pending = None;
        cx.notify();
    }
}

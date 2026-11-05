//! "Puxar alterações": the task's own branch against its remote copy. When
//! the task opens its branch is fetched (at most every few minutes), so the
//! header knows how many commits someone else pushed and offers to bring
//! them in first; pulling rebases the local commits on top when the
//! checkout is clean, else merges, and conflicts go to the resolver like a
//! base update.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use gpui::Context;

use super::super::CodeColumn;
use crate::i18n::{Args, Translate};
use crate::ui::{ToastKind, Toaster};

/// A task's branch is fetched again only after this long.
const FETCH_EVERY: Duration = Duration::from_secs(2 * 60);

/// When each task's branch was last fetched (by task id).
#[derive(Default)]
pub(crate) struct UpstreamFetches(HashMap<String, Instant>);

impl CodeColumn {
    /// The task's branch fetched in the background, refs only; the status
    /// read after it counts what is behind.
    pub(crate) fn fetch_upstream(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.task_id() else {
            return;
        };
        let now = Instant::now();
        let fetched = &mut self.delivery.upstream.0;
        if fetched
            .get(&id)
            .is_some_and(|at| now.duration_since(*at) < FETCH_EVERY)
        {
            return;
        }
        fetched.insert(id.clone(), now);
        let api = self.api();
        cx.spawn(async move |this, cx| {
            let fetched = cx
                .background_executor()
                .spawn(async move { api.and_then(|api| api.fetch_upstream(&id)) })
                .await;
            // Offline, or a branch not pushed yet: nothing to tell.
            if fetched.is_ok() {
                let _ = this.update(cx, |column, cx| column.refresh_git(cx));
            }
        })
        .detach();
    }

    /// "Puxar alterações".
    pub(crate) fn pull(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.task_id() else {
            return;
        };
        let toast = Toaster::loading(cx.t("composer.executionDock.actions.pullLoading"), cx);
        let api = self.api();
        self.delivery.syncing = true;
        cx.notify();
        self._delivery = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api?.pull(&id) })
                .await;
            let _ = this.update(cx, |column, cx| {
                column.delivery.syncing = false;
                match result {
                    Ok(pulled) => {
                        let args = Args::default()
                            .with("ref", format!("{}/{}", pulled.remote, pulled.base_branch));
                        let (kind, key) = if pulled.updated {
                            (
                                ToastKind::Success,
                                "composer.executionDock.actions.pullSuccess",
                            )
                        } else {
                            (
                                ToastKind::Info,
                                "composer.executionDock.actions.pullCurrent",
                            )
                        };
                        Toaster::settle(toast, kind, cx.t_with(key, &args), None, cx);
                    }
                    Err(error) => {
                        let text = cx.t_with(
                            "composer.executionDock.actions.pullFailed",
                            &Args::default().with("message", super::actions::message_of(error, cx)),
                        );
                        Toaster::settle(toast, ToastKind::Error, text, None, cx);
                    }
                }
                column.refresh_git(cx);
                cx.notify();
            });
        }));
    }
}

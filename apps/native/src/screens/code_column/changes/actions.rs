//! The git actions of the list (`runGit`): stage, unstage or discard one
//! file, stage or unstage all, revert everything. They run in the desktop
//! (it waits for an agent's edit to the same checkout); the status is read
//! again right after, and a failure shows as a toast.

use gpui::{App, Context};

use super::super::CodeColumn;
use super::Scope;
use crate::data::desktop_api::{ApiError, DesktopApi, GitAction};
use crate::i18n::Translate;
use crate::ui::Toaster;

impl CodeColumn {
    /// Runs the steps in order, stopping at the first failure.
    pub(crate) fn run_git(&mut self, steps: Vec<(GitAction, String)>, cx: &mut Context<Self>) {
        let Some(root) = self.root.clone() else {
            return;
        };
        if self.changes.busy || steps.is_empty() {
            return;
        }
        self.changes.busy = true;
        cx.notify();
        let endpoint = self.endpoint.clone();
        self._git = Some(cx.spawn(async move |this, cx| {
            let path = root.clone();
            let (result, status) = cx
                .background_executor()
                .spawn(async move {
                    let result = endpoint
                        .or_else(DesktopApi::load)
                        .ok_or(ApiError::DesktopClosed)
                        .and_then(|api| {
                            steps
                                .iter()
                                .try_for_each(|(action, file)| api.git_action(*action, &path, file))
                        });
                    let status = dcc_infra::workspace_git::read_workspace_git_status(&path);
                    (result, status)
                })
                .await;
            let _ = this.update(cx, |column, cx| {
                column.changes.busy = false;
                if column.root.as_deref() != Some(root.as_str()) {
                    return;
                }
                if let Err(error) = result {
                    report(error, cx);
                }
                if let Ok(status) = status {
                    column.apply_status(status, cx);
                }
                cx.notify();
            });
        }));
    }

    pub(crate) fn git_file(&mut self, action: GitAction, path: &str, cx: &mut Context<Self>) {
        self.run_git(vec![(action, path.to_string())], cx);
    }

    /// "Preparar tudo": `git add -A`.
    pub(crate) fn stage_all(&mut self, cx: &mut Context<Self>) {
        self.run_git(vec![(GitAction::StageAll, ".".to_string())], cx);
    }

    /// "Remover tudo": every staged file back out of the index.
    pub(crate) fn unstage_all(&mut self, cx: &mut Context<Self>) {
        let steps = self
            .staged_paths()
            .into_iter()
            .map(|path| (GitAction::Unstage, path))
            .collect();
        self.run_git(steps, cx);
    }

    pub(crate) fn ask_revert_all(&mut self, open: bool, cx: &mut Context<Self>) {
        self.changes.confirm_revert = open;
        cx.notify();
    }

    /// "Reverter mudanças": the staged files out of the index, then every
    /// file back to HEAD (a new one removed).
    pub(crate) fn revert_all(&mut self, cx: &mut Context<Self>) {
        self.changes.confirm_revert = false;
        let staged = self.staged_paths();
        let mut all: Vec<String> = Vec::new();
        for path in self
            .changes
            .entries(Scope::Working)
            .map(|entry| entry.path.clone())
        {
            if !all.contains(&path) {
                all.push(path);
            }
        }
        let steps = staged
            .into_iter()
            .map(|path| (GitAction::Unstage, path))
            .chain(all.into_iter().map(|path| (GitAction::Discard, path)))
            .collect();
        self.run_git(steps, cx);
    }

    fn staged_paths(&self) -> Vec<String> {
        self.status
            .as_ref()
            .map(|status| {
                status
                    .staged
                    .iter()
                    .map(|entry| entry.path.clone())
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// `toast.error(error.message ?? "Git command failed")`
fn report(error: ApiError, cx: &mut App) {
    match error {
        ApiError::DesktopClosed => Toaster::error(cx.t("native.desktopClosed"), cx),
        error => Toaster::error(error.to_string(), cx),
    }
}

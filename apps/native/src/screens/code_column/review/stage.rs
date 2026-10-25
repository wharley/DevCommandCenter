//! The file's pull request conversations go with its diff to the stage
//! (`enrichPreviewSelection`): a card's diff carries them, its badge lands
//! on the first open one, a listed conversation opens in the pull
//! request's diff (`handleOpenReviewThread`), and a file with conversations
//! picked in the tree opens there too (`handleOpenCodeFileFromTree`).

use dcc_core::domain::review_threads::{ReviewThread, review_thread_root};
use dcc_core::domain::workspace_git::WorkspaceGitPreviewScope;
use gpui::Context;

use super::super::{CodeColumn, CodeColumnEvent};
use crate::screens::stage::{DiffRequest, ReviewRequest};

impl CodeColumn {
    /// The file's conversations and where they come from, while the pull
    /// request shows them.
    pub(crate) fn review_request(
        &self,
        path: &str,
        focus: bool,
        thread_id: Option<String>,
    ) -> Option<ReviewRequest> {
        let number = self.delivery.pr.as_ref().and_then(|pr| pr.number);
        if !self.review.comments_enabled(number) {
            return None;
        }
        let comments = self
            .review
            .comments
            .as_ref()
            .map(|output| {
                output
                    .comments
                    .iter()
                    .filter(|comment| comment.path == path)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        Some(ReviewRequest {
            root: self.root.clone()?,
            branch: self.current_branch(),
            forge_login: self.review.login(),
            comments,
            focus,
            thread_id,
        })
    }

    /// The file in the branch's diff against its base, when it has one.
    fn pr_diff_request(&self, path: &str, review: Option<ReviewRequest>) -> Option<DiffRequest> {
        let base = self.changes.base_branch()?;
        let status = match &self.changes.branch {
            super::super::Loaded::Ready(diff) => diff
                .changes
                .iter()
                .find(|entry| entry.path == path)
                .map(|entry| entry.status.clone()),
            _ => None,
        };
        Some(DiffRequest {
            root: self.root.clone()?,
            path: path.to_string(),
            name: path.rsplit('/').next().unwrap_or(path).to_string(),
            status: status.unwrap_or_else(|| "M".into()),
            scope: WorkspaceGitPreviewScope::Committed,
            base: Some(base),
            from_changes: true,
            review,
        })
    }

    /// A conversation from "Conversas do PR", opened on its line.
    pub(crate) fn open_thread_on_stage(&mut self, thread: &ReviewThread, cx: &mut Context<Self>) {
        let path = review_thread_root(thread).path.clone();
        let review = self.review_request(&path, true, Some(thread.id.clone()));
        let request = self.pr_diff_request(&path, review.clone()).or_else(|| {
            // Without a base the file still opens on its conversation list.
            Some(DiffRequest {
                root: self.root.clone()?,
                name: path.rsplit('/').next().unwrap_or(&path).to_string(),
                path: path.clone(),
                status: "M".into(),
                scope: WorkspaceGitPreviewScope::Unstaged,
                base: None,
                from_changes: true,
                review,
            })
        });
        if let Some(request) = request {
            cx.emit(CodeColumnEvent::OpenDiff(request));
        }
    }

    /// A tree file with conversations opens in the pull request's diff.
    pub(crate) fn file_pr_diff(&self, path: &str) -> Option<DiffRequest> {
        let review = self.review_request(path, false, None)?;
        if review.comments.is_empty() {
            return None;
        }
        self.pr_diff_request(path, Some(review))
    }
}

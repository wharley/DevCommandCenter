//! The pull request's conversations on the stage's diff (`WorkspaceEditorSurface`
//! with `reviewSource`): the file's threads followed live (every 30s, and
//! right after a reply or a resolve), marked on the lines when the pull
//! request's diff shows (a local diff switches to it on "Ver diff do PR"),
//! listed in the bar, and one open in the drawer to reply, resolve, or hand
//! to the agent. Arriving from a file's badge lands on its first open
//! conversation (or the one asked for) in the pull request's diff.

use std::time::Duration;

use dcc_core::domain::code_annotation::CodeAnnotation;
use dcc_core::domain::review_thread_detail::{
    build_review_thread_prompt, review_comment_side, review_thread_anchorable,
    review_thread_annotation, review_thread_mark, review_thread_snippet,
};
use dcc_core::domain::review_threads::{
    ReviewThread, WorkspacePrReviewComment, build_review_threads, review_comment_line,
    review_thread_root, review_thread_state, sort_review_threads_for_list,
};
use dcc_core::domain::workspace_git::WorkspaceGitPreviewScope;
use gpui::{AppContext, Context, Entity, Subscription, Task, Window};

use super::{DiffLoad, Stage, StageEvent};
use crate::data::desktop_api::{ApiError, DesktopApi, ThreadReply};
use crate::i18n::Translate;
use crate::ui::{TextArea, TextAreaEvent};

/// `WORKSPACE_PR_REVIEW_COMMENTS_REFETCH_INTERVAL_MS`
const EVERY: Duration = Duration::from_secs(30);

/// Where the file's conversations come from (`WorkspaceReviewSource`), what
/// was known when it opened, and where to land.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewRequest {
    pub root: String,
    pub branch: Option<String>,
    pub forge_login: Option<String>,
    /// The file's comments as last read.
    pub comments: Vec<WorkspacePrReviewComment>,
    /// Opened from a badge or a listed conversation (`reviewFocus`).
    pub focus: bool,
    pub thread_id: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ForgeBusy {
    Reply,
    Resolve,
}

pub(crate) struct StageReview {
    pub source: ReviewRequest,
    pub comments: Vec<WorkspacePrReviewComment>,
    /// The pull request's number, once read (reply and resolve need it).
    pub number: Option<u32>,
    /// The branch's base and this file's status there, for its PR diff.
    pub pr_base: Option<String>,
    pub pr_status: Option<String>,
    pub branch_read: bool,
    pub prefer_pr: bool,
    pub bar_open: bool,
    pub active: Option<String>,
    pub reply: Entity<TextArea>,
    pub busy: Option<ForgeBusy>,
    pub error: Option<String>,
    focus_handled: bool,
    _reply_events: Subscription,
    _poll: Option<Task<()>>,
    _branch: Option<Task<()>>,
    _forge: Option<Task<()>>,
}

impl StageReview {
    pub fn threads(&self) -> Vec<ReviewThread> {
        build_review_threads(&self.comments)
    }

    /// The bar's order: open first, in file order.
    pub fn listed(&self) -> Vec<ReviewThread> {
        sort_review_threads_for_list(self.threads())
    }

    pub fn active_thread(&self) -> Option<ReviewThread> {
        let id = self.active.as_ref()?;
        self.threads().into_iter().find(|thread| &thread.id == id)
    }
}

fn api(endpoint: Option<DesktopApi>) -> Result<DesktopApi, ApiError> {
    endpoint
        .or_else(DesktopApi::load)
        .ok_or(ApiError::DesktopClosed)
}

impl Stage {
    pub(crate) fn new_review(
        &self,
        source: ReviewRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> StageReview {
        let placeholder = cx.t("reviewThread.replyPlaceholder");
        let reply = cx.new(|cx| {
            TextArea::new(window, cx)
                .with_placeholder(placeholder)
                .with_max_height(120.0)
        });
        let events = cx.subscribe(
            &reply,
            |stage, area, event: &TextAreaEvent, cx| match event {
                TextAreaEvent::Changed => cx.notify(),
                TextAreaEvent::Submitted => area.update(cx, |area, cx| area.insert("\n", cx)),
                TextAreaEvent::Cancelled => stage.close_thread(cx),
                _ => {}
            },
        );
        StageReview {
            comments: source.comments.clone(),
            prefer_pr: source.focus,
            bar_open: source.focus,
            source,
            number: None,
            pr_base: None,
            pr_status: None,
            branch_read: false,
            active: None,
            reply,
            busy: None,
            error: None,
            focus_handled: false,
            _reply_events: events,
            _poll: None,
            _branch: None,
            _forge: None,
        }
    }

    pub(crate) fn review(&self) -> Option<&StageReview> {
        self.diff.as_ref()?.review.as_ref()
    }

    fn review_mut(&mut self) -> Option<&mut StageReview> {
        self.diff.as_mut()?.review.as_mut()
    }

    /// `showingPullRequestDiff`
    pub(crate) fn showing_pr_diff(&self) -> bool {
        let Some(diff) = self.diff.as_ref() else {
            return false;
        };
        diff.request.scope == WorkspaceGitPreviewScope::Committed
            || diff
                .review
                .as_ref()
                .is_some_and(|review| review.prefer_pr && review.pr_base.is_some())
    }

    /// What to read for the diff on screen: the PR's when it shows.
    pub(crate) fn content_params(
        &self,
    ) -> Option<(WorkspaceGitPreviewScope, String, Option<String>)> {
        let diff = self.diff.as_ref()?;
        let request = &diff.request;
        let committed = request.scope == WorkspaceGitPreviewScope::Committed;
        if self.showing_pr_diff() && !committed {
            let review = diff.review.as_ref()?;
            let status = review
                .pr_status
                .clone()
                .unwrap_or_else(|| request.status.clone());
            return Some((
                WorkspaceGitPreviewScope::Committed,
                status,
                review.pr_base.clone(),
            ));
        }
        Some((
            request.scope.clone(),
            request.status.clone(),
            request.base.clone(),
        ))
    }

    /// Starts following the file's conversations (and its PR diff's base).
    pub(crate) fn follow_review(&mut self, cx: &mut Context<Self>) {
        self.read_comments(cx);
        let committed = self
            .diff
            .as_ref()
            .is_some_and(|diff| diff.request.scope == WorkspaceGitPreviewScope::Committed);
        if committed {
            if let Some(review) = self.review_mut() {
                review.branch_read = true;
            }
            self.review_changed(cx);
            return;
        }
        let Some(root) = self.diff.as_ref().map(|diff| diff.request.root.clone()) else {
            return;
        };
        let path = self.diff.as_ref().map(|diff| diff.request.path.clone());
        let db_path = self.db_path.clone();
        let task = cx.spawn(async move |this, cx| {
            let branch = cx
                .background_executor()
                .spawn(async move {
                    dcc_infra::workspace_git::workspace_git_branch_diff(&db_path, &root).await
                })
                .await
                .ok();
            let _ = this.update(cx, |stage, cx| {
                let Some(review) = stage.review_mut() else {
                    return;
                };
                if let Some(branch) = branch {
                    review.pr_status = branch
                        .changes
                        .iter()
                        .find(|entry| Some(&entry.path) == path.as_ref())
                        .map(|entry| entry.status.clone());
                    review.pr_base = branch.base_branch;
                }
                review.branch_read = true;
                let prefer = review.prefer_pr && review.pr_base.is_some();
                if prefer {
                    stage.reload_diff(cx);
                }
                stage.review_changed(cx);
            });
        });
        if let Some(review) = self.review_mut() {
            review._branch = Some(task);
        }
    }

    /// Reads the comments now, then every 30s while the file shows.
    fn read_comments(&mut self, cx: &mut Context<Self>) {
        let Some(diff) = self.diff.as_ref() else {
            return;
        };
        let Some(review) = diff.review.as_ref() else {
            return;
        };
        let (root, branch, login) = (
            review.source.root.clone(),
            review.source.branch.clone(),
            review.source.forge_login.clone(),
        );
        let path = diff.request.path.clone();
        let endpoint = self.endpoint.clone();
        let task = cx.spawn(async move |this, cx| {
            loop {
                let (root, branch, login, endpoint) = (
                    root.clone(),
                    branch.clone(),
                    login.clone(),
                    endpoint.clone(),
                );
                let read = cx
                    .background_executor()
                    .spawn(async move {
                        api(endpoint)?.review_comments(&root, branch.as_deref(), login.as_deref())
                    })
                    .await;
                let alive = this.update(cx, |stage, cx| {
                    let Some(review) = stage
                        .diff
                        .as_mut()
                        .filter(|diff| diff.request.path == path)
                        .and_then(|diff| diff.review.as_mut())
                    else {
                        return false;
                    };
                    if let Ok(output) = read {
                        review.number = output.number;
                        review.comments = output
                            .comments
                            .into_iter()
                            .filter(|comment| comment.path == path)
                            .collect();
                        stage.review_changed(cx);
                    }
                    true
                });
                if !matches!(alive, Ok(true)) {
                    return;
                }
                cx.background_executor().timer(EVERY).await;
            }
        });
        if let Some(review) = self.review_mut() {
            review._poll = Some(task);
        }
    }

    /// The marks follow the threads; a pending landing happens once it can.
    pub(crate) fn review_changed(&mut self, cx: &mut Context<Self>) {
        let showing = self.showing_pr_diff();
        let Some(diff) = self.diff.as_ref() else {
            return;
        };
        let (Some(review), DiffLoad::Ready(view)) = (diff.review.as_ref(), &diff.load) else {
            cx.notify();
            return;
        };
        let marks = review
            .threads()
            .iter()
            .filter(|thread| review_thread_anchorable(thread, showing))
            .map(review_thread_mark)
            .collect();
        let view = view.clone();
        view.update(cx, |view, cx| view.set_marks(marks, cx));
        // `reviewFocus`: the first open conversation, or the one asked for.
        let settled = showing || review.branch_read;
        if review.source.focus && !review.focus_handled && settled {
            let listed = review.listed();
            let target = listed
                .iter()
                .find(|thread| Some(&thread.id) == review.source.thread_id.as_ref())
                .or_else(|| listed.first())
                .map(|thread| thread.id.clone());
            if let Some(id) = target {
                if let Some(review) = self.review_mut() {
                    review.focus_handled = true;
                }
                self.open_thread(id, true, cx);
            }
        }
        cx.notify();
    }

    /// `openReviewThread`: the drawer, and the line in view when it shows.
    pub(crate) fn open_thread(&mut self, id: String, reveal: bool, cx: &mut Context<Self>) {
        let showing = self.showing_pr_diff();
        let Some(review) = self.review_mut() else {
            return;
        };
        if review.active.as_ref() != Some(&id) {
            review.error = None;
            review.reply.update(cx, |area, cx| area.set_text("", cx));
        }
        review.active = Some(id);
        let Some(thread) = review.active_thread() else {
            return;
        };
        let root = review_thread_root(&thread).clone();
        let target = review_comment_line(&root)
            .filter(|_| reveal && review_thread_anchorable(&thread, showing))
            .map(|line| (review_comment_side(&root), line.max(1) as usize));
        if let (Some((side, line)), Some(DiffLoad::Ready(view))) =
            (target, self.diff.as_ref().map(|diff| &diff.load))
        {
            let view = view.clone();
            view.update(cx, |view, cx| view.reveal_line(side, line, cx));
        }
        cx.notify();
    }

    pub(crate) fn close_thread(&mut self, cx: &mut Context<Self>) {
        if let Some(review) = self.review_mut()
            && review.active.take().is_some()
        {
            cx.notify();
        }
    }

    pub(crate) fn toggle_review_bar(&mut self, cx: &mut Context<Self>) {
        if let Some(review) = self.review_mut() {
            review.bar_open = !review.bar_open;
            cx.notify();
        }
    }

    /// "Ver diff do PR" / "Ver diff local".
    pub(crate) fn set_pr_diff(&mut self, prefer: bool, cx: &mut Context<Self>) {
        let Some(review) = self.review_mut() else {
            return;
        };
        if review.prefer_pr == prefer {
            return;
        }
        review.prefer_pr = prefer;
        self.reload_diff(cx);
    }

    /// The code the open thread is about, from the diff on screen.
    pub(crate) fn thread_snippet(&self, thread: &ReviewThread, cx: &gpui::App) -> String {
        let showing = self.showing_pr_diff();
        let texts = match self.diff.as_ref().map(|diff| &diff.load) {
            Some(DiffLoad::Ready(view)) => view.read(cx).texts(),
            _ => None,
        };
        let (original, modified) = texts.unwrap_or_default();
        review_thread_snippet(thread, showing, &original, &modified)
    }

    /// "Enviar ao agente" / "Composer": the thread as a request.
    pub(crate) fn thread_to_agent(&mut self, send: bool, cx: &mut Context<Self>) {
        let Some(thread) = self.review().and_then(StageReview::active_thread) else {
            return;
        };
        let Some(request) = self.diff.as_ref().map(|diff| diff.request.clone()) else {
            return;
        };
        let snippet = self.thread_snippet(&thread, cx);
        let instruction =
            build_review_thread_prompt(&request.path, &thread, &snippet, request.base.as_deref());
        let annotation: CodeAnnotation = review_thread_annotation(&request.path, &thread, &snippet);
        if let Some(review) = self.review_mut() {
            review.active = None;
        }
        cx.emit(StageEvent::ThreadToAgent {
            annotation,
            instruction,
            send,
        });
        self.show_chat(cx);
    }

    /// "Responder" with what was typed.
    pub(crate) fn reply_to_thread(&mut self, cx: &mut Context<Self>) {
        let Some(review) = self.review() else {
            return;
        };
        let body = review.reply.read(cx).text().trim().to_string();
        let (Some(thread), Some(number)) = (review.active_thread(), review.number) else {
            return;
        };
        if body.is_empty() || review.busy.is_some() {
            return;
        }
        let reply = ThreadReply {
            root: review.source.root.clone(),
            number,
            comment_id: review_thread_root(&thread).id.clone(),
            thread_id: thread.id.clone(),
            body,
            login: review.source.forge_login.clone(),
        };
        let endpoint = self.endpoint.clone();
        self.forge_action(
            ForgeBusy::Reply,
            move || api(endpoint)?.reply_thread(&reply),
            cx,
        );
    }

    /// "Resolver" / "Reabrir".
    pub(crate) fn toggle_thread_resolved(&mut self, cx: &mut Context<Self>) {
        let Some(review) = self.review() else {
            return;
        };
        let (Some(thread), Some(number)) = (review.active_thread(), review.number) else {
            return;
        };
        if review.busy.is_some() {
            return;
        }
        let resolved = review_thread_state(&thread)
            != dcc_core::domain::review_threads::ReviewThreadState::Resolved;
        let (root, login) = (
            review.source.root.clone(),
            review.source.forge_login.clone(),
        );
        let endpoint = self.endpoint.clone();
        self.forge_action(
            ForgeBusy::Resolve,
            move || {
                api(endpoint)?.resolve_thread(&root, number, &thread.id, resolved, login.as_deref())
            },
            cx,
        );
    }

    fn forge_action(
        &mut self,
        kind: ForgeBusy,
        action: impl FnOnce() -> Result<(), ApiError> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        let Some(review) = self.review_mut() else {
            return;
        };
        review.busy = Some(kind);
        review.error = None;
        let task = cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { action() })
                .await;
            let _ = this.update(cx, |stage, cx| {
                let failed = cx.t("native.sendFailed").to_string();
                let Some(review) = stage.review_mut() else {
                    return;
                };
                review.busy = None;
                match result {
                    Ok(()) => {
                        if kind == ForgeBusy::Reply {
                            review.reply.update(cx, |area, cx| area.set_text("", cx));
                        }
                        stage.read_comments(cx);
                    }
                    Err(ApiError::Failed(message)) => review.error = Some(message),
                    Err(_) => review.error = Some(failed),
                }
                cx.notify();
            });
        });
        if let Some(review) = self.review_mut() {
            review._forge = Some(task);
        }
        cx.notify();
    }
}

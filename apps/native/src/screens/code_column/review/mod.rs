//! The pull request's sections under the delivery header: the last
//! delivery failure the desktop captured (with its safe recoveries), the
//! review's state with its reviewers (ask, ask again, withdraw), and who the
//! forge acts as (the identity chip in the header). All of it comes from the
//! desktop, which keeps the failure and runs the forge's CLI.

mod checks;
mod failure;
mod identity;
mod picker;
mod pipeline;
mod stage;
mod state;
#[cfg(test)]
mod tests;
mod threads;

pub(crate) use failure::{failure_section, recovery_confirm};
pub(crate) use identity::identity_chip;
pub(crate) use pipeline::{pipeline_retry_confirm, pipeline_section};
pub(crate) use state::review_section;
pub(crate) use threads::{first_line, review_badge, state_chip, threads_section};

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use dcc_core::domain::review_threads::WorkspacePrReviewCommentsOutput;

use gpui::{Context, Entity, SharedString, Task};

use super::CodeColumn;
use crate::data::desktop_api::{ApiError, DesktopApi};
use crate::data::forge::{
    DeliveryFailure, ForgeAccount, ForgeContext, JobLog, Pipeline, ReviewState, ReviewerAction,
    ReviewerCandidate,
};
use crate::i18n::{Args, Translate};
use crate::ui::{TextInput, Toaster};

/// `DELIVERY_FAILURE_REFETCH_INTERVAL_MS` / the review state's and the
/// pipeline's interval.
const EVERY: Duration = Duration::from_secs(15);
/// `WORKSPACE_PR_REVIEW_COMMENTS_REFETCH_INTERVAL_MS`
const COMMENTS_EVERY: Duration = Duration::from_secs(30);
/// A GitHub pull request's failing checks (and their failing step's log)
/// are read at most this often, and only while it has some.
const CHECKS_EVERY: Duration = Duration::from_secs(60);

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum PipelineLoad {
    Pending,
    Ready(Option<Box<Pipeline>>),
    Failed,
}

/// Everything one read brings back.
struct Read {
    forge: Option<ForgeContext>,
    account: Option<ForgeAccount>,
    failure: Option<DeliveryFailure>,
    state: Option<ReviewLoad>,
    /// `None`: not read this time (kept); `Some(None)`: not shown.
    comments: Option<Option<WorkspacePrReviewCommentsOutput>>,
    pipeline: Option<PipelineLoad>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ReviewLoad {
    Pending,
    Ready(Box<ReviewState>),
    Failed,
}

/// "Pedir revisão": the search and the people found for it.
pub(crate) struct Picker {
    pub search: Entity<TextInput>,
    pub query: String,
    pub candidates: Option<Result<Vec<ReviewerCandidate>, ()>>,
    _read: Option<Task<()>>,
}

#[derive(Default)]
pub(crate) struct Review {
    pub forge: Option<ForgeContext>,
    pub account: Option<ForgeAccount>,
    /// The CLI's accounts were read for this checkout (found or not).
    accounts_read: bool,
    pub failure: Option<DeliveryFailure>,
    pub failure_open: bool,
    /// The recovery running, or the one waiting for its confirmation.
    pub recovering: Option<String>,
    pub confirm_recovery: Option<String>,
    pub state: Option<ReviewLoad>,
    pub state_open: bool,
    /// The reviewer whose request is changing, and how.
    pub changing: Option<(String, ReviewerAction)>,
    pub picker: Option<Picker>,
    /// The pull request's review comments, and open conversations by file.
    pub comments: Option<WorkspacePrReviewCommentsOutput>,
    pub open_by_path: HashMap<String, usize>,
    pub threads_open: bool,
    /// Conversations picked for the agent, and the ones seen (a new open
    /// one joins the pick; an unpick stays).
    pub picked: HashSet<String>,
    seen: HashSet<String>,
    pub pipeline: Option<PipelineLoad>,
    pub pipeline_open: bool,
    pub active_job: Option<u64>,
    pub job_log: Option<Result<JobLog, String>>,
    pub retrying_job: Option<u64>,
    pub confirm_job_retry: Option<(u64, String)>,
    read_at: Option<Instant>,
    comments_at: Option<Instant>,
    checks_at: Option<Instant>,
}

impl Review {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn login(&self) -> Option<String> {
        self.forge
            .as_ref()
            .and_then(|forge| forge.effective_login.clone())
    }

    /// The conversations show on GitHub or GitLab with a pull request.
    pub fn comments_enabled(&self, pr_number: Option<u32>) -> bool {
        self.forge
            .as_ref()
            .is_some_and(|forge| matches!(forge.provider.as_deref(), Some("github" | "gitlab")))
            && pr_number.is_some()
    }

    /// The pipeline shows on a connected GitLab.
    pub fn pipeline_enabled(&self) -> bool {
        self.forge
            .as_ref()
            .is_some_and(|forge| forge.connected() && forge.provider.as_deref() == Some("gitlab"))
    }

    fn take_comments(&mut self, comments: Option<WorkspacePrReviewCommentsOutput>) {
        use dcc_core::domain::review_threads::{
            ReviewThreadState, build_review_threads, count_open_review_threads,
            review_comments_by_path, review_thread_state,
        };
        let all = comments
            .as_ref()
            .map(|output| output.comments.as_slice())
            .unwrap_or_default();
        self.open_by_path = review_comments_by_path(all)
            .into_iter()
            .map(|(path, comments)| (path, count_open_review_threads(&comments)))
            .filter(|(_, count)| *count > 0)
            .collect();
        for thread in build_review_threads(all) {
            if self.seen.insert(thread.id.clone())
                && review_thread_state(&thread) != ReviewThreadState::Resolved
            {
                self.picked.insert(thread.id);
            }
        }
        self.comments = comments;
    }

    /// The review state shows on a connected forge with a pull request.
    pub fn review_enabled(&self, pr_number: Option<u32>) -> bool {
        self.forge.as_ref().is_some_and(ForgeContext::connected) && pr_number.is_some()
    }
}

impl CodeColumn {
    fn review_api(&self) -> Result<DesktopApi, ApiError> {
        self.endpoint
            .clone()
            .or_else(DesktopApi::load)
            .ok_or(ApiError::DesktopClosed)
    }

    /// The forge, the failure and the review state, read while "Mudanças"
    /// shows: the forge once per checkout, the rest every 15 seconds (or
    /// right after an action, with `force`).
    pub(crate) fn read_review(&mut self, force: bool, cx: &mut Context<Self>) {
        let Some(root) = self.root.clone() else {
            return;
        };
        if self.tab != super::CodeColumnTab::Changes {
            return;
        }
        if !force && self.review.read_at.is_some_and(|at| at.elapsed() < EVERY) {
            return;
        }
        self.review.read_at = Some(Instant::now());
        let api = self.review_api();
        let known_forge = self.review.forge.clone();
        let known_account = self.review.account.clone();
        let accounts_read = self.review.accounts_read;
        let branch = self.current_branch();
        let pr_number = self.delivery.pr.as_ref().and_then(|pr| pr.number);
        if self.review.review_enabled(pr_number) && self.review.state.is_none() {
            self.review.state = Some(ReviewLoad::Pending);
        }
        if self.review.pipeline_enabled() && self.review.pipeline.is_none() {
            self.review.pipeline = Some(PipelineLoad::Pending);
        }
        let read_comments = force
            || self
                .review
                .comments_at
                .is_none_or(|at| at.elapsed() >= COMMENTS_EVERY);
        if read_comments {
            self.review.comments_at = Some(Instant::now());
        }
        // Only while the pull request's checks fail (or a failure from them
        // is still shown, so it clears once they pass): a green one costs
        // nothing, and the logs are read once per commit.
        let checks_failing = self.delivery.pr.as_ref().is_some_and(|pr| {
            pr.merge_state_status.as_deref() == Some("UNSTABLE") && pr.checks_failing != Some(false)
        }) || self
            .review
            .failure
            .as_ref()
            .is_some_and(|failure| failure.operation == "pipeline");
        let read_checks = checks_failing
            && (force
                || self
                    .review
                    .checks_at
                    .is_none_or(|at| at.elapsed() >= CHECKS_EVERY));
        if read_checks {
            self.review.checks_at = Some(Instant::now());
        }
        self._review = Some(cx.spawn(async move |this, cx| {
            let path = root.clone();
            let read = cx
                .background_executor()
                .spawn(async move {
                    let api = api.ok()?;
                    let forge = match known_forge {
                        Some(forge) => Some(forge),
                        None => api.forge_context(&path).ok(),
                    };
                    let login = forge.as_ref().and_then(|forge| {
                        forge
                            .effective_login
                            .clone()
                            .or_else(|| forge.bound_login.clone())
                    });
                    let account = match (&known_account, &forge, &login) {
                        (Some(account), _, _) => Some(account.clone()),
                        _ if accounts_read => None,
                        (None, Some(forge), Some(login)) => {
                            forge.provider.as_deref().and_then(|provider| {
                                api.forge_accounts(provider, forge.host.as_deref())
                                    .ok()?
                                    .accounts
                                    .into_iter()
                                    .find(|account| &account.login == login)
                            })
                        }
                        _ => None,
                    };
                    let github = forge.as_ref().is_some_and(|forge| {
                        forge.connected() && forge.provider.as_deref() == Some("github")
                    });
                    if read_checks && github {
                        // Stores the checks that failed, with their failing
                        // step's log, as the delivery failure read below.
                        let effective = forge.as_ref().and_then(|f| f.effective_login.clone());
                        let _ = api.pipeline(&path, effective.as_deref());
                    }
                    let failure = api.delivery_failure(&path).ok().flatten();
                    let connected = forge.as_ref().is_some_and(ForgeContext::connected);
                    let forge_login = forge.as_ref().and_then(|f| f.effective_login.clone());
                    let state = (connected && pr_number.is_some()).then(|| {
                        api.review_state(&path, branch.as_deref(), forge_login.as_deref())
                            .map_or(ReviewLoad::Failed, |state| {
                                ReviewLoad::Ready(Box::new(state))
                            })
                    });
                    let provider = forge.as_ref().and_then(|forge| forge.provider.clone());
                    let reviewed = matches!(provider.as_deref(), Some("github" | "gitlab"))
                        && pr_number.is_some();
                    let comments = if !reviewed {
                        Some(None)
                    } else if read_comments {
                        api.review_comments(&path, branch.as_deref(), forge_login.as_deref())
                            .ok()
                            .map(Some)
                    } else {
                        None
                    };
                    let pipeline =
                        (connected && provider.as_deref() == Some("gitlab")).then(|| {
                            api.pipeline(&path, forge_login.as_deref())
                                .map_or(PipelineLoad::Failed, |status| {
                                    PipelineLoad::Ready(status.pipeline.map(Box::new))
                                })
                        });
                    Some(Read {
                        forge,
                        account,
                        failure,
                        state,
                        comments,
                        pipeline,
                    })
                })
                .await;
            let _ = this.update(cx, |column, cx| {
                if column.root.as_deref() != Some(root.as_str()) {
                    return;
                }
                if let Some(read) = read {
                    column.review.accounts_read |= read.forge.as_ref().is_some_and(|forge| {
                        forge.effective_login.is_some() || forge.bound_login.is_some()
                    });
                    column.review.forge = read.forge;
                    column.review.account = read.account;
                    column.review.failure = read.failure;
                    column.review.state = read.state;
                    if let Some(comments) = read.comments {
                        column.review.take_comments(comments);
                    }
                    column.review.pipeline = read.pipeline;
                }
                cx.notify();
            });
        }));
    }

    pub(crate) fn toggle_failure(&mut self, cx: &mut Context<Self>) {
        self.review.failure_open = !self.review.failure_open;
        cx.notify();
    }

    pub(crate) fn toggle_review_state(&mut self, cx: &mut Context<Self>) {
        self.review.state_open = !self.review.state_open;
        cx.notify();
    }

    /// One of the failure's actions; "Tentar novamente" and "Sincronizar"
    /// ask first (`window.confirm`).
    pub(crate) fn recover(&mut self, action: &str, confirmed: bool, cx: &mut Context<Self>) {
        let Some(failure) = self.review.failure.clone() else {
            return;
        };
        if self.review.recovering.is_some() {
            return;
        }
        if matches!(action, "retry" | "synchronize") && !confirmed {
            self.review.confirm_recovery = Some(action.to_string());
            cx.notify();
            return;
        }
        self.review.confirm_recovery = None;
        self.review.recovering = Some(action.to_string());
        cx.notify();
        let api = self.review_api();
        let login = self.review.login();
        let action = action.to_string();
        self._recovery = Some(cx.spawn(async move |this, cx| {
            let token = failure.attempt_token.clone();
            let root = failure.workspace_root.clone();
            let step = action.clone();
            let result = cx
                .background_executor()
                .spawn(
                    async move { api?.delivery_recovery(&root, &token, &step, login.as_deref()) },
                )
                .await;
            let _ = this.update(cx, |column, cx| {
                column.review.recovering = None;
                match result {
                    Ok(recovered) => match action.as_str() {
                        "send-to-agent" => cx.emit(super::CodeColumnEvent::AskAgent {
                            prompt: recovered.snapshot.agent_prompt(),
                            delivered: cx.t("inspector.deliveryFailure.sentToAgent"),
                        }),
                        "open-external" => match recovered.snapshot.external_url {
                            Some(url) => cx.open_url(&url),
                            None => Toaster::error_with(
                                cx.t("inspector.deliveryFailure.actionFailed"),
                                cx.t("inspector.deliveryFailure.externalUnavailable"),
                                cx,
                            ),
                        },
                        _ => {
                            let key = if action == "retry" {
                                "inspector.deliveryFailure.retrySuccess"
                            } else {
                                "inspector.deliveryFailure.synchronizeSuccess"
                            };
                            Toaster::success(cx.t(key), cx);
                        }
                    },
                    Err(error) => Toaster::error_with(
                        cx.t("inspector.deliveryFailure.actionFailed"),
                        error.to_string(),
                        cx,
                    ),
                }
                column.refresh_git(cx);
                column.read_review(true, cx);
                cx.notify();
            });
        }));
    }

    pub(crate) fn cancel_recovery(&mut self, cx: &mut Context<Self>) {
        self.review.confirm_recovery = None;
        cx.notify();
    }

    /// Ask, ask again or withdraw one reviewer (`workspaceRequestReviewers`).
    pub(crate) fn change_reviewer(
        &mut self,
        login: String,
        action: ReviewerAction,
        cx: &mut Context<Self>,
    ) {
        let Some(root) = self.root.clone() else {
            return;
        };
        let Some(number) = self.reviewed_number() else {
            return;
        };
        if self.review.changing.is_some() {
            return;
        }
        self.review.changing = Some((login.clone(), action));
        self.review.picker = None;
        cx.notify();
        let api = self.review_api();
        let forge_login = self.review.login();
        self._reviewers = Some(cx.spawn(async move |this, cx| {
            let who = login.clone();
            let result = cx
                .background_executor()
                .spawn(async move {
                    let (add, remove) = if action == ReviewerAction::Remove {
                        (Vec::new(), vec![who])
                    } else {
                        (vec![who], Vec::new())
                    };
                    api?.request_reviewers(&root, number, &add, &remove, forge_login.as_deref())
                })
                .await;
            let _ = this.update(cx, |column, cx| {
                column.review.changing = None;
                let args = Args::default().with("login", login.clone());
                match result {
                    Ok(()) => {
                        let key = match action {
                            ReviewerAction::Remove => "inspector.reviewState.reviewRequestRemoved",
                            ReviewerAction::Rerequest => "inspector.reviewState.reviewRerequested",
                            ReviewerAction::Request => "inspector.reviewState.reviewRequested",
                        };
                        Toaster::success(cx.t_with(key, &args), cx);
                    }
                    Err(error) => {
                        let detail: String = error.to_string().chars().take(280).collect();
                        Toaster::error_with(
                            cx.t("inspector.reviewState.reviewRequestFailed"),
                            detail,
                            cx,
                        );
                    }
                }
                column.read_review(true, cx);
                cx.notify();
            });
        }));
    }

    pub(crate) fn reviewed_number(&self) -> Option<u32> {
        match &self.review.state {
            Some(ReviewLoad::Ready(state)) => state.number,
            _ => None,
        }
    }

    pub(crate) fn retry_review(&mut self, cx: &mut Context<Self>) {
        self.review.state = Some(ReviewLoad::Pending);
        cx.notify();
        self.read_review(true, cx);
    }
}

/// The label of a state under `prefix`, or the raw value (`defaultValue`).
pub(crate) fn state_label(prefix: &str, state: &str, cx: &gpui::App) -> SharedString {
    let key = format!("{prefix}.{state}");
    if cx.global::<crate::i18n::I18n>().has(&key, &Args::default()) {
        cx.t(&key)
    } else {
        state.to_string().into()
    }
}

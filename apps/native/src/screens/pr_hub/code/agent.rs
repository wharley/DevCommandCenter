//! The reviewer on the code tab: the whole review as a job that outlives
//! the screen, its answer turned into drafts once per visit, and "Preparar
//! com agente" for one line's comment or a reply.

use gpui::Context;

use crate::data::pr_review::{InlineComment, PatchRow};
use crate::data::pr_review_agent::{
    inline_prompt, parse_reply, parse_review, review_prompt, thread_reply_prompt,
};
use crate::data::pr_review_follow_up::drop_repeats;
use crate::data::pr_review_jobs::{JobStatus, PrReviewJob, PrReviewJobs};
use crate::i18n::Translate;
use crate::screens::pr_hub::{PrHub, message};

impl PrHub {
    pub(crate) fn review_job(&self, cx: &gpui::App) -> Option<PrReviewJob> {
        PrReviewJobs::job(cx, self.selected.as_deref()?)
    }

    /// `busyKey`: "review" while the job runs, else the line or reply being prepared.
    pub(crate) fn agent_busy(&self, cx: &gpui::App) -> Option<String> {
        if self
            .review_job(cx)
            .is_some_and(|job| job.status == JobStatus::Running)
        {
            return Some("review".into());
        }
        self.review.agent_pending.clone()
    }

    pub(crate) fn toggle_agent_panel(&mut self, cx: &mut Context<Self>) {
        self.review.agent_panel_open = !self.review.agent_panel_open;
        self.review.agent_error = None;
        if self.review.agent_panel_open {
            self.account_picks = crate::data::account_selection::load();
            self.read_accounts(cx);
        }
        cx.notify();
    }

    /// The accounts its menu offers (they may have changed in Settings).
    fn read_accounts(&mut self, cx: &mut Context<Self>) {
        let Some(api) = self.api.clone() else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let read = cx
                .background_executor()
                .spawn(async move { api.provider_accounts() })
                .await;
            if read.is_ok() {
                let _ = this.update(cx, |_, cx| cx.notify());
            }
        })
        .detach();
    }

    pub(crate) fn close_agent_panel(&mut self, cx: &mut Context<Self>) {
        self.review.agent_panel_open = false;
        cx.notify();
    }

    /// The person reviewed this pull request before: the next review follows up.
    pub(crate) fn follows_up(&self, cx: &gpui::App) -> bool {
        let Some(item) = self.selected_item(cx) else {
            return false;
        };
        let (Some(login), Some(Ok(detail))) = (item.forge_login, self.code_read(&item.id)) else {
            return false;
        };
        !crate::data::pr_review_follow_up::earlier_threads(detail, &login).is_empty()
    }

    fn instruction(&self, cx: &gpui::App) -> String {
        self.review.instruction.read(cx).text().to_string()
    }

    /// `prepareFullReview`
    pub(crate) fn start_full_review(&mut self, cx: &mut Context<Self>) {
        let Some(item) = self.selected_item(cx) else {
            return;
        };
        let Some(Ok(detail)) = self.code_read(&item.id).cloned() else {
            return;
        };
        if self.agent_busy(cx).is_some() {
            return;
        }
        self.review.agent_error = None;
        self.review.agent_result = None;
        self.review.agent_panel_open = false;
        let prompt = review_prompt(&item, &detail, &self.instruction(cx));
        let (Some(api), Some(run)) = (
            self.api.clone(),
            self.agent_run(&item.repository_root, prompt, true),
        ) else {
            self.review.agent_error = Some(cx.t("pullRequests.code.noReviewProvider").to_string());
            cx.notify();
            return;
        };
        let label = format!("{}#{}", item.repository_name, item.number);
        if !PrReviewJobs::start(cx, &item.id, label, item.title.clone()) {
            return;
        }
        let id = item.id.clone();
        cx.spawn(async move |_, cx| {
            let answer = cx
                .background_executor()
                .spawn(async move { api.pr_review_agent(&run) })
                .await;
            cx.update(|cx| {
                let answer = answer.map_err(|error| message(error, cx));
                PrReviewJobs::finish(cx, &id, answer);
            });
        })
        .detach();
        cx.notify();
    }

    /// The person accepted the offer: start once the patches are in, once.
    pub(crate) fn start_auto_review(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.auto_start_for.clone() else {
            return;
        };
        if self.selected.as_deref() != Some(id.as_str())
            || !matches!(self.code_read(&id), Some(Ok(_)))
        {
            return;
        }
        self.auto_start_for = None;
        if self.review_job(cx).is_none() {
            self.start_full_review(cx);
        }
    }

    /// A finished review lands once per visit: its drafts, its summary as
    /// the review's text when that is empty, the first file it commented on.
    pub(crate) fn apply_review_job(&mut self, cx: &mut Context<Self>) {
        if self.tab != super::Tab::Code {
            return;
        }
        let Some(job) = self.review_job(cx) else {
            return;
        };
        let Some(Ok(detail)) = self.code_read(&job.pr_id).cloned() else {
            return;
        };
        if job.status == JobStatus::Running || self.review.applied == job.finished {
            return;
        }
        self.review.applied = job.finished;
        PrReviewJobs::mark_seen(cx, &job.pr_id);
        let fallback = || cx.t("pullRequests.code.agentError").to_string();
        if job.status == JobStatus::Failed {
            self.review.agent_error = Some(job.error.clone().unwrap_or_else(fallback));
            PrReviewJobs::dismiss(cx, &job.pr_id);
            cx.notify();
            return;
        }
        let login = self.selected_item(cx).and_then(|item| item.forge_login);
        match parse_review(job.response.as_deref().unwrap_or(""), &detail) {
            Ok(mut review) => {
                if let Some(login) = login {
                    drop_repeats(&mut review, &detail, &login);
                }
                self.review.agent_result = Some(review.comments.len());
                if let Some(file) = detail.files.iter().find(|file| {
                    review
                        .comments
                        .iter()
                        .any(|comment| comment.path == file.path)
                }) {
                    self.review.path = Some(file.path.clone());
                }
                if self.review.body.read(cx).text().is_empty() && !review.summary.is_empty() {
                    let body = self.review.body.clone();
                    body.update(cx, |area, cx| area.set_text(&review.summary, cx));
                }
                let generated: std::collections::HashSet<String> =
                    review.comments.iter().map(|draft| draft.key()).collect();
                self.review
                    .drafts
                    .retain(|draft| !generated.contains(&draft.key()));
                self.review.drafts.extend(review.comments);
                self.review.submitted = false;
            }
            Err(error) => {
                self.review.agent_error = Some(if error.is_empty() { fallback() } else { error });
                PrReviewJobs::dismiss(cx, &job.pr_id);
            }
        }
        cx.notify();
    }

    /// One prompt to the reviewer's provider; its answer, read by `parse`.
    fn prepare(
        &mut self,
        pending: String,
        prompt: String,
        done: impl FnOnce(&mut Self, String, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) {
        let Some(item) = self.selected_item(cx) else {
            return;
        };
        if self.agent_busy(cx).is_some() {
            return;
        }
        let (Some(api), Some(run)) = (
            self.api.clone(),
            self.agent_run(&item.repository_root, prompt, false),
        ) else {
            self.review.agent_error = Some(cx.t("pullRequests.code.noReviewProvider").to_string());
            cx.notify();
            return;
        };
        self.review.agent_pending = Some(pending);
        self.review.agent_error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let answer = cx
                .background_executor()
                .spawn(async move { api.pr_review_agent(&run) })
                .await;
            let _ = this.update(cx, |hub, cx| {
                hub.review.agent_pending = None;
                let fallback = cx.t("pullRequests.code.agentError").to_string();
                match answer
                    .map_err(|error| message(error, cx))
                    .and_then(|text| parse_reply(&text))
                {
                    Ok(reply) => done(hub, reply, cx),
                    Err(error) => {
                        hub.review.agent_error =
                            Some(if error.is_empty() { fallback } else { error })
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// `prepareInline`: the draft for that line, written by the agent.
    pub(crate) fn prepare_inline(&mut self, path: String, row: PatchRow, cx: &mut Context<Self>) {
        let Some(item) = self.selected_item(cx) else {
            return;
        };
        let Some((line, side)) = row.review else {
            return;
        };
        let patch = self.patch_of(&item.id, &path);
        let key = crate::data::pr_review::draft_key(&path, line, side);
        let prompt = inline_prompt(&item, &path, &row, patch.as_deref(), &self.instruction(cx));
        self.prepare(
            format!("inline:{key}"),
            prompt,
            move |hub, reply, cx| {
                let draft = hub.review.draft.clone();
                draft.update(cx, |area, cx| area.set_text(&reply, cx));
                hub.review.editing = Some(key);
            },
            cx,
        );
    }

    /// `prepareReply`
    pub(crate) fn prepare_reply(&mut self, comment: InlineComment, cx: &mut Context<Self>) {
        let Some(item) = self.selected_item(cx) else {
            return;
        };
        let current = self
            .code_read(&item.id)
            .and_then(|read| read.as_ref().ok())
            .and_then(|detail| self.current_file(detail).map(|file| file.path.clone()));
        let Some(path) = current else {
            return;
        };
        let patch = self.patch_of(&item.id, &path);
        let prompt = thread_reply_prompt(&item, &comment, patch.as_deref(), &self.instruction(cx));
        let id = comment.id.clone();
        self.prepare(
            format!("reply:{id}"),
            prompt,
            move |hub, reply, cx| {
                let area = hub.review.reply.clone();
                area.update(cx, |area, cx| area.set_text(&reply, cx));
                hub.review.replying_to = Some(id);
            },
            cx,
        );
    }

    fn patch_of(&self, id: &str, path: &str) -> Option<String> {
        self.code_read(id)?
            .as_ref()
            .ok()?
            .files
            .iter()
            .find(|file| file.path == path)?
            .patch
            .clone()
    }
}

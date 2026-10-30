//! Pull request reviews the built-in reviewer runs from the hub
//! (`pr-review-jobs.ts`). Someone else's pull request has no task or
//! session to hold the work, so the job lives here, outside any screen: it
//! keeps running and keeps its answer when the person leaves the pull
//! request. In memory only; quitting the app drops it. Also a pull request asked
//! to open from elsewhere.

use gpui::{App, Global};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobStatus {
    Running,
    Done,
    Failed,
}

/// `PrReviewJob`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrReviewJob {
    pub pr_id: String,
    /// `repository#number`
    pub label: String,
    pub title: String,
    pub status: JobStatus,
    /// The agent's raw answer; the code tab reads it against the patches.
    pub response: Option<String>,
    pub error: Option<String>,
    /// Which finish this is, to apply each one once.
    pub finished: Option<u64>,
    /// The person has been on the pull request's code tab since it finished.
    pub seen: bool,
}

impl PrReviewJob {
    /// A finished answer nobody looked at yet.
    pub fn unread(&self) -> bool {
        self.status == JobStatus::Done && !self.seen
    }
}

#[derive(Default)]
pub struct PrReviewJobs {
    pub jobs: Vec<PrReviewJob>,
    finishes: u64,
    /// `requestOpenPullRequest`, taken by the hub once it shows.
    pending_open: Option<String>,
}

impl Global for PrReviewJobs {}

impl PrReviewJobs {
    pub fn get(cx: &App) -> Option<&Self> {
        cx.try_global::<Self>()
    }

    pub fn job(cx: &App, pr_id: &str) -> Option<PrReviewJob> {
        Self::get(cx)?
            .jobs
            .iter()
            .find(|job| job.pr_id == pr_id)
            .cloned()
    }

    pub fn all(cx: &App) -> Vec<PrReviewJob> {
        Self::get(cx)
            .map(|jobs| jobs.jobs.clone())
            .unwrap_or_default()
    }

    /// `startPrReviewJob`: false when one already runs for it.
    pub fn start(cx: &mut App, pr_id: &str, label: String, title: String) -> bool {
        let jobs = cx.default_global::<Self>();
        if jobs
            .jobs
            .iter()
            .any(|job| job.pr_id == pr_id && job.status == JobStatus::Running)
        {
            return false;
        }
        jobs.jobs.retain(|job| job.pr_id != pr_id);
        jobs.jobs.insert(
            0,
            PrReviewJob {
                pr_id: pr_id.to_string(),
                label,
                title,
                status: JobStatus::Running,
                response: None,
                error: None,
                finished: None,
                seen: false,
            },
        );
        true
    }

    pub fn finish(cx: &mut App, pr_id: &str, result: Result<String, String>) {
        let jobs = cx.default_global::<Self>();
        jobs.finishes += 1;
        let finished = jobs.finishes;
        if let Some(job) = jobs.jobs.iter_mut().find(|job| job.pr_id == pr_id) {
            match result {
                Ok(response) => {
                    job.status = JobStatus::Done;
                    job.response = Some(response);
                }
                Err(error) => {
                    job.status = JobStatus::Failed;
                    job.error = Some(error);
                }
            }
            job.finished = Some(finished);
        }
    }

    /// `markPrReviewJobSeen`
    pub fn mark_seen(cx: &mut App, pr_id: &str) {
        let unseen = Self::get(cx).is_some_and(|jobs| {
            jobs.jobs
                .iter()
                .any(|job| job.pr_id == pr_id && job.status != JobStatus::Running && !job.seen)
        });
        if unseen {
            let jobs = cx.global_mut::<Self>();
            if let Some(job) = jobs.jobs.iter_mut().find(|job| job.pr_id == pr_id) {
                job.seen = true;
            }
        }
    }

    /// `dismissPrReviewJob`: the review was sent, or failed.
    pub fn dismiss(cx: &mut App, pr_id: &str) {
        if Self::get(cx).is_some_and(|jobs| jobs.jobs.iter().any(|job| job.pr_id == pr_id)) {
            cx.global_mut::<Self>()
                .jobs
                .retain(|job| job.pr_id != pr_id);
        }
    }

    pub fn request_open(cx: &mut App, pr_id: &str) {
        cx.default_global::<Self>().pending_open = Some(pr_id.to_string());
    }

    pub fn take_pending_open(cx: &mut App) -> Option<String> {
        Self::get(cx)?.pending_open.as_ref()?;
        cx.global_mut::<Self>().pending_open.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    fn a_job_runs_once_finishes_and_is_seen(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            assert!(PrReviewJobs::start(
                cx,
                "pr-1",
                "acme#1".into(),
                "Fix".into()
            ));
            assert!(!PrReviewJobs::start(
                cx,
                "pr-1",
                "acme#1".into(),
                "Fix".into()
            ));
            PrReviewJobs::finish(cx, "pr-1", Ok("{}".into()));
            let job = PrReviewJobs::job(cx, "pr-1").unwrap();
            assert_eq!(
                (job.status, job.finished, job.unread()),
                (JobStatus::Done, Some(1), true)
            );
            PrReviewJobs::mark_seen(cx, "pr-1");
            assert!(!PrReviewJobs::job(cx, "pr-1").unwrap().unread());
            // A new run replaces the finished one.
            assert!(PrReviewJobs::start(
                cx,
                "pr-1",
                "acme#1".into(),
                "Fix".into()
            ));
            PrReviewJobs::finish(cx, "pr-1", Err("boom".into()));
            assert_eq!(PrReviewJobs::job(cx, "pr-1").unwrap().finished, Some(2));
            PrReviewJobs::dismiss(cx, "pr-1");
            assert!(PrReviewJobs::job(cx, "pr-1").is_none());
            PrReviewJobs::request_open(cx, "pr-3");
            assert_eq!(PrReviewJobs::take_pending_open(cx).as_deref(), Some("pr-3"));
            assert_eq!(PrReviewJobs::take_pending_open(cx), None);
        });
    }
}

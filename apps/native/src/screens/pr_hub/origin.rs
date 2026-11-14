//! "Origem" on a pull request (`ui::origin_strip`): when the pull request
//! is a task's and an agent opened that task, where it came from — the
//! same chain the task shows, then the pull request itself and the
//! Reviewer's review of it. Read once when the pull request is picked.

use gpui::{AnyElement, Context, Task, div, prelude::*, px};

use super::{PrHub, PrHubEvent};
use crate::data::pr_hub::Item;
use crate::data::pr_review_jobs::{JobStatus, PrReviewJobs};
use crate::data::pr_review_verdict::verdict_of;
use crate::data::task_origin::{ChainRow, OriginRead};
use crate::i18n::Translate;
use crate::ui::origin_strip::origin_strip;

#[derive(Default)]
pub(crate) struct PrOrigin {
    /// The task whose origin was asked.
    task: Option<String>,
    pub(crate) read: Option<OriginRead>,
    pub(crate) open: bool,
    _reading: Option<Task<()>>,
}

impl PrHub {
    /// While the hub draws: the picked pull request's task changed, so its
    /// origin is read (a test sets its own).
    pub(super) fn sync_origin(&mut self, cx: &mut Context<Self>) {
        let task = self
            .selected_item(cx)
            .and_then(|item| item.linked_workspace_id);
        if self.origin.task == task {
            return;
        }
        self.origin = PrOrigin {
            task: task.clone(),
            ..PrOrigin::default()
        };
        let Some(task) = task.filter(|_| !cfg!(test)) else {
            return;
        };
        self.origin._reading = Some(cx.spawn(async move |hub, cx| {
            let wanted = task.clone();
            let read = cx
                .background_executor()
                .spawn(async move { crate::data::task_origin::read(&wanted) })
                .await;
            let _ = hub.update(cx, |hub, cx| {
                if hub.origin.task.as_deref() == Some(task.as_str()) && read.is_some() {
                    hub.origin.read = read;
                    cx.notify();
                }
            });
        }));
    }

    /// The pull request and its review, after the task in the chain.
    fn origin_tail(&self, item: &Item, cx: &gpui::App) -> Vec<ChainRow> {
        let mut rows = vec![ChainRow {
            icon: "git-pull-request",
            title: (
                "origin.row.pr",
                vec![("label", format!("{}#{}", item.repository_name, item.number))],
            ),
            detail: String::new(),
            at: String::new(),
            place: None,
            link: Some(item.url.clone()).filter(|url| !url.is_empty()),
        }];
        if let Some(job) = PrReviewJobs::job(cx, &item.id) {
            let verdict = job.response.as_deref().and_then(verdict_of);
            let title = match (job.status, verdict) {
                (JobStatus::Running, _) => Some(("origin.row.reviewRunning", Vec::new())),
                (JobStatus::Done, Some(verdict)) => Some((
                    "origin.row.review",
                    vec![("verdict", cx.t(verdict.key()).to_string())],
                )),
                _ => None,
            };
            if let Some(title) = title {
                rows.push(ChainRow {
                    icon: "eye",
                    title,
                    detail: String::new(),
                    at: String::new(),
                    place: None,
                    link: None,
                });
            }
        }
        rows
    }

    /// The line under the pull request's title.
    pub(super) fn origin_strip(&self, item: &Item, cx: &mut Context<Self>) -> Option<AnyElement> {
        let read = self.origin.read.as_ref()?;
        let strip = origin_strip(
            "pr-origin",
            read,
            self.origin.open,
            self.origin_tail(item, cx),
            |hub: &mut Self, cx| {
                hub.origin.open = !hub.origin.open;
                cx.notify();
            },
            |_, agent_id, room, cx| cx.emit(PrHubEvent::OpenAgentRoom { agent_id, room }),
            cx,
        )?;
        Some(div().mt(px(8.0)).child(strip).into_any_element())
    }
}

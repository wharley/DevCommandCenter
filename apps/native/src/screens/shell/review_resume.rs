//! Starting again the reviews an app that quit cut short
//! (`data::pr_reviews_running`): once the agents and the providers are
//! known, each review still written down as running is started as it was
//! — the trigger's through `review_pull_requests`, an agent's request
//! through `review_requests_arrived` — and marked so it is not started a
//! second time. One that cannot start now (its agent is gone, no provider,
//! the pull request merged meanwhile) is dropped.

use gpui::Context;

use super::Shell;
use crate::data::pr_hub::Reviewed;
use crate::data::pr_reviews_running::{self, Started};

impl Shell {
    /// Once per run of the app, when there is who can review.
    pub(super) fn resume_reviews(&mut self, cx: &mut Context<Self>) {
        if self.reviews_resumed || cfg!(test) {
            return;
        }
        let ready = !self.sidebar.read(cx).agents().is_empty()
            && self
                .conversation
                .read(cx)
                .composer
                .read(cx)
                .catalog
                .is_some();
        if !ready {
            return;
        }
        self.reviews_resumed = true;
        for review in pr_reviews_running::load() {
            // Whatever comes of it, it is not the same entry any more: a
            // start writes its own, marked as resumed.
            pr_reviews_running::remove(&review.pr_id);
            if review.resumed {
                continue;
            }
            self.resuming_review = true;
            match review.started {
                Started::Trigger {
                    trigger,
                    url,
                    number,
                    repository,
                    title,
                    author,
                } => {
                    let pr = Reviewed {
                        url,
                        number,
                        repository,
                        title,
                        author,
                        ..Reviewed::default()
                    };
                    self.review_pull_requests(trigger, &[pr], cx);
                }
                Started::Asked { request } => self.review_requests_arrived(&[request], cx),
            }
            self.resuming_review = false;
        }
    }
}

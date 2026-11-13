//! The review being written survives leaving its pull request: what the
//! code tab holds (line comments, the text, the decision) is kept on disk
//! when a draft is added or removed, when the decision changes and when
//! another pull request takes the screen, and comes back when this one
//! does. A review the forge took leaves nothing behind.

use gpui::Context;

use super::PrHub;
use crate::data::pr_review_drafts::{self, Kept};

impl PrHub {
    /// Keeps the review on screen under its pull request.
    pub(crate) fn keep_review(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.review.for_id.clone() else {
            return;
        };
        let body = self.review.body.read(cx).text().to_string();
        let mut kept = Kept::new(&self.review.drafts, &body, self.review.event);
        kept.viewed = self.review.viewed.clone();
        pr_review_drafts::save(&id, kept);
    }

    /// Brings back what was kept of the pull request now on screen.
    pub(super) fn restore_review(&mut self, cx: &mut Context<Self>) {
        let Some(kept) = self
            .review
            .for_id
            .as_deref()
            .and_then(pr_review_drafts::load)
        else {
            return;
        };
        self.apply_kept(kept, cx);
    }

    pub(crate) fn apply_kept(&mut self, kept: Kept, cx: &mut Context<Self>) {
        self.review.drafts = kept.review_drafts();
        self.review.event = kept.review_event();
        self.review.viewed = kept.viewed.clone();
        self.review.body_seen = kept.body.clone();
        let body = self.review.body.clone();
        body.update(cx, |area, cx| area.set_text(&kept.body, cx));
    }
}

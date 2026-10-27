//! "Feedback DCC" over the window (`feedbackOpen`): the sidebar's speech
//! bubble opens it; it goes out through the person's GitHub account.

use std::sync::Arc;

use gpui::{AnyElement, AppContext, Context, Entity, IntoElement, Subscription, Window};

use super::Shell;
#[cfg(not(test))]
use crate::data::feedback::GitHub;
use crate::data::feedback::Store;
use crate::screens::feedback::{Feedback, FeedbackEvent};

pub(super) type OpenFeedback = (Entity<Feedback>, Subscription);

impl Shell {
    pub(super) fn open_feedback(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.feedback.is_some() {
            return;
        }
        #[cfg(not(test))]
        let (backend, store): (crate::data::feedback::SharedBackend, Store) = (
            Arc::new(GitHub {
                db_path: crate::data::loader::database_path(),
            }),
            Store::default_file(),
        );
        #[cfg(test)]
        let (backend, store): (crate::data::feedback::SharedBackend, Store) = (
            Arc::new(crate::data::feedback::Offline),
            Store::new(std::env::temp_dir().join("dcc-native-test-feedback.json")),
        );
        let feedback = cx.new(|cx| Feedback::new(backend, store, window, cx));
        let events = cx.subscribe_in(&feedback, window, |shell, _, event, window, cx| {
            shell.feedback = None;
            if matches!(event, FeedbackEvent::OpenSettings) {
                shell.open_settings(None, window, cx);
            }
            cx.notify();
        });
        self.feedback = Some((feedback, events));
        cx.notify();
    }

    pub(super) fn feedback_overlay(&self) -> Option<AnyElement> {
        self.feedback
            .as_ref()
            .map(|(feedback, _)| feedback.clone().into_any_element())
    }
}

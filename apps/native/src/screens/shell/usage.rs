//! "DCC Usage" over the window (`setIsUsageOpen`): the sidebar's chart
//! button and the palette open it, with the open task's project for
//! "Projeto atual".

use gpui::{AnyElement, AppContext, Context, Entity, IntoElement, Subscription};

use super::Shell;
use crate::screens::usage::{Usage, UsageEvent};

pub(super) type OpenUsage = (Entity<Usage>, Subscription);

impl Shell {
    pub(super) fn open_usage(&mut self, cx: &mut Context<Self>) {
        if self.usage.is_some() {
            return;
        }
        // `selectedWorkspace.projectId`
        let project_id = self
            .conversation
            .read(cx)
            .data()
            .map(|data| data.task_id.clone())
            .and_then(|task| {
                self.sidebar
                    .read(cx)
                    .tasks()
                    .iter()
                    .find(|summary| summary.id == task)
                    .map(|summary| summary.project_id.clone())
            });
        let db_path = crate::data::loader::database_path();
        let usage = cx.new(|cx| Usage::new(db_path, project_id, cx));
        let events = cx.subscribe(&usage, |shell, _, event, cx| match event {
            UsageEvent::Closed => {
                shell.usage = None;
                cx.notify();
            }
        });
        self.usage = Some((usage, events));
        cx.notify();
    }

    pub(super) fn usage_overlay(&self) -> Option<AnyElement> {
        self.usage
            .as_ref()
            .map(|(usage, _)| usage.clone().into_any_element())
    }
}

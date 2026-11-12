//! "DCC Usage" over the window (`setIsUsageOpen`): the sidebar's chart
//! button and the palette open it.

use gpui::{AnyElement, AppContext, Context, Entity, IntoElement, Subscription};

use super::Shell;
use crate::screens::usage::{Usage, UsageEvent};

pub(super) type OpenUsage = (Entity<Usage>, Subscription);

impl Shell {
    pub(super) fn open_usage(&mut self, cx: &mut Context<Self>) {
        if self.usage.is_some() {
            return;
        }
        let db_path = crate::data::loader::database_path();
        let usage = cx.new(|cx| Usage::new(db_path, cx));
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

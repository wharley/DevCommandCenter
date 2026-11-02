//! The Device panel (an iOS Simulator) beside the conversation, opened from
//! the header's phone. It stays while the person moves between tasks: the
//! simulator is the Mac's, not the task's.

use gpui::{AnyElement, AppContext, Context, Entity, Subscription, Window, div, prelude::*};

use super::Shell;
use crate::screens::device::{DeviceEvent, DevicePanel};

pub(super) type OpenDevice = (Entity<DevicePanel>, Subscription);

impl Shell {
    pub(super) fn toggle_device(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.device.take().is_none() {
            let panel = cx.new(DevicePanel::new);
            let events = cx.subscribe_in(&panel, window, |shell, _, event, _, cx| match event {
                DeviceEvent::Closed => {
                    shell.device = None;
                    shell.sync_header(cx);
                    cx.notify();
                }
            });
            self.device = Some((panel, events));
        }
        self.sync_header(cx);
        cx.notify();
    }

    pub(super) fn device_open(&self) -> bool {
        self.device.is_some()
    }

    /// The conversation with the panel at its right.
    pub(super) fn with_device(&self, content: AnyElement) -> AnyElement {
        let Some((panel, _)) = &self.device else {
            return content;
        };
        div()
            .flex()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .child(content),
            )
            .child(panel.clone())
            .into_any_element()
    }
}

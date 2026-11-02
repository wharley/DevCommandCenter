//! The Device panel (an iOS Simulator) beside the conversation, opened from
//! the header's phone or by an agent's `dcc_device_open`. It stays while the
//! person moves between tasks: the simulator is the Mac's, not the task's.

use std::sync::Arc;

use futures::StreamExt;
use futures::channel::mpsc;
use gpui::{AnyElement, AppContext, Context, Entity, Subscription, Window, div, prelude::*};

use super::Shell;
use crate::screens::device::agent::{AgentHost, AgentRequest};
use crate::screens::device::{DeviceEvent, DevicePanel};

pub(super) type OpenDevice = (Entity<DevicePanel>, Subscription);

/// The agents' simulator tools reach the panel from here on.
pub(super) fn serve_agents(window: &mut Window, cx: &mut Context<Shell>) {
    if cfg!(test) {
        return;
    }
    let (requests, mut received) = mpsc::unbounded();
    dcc_server::device_tools::register_device_host(Arc::new(AgentHost::new(requests)));
    cx.spawn_in(window, async move |shell, cx| {
        while let Some(request) = received.next().await {
            let handled = shell.update_in(cx, |shell, window, cx| {
                shell.agent_device(request, window, cx)
            });
            if handled.is_err() {
                return;
            }
        }
    })
    .detach();
}

impl Shell {
    fn agent_device(&mut self, request: AgentRequest, window: &mut Window, cx: &mut Context<Self>) {
        match request {
            AgentRequest::Open { udid, reply } => {
                if self.device.is_none() {
                    self.toggle_device(window, cx);
                }
                if let Some((panel, _)) = &self.device {
                    panel.update(cx, |panel, cx| panel.agent_open(udid, reply, cx));
                }
            }
            AgentRequest::Link { udid, reply } => {
                let link = match &self.device {
                    Some((panel, _)) => panel.read(cx).agent_link(&udid),
                    None => Err("the Device panel was closed; call dcc_device_open".into()),
                };
                let _ = reply.send(link);
            }
        }
    }

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

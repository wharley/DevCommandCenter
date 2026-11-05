//! The Device panel (an iOS Simulator) beside the conversation, opened from
//! the header's phone or by an agent's `dcc_device_open`. It stays while the
//! person moves between tasks and shows each task's own simulator.

use std::sync::Arc;

use futures::StreamExt;
use futures::channel::mpsc;
use gpui::{AnyElement, AppContext, Context, Entity, Subscription, Window, div, prelude::*};

use super::Shell;
use crate::screens::device::agent::{AgentHost, AgentRequest};
use crate::screens::device::owners::Owner;
use crate::screens::device::{DeviceEvent, DevicePanel};

/// The tasks among `tasks` that were completed, archived or deleted.
fn finished_tasks(tasks: Vec<String>) -> Vec<String> {
    use dcc_core::domain::workspace::{WorkspaceId, WorkspaceState};
    use dcc_core::ports::WorkspaceRepo;
    if tasks.is_empty() {
        return tasks;
    }
    let db = crate::data::loader::database_path();
    let Ok(repo) = dcc_infra::db::SqliteWorkspaceRepo::open(&db) else {
        return Vec::new();
    };
    tasks
        .into_iter()
        .filter(|id| {
            match futures::executor::block_on(repo.get_workspace(&WorkspaceId(id.clone()))) {
                Ok(Some(task)) => {
                    matches!(
                        task.state,
                        WorkspaceState::Completed | WorkspaceState::Archived
                    )
                }
                Ok(None) => true,
                Err(_) => false,
            }
        })
        .collect()
}

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
            AgentRequest::Open { udid, owner, reply } => {
                if self.device.is_none() {
                    self.toggle_device(window, cx);
                }
                if let Some((panel, _)) = &self.device {
                    panel.update(cx, |panel, cx| panel.agent_open(owner, udid, reply, cx));
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
            self.device_task_changed(cx);
        }
        self.sync_header(cx);
        cx.notify();
    }

    /// The task on screen, as the Device panel names it.
    fn task_on_screen(&self, cx: &gpui::App) -> Option<Owner> {
        let data = self.conversation.read(cx).data()?;
        let title = data
            .workspace_name
            .clone()
            .filter(|name| !name.trim().is_empty())
            .or_else(|| data.branch.clone())
            .unwrap_or_else(|| data.task_id.clone());
        Some(Owner {
            task_id: data.task_id.clone(),
            title,
        })
    }

    /// The panel follows the task on screen to its own simulator; the
    /// simulators of tasks that finished go down.
    pub(super) fn device_task_changed(&mut self, cx: &mut Context<Self>) {
        let Some((panel, _)) = &self.device else {
            return;
        };
        let panel = panel.clone();
        let task = self.task_on_screen(cx);
        let (same, held) = {
            let panel = panel.read(cx);
            (panel.task == task, panel.owners.tasks())
        };
        if same {
            return;
        }
        let finished = finished_tasks(held);
        panel.update(cx, |panel, cx| panel.task_changed(task, &finished, cx));
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

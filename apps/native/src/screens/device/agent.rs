//! The agents' simulator tools (`dcc_server::device_tools`) through the
//! Device panel: an agent opening a simulator opens the panel on it, so the
//! person sees what the agent does, and its touches, keys and accessibility
//! reads go through the panel's own helper. The backend runs on its own
//! threads; it asks the shell over a channel and the shell answers on the
//! UI thread.

use std::sync::Arc;

use dcc_server::device_tools::{DeviceFuture, DeviceHost, OpenDevice};
use futures::channel::{mpsc, oneshot};
use gpui::Context;
use serde_json::{Value, json};

use super::owners::Owner;
use super::{DevicePanel, Phase, keys};
use crate::data::device::link::DeviceLink;
use crate::data::device::{self, Simulator};

pub(crate) type Waiting = oneshot::Sender<Result<OpenDevice, String>>;

/// What the backend asks of the Device panel.
pub enum AgentRequest {
    /// Show this simulator (or the panel's, or the most recent) and answer
    /// once it streams.
    Open {
        udid: Option<String>,
        /// The task of the agent's session.
        owner: Option<Owner>,
        reply: Waiting,
    },
    /// The panel's helper for `udid`, while it shows it.
    Link {
        udid: String,
        reply: oneshot::Sender<Result<Arc<DeviceLink>, String>>,
    },
}

pub struct AgentHost {
    requests: mpsc::UnboundedSender<AgentRequest>,
}

impl AgentHost {
    pub fn new(requests: mpsc::UnboundedSender<AgentRequest>) -> Self {
        Self { requests }
    }

    fn link(&self, udid: String) -> DeviceFuture<Arc<DeviceLink>> {
        let (reply, answer) = oneshot::channel();
        let sent = self
            .requests
            .unbounded_send(AgentRequest::Link { udid, reply });
        Box::pin(async move {
            sent.map_err(|_| closed())?;
            answer.await.map_err(|_| closed())?
        })
    }
}

fn closed() -> String {
    "the DCC window is closed".into()
}

/// The task a provider session works in, named for "in use by …".
fn session_owner(session_id: &str) -> Option<Owner> {
    use dcc_core::domain::session::SessionId;
    use dcc_core::ports::{SessionRepo, WorkspaceRepo};
    let db = crate::data::loader::database_path();
    let sessions = dcc_infra::db::SqliteSessionRepo::open(&db).ok()?;
    let session =
        futures::executor::block_on(sessions.get_session(&SessionId(session_id.into()))).ok()??;
    let workspaces = dcc_infra::db::SqliteWorkspaceRepo::open(&db).ok()?;
    let workspace =
        futures::executor::block_on(workspaces.get_workspace(&session.workspace_id)).ok()??;
    let title = workspace
        .name
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| workspace.base_branch.clone());
    Some(Owner {
        task_id: workspace.id.0,
        title,
    })
}

impl DeviceHost for AgentHost {
    fn open(&self, session_id: &str, udid: Option<String>) -> DeviceFuture<OpenDevice> {
        let (reply, answer) = oneshot::channel();
        let owner = session_owner(session_id);
        let sent = self
            .requests
            .unbounded_send(AgentRequest::Open { udid, owner, reply });
        Box::pin(async move {
            sent.map_err(|_| closed())?;
            answer.await.map_err(|_| closed())?
        })
    }

    fn command(&self, udid: String, command: Value) -> DeviceFuture<Value> {
        let link = self.link(udid);
        Box::pin(async move {
            let link = link.await?;
            link.request(command)
                .await
                .map_err(|_| "the simulator helper ended".to_string())?
        })
    }

    fn type_text(&self, udid: String, text: String) -> DeviceFuture<()> {
        let link = self.link(udid);
        Box::pin(async move {
            let link = link.await?;
            let unknown: String = text
                .chars()
                .filter(|ch| keys::for_char(*ch).is_none())
                .collect();
            if !unknown.is_empty() {
                return Err(format!(
                    "these characters are not on the US keyboard: {unknown}; type the rest, or paste with a link"
                ));
            }
            let mut presses = Vec::new();
            for (usage, shift) in text.chars().filter_map(keys::for_char) {
                if shift {
                    presses.push(("down", keys::SHIFT));
                }
                presses.extend([("down", usage), ("up", usage)]);
                if shift {
                    presses.push(("up", keys::SHIFT));
                }
            }
            let key =
                |(phase, usage): (&str, u32)| json!({"op": "key", "phase": phase, "usage": usage});
            let last = presses.pop().ok_or("nothing to type")?;
            for press in presses {
                link.send(key(press));
            }
            // Its answer means every key before it went through.
            link.request(key(last))
                .await
                .map_err(|_| "the simulator helper ended".to_string())??;
            Ok(())
        })
    }
}

impl DevicePanel {
    fn open_device(&self) -> Option<OpenDevice> {
        let sim = self.current.as_ref()?;
        (self.phase == Phase::Live && self.size.0 > 0).then(|| OpenDevice {
            udid: sim.udid.clone(),
            name: sim.name.clone(),
            runtime: sim.runtime.clone(),
            width: self.size.0,
            height: self.size.1,
        })
    }

    pub(super) fn answer_agents(&mut self, result: Result<OpenDevice, String>) {
        for waiting in self.agent_waiting.drain(..) {
            let _ = waiting.send(result.clone());
        }
    }

    pub(super) fn answer_agents_when_live(&mut self) {
        if let Some(device) = self.open_device() {
            self.answer_agents(Ok(device));
        }
    }

    /// Why `task`'s agent may not have the panel now: it shows, for the
    /// person, the simulator of the task they have open.
    fn busy_for(&self, task: Option<&str>) -> Option<String> {
        let shown = self.current.as_ref()?;
        let holder = self.owners.owner(&shown.udid)?;
        let on_screen = self.task.as_ref()?;
        (on_screen.task_id == holder.task_id && task != Some(holder.task_id.as_str())).then(|| {
            format!(
                "DCC's Device panel shows the simulator of the task \"{}\", which the person has open; this task's simulator shows when they come back to it. Do not retry now.",
                holder.title
            )
        })
    }

    /// An agent's `dcc_device_open`: the simulator it names, its task's, the
    /// one shown, or the most recently used — never another task's.
    pub(crate) fn agent_open(
        &mut self,
        owner: Option<Owner>,
        udid: Option<String>,
        reply: Waiting,
        cx: &mut Context<Self>,
    ) {
        let task = owner.as_ref().map(|owner| owner.task_id.clone());
        if let Some(busy) = self.busy_for(task.as_deref()) {
            let _ = reply.send(Err(busy));
            return;
        }
        if let Some(udid) = &udid
            && let Some(holder) = self.owners.taken(udid, task.as_deref())
        {
            let _ = reply.send(Err(format!(
                "simulator {udid} is in use by the task \"{}\"; pick another with dcc_device_list",
                holder.title
            )));
            return;
        }
        let wanted = udid.or_else(|| {
            task.as_deref()
                .and_then(|task| self.owners.of_task(task))
                .map(str::to_string)
        });
        let shown = self.current.as_ref().map(|sim| sim.udid.clone());
        let shown_free = shown
            .as_deref()
            .is_some_and(|udid| self.owners.taken(udid, task.as_deref()).is_none());
        if (wanted.is_none() && shown_free) || (wanted.is_some() && wanted == shown) {
            if let (Some(owner), Some(udid)) = (owner.clone(), shown.as_deref()) {
                self.owners.claim(udid, owner);
            }
            if let Some(device) = self.open_device() {
                let _ = reply.send(Ok(device));
                return;
            }
            if self.phase == Phase::Starting {
                self.agent_waiting.push(reply);
                return;
            }
        }
        self.agent_choosing = true;
        self.agent_waiting.push(reply);
        cx.spawn(async move |this, cx| {
            let listed = cx
                .background_executor()
                .spawn(async { device::list() })
                .await;
            let _ = this.update(cx, |panel, cx| {
                panel.agent_choosing = false;
                let chosen: Result<Simulator, String> = listed.and_then(|simulators| {
                    let free =
                        |sim: &&Simulator| panel.owners.taken(&sim.udid, task.as_deref()).is_none();
                    let found = match &wanted {
                        Some(udid) => simulators.iter().find(|sim| &sim.udid == udid),
                        // Running or most recently used first, then iPhones.
                        None => simulators.iter().filter(free).find(|sim| !sim.ipad),
                    };
                    let chosen = found.cloned().ok_or_else(|| match &wanted {
                        Some(udid) => format!("no simulator {udid}; call dcc_device_list"),
                        None => "no free iPhone simulator; install one with Xcode".to_string(),
                    });
                    panel.simulators = simulators;
                    chosen
                });
                match chosen {
                    Ok(sim) => panel.open_for(sim, owner, cx),
                    Err(error) => panel.answer_agents(Err(error)),
                }
            });
        })
        .detach();
    }

    /// The helper for `udid`, while the panel shows it.
    pub fn agent_link(&self, udid: &str) -> Result<Arc<DeviceLink>, String> {
        match (&self.current, &self.link) {
            (Some(sim), Some(link)) if sim.udid == udid && self.phase == Phase::Live => {
                Ok(link.clone())
            }
            _ => Err("the simulator is no longer shown in DCC; call dcc_device_open".into()),
        }
    }
}

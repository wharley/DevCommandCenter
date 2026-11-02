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

impl DeviceHost for AgentHost {
    fn open(&self, udid: Option<String>) -> DeviceFuture<OpenDevice> {
        let (reply, answer) = oneshot::channel();
        let sent = self
            .requests
            .unbounded_send(AgentRequest::Open { udid, reply });
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

    /// An agent's `dcc_device_open`: the simulator it names, the one shown,
    /// or the most recently used.
    pub fn agent_open(&mut self, udid: Option<String>, reply: Waiting, cx: &mut Context<Self>) {
        let shown = self.current.as_ref().map(|sim| sim.udid.clone());
        if udid.is_none() || udid == shown {
            if let Some(device) = self.open_device() {
                let _ = reply.send(Ok(device));
                return;
            }
            if shown.is_some() && self.phase == Phase::Starting {
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
                    let found = match &udid {
                        Some(udid) => simulators.iter().find(|sim| &sim.udid == udid),
                        // Running or most recently used first, then iPhones.
                        None => simulators.iter().find(|sim| !sim.ipad),
                    };
                    let chosen = found.cloned().ok_or_else(|| match &udid {
                        Some(udid) => format!("no simulator {udid}; call dcc_device_list"),
                        None => "no iPhone simulator; install one with Xcode".to_string(),
                    });
                    panel.simulators = simulators;
                    chosen
                });
                match chosen {
                    Ok(sim) => panel.open(sim, cx),
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

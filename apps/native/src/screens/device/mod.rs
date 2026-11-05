//! The Device panel: an iOS Simulator beside the conversation, drawn by
//! the app (its H.264 decoded by VideoToolbox into GPUI's `surface`) and
//! used with the mouse and keyboard like the Simulator app — no Simulator
//! window. The simulator is chosen from the ones Xcode has; one not running
//! is booted without its window. Each task keeps its own simulator
//! (`owners`): the panel follows the task on screen, and another task's
//! simulator keeps running. The simulators the panel booted are shut down
//! when it closes, when their task finishes, or when a task changes to
//! another one; one that was running already (Xcode's) is left as it was.
//! The power mark shuts one down.

pub(crate) mod agent;
mod keys;
pub(crate) mod owners;
mod view;

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use futures::StreamExt;
use futures::channel::mpsc;
use gpui::{Bounds, Context, FocusHandle, Keystroke, Pixels, Point, Task, Window};
use serde_json::json;

use crate::data::device::decode::{Decoder, Picture};
use crate::data::device::link::{DeviceLink, LinkEvent};
use crate::data::device::{self, Simulator};
use crate::i18n::Translate;
use owners::{Owner, Owners};

impl Drop for DevicePanel {
    fn drop(&mut self) {
        self.release();
        self.shut_down_started();
    }
}

pub enum DeviceEvent {
    Closed,
}

impl gpui::EventEmitter<DeviceEvent> for DevicePanel {}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Phase {
    /// No simulator open: the list shows.
    Choosing,
    /// Booting it and starting the helper.
    Starting,
    Live,
    Failed(String),
}

/// A decoded picture on its way to the main thread. CoreVideo pixel
/// buffers are thread-safe; this one is not touched again by the decoder.
struct Shown(Picture);
// SAFETY: a CVPixelBuffer may be passed between threads (CoreVideo).
unsafe impl Send for Shown {}

enum Update {
    Picture(Shown),
    Size(u32, u32),
    Closed(Option<String>),
}

pub struct DevicePanel {
    pub(crate) simulators: Vec<Simulator>,
    pub(crate) listing: bool,
    pub(crate) list_error: Option<String>,
    pub(crate) current: Option<Simulator>,
    pub(crate) phase: Phase,
    link: Option<Arc<DeviceLink>>,
    pub(crate) picture: Option<Picture>,
    /// The screen's size in pixels.
    pub(crate) size: (u32, u32),
    pub(crate) focus: FocusHandle,
    /// Where the screen was painted, for the pointer.
    pub(crate) screen: Rc<Cell<Bounds<Pixels>>>,
    /// The room the panel has below its bar, as last laid out.
    pub(crate) room: Rc<Cell<gpui::Size<Pixels>>>,
    /// A finger is down on the screen.
    pressed: bool,
    /// The simulators this panel booted (it shuts them down on leaving).
    pub(crate) started: std::collections::HashSet<String>,
    /// Agents waiting for the simulator they opened to stream (`agent`).
    agent_waiting: Vec<agent::Waiting>,
    /// An agent is choosing the simulator: the running one does not open by itself.
    agent_choosing: bool,
    /// The task each simulator belongs to.
    pub(crate) owners: Owners,
    /// The task on screen.
    pub(crate) task: Option<Owner>,
    _list: Option<Task<()>>,
    _start: Option<Task<()>>,
    _frames: Option<Task<()>>,
}

impl DevicePanel {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let mut panel = Self {
            simulators: Vec::new(),
            listing: false,
            list_error: None,
            current: None,
            phase: Phase::Choosing,
            link: None,
            picture: None,
            size: (0, 0),
            focus: cx.focus_handle(),
            screen: Rc::default(),
            room: Rc::default(),
            pressed: false,
            started: Default::default(),
            agent_waiting: Vec::new(),
            agent_choosing: false,
            owners: Owners::default(),
            task: None,
            _list: None,
            _start: None,
            _frames: None,
        };
        panel.refresh(cx);
        panel
    }

    /// The simulators again (`simctl list`), the open one first if it runs.
    pub(crate) fn refresh(&mut self, cx: &mut Context<Self>) {
        if cfg!(test) {
            return;
        }
        self.listing = true;
        self._list = Some(cx.spawn(async move |this, cx| {
            let listed = cx
                .background_executor()
                .spawn(async { device::list() })
                .await;
            let _ = this.update(cx, |panel, cx| {
                panel.listing = false;
                match listed {
                    Ok(simulators) => {
                        panel.list_error = None;
                        panel.simulators = simulators;
                        if panel.current.is_none()
                            && !panel.agent_choosing
                            && let Some(sim) = panel.opens_by_itself()
                        {
                            panel.open(sim, cx);
                        }
                    }
                    Err(error) => panel.list_error = Some(error),
                }
                cx.notify();
            });
        }));
    }

    /// Ends the helper off the main thread (it waits for the process).
    fn release(&mut self) {
        // The agents check their visible changes only while one is live.
        dcc_tauri::device_routing::set_device_panel_open(false);
        if let Some(link) = self.link.take() {
            std::thread::spawn(move || drop(link));
        }
    }

    /// What shows without a choice: the task's own simulator, else one
    /// already running that no other task holds.
    pub(crate) fn opens_by_itself(&self) -> Option<Simulator> {
        let task = self.task.as_ref().map(|task| task.task_id.as_str());
        if let Some(own) = task.and_then(|task| self.owners.of_task(task)) {
            return self.simulators.iter().find(|sim| sim.udid == own).cloned();
        }
        self.simulators
            .iter()
            .find(|sim| sim.booted && self.owners.taken(&sim.udid, task).is_none())
            .cloned()
    }

    /// Shuts `udid` down if the panel booted it.
    fn shut_down(&mut self, udid: &str) {
        if self.started.remove(udid) {
            device::shutdown_detached(udid);
        }
    }

    fn shut_down_started(&mut self) {
        for udid in std::mem::take(&mut self.started) {
            device::shutdown_detached(&udid);
        }
    }

    /// The list again, the simulator shown before still running.
    pub(crate) fn stop_showing(&mut self, cx: &mut Context<Self>) {
        self.release();
        self._start = None;
        self._frames = None;
        self.picture = None;
        self.current = None;
        self.phase = Phase::Choosing;
        cx.notify();
    }

    /// The task on screen changed: its own simulator shows, or the list
    /// when the one shown is another task's. The simulators of `finished`
    /// tasks are let go and shut down.
    pub(crate) fn task_changed(
        &mut self,
        task: Option<Owner>,
        finished: &[String],
        cx: &mut Context<Self>,
    ) {
        for task_id in finished {
            if let Some(udid) = self.owners.release_task(task_id) {
                if self.current.as_ref().is_some_and(|sim| sim.udid == udid) {
                    self.stop_showing(cx);
                }
                self.shut_down(&udid);
            }
        }
        if task == self.task {
            return;
        }
        self.task = task.clone();
        let Some(task) = task else {
            return;
        };
        let shown = self.current.as_ref().map(|sim| sim.udid.clone());
        let own = self.owners.of_task(&task.task_id).map(str::to_string);
        if own.is_some() && own == shown {
            return;
        }
        let others = shown
            .as_deref()
            .is_some_and(|udid| self.owners.taken(udid, Some(&task.task_id)).is_some());
        if own.is_some() || others {
            self.stop_showing(cx);
        }
        if self.current.is_none()
            && let Some(sim) = self.opens_by_itself()
        {
            self.open(sim, cx);
        }
    }

    /// Boots `sim` if needed and shows it for the task on screen; never
    /// another task's.
    pub(crate) fn open(&mut self, sim: Simulator, cx: &mut Context<Self>) {
        let owner = self.task.clone();
        let task = owner.as_ref().map(|owner| owner.task_id.as_str());
        if self.owners.taken(&sim.udid, task).is_some() {
            return;
        }
        self.open_for(sim, owner, cx);
    }

    /// Shows `sim` as `owner`'s. The one shown before goes down if the panel
    /// booted it and no other task holds it, and so does the one `owner`
    /// held before.
    pub(crate) fn open_for(
        &mut self,
        sim: Simulator,
        owner: Option<Owner>,
        cx: &mut Context<Self>,
    ) {
        self.release();
        let task = owner.as_ref().map(|owner| owner.task_id.clone());
        let mut going = Vec::new();
        if let Some(current) = self
            .current
            .as_ref()
            .filter(|current| current.udid != sim.udid)
            && self.owners.taken(&current.udid, task.as_deref()).is_none()
        {
            going.push(current.udid.clone());
        }
        if let Some(owner) = owner {
            going.extend(self.owners.claim(&sim.udid, owner));
        }
        for udid in going {
            self.owners.release(&udid);
            self.shut_down(&udid);
        }
        self._frames = None;
        self.picture = None;
        self.current = Some(sim.clone());
        let Some(helper) = device::helper() else {
            self.phase = Phase::Failed(cx.t("device.noHelper").to_string());
            cx.notify();
            return;
        };
        self.phase = Phase::Starting;
        cx.notify();
        self._start = Some(cx.spawn(async move |this, cx| {
            let udid = sim.udid.clone();
            let (booted, started) = cx
                .background_executor()
                .spawn(async move {
                    match device::boot(&udid) {
                        Ok(booted) => (booted, DeviceLink::open(&helper, &udid)),
                        Err(error) => (false, Err(error)),
                    }
                })
                .await;
            let _ = this.update(cx, |panel, cx| {
                if booted {
                    panel.started.insert(sim.udid.clone());
                }
                panel.started_link(started, cx);
            });
        }));
    }

    fn started_link(
        &mut self,
        started: Result<(DeviceLink, mpsc::UnboundedReceiver<LinkEvent>), String>,
        cx: &mut Context<Self>,
    ) {
        let panel = self;
        match started {
            Ok((link, events)) => {
                panel.link = Some(Arc::new(link));
                panel.phase = Phase::Live;
                dcc_tauri::device_routing::set_device_panel_open(true);
                panel.follow(events, cx);
                cx.notify();
            }
            Err(error) => {
                panel.answer_agents(Err(error.clone()));
                panel.phase = Phase::Failed(error);
                cx.notify();
            }
        }
    }

    /// Decodes off the main thread; the screen shows the newest picture.
    fn follow(&mut self, events: mpsc::UnboundedReceiver<LinkEvent>, cx: &mut Context<Self>) {
        let (updates, mut received) = mpsc::unbounded::<Update>();
        std::thread::spawn(move || decode_loop(events, updates));
        self._frames = Some(cx.spawn(async move |this, cx| {
            while let Some(first) = received.next().await {
                let mut batch = vec![first];
                while let Ok(more) = received.try_recv() {
                    batch.push(more);
                }
                let alive = this.update(cx, |panel, cx| {
                    for update in batch {
                        match update {
                            Update::Picture(Shown(picture)) => panel.picture = Some(picture),
                            Update::Size(width, height) => {
                                panel.size = (width, height);
                                panel.answer_agents_when_live();
                            }
                            Update::Closed(reason) => {
                                panel.link = None;
                                let reason =
                                    reason.unwrap_or_else(|| cx.t("device.ended").to_string());
                                panel.answer_agents(Err(reason.clone()));
                                panel.phase = Phase::Failed(reason);
                            }
                        }
                    }
                    cx.notify();
                });
                if alive.is_err() {
                    return;
                }
            }
        }));
    }

    /// Every simulator the panel booted goes down, each task's.
    pub(crate) fn close(&mut self, cx: &mut Context<Self>) {
        self.release();
        self.shut_down_started();
        cx.emit(DeviceEvent::Closed);
    }

    /// Shuts the open simulator down; the list shows again.
    pub(crate) fn power_off(&mut self, cx: &mut Context<Self>) {
        let Some(sim) = self.current.take() else {
            return;
        };
        self.started.remove(&sim.udid);
        self.owners.release(&sim.udid);
        self.release();
        self._frames = None;
        self.picture = None;
        self.phase = Phase::Choosing;
        cx.notify();
        self._start = Some(cx.spawn(async move |this, cx| {
            let _ = cx
                .background_executor()
                .spawn(async move { device::shutdown(&sim.udid) })
                .await;
            let _ = this.update(cx, |panel, cx| panel.refresh(cx));
        }));
    }

    pub(crate) fn button(&self, name: &str) {
        if let Some(link) = &self.link {
            link.send(json!({"op": "button", "name": name}));
        }
    }

    /// `position` in window coordinates, as 0..1 of the screen.
    fn unit(&self, position: Point<Pixels>) -> (f64, f64) {
        let bounds = self.screen.get();
        let width = f32::from(bounds.size.width).max(1.0);
        let height = f32::from(bounds.size.height).max(1.0);
        let x = (f32::from(position.x - bounds.origin.x) / width).clamp(0.0, 1.0);
        let y = (f32::from(position.y - bounds.origin.y) / height).clamp(0.0, 1.0);
        (x as f64, y as f64)
    }

    pub(crate) fn touch(
        &mut self,
        phase: &str,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut gpui::App,
    ) {
        match phase {
            "begin" => {
                self.pressed = true;
                window.focus(&self.focus, cx);
            }
            "move" if !self.pressed => return,
            "end" if !self.pressed => return,
            "end" => self.pressed = false,
            _ => {}
        }
        let (x, y) = self.unit(position);
        if let Some(link) = &self.link {
            link.send(json!({"op": "touch", "phase": phase, "x": x, "y": y}));
        }
    }

    /// A wheel or trackpad scroll, in the screen's own pixels.
    pub(crate) fn scroll(&self, delta: Point<Pixels>, position: Point<Pixels>) {
        let bounds = self.screen.get();
        let scale = self.size.0 as f32 / f32::from(bounds.size.width).max(1.0);
        let (x, y) = self.unit(position);
        if let Some(link) = &self.link {
            link.send(json!({
                "op": "scroll",
                "dx": f32::from(delta.x) * scale,
                "dy": f32::from(delta.y) * scale,
                "x": x,
                "y": y,
            }));
        }
    }

    /// A key typed while the panel has the focus; `false` when it is not
    /// the simulator's (the app's shortcuts).
    pub(crate) fn key(&self, keystroke: &Keystroke) -> bool {
        let (Some(link), Some((usage, shift))) = (&self.link, keys::for_keystroke(keystroke))
        else {
            return false;
        };
        let press = |phase: &str, usage: u32| {
            link.send(json!({"op": "key", "phase": phase, "usage": usage}));
        };
        if shift {
            press("down", keys::SHIFT);
        }
        press("down", usage);
        press("up", usage);
        if shift {
            press("up", keys::SHIFT);
        }
        true
    }
}

fn decode_loop(
    mut events: mpsc::UnboundedReceiver<LinkEvent>,
    updates: mpsc::UnboundedSender<Update>,
) {
    let mut decoder: Option<Decoder> = None;
    while let Some(event) = futures::executor::block_on(events.next()) {
        let update = match event {
            LinkEvent::Description(avcc) => {
                decoder = Decoder::new(&avcc).ok();
                continue;
            }
            LinkEvent::Frame { data, .. } => {
                match decoder.as_mut().and_then(|decoder| decoder.decode(&data)) {
                    Some(picture) => Update::Picture(Shown(picture)),
                    None => continue,
                }
            }
            LinkEvent::Size(width, height) => Update::Size(width, height),
            LinkEvent::Closed(reason) => {
                let _ = updates.unbounded_send(Update::Closed(reason));
                return;
            }
        };
        if updates.unbounded_send(update).is_err() {
            return;
        }
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

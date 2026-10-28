//! "Permitir Computer Use?" (`ComputerUseApprovalHost`): an agent asks to
//! control apps in a conversation. While this app listens to the desktop
//! it answers them, one card at a time, soonest to expire first: the
//! provider, the conversation, the reason, the time left, the apps with a
//! window to choose from (none chosen by default, at most 16), and the two
//! macOS permissions the desktop needs, with a button to each one's
//! settings. "Permitir" needs all of it; "Negar" is always there, and only
//! the buttons answer. The Mac's state is read again every 1.5s while the
//! card shows. A request that runs out unanswered says so in a toast.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use futures::StreamExt;
use gpui::{Context, EventEmitter, Task};

use crate::data::computer_use::{AccessKind, ComputerEvent, ComputerStatus, ControlRequest};
use crate::data::desktop_api::DesktopApi;
use crate::i18n::Translate;
use crate::screens::browser::approval::TitleOf;
use crate::ui::Toaster;

const RECONNECT: Duration = Duration::from_secs(2);
const STATUS_EVERY: Duration = Duration::from_millis(1500);
/// `selectedBundleIds.slice(0, 16)`
pub const MAX_APPS: usize = 16;

/// What the agent does and its preview's frames, for the preview.
pub struct LiveEvent(pub ComputerEvent);

impl EventEmitter<LiveEvent> for ComputerApprovals {}

/// A request, with when it runs out here.
#[derive(Clone, Debug)]
pub struct Waiting {
    pub request: ControlRequest,
    pub deadline: Instant,
}

impl Waiting {
    pub fn remaining_ms(&self) -> u64 {
        self.deadline
            .saturating_duration_since(Instant::now())
            .as_millis() as u64
    }
}

pub struct ComputerApprovals {
    pub(super) api: Option<DesktopApi>,
    pub requests: Vec<Waiting>,
    pub status: Option<ComputerStatus>,
    pub status_failed: bool,
    /// The request the status is for.
    status_for: Option<String>,
    /// The apps chosen, per request.
    pub(super) chosen: HashMap<String, Vec<String>>,
    pub busy: Option<String>,
    pub(super) busy_access: Option<AccessKind>,
    pub access_error: Option<String>,
    pub decision_error: Option<String>,
    decided: HashSet<String>,
    expired: HashSet<String>,
    announced: HashSet<String>,
    pub(super) title_of: TitleOf,
    _stream: Option<Task<()>>,
    _status: Option<Task<()>>,
    _tick: Option<Task<()>>,
    _decide: Option<Task<()>>,
    _access: Option<Task<()>>,
}

/// `formatRemaining`: `1:05`, or `45s` under a minute.
pub fn format_remaining(ms: u64) -> String {
    let seconds = ms.div_ceil(1000);
    let (minutes, rest) = (seconds / 60, seconds % 60);
    if minutes > 0 {
        format!("{minutes}:{rest:02}")
    } else {
        format!("{rest}s")
    }
}

impl ComputerApprovals {
    pub fn new(api: Option<DesktopApi>, title_of: TitleOf, cx: &mut Context<Self>) -> Self {
        let mut approvals = Self {
            api,
            requests: Vec::new(),
            status: None,
            status_failed: false,
            status_for: None,
            chosen: HashMap::new(),
            busy: None,
            busy_access: None,
            access_error: None,
            decision_error: None,
            decided: HashSet::new(),
            expired: HashSet::new(),
            announced: HashSet::new(),
            title_of,
            _stream: None,
            _status: None,
            _tick: None,
            _decide: None,
            _access: None,
        };
        approvals.listen(cx);
        approvals
    }

    fn listen(&mut self, cx: &mut Context<Self>) {
        let Some(api) = self.api.clone().filter(|_| !cfg!(test)) else {
            return;
        };
        let (sender, mut events) = futures::channel::mpsc::unbounded::<ComputerEvent>();
        std::thread::spawn(move || {
            loop {
                if let Ok(stream) = api.computer_events() {
                    for event in stream {
                        if sender.unbounded_send(event).is_err() {
                            return;
                        }
                    }
                }
                if sender
                    .unbounded_send(ComputerEvent::Requests(Vec::new()))
                    .is_err()
                {
                    return;
                }
                std::thread::sleep(RECONNECT);
            }
        });
        self._stream = Some(cx.spawn(async move |this, cx| {
            while let Some(event) = events.next().await {
                if this
                    .update(cx, |approvals, cx| approvals.apply(event, cx))
                    .is_err()
                {
                    return;
                }
            }
        }));
    }

    pub fn current(&self) -> Option<&Waiting> {
        self.requests.first()
    }

    /// What the stream said.
    pub fn apply(&mut self, event: ComputerEvent, cx: &mut Context<Self>) {
        let requests = match event {
            ComputerEvent::Requests(requests) => requests,
            // The preview's: the shell passes them on.
            live => {
                cx.emit(LiveEvent(live));
                return;
            }
        };
        let now = Instant::now();
        let before = self.current().cloned();
        self.requests = requests
            .into_iter()
            .map(|request| Waiting {
                deadline: now + Duration::from_millis(request.remaining_ms),
                request,
            })
            .collect();
        let ids: HashSet<&str> = self
            .requests
            .iter()
            .map(|waiting| waiting.request.request_id.as_str())
            .collect();
        self.chosen.retain(|id, _| ids.contains(id.as_str()));
        if self.busy.as_deref().is_some_and(|id| !ids.contains(id)) {
            self.busy = None;
        }
        // It left past its time, unanswered: it expired.
        if let Some(before) = before
            && !ids.contains(before.request.request_id.as_str())
            && before.deadline <= now
        {
            self.toast_expired(&before.request.request_id, cx);
        }
        let first = self
            .current()
            .map(|waiting| waiting.request.request_id.clone());
        if first != self.status_for {
            self.access_error = None;
            self.decision_error = None;
            self.status = None;
            self.status_failed = false;
            self.status_for = first.clone();
        }
        match first {
            Some(id) => {
                if self.announced.insert(id) {
                    let reason = self
                        .current()
                        .map(|waiting| waiting.request.reason.clone())
                        .unwrap_or_default();
                    crate::ui::attention::request_attention(
                        &cx.t("settings.computerUse.approval.title"),
                        &reason,
                    );
                }
                self.follow(cx);
            }
            None => {
                self._status = None;
                self._tick = None;
            }
        }
        cx.notify();
    }

    /// The Mac's state every 1.5s, and the clock every second.
    fn follow(&mut self, cx: &mut Context<Self>) {
        if self._status.is_none() {
            self._status = Some(cx.spawn(async move |this, cx| {
                loop {
                    let ask = this.update(cx, |approvals, _| {
                        let session = approvals.current()?.request.session_id.clone();
                        Some((approvals.api.clone()?, session))
                    });
                    let Ok(ask) = ask else {
                        return;
                    };
                    if let Some((api, session)) = ask {
                        let asked = session.clone();
                        let status = cx
                            .background_executor()
                            .spawn(async move { api.computer_status(&asked) })
                            .await;
                        let done = this.update(cx, |approvals, cx| {
                            // Only for the card still shown.
                            let shown = approvals.current().map(|w| w.request.session_id.clone());
                            if shown.as_deref() != Some(session.as_str()) {
                                return;
                            }
                            match status {
                                Ok(status) => {
                                    approvals.status = Some(status);
                                    approvals.status_failed = false;
                                }
                                Err(_) => approvals.status_failed = true,
                            }
                            cx.notify();
                        });
                        if done.is_err() {
                            return;
                        }
                    }
                    cx.background_executor().timer(STATUS_EVERY).await;
                }
            }));
        }
        if self._tick.is_none() {
            self._tick = Some(cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor().timer(Duration::from_secs(1)).await;
                    let alive = this.update(cx, |approvals, cx| {
                        if let Some(waiting) = approvals.current().cloned()
                            && waiting.remaining_ms() == 0
                        {
                            approvals.toast_expired(&waiting.request.request_id, cx);
                        }
                        cx.notify();
                    });
                    if alive.is_err() {
                        return;
                    }
                }
            }));
        }
    }

    fn toast_expired(&mut self, request_id: &str, cx: &mut Context<Self>) {
        if !self.decided.contains(request_id) && self.expired.insert(request_id.to_string()) {
            Toaster::info(cx.t("settings.computerUse.approval.expired"), cx);
        }
    }

    /// The apps chosen for the current request, among those still open.
    pub fn chosen_apps(&self) -> Vec<String> {
        let (Some(waiting), Some(status)) = (self.current(), self.status.as_ref()) else {
            return Vec::new();
        };
        let apps = status.apps();
        self.chosen
            .get(&waiting.request.request_id)
            .map(|chosen| {
                chosen
                    .iter()
                    .filter(|id| apps.iter().any(|app| &app.bundle_id == *id))
                    .take(MAX_APPS)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn toggle_app(&mut self, bundle_id: &str, cx: &mut Context<Self>) {
        let Some(id) = self.current().map(|w| w.request.request_id.clone()) else {
            return;
        };
        let chosen = self.chosen.entry(id).or_default();
        if let Some(index) = chosen.iter().position(|app| app == bundle_id) {
            chosen.remove(index);
        } else {
            chosen.push(bundle_id.to_string());
        }
        cx.notify();
    }

    /// `canAllow`
    pub fn can_allow(&self) -> bool {
        let (Some(waiting), Some(status)) = (self.current(), self.status.as_ref()) else {
            return false;
        };
        status.supported
            && status.runtime_attached
            && status.permissions_ready()
            && !status.apps().is_empty()
            && !self.chosen_apps().is_empty()
            && waiting.remaining_ms() > 0
            && !self.status_failed
            && self.busy.is_none()
    }

    /// "Permitir" (with the apps chosen) or "Negar".
    pub fn decide(&mut self, allowed: bool, cx: &mut Context<Self>) {
        let Some(waiting) = self.current().cloned() else {
            return;
        };
        if self.busy.is_some() || (allowed && !self.can_allow()) {
            return;
        }
        let Some(api) = self.api.clone() else {
            return;
        };
        let id = waiting.request.request_id.clone();
        let apps = if allowed {
            self.chosen_apps()
        } else {
            Vec::new()
        };
        self.decided.insert(id.clone());
        self.busy = Some(id.clone());
        cx.notify();
        self._decide = Some(cx.spawn(async move |this, cx| {
            let answer = id.clone();
            let result = cx
                .background_executor()
                .spawn(async move { api.computer_respond(&answer, allowed, &apps) })
                .await;
            let _ = this.update(cx, |approvals, cx| {
                if let Err(error) = result {
                    if approvals.current().map(|w| &w.request.request_id) == Some(&id) {
                        approvals.decision_error = Some(error.to_string());
                    }
                    if approvals.busy.as_deref() == Some(id.as_str()) {
                        approvals.busy = None;
                    }
                }
                cx.notify();
            });
        }));
    }

    /// "Abrir ajustes de …": the desktop opens the system settings.
    pub fn open_access(&mut self, kind: AccessKind, cx: &mut Context<Self>) {
        if self.busy_access.is_some() {
            return;
        }
        let Some(api) = self.api.clone() else {
            return;
        };
        let request = self.current().map(|w| w.request.request_id.clone());
        self.busy_access = Some(kind);
        self.access_error = None;
        cx.notify();
        self._access = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api.computer_access(kind) })
                .await;
            let _ = this.update(cx, |approvals, cx| {
                approvals.busy_access = None;
                let same = approvals.current().map(|w| w.request.request_id.clone()) == request;
                match result {
                    Ok(status) if same => approvals.status = Some(status),
                    Ok(_) => {}
                    Err(error) if same => {
                        approvals.access_error = Some(error.to_string());
                    }
                    Err(_) => {}
                }
                cx.notify();
            });
        }));
    }

    #[cfg(test)]
    pub fn set_api(&mut self, api: DesktopApi) {
        self.api = Some(api);
    }
}

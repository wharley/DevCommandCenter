//! The agent's screen, floating at the conversation's bottom right
//! (`ComputerUsePreview`): the authorized window's latest frame, what the
//! agent is doing (a dot that pings while a step runs, red when it failed,
//! green when done), "Parar agente" while the turn runs and "Encerrar
//! acesso" after it, expand and hide
//! ("Mostrar tela do agente" brings it back). It follows the conversation
//! on screen: a new frame or the end of the grant arrive from the desktop
//! (`computer-preview-updated`, `-cleared`); while the turn runs the frame
//! is captured again every second; a step's label clears 4s after it ends,
//! and the frame goes when the grant runs out.

use std::sync::Arc;
use std::time::Duration;

use gpui::{Context, EventEmitter, Image, ImageFormat, Task};

use crate::data::computer_use::{Activity, ComputerEvent, PreviewFrame};
use crate::data::desktop_api::DesktopApi;

/// A step's label stays this long after it ends.
const ACTIVITY_LINGER: Duration = Duration::from_secs(4);
const LIVE_EVERY: Duration = Duration::from_secs(1);

pub enum PreviewEvent {
    /// "Parar agente"
    Stop,
}

/// A frame, decoded once.
pub struct Frame {
    pub app: String,
    pub width: u32,
    pub height: u32,
    pub image: Arc<Image>,
}

impl Frame {
    fn from(frame: PreviewFrame) -> Option<Self> {
        Some(Self {
            image: Arc::new(Image::from_bytes(ImageFormat::Png, frame.png()?)),
            app: frame.target.name,
            width: frame.target.width.max(0.0).round() as u32,
            height: frame.target.height.max(0.0).round() as u32,
        })
    }
}

pub struct ComputerPreview {
    pub(super) api: Option<DesktopApi>,
    pub session: Option<String>,
    pub turn_active: bool,
    pub frame: Option<Frame>,
    pub activity: Option<Activity>,
    pub hidden: bool,
    pub expanded: bool,
    _frame: Option<Task<()>>,
    _grant: Option<Task<()>>,
    _linger: Option<Task<()>>,
    _live: Option<Task<()>>,
}

impl EventEmitter<PreviewEvent> for ComputerPreview {}

impl ComputerPreview {
    pub fn new(api: Option<DesktopApi>) -> Self {
        Self {
            api,
            session: None,
            turn_active: false,
            frame: None,
            activity: None,
            hidden: false,
            expanded: false,
            _frame: None,
            _grant: None,
            _linger: None,
            _live: None,
        }
    }

    /// The conversation on screen, and whether its turn runs.
    pub fn follow(&mut self, session: Option<String>, turn_active: bool, cx: &mut Context<Self>) {
        if self.session != session {
            self.session = session;
            self.frame = None;
            self.activity = None;
            self.hidden = false;
            self.expanded = false;
            self._linger = None;
            self.turn_active = turn_active;
            self.read_frame(false, cx);
            self.watch_grant(cx);
            self.keep_live(cx);
            cx.notify();
        } else if self.turn_active != turn_active {
            self.turn_active = turn_active;
            self.keep_live(cx);
            cx.notify();
        }
    }

    /// What the desktop said about computer use.
    pub fn apply(&mut self, event: &ComputerEvent, cx: &mut Context<Self>) {
        let Some(session) = self.session.clone() else {
            return;
        };
        match event {
            ComputerEvent::Activity(activity) if activity.session_id == session => {
                self.activity = Some(activity.clone());
                self._linger = None;
                if activity.phase != "started" {
                    let id = activity.activity_id.clone();
                    self._linger = Some(cx.spawn(async move |this, cx| {
                        cx.background_executor().timer(ACTIVITY_LINGER).await;
                        let _ = this.update(cx, |preview, cx| {
                            if preview.activity.as_ref().map(|a| &a.activity_id) == Some(&id) {
                                preview.activity = None;
                                cx.notify();
                            }
                        });
                    }));
                }
                cx.notify();
            }
            ComputerEvent::PreviewUpdated(id) if *id == session => {
                self.read_frame(false, cx);
                self.watch_grant(cx);
            }
            ComputerEvent::PreviewCleared(id) if *id == session => {
                self.frame = None;
                self.activity = None;
                self.keep_live(cx);
                cx.notify();
            }
            _ => {}
        }
    }

    /// The latest frame (`refresh` captures one first).
    fn read_frame(&mut self, refresh: bool, cx: &mut Context<Self>) {
        let (Some(api), Some(session)) = (self.api.clone(), self.session.clone()) else {
            return;
        };
        self._frame = Some(cx.spawn(async move |this, cx| {
            let asked = session.clone();
            let frame = cx
                .background_executor()
                .spawn(async move { api.computer_preview(&asked, refresh) })
                .await;
            let _ = this.update(cx, |preview, cx| {
                if preview.session.as_deref() != Some(session.as_str()) {
                    return;
                }
                match frame {
                    Ok(Some(frame)) => preview.frame = Frame::from(frame),
                    // Nothing new captured: what the desktop has now.
                    Ok(None) if refresh => preview.read_frame(false, cx),
                    Ok(None) => preview.frame = None,
                    Err(_) => {}
                }
                preview.keep_live(cx);
                cx.notify();
            });
        }));
    }

    /// The frame goes when the grant does (`refreshGrantExpiry`).
    fn watch_grant(&mut self, cx: &mut Context<Self>) {
        let (Some(api), Some(session)) = (self.api.clone(), self.session.clone()) else {
            return;
        };
        self._grant = Some(cx.spawn(async move |this, cx| {
            let asked = session.clone();
            let reader = api.clone();
            let Ok(grant) = cx
                .background_executor()
                .spawn(async move { reader.computer_grant(&asked) })
                .await
            else {
                return;
            };
            if !grant.armed {
                let _ = this.update(cx, |preview, cx| {
                    preview.frame = None;
                    preview.keep_live(cx);
                    cx.notify();
                });
                return;
            }
            cx.background_executor()
                .timer(Duration::from_millis(grant.remaining_ms))
                .await;
            let latest = cx
                .background_executor()
                .spawn(async move { api.computer_preview(&session, false) })
                .await;
            if let Ok(None) = latest {
                let _ = this.update(cx, |preview, cx| {
                    preview.frame = None;
                    preview.keep_live(cx);
                    cx.notify();
                });
            }
        }));
    }

    /// A new capture every second while the turn runs and the frame shows.
    fn keep_live(&mut self, cx: &mut Context<Self>) {
        let live = self.frame.is_some() && self.turn_active && !self.hidden;
        if !live {
            self._live = None;
            return;
        }
        if self._live.is_some() {
            return;
        }
        self._live = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(LIVE_EVERY).await;
                let going = this.update(cx, |preview, cx| {
                    preview.read_frame(true, cx);
                });
                if going.is_err() {
                    return;
                }
            }
        }));
    }

    pub fn set_hidden(&mut self, hidden: bool, cx: &mut Context<Self>) {
        self.hidden = hidden;
        self.keep_live(cx);
        cx.notify();
    }

    pub fn toggle_expanded(&mut self, cx: &mut Context<Self>) {
        self.expanded = !self.expanded;
        cx.notify();
    }

    pub fn stop(&mut self, cx: &mut Context<Self>) {
        cx.emit(PreviewEvent::Stop);
    }

    /// "Encerrar acesso" once the turn is over: the agent's grant ends now,
    /// and with it this frame and the agent's cursor.
    pub fn end_access(&mut self, cx: &mut Context<Self>) {
        let (Some(api), Some(session)) = (self.api.clone(), self.session.clone()) else {
            return;
        };
        self._grant = Some(cx.spawn(async move |this, cx| {
            let ended = cx
                .background_executor()
                .spawn(async move { api.computer_disarm(&session) })
                .await;
            if ended.is_ok() {
                let _ = this.update(cx, |preview, cx| {
                    preview.frame = None;
                    preview.activity = None;
                    preview.keep_live(cx);
                    cx.notify();
                });
            }
        }));
    }

    #[cfg(test)]
    pub fn set_api(&mut self, api: DesktopApi) {
        self.api = Some(api);
    }
}

/// `activityLabelKey`
pub fn activity_label_key(tool: &str) -> &'static str {
    match tool {
        "dcc_computer_status" => "status",
        "dcc_computer_request_control" => "requesting",
        "dcc_computer_capture" => "observing",
        "dcc_computer_click" => "clicking",
        "dcc_computer_scroll" => "scrolling",
        "dcc_computer_drag" => "dragging",
        "dcc_computer_set_value" => "settingValue",
        "dcc_computer_type" => "typing",
        "dcc_computer_key" => "key",
        _ => "working",
    }
}

//! "Abrir uma página no navegador?" (`BrowserApprovalHost`): an agent's
//! `dcc_browser_open` waiting for the person. While this app listens to
//! the desktop it answers them — one card at a time, soonest to expire
//! first, with the exact destination, the conversation, the provider, the
//! reason and the time left; only its buttons answer (Esc or a click
//! outside must not deny by accident). A preapproved request for the task
//! on screen opens without a card. Allowed here, by a phone or the
//! desktop's panel, the shell opens the task's Browser on it, and this
//! confirms it with the Browser's lifecycle token. Away from the app, the
//! Dock icon bounces for a new card.

use std::collections::HashSet;
use std::rc::Rc;
use std::time::Duration;

use futures::StreamExt;
use gpui::{
    App, Context, EventEmitter, FontWeight, IntoElement, Render, Task, Window, div, prelude::*, px,
};

use crate::data::browser_opens::{OpenRequest, OpensEvent};
use crate::data::desktop_api::DesktopApi;
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::ui::dialog::{Dialog, dialog_footer};
use crate::ui::{Button, ButtonVariant, Icon, Toaster};

/// After the stream ends (the desktop restarted), try again this soon.
const RECONNECT: Duration = Duration::from_secs(2);

/// The conversation's title for a request, from what the shell has.
pub type TitleOf = Rc<dyn Fn(&OpenRequest, &App) -> Option<String>>;

pub enum ApprovalEvent {
    /// Allowed (here or elsewhere): open the task's Browser on it.
    Open(OpenRequest),
}

pub struct BrowserApprovals {
    api: Option<DesktopApi>,
    pub requests: Vec<OpenRequest>,
    /// The conversation on the person's screen.
    visible_session: Option<String>,
    pub busy: Option<String>,
    pub error: Option<String>,
    opened: HashSet<String>,
    announced: HashSet<String>,
    title_of: TitleOf,
    _stream: Option<Task<()>>,
    _tick: Option<Task<()>>,
    _decide: Option<Task<()>>,
}

impl EventEmitter<ApprovalEvent> for BrowserApprovals {}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or_default()
}

/// `remaining`: whole seconds, rounded up.
fn remaining(ms: u64) -> String {
    format!("{}s", ms.div_ceil(1000))
}

impl BrowserApprovals {
    /// Listens to the desktop (`api`), or stays empty without it.
    pub fn new(api: Option<DesktopApi>, title_of: TitleOf, cx: &mut Context<Self>) -> Self {
        let mut approvals = Self {
            api,
            requests: Vec::new(),
            visible_session: None,
            busy: None,
            error: None,
            opened: HashSet::new(),
            announced: HashSet::new(),
            title_of,
            _stream: None,
            _tick: None,
            _decide: None,
        };
        approvals.listen(cx);
        approvals
    }

    fn listen(&mut self, cx: &mut Context<Self>) {
        let Some(api) = self.api.clone().filter(|_| !cfg!(test)) else {
            return;
        };
        let (sender, mut events) = futures::channel::mpsc::unbounded::<OpensEvent>();
        std::thread::spawn(move || {
            loop {
                if let Ok(stream) = api.browser_opens() {
                    for event in stream {
                        if sender.unbounded_send(event).is_err() {
                            return;
                        }
                    }
                }
                // Nothing is waiting while the desktop is away.
                if sender
                    .unbounded_send(OpensEvent::Waiting(Vec::new()))
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

    /// What the stream said.
    pub fn apply(&mut self, event: OpensEvent, cx: &mut Context<Self>) {
        match event {
            OpensEvent::Waiting(requests) => {
                let first = requests.first().map(|request| request.request_id.clone());
                if self.current().map(|request| &request.request_id) != first.as_ref() {
                    self.error = None;
                }
                if self.busy.is_some() && self.busy != first {
                    self.busy = None;
                }
                self.requests = requests;
                self.follow(cx);
            }
            // Claimed by a phone or the desktop's panel; open it once.
            OpensEvent::Approved(request) => {
                if self.opened.insert(request.request_id.clone()) {
                    cx.emit(ApprovalEvent::Open(request));
                }
            }
        }
        cx.notify();
    }

    /// The conversation on screen changed.
    pub fn set_visible_session(&mut self, session: Option<String>, cx: &mut Context<Self>) {
        if self.visible_session != session {
            self.visible_session = session;
            self.follow(cx);
            cx.notify();
        }
    }

    pub fn current(&self) -> Option<&OpenRequest> {
        self.requests.first()
    }

    /// `opensItself`: Full access, or a task already allowed, on screen.
    fn opens_itself(&self) -> bool {
        self.current().is_some_and(|request| {
            request.preapproved && Some(&request.session_id) == self.visible_session.as_ref()
        })
    }

    /// The card's clock, the preapproved open and the Dock's bounce.
    fn follow(&mut self, cx: &mut Context<Self>) {
        let Some(request) = self.current().cloned() else {
            self._tick = None;
            return;
        };
        if self.opens_itself() {
            self.decide(true, cx);
            return;
        }
        if self.announced.insert(request.request_id.clone()) {
            crate::ui::attention::request_attention();
        }
        if self._tick.is_none() {
            self._tick = Some(cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(500))
                        .await;
                    if this.update(cx, |_, cx| cx.notify()).is_err() {
                        return;
                    }
                }
            }));
        }
    }

    /// "Abrir e permitir" or "Negar" on the current card.
    pub fn decide(&mut self, allow: bool, cx: &mut Context<Self>) {
        let Some(request) = self.current().cloned() else {
            return;
        };
        if self.busy.is_some() || request.expires_at_ms <= now_ms() {
            return;
        }
        let Some(api) = self.api.clone() else {
            return;
        };
        self.busy = Some(request.request_id.clone());
        self.error = None;
        cx.notify();
        self._decide = Some(cx.spawn(async move |this, cx| {
            let answer = request.clone();
            let result = cx
                .background_executor()
                .spawn(async move {
                    if allow {
                        api.browser_open_allow(&answer)
                    } else {
                        api.browser_open_deny(&answer)
                    }
                })
                .await;
            let _ = this.update(cx, |approvals, cx| {
                match result {
                    // Gone from the list once the desktop sees it answered.
                    Ok(()) if allow => {
                        approvals.opened.insert(request.request_id.clone());
                        cx.emit(ApprovalEvent::Open(request));
                    }
                    Ok(()) => {}
                    Err(error) => {
                        approvals.busy = None;
                        if approvals.current().map(|r| &r.request_id) == Some(&request.request_id) {
                            approvals.error = Some(error.to_string());
                        }
                    }
                }
                cx.notify();
            });
        }));
    }

    /// `acknowledgeBrowserOpen`: the Browser opened `request` on `token`.
    pub fn confirm(&mut self, request: OpenRequest, token: u64, cx: &mut Context<Self>) {
        let Some(api) = self.api.clone() else {
            return;
        };
        cx.spawn(async move |_, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api.browser_open_ack(&request, token) })
                .await;
            if let Err(error) = result {
                cx.update(|cx| Toaster::error(error.to_string(), cx));
            }
        })
        .detach();
    }

    /// A test's desktop.
    #[cfg(test)]
    pub fn set_api(&mut self, api: DesktopApi) {
        self.api = Some(api);
    }
}

impl BrowserApprovals {
    /// `rounded-md border border-border/70 p-3`, the label above.
    fn field(
        label: gpui::SharedString,
        value: impl IntoElement,
        tinted: bool,
        cx: &App,
    ) -> gpui::AnyElement {
        let colors = cx.theme().colors.clone();
        div()
            .when(tinted, |field| field.bg(colors.muted.opacity(0.3)))
            .rounded(px(6.0))
            .border_1()
            .border_color(colors.border.opacity(0.7))
            .p(px(12.0))
            .child(
                div()
                    .mb(px(4.0))
                    .text_size(px(12.0))
                    .line_height(px(16.0))
                    .text_color(colors.muted_foreground)
                    .child(label),
            )
            .child(value)
            .into_any_element()
    }
}

impl Render for BrowserApprovals {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(request) = self.current().cloned() else {
            return div().into_any_element();
        };
        let live = request.expires_at_ms.saturating_sub(now_ms());
        if self.busy.as_ref() == Some(&request.request_id) || self.opens_itself() || live == 0 {
            return div().into_any_element();
        }
        let colors = cx.theme().colors.clone();
        let busy = self.busy.is_some();
        let conversation = (self.title_of)(&request, cx).unwrap_or(request.session_id.clone());
        let muted = colors.muted_foreground;
        let header = div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .text_size(px(16.0))
                    .line_height(px(16.0))
                    .font_weight(FontWeight::MEDIUM)
                    .child(Icon::new("shield-alert").size(16.0))
                    .child(cx.t("browser.approval.title")),
            )
            .child(
                div()
                    .text_color(muted)
                    .child(cx.t("browser.approval.description")),
            );
        let destination = Self::field(
            cx.t("browser.approval.destination"),
            div()
                .font_weight(FontWeight::MEDIUM)
                .child(request.url.clone()),
            true,
            cx,
        );
        let details = div()
            .flex()
            .gap(px(12.0))
            .text_size(px(12.0))
            .line_height(px(16.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .text_color(muted)
                    .child(cx.t("browser.approval.conversation"))
                    .child(cx.t("browser.approval.provider")),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .min_w_0()
                    .child(div().font_weight(FontWeight::MEDIUM).child(conversation))
                    .child(request.provider_id.clone()),
            );
        let reason = Self::field(
            cx.t("browser.approval.reason"),
            div().child(request.reason.clone()),
            false,
            cx,
        );
        let clock = div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .text_size(px(12.0))
            .line_height(px(16.0))
            .text_color(muted)
            .child(Icon::new("globe-2").size(14.0))
            .child(Icon::new("timer").size(14.0))
            .child(cx.t_with(
                "browser.approval.remaining",
                &Args::default().with("time", remaining(live)),
            ));
        let deny = Button::new("browser-approval-deny")
            .variant(ButtonVariant::Outline)
            .disabled(busy)
            .on_click(cx.listener(|approvals, _, _, cx| approvals.decide(false, cx)))
            .child(cx.t("browser.approval.deny"));
        let allow = Button::new("browser-approval-allow")
            .disabled(busy)
            .on_click(cx.listener(|approvals, _, _, cx| approvals.decide(true, cx)))
            .child(cx.t("browser.approval.allow"));
        Dialog::new("browser-approval", |_, _| {})
            .max_width(520.0)
            .close_button(false)
            .child(header)
            .child(
                div()
                    .debug_selector(|| "browser-approval".into())
                    .flex()
                    .flex_col()
                    .gap(px(16.0))
                    .child(destination)
                    .child(details)
                    .child(reason)
                    .child(clock)
                    .child(
                        div()
                            .text_size(px(12.0))
                            .line_height(px(16.0))
                            .text_color(muted)
                            .child(cx.t("browser.approval.taskWide")),
                    )
                    .children(self.error.clone().map(|error| {
                        div()
                            .text_size(px(12.0))
                            .line_height(px(16.0))
                            .text_color(colors.destructive)
                            .child(error)
                    })),
            )
            .child(dialog_footer(
                [deny.into_any_element(), allow.into_any_element()],
                cx,
            ))
            .into_any_element()
    }
}

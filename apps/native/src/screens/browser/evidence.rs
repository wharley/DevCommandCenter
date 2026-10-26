//! "Capturar evidência da página, console e rede" (the agent's menu, while
//! its control is on): the desktop reads the page and starts a capture
//! anchored on it; for its time the strip counts down with "Coletar" and
//! "Descartar". Collecting reads the console and network events once and
//! sends them with the page's text to the composer as `<browser_evidence>`;
//! discarding only forgets the handle (the page's wrapper unwinds itself).
//! The capture ends with the grant, the page it started on, or its time.

use std::time::{Duration, Instant};

use dcc_core::domain::browser_evidence::{EvidenceCapture, EvidenceLabels, format_evidence};
use gpui::{AnyElement, App, Context, div, prelude::*, px};

use super::surface::{BrowserEvent, BrowserSurface};
use crate::data::browser_bridge::EvidenceStart;
use crate::i18n::{Args, Translate};
use crate::theme::tailwind as tw;
use crate::theme::{ActiveTheme, Appearance};
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon};

/// A capture started on the desktop, until it is collected.
pub struct Capture {
    pub capture_id: String,
    /// The desktop's lifecycle token it was started on.
    pub token: u64,
    pub page: EvidenceStart,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub started: Instant,
    pub expires: Instant,
}

impl Capture {
    pub fn seconds_left(&self) -> u64 {
        self.expires
            .saturating_duration_since(Instant::now())
            .as_millis()
            .div_ceil(1000) as u64
    }
}

/// `browser.evidence.*` and `browser.agentContext.*`, for the block.
fn labels(cx: &App) -> EvidenceLabels {
    let t = |key: &str| cx.t(key).to_string();
    EvidenceLabels {
        no_events: t("browser.evidence.noEvents"),
        no_page_text: t("browser.evidence.noPageText"),
        page_snapshot: t("browser.evidence.pageSnapshot"),
        events_after_start: t("browser.evidence.eventsAfterStart"),
        selection: t("browser.agentContext.selection"),
        visible_text: t("browser.agentContext.visibleText"),
        yes: t("browser.agentContext.yes"),
        no: t("browser.agentContext.no"),
    }
}

impl BrowserSurface {
    fn token(&self) -> Option<u64> {
        self.bridge.as_ref().and_then(|bridge| bridge.token)
    }

    /// The menu item is on while the agent may act and nothing runs.
    pub fn can_start_evidence(&self) -> bool {
        self.request.session_id.is_some()
            && self.token().is_some()
            && self.control.armed
            && !self.loading
            && !self.evidence_busy
            && self.evidence.is_none()
    }

    /// `handleStartEvidence`
    pub fn start_evidence(&mut self, cx: &mut Context<Self>) {
        let (Some(api), Some(token)) = (self.bridge_api.clone(), self.token()) else {
            return;
        };
        if !self.can_start_evidence() {
            return;
        }
        self.error = None;
        self.evidence_busy = true;
        cx.notify();
        let request = self.request.clone();
        self._evidence = Some(cx.spawn(async move |this, cx| {
            let started = cx
                .background_executor()
                .spawn(async move {
                    api.browser_evidence_start(
                        &request.workspace_id,
                        request.session_id.as_deref(),
                        token,
                    )
                })
                .await;
            let _ = this.update(cx, |surface, cx| {
                surface.evidence_busy = false;
                match started {
                    Ok(page) if surface.token() == Some(token) => {
                        let now = Instant::now();
                        surface.evidence = Some(Capture {
                            capture_id: page.capture_id.clone(),
                            token,
                            expires: now + Duration::from_millis(page.remaining_ms),
                            started: now,
                            started_at: chrono::Utc::now(),
                            page,
                        });
                        surface.tick_evidence(cx);
                    }
                    Ok(_) => {}
                    Err(error) => surface.error = Some(error.to_string()),
                }
                cx.notify();
            });
        }));
    }

    /// The countdown; at its end the capture is gone.
    fn tick_evidence(&mut self, cx: &mut Context<Self>) {
        self._evidence_tick = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let going = this.update(cx, |surface, cx| {
                    let left = surface.evidence.as_ref().map(Capture::seconds_left);
                    if left == Some(0) {
                        surface.expire_evidence(cx);
                    }
                    cx.notify();
                    left.is_some_and(|left| left > 0)
                });
                if !matches!(going, Ok(true)) {
                    return;
                }
            }
        }));
    }

    /// The capture can no longer be read: `browser.evidence.expired`.
    pub(super) fn expire_evidence(&mut self, cx: &mut Context<Self>) {
        if self.evidence.take().is_some() {
            self._evidence_tick = None;
            self.error = Some(cx.t("browser.evidence.expired").to_string());
            cx.notify();
        }
    }

    /// `handleCollectEvidence`: one read, then the handle is spent.
    pub fn collect_evidence(&mut self, cx: &mut Context<Self>) {
        if self.evidence_busy {
            return;
        }
        let Some(capture) = self.evidence.as_ref() else {
            return;
        };
        let Some(api) = self.bridge_api.clone() else {
            return;
        };
        if self.token() != Some(capture.token) {
            self.discard_evidence(cx);
            return;
        }
        self.error = None;
        self.evidence_busy = true;
        cx.notify();
        let request = self.request.clone();
        let capture_id = capture.capture_id.clone();
        self._evidence = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    api.browser_evidence_read(
                        &request.workspace_id,
                        request.session_id.as_deref(),
                        &capture_id,
                    )
                })
                .await;
            let _ = this.update(cx, |surface, cx| {
                let capture = surface.evidence.take();
                surface._evidence_tick = None;
                surface.evidence_busy = false;
                match (result, capture) {
                    (Ok(result), Some(capture)) => {
                        let (text, _) = format_evidence(
                            &EvidenceCapture {
                                url: capture.page.url,
                                title: capture.page.title,
                                started_at: capture
                                    .started_at
                                    .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
                                window_ms: capture.started.elapsed().as_millis() as u64,
                                text: capture.page.text,
                                selection_only: capture.page.selection_only,
                                page_truncated: capture.page.truncated,
                                result,
                            },
                            &labels(cx),
                        );
                        cx.emit(BrowserEvent::SendEvidence(text));
                    }
                    (Ok(_), None) => {}
                    (Err(error), _) => surface.error = Some(error.to_string()),
                }
                cx.notify();
            });
        }));
    }

    /// `handleDiscardEvidence`
    pub fn discard_evidence(&mut self, cx: &mut Context<Self>) {
        self.evidence = None;
        self._evidence_tick = None;
        cx.notify();
    }

    /// The strip while a capture runs: cyan, the seconds left, "Coletar"
    /// and "Descartar".
    pub(super) fn evidence_strip(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let capture = self.evidence.as_ref()?;
        let colors = cx.theme().colors.clone();
        let busy = self.evidence_busy;
        let cyan = match cx.theme().appearance {
            Appearance::Dark => tw::CYAN_400,
            Appearance::Light => tw::CYAN_600,
        };
        Some(
            div()
                .flex()
                .flex_1()
                .min_w_0()
                .items_center()
                .gap(px(8.0))
                .text_color(cyan.hsla())
                .child(
                    div()
                        .flex_shrink_0()
                        .child(Icon::new("activity").size(12.0)),
                )
                .child(div().min_w_0().truncate().child(cx.t_with(
                    "browser.evidence.active",
                    &Args::default().with("seconds", capture.seconds_left().to_string()),
                )))
                .child(
                    div().ml_auto().flex_shrink_0().child(
                        Button::new("browser-evidence-collect")
                            .variant(ButtonVariant::Outline)
                            .size(ButtonSize::Xs)
                            .height(20.0)
                            .padding_x(8.0)
                            .disabled(busy)
                            .on_click(cx.listener(|surface, _, _, cx| surface.collect_evidence(cx)))
                            .child(cx.t("browser.evidence.collect")),
                    ),
                )
                .child(
                    Button::new("browser-evidence-discard")
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Xs)
                        .height(20.0)
                        .padding_x(8.0)
                        .text_color(colors.muted_foreground)
                        .disabled(busy)
                        .on_click(cx.listener(|surface, _, _, cx| surface.discard_evidence(cx)))
                        .child(cx.t("browser.evidence.discard")),
                )
                .into_any_element(),
        )
    }
}

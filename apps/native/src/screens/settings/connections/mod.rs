//! "Conexões" (`PairedDevicesPanel`): the phones paired through the relay
//! (refresh, revoke), the relay's card (its address, "Parear pelo relay"
//! with the QR, and "Aprovar …?" for a phone that scanned it), and the
//! pairing audit log. While the section shows, the relay's status and the
//! phones waiting are read every 2s (the desktop hears them as events).
//! The desktop writes this screen's text in Portuguese, so does this.

mod relay;
mod view;

use std::time::{Duration, Instant};

use gpui::{AppContext, Context, Entity, Subscription, Task, Window};

use super::{SectionId, Settings};
use crate::data::desktop_api::{ApiError, DesktopApi};
use crate::data::remote_access::{Approval, AuditEntry, Device, Offer, RelayStatus};
use crate::ui::Toaster;
use crate::ui::text_input::{TextInput, TextInputEvent};

const POLL: Duration = Duration::from_secs(2);

/// "Parear pelo relay" while it is open.
#[derive(Default)]
pub(super) struct Pairing {
    /// The offer and when it expires on this clock.
    pub(super) offer: Option<(Offer, Instant)>,
    pub(super) loading: bool,
}

pub(super) struct Connections {
    pub(super) devices: Vec<Device>,
    pub(super) loading: bool,
    pub(super) revoking: Option<String>,
    /// `None` until read, or when the desktop has no relay (the card hides).
    pub(super) relay: Option<RelayStatus>,
    pub(super) relay_input: Entity<TextInput>,
    pub(super) saving: bool,
    pub(super) pairing: Option<Pairing>,
    /// Phones that scanned the QR, first one asked.
    pub(super) approvals: Vec<Approval>,
    pub(super) answering: bool,
    /// The log, when open: its entries and whether it is reading.
    pub(super) audit: Option<(Vec<AuditEntry>, bool)>,
    started: bool,
    poll: Option<Task<()>>,
    _input: Subscription,
}

fn message(error: ApiError, fallback: &str) -> String {
    match error {
        ApiError::Failed(message) if !message.trim().is_empty() => message,
        ApiError::DesktopClosed => "o desktop não está aberto".into(),
        _ => fallback.into(),
    }
}

/// When an offer that expires at `expires_at` (RFC 3339) ends here.
fn deadline(expires_at: &str) -> Instant {
    let left = chrono::DateTime::parse_from_rfc3339(expires_at)
        .map(|at| at.with_timezone(&chrono::Utc) - chrono::Utc::now())
        .ok()
        .and_then(|left| left.to_std().ok())
        .unwrap_or_default();
    Instant::now() + left
}

impl Connections {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Settings>) -> Self {
        let relay_input =
            cx.new(|cx| TextInput::new(window, cx).with_placeholder("wss://relay.exemplo.dev"));
        let input = cx.subscribe(&relay_input, |settings, _, event, cx| match event {
            TextInputEvent::Changed => cx.notify(),
            TextInputEvent::Submitted => settings.save_relay(cx),
            _ => {}
        });
        Self {
            devices: Vec::new(),
            loading: false,
            revoking: None,
            relay: None,
            relay_input,
            saving: false,
            pairing: None,
            approvals: Vec::new(),
            answering: false,
            audit: None,
            started: false,
            poll: None,
            _input: input,
        }
    }

    #[cfg(test)]
    pub(super) fn reset(&mut self) {
        self.started = false;
        self.poll = None;
        self.devices.clear();
        self.relay = None;
        self.approvals.clear();
    }
}

impl Settings {
    fn connections_api(&self) -> Option<DesktopApi> {
        self.general.api()
    }

    /// Read when the section opens; the relay is watched while it shows.
    pub(super) fn ensure_connections(&mut self, cx: &mut Context<Self>) {
        if !self.connections.started {
            self.connections.started = true;
            self.load_devices(cx);
            self.load_relay(true, cx);
            self.load_approvals(cx);
        }
        if self.connections.poll.is_some() {
            return;
        }
        self.connections.poll = Some(cx.spawn(async move |this, cx| {
            let mut last = Instant::now();
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let Ok(()) = this.update(cx, |settings, cx| {
                    if settings.active != SectionId::Connections {
                        return;
                    }
                    if last.elapsed() >= POLL {
                        last = Instant::now();
                        settings.load_relay(false, cx);
                        settings.load_approvals(cx);
                    }
                    // The QR's countdown.
                    if settings.connections.pairing.is_some() {
                        cx.notify();
                    }
                }) else {
                    return;
                };
            }
        }));
    }

    pub(super) fn load_devices(&mut self, cx: &mut Context<Self>) {
        let Some(api) = self.connections_api() else {
            return;
        };
        self.connections.loading = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let read = cx
                .background_executor()
                .spawn(async move { api.paired_devices() })
                .await;
            let _ = this.update(cx, |settings, cx| {
                settings.connections.loading = false;
                match read {
                    Ok(devices) => settings.connections.devices = devices,
                    Err(error) => {
                        Toaster::error(message(error, "Falha ao carregar dispositivos"), cx)
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The relay's status; `fill` puts its address in the field (on open
    /// and after saving, never while someone types).
    fn load_relay(&mut self, fill: bool, cx: &mut Context<Self>) {
        let Some(api) = self.connections_api() else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let read = cx
                .background_executor()
                .spawn(async move { api.relay_status() })
                .await;
            let _ = this.update(cx, |settings, cx| {
                let status = read.ok();
                if let (true, Some(status)) = (fill, &status) {
                    let url = status.relay_url.clone().unwrap_or_default();
                    settings
                        .connections
                        .relay_input
                        .update(cx, |input, cx| input.set_text(url, cx));
                }
                settings.connections.relay = status;
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn load_approvals(&mut self, cx: &mut Context<Self>) {
        let Some(api) = self.connections_api() else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let read = cx
                .background_executor()
                .spawn(async move { api.pairing_approvals() })
                .await;
            let _ = this.update(cx, |settings, cx| {
                if let Ok(approvals) = read
                    && !settings.connections.answering
                {
                    settings.connections.approvals = approvals;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(super) fn revoke_device(&mut self, device: Device, cx: &mut Context<Self>) {
        let Some(api) = self.connections_api() else {
            return;
        };
        self.connections.revoking = Some(device.device_id.clone());
        cx.notify();
        cx.spawn(async move |this, cx| {
            let id = device.device_id.clone();
            let done = cx
                .background_executor()
                .spawn(async move { api.revoke_device(&id) })
                .await;
            let _ = this.update(cx, |settings, cx| {
                settings.connections.revoking = None;
                match done {
                    Ok(_) => Toaster::success(format!("{} desvinculado", device.device_name), cx),
                    Err(error) => Toaster::error(message(error, "Falha ao revogar"), cx),
                }
                settings.load_devices(cx);
            });
        })
        .detach();
    }

    pub(super) fn save_relay(&mut self, cx: &mut Context<Self>) {
        let Some(api) = self.connections_api() else {
            return;
        };
        let draft = self
            .connections
            .relay_input
            .read(cx)
            .text()
            .trim()
            .to_string();
        let unchanged = self
            .connections
            .relay
            .as_ref()
            .is_none_or(|status| draft == status.relay_url.clone().unwrap_or_default());
        if self.connections.saving || unchanged {
            return;
        }
        self.connections.saving = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let saved = cx
                .background_executor()
                .spawn(async move { api.set_relay((!draft.is_empty()).then_some(draft.as_str())) })
                .await;
            let _ = this.update(cx, |settings, cx| {
                settings.connections.saving = false;
                match saved {
                    Ok(status) => {
                        Toaster::success(
                            if status.relay_url.is_some() {
                                "Relay salvo"
                            } else {
                                "Acesso pelo relay desligado"
                            },
                            cx,
                        );
                        settings.connections.relay = Some(status);
                    }
                    Err(error) => Toaster::error(message(error, "Falha ao salvar o relay"), cx),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// "Parear pelo relay": the dialog and a fresh QR.
    pub(super) fn open_pairing(&mut self, cx: &mut Context<Self>) {
        self.connections.pairing = Some(Pairing::default());
        self.generate_offer(cx);
    }

    pub(super) fn close_pairing(&mut self, cx: &mut Context<Self>) {
        self.connections.pairing = None;
        cx.notify();
    }

    pub(super) fn generate_offer(&mut self, cx: &mut Context<Self>) {
        let (Some(api), Some(pairing)) =
            (self.connections_api(), self.connections.pairing.as_mut())
        else {
            return;
        };
        pairing.loading = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let offer = cx
                .background_executor()
                .spawn(async move { api.pairing_offer() })
                .await;
            let _ = this.update(cx, |settings, cx| {
                let Some(pairing) = settings.connections.pairing.as_mut() else {
                    return;
                };
                pairing.loading = false;
                match offer {
                    Ok(offer) => {
                        let ends = deadline(&offer.expires_at);
                        pairing.offer = Some((offer, ends));
                    }
                    Err(error) => Toaster::error(message(error, "Falha ao gerar o QR"), cx),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// "Aprovar" or "Recusar" the first phone waiting.
    pub(super) fn answer_pairing(&mut self, approve: bool, cx: &mut Context<Self>) {
        let (Some(api), Some(current)) = (
            self.connections_api(),
            self.connections.approvals.first().cloned(),
        ) else {
            return;
        };
        if self.connections.answering {
            return;
        }
        self.connections.answering = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let id = current.id.clone();
            let delivered = cx
                .background_executor()
                .spawn(async move { api.answer_pairing(&id, approve) })
                .await;
            let _ = this.update(cx, |settings, cx| {
                match delivered {
                    Ok(false) => Toaster::warning("O celular desistiu ou o pedido expirou", cx),
                    Ok(true) if approve => {
                        Toaster::success(format!("{} pareado", current.device_name), cx);
                        settings.load_devices(cx);
                    }
                    Ok(true) => {}
                    Err(error) => Toaster::error(message(error, "Falha ao responder"), cx),
                }
                let connections = &mut settings.connections;
                connections.approvals.retain(|item| item.id != current.id);
                connections.answering = false;
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn toggle_audit(&mut self, cx: &mut Context<Self>) {
        if self.connections.audit.take().is_none() {
            self.connections.audit = Some((Vec::new(), false));
            self.load_audit(cx);
        }
        cx.notify();
    }

    pub(super) fn load_audit(&mut self, cx: &mut Context<Self>) {
        let (Some(api), Some(audit)) = (self.connections_api(), self.connections.audit.as_mut())
        else {
            return;
        };
        audit.1 = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let read = cx
                .background_executor()
                .spawn(async move { api.pairing_audit(50) })
                .await;
            let _ = this.update(cx, |settings, cx| {
                let Some(audit) = settings.connections.audit.as_mut() else {
                    return;
                };
                audit.1 = false;
                match read {
                    Ok(entries) => audit.0 = entries,
                    Err(error) => Toaster::error(message(error, "Falha ao carregar log"), cx),
                }
                cx.notify();
            });
        })
        .detach();
    }
}

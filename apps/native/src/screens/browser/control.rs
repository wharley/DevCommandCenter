//! The agent's pill in the Browser's bar (only in a conversation): the
//! shield, "Agente" or "Ativo · Ns" counting down (emerald while armed), and
//! its menu's toggle — "Permitir controle do agente por 60 segundos", or
//! turn it off. The grant lives in the desktop (`browser_arm_control`);
//! here only its remaining time is shown, read again when it runs out.
//! Below it, evidence capture (with the control on) and, after a separator,
//! the Browser's activity, whose card opens under the pill.

use std::time::{Duration, Instant};

use gpui::{AnyElement, Context, div, prelude::*};

use super::surface::BrowserSurface;
use crate::data::browser_bridge::ControlStatus;
use crate::i18n::{Args, Translate};
use crate::theme::tailwind as tw;
use crate::ui::{
    Button, ButtonSize, ButtonVariant, Icon, Menu, MenuAlign, MenuEntry, MenuItem, spinner,
    tabular_nums,
};

const TICK: Duration = Duration::from_secs(1);

impl BrowserSurface {
    /// Seconds left of the grant, as the pill counts them.
    pub fn control_seconds(&self) -> u64 {
        self.control_until.map_or(0, |until| {
            until
                .saturating_duration_since(Instant::now())
                .as_millis()
                .div_ceil(1000) as u64
        })
    }

    pub(super) fn set_control(&mut self, status: ControlStatus, cx: &mut Context<Self>) {
        self.control = status;
        self.control_until = status
            .armed
            .then(|| Instant::now() + Duration::from_millis(status.remaining_ms));
        self.control_busy = false;
        if !status.armed {
            // Reading a capture needs the grant that started it.
            self.expire_evidence(cx);
        }
        if status.armed {
            self.tick_control(cx);
        } else {
            self._control = None;
        }
        cx.notify();
    }

    /// The countdown, and the desktop's word once it runs out.
    fn tick_control(&mut self, cx: &mut Context<Self>) {
        self._control = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(TICK).await;
                let expired = this.update(cx, |surface, cx| {
                    cx.notify();
                    surface.control_seconds() == 0
                });
                match expired {
                    Ok(true) => {
                        let _ = this.update(cx, |surface, cx| surface.ask_control(None, cx));
                        return;
                    }
                    Ok(false) => {}
                    Err(_) => return,
                }
            }
        }));
    }

    /// `armBrowserControl` / `disarmBrowserControl` (`None` reads it).
    pub fn ask_control(&mut self, armed: Option<bool>, cx: &mut Context<Self>) {
        let (Some(api), Some(token)) = (
            self.bridge_api.clone(),
            self.bridge.as_ref().and_then(|bridge| bridge.token),
        ) else {
            return;
        };
        if armed.is_some() {
            self.control_busy = true;
            cx.notify();
        }
        let request = self.request.clone();
        self._control = Some(cx.spawn(async move |this, cx| {
            let status = cx
                .background_executor()
                .spawn(async move {
                    api.browser_control(
                        &request.workspace_id,
                        request.session_id.as_deref(),
                        token,
                        armed,
                    )
                })
                .await;
            let _ = this.update(cx, |surface, cx| match status {
                Ok(status) => surface.set_control(status, cx),
                Err(error) => {
                    surface.control_busy = false;
                    surface.error = Some(error.to_string());
                    surface.set_control(ControlStatus::default(), cx);
                }
            });
        }));
    }

    pub(super) fn agent_pill(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.request.session_id.as_ref()?;
        let armed = self.control.armed;
        let busy = self.control_busy;
        let ready = self
            .bridge
            .as_ref()
            .and_then(|bridge| bridge.token)
            .is_some();
        let label = if armed {
            cx.t_with(
                "browser.controlCountdown",
                &Args::default().with("seconds", self.control_seconds().to_string()),
            )
        } else {
            cx.t("browser.agentLabel")
        };
        let toggle = if armed {
            cx.t("browser.disarmControl")
        } else {
            cx.t("browser.armControl")
        };
        let loading = self.loading;
        let evidence_on = self.can_start_evidence();
        let evidence = cx.t("browser.evidence.start");
        let requires = (!armed).then(|| cx.t("browser.evidence.requiresControl"));
        let activity = cx.t("browser.audit.title");
        let view = cx.entity().downgrade();
        let trigger = move |_open: bool| {
            Button::new("browser-agent")
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Sm)
                .height(24.0)
                .padding_x(6.0)
                .text_size(12.0)
                .disabled(!ready)
                .when(armed, |button| button.text_color(tw::EMERALD_400.hsla()))
                .when(!armed, |button| button.muted())
                .child(if busy {
                    spinner("browser-agent-busy", 14.0).into_any_element()
                } else {
                    Icon::new(if armed { "shield-check" } else { "shield" })
                        .size(14.0)
                        .into_any_element()
                })
                .child(div().font_features(tabular_nums()).child(label.clone()))
                .child(
                    div()
                        .opacity(0.5)
                        .child(Icon::new("chevron-down").size(12.0)),
                )
                .into_any_element()
        };
        let menu = Menu::dropdown("browser-agent-menu", trigger, move |_, _| {
            let (toggle_view, evidence_view, audit_view) =
                (view.clone(), view.clone(), view.clone());
            let mut start = MenuItem::new(evidence.clone(), move |_, cx| {
                let _ = evidence_view.update(cx, |surface, cx| surface.start_evidence(cx));
            })
            .icon(Icon::new("activity").size(14.0))
            .disabled(!evidence_on);
            if let Some(requires) = requires.clone() {
                start = start.detail(requires);
            }
            vec![
                MenuEntry::from(
                    MenuItem::new(toggle.clone(), move |_, cx| {
                        let _ = toggle_view.update(cx, |surface, cx| {
                            surface.ask_control(Some(!surface.control.armed), cx)
                        });
                    })
                    .icon(Icon::new(if armed { "shield-check" } else { "shield" }).size(14.0))
                    .disabled(loading || busy),
                ),
                MenuEntry::from(start),
                MenuEntry::Separator,
                MenuEntry::from(
                    MenuItem::new(activity.clone(), move |_, cx| {
                        let _ =
                            audit_view.update(cx, |surface, cx| surface.set_audit_open(true, cx));
                    })
                    .icon(Icon::new("history").size(14.0)),
                ),
            ]
        })
        .align(MenuAlign::End)
        .min_width(256.0);
        // The activity's card opens under the pill, as its popover's anchor.
        let audit = self.audit_popover(div(), cx);
        Some(
            div()
                .relative()
                .flex_shrink_0()
                .child(menu)
                .child(div().absolute().right_0().bottom_0().child(audit))
                .into_any_element(),
        )
    }
}

//! The section: the paired phones' card with "Atualizar", the relay's card,
//! the devices' table with "Revogar", and the audit log (collapsed until
//! asked).

use gpui::{AnyElement, Context, FontWeight, SharedString, div, prelude::*, px};

use super::super::Settings;
use crate::data::remote_access::{AuditEntry, Device, Tone, audit_time, event_label, relative};
use crate::theme::ActiveTheme;
use crate::ui::{Badge, BadgeVariant, Button, ButtonSize, ButtonVariant, Icon, spinner};

/// `rounded-xl border border-border/60`
pub(super) fn card(cx: &gpui::App) -> gpui::Div {
    div()
        .rounded(px(12.0))
        .border_1()
        .border_color(cx.theme().colors.border.opacity(0.6))
}

pub(super) fn icon_label(icon: AnyElement, label: impl Into<SharedString>) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .child(icon)
        .child(label.into())
        .into_any_element()
}

/// The refresh icon, spinning while it reads (`animate-spin`).
fn refresh_icon(id: &'static str, loading: bool) -> AnyElement {
    if loading {
        spinner(id, 14.0).into_any_element()
    } else {
        Icon::new("refresh-cw").size(14.0).into_any_element()
    }
}

impl Settings {
    pub(in super::super) fn connections_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let connections = &self.connections;
        let count = connections.devices.len();
        let header = card(cx)
            .bg(colors.muted.opacity(0.15))
            .p(px(16.0))
            .child(
                div()
                    .flex()
                    .items_start()
                    .justify_between()
                    .gap(px(12.0))
                    .child(
                        div()
                            .min_w_0()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .child(
                                        Icon::new("smartphone")
                                            .size(16.0)
                                            .stroke(1.9)
                                            .color(colors.muted_foreground),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(14.0))
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(colors.foreground)
                                            .child("Dispositivos pareados"),
                                    ),
                            )
                            .child(
                                div()
                                    .mt(px(4.0))
                                    .text_size(px(12.0))
                                    .line_height(px(19.5))
                                    .text_color(colors.muted_foreground)
                                    .child(
                                        "Cada celular pareado pelo relay autentica com a própria chave \
                                         (X25519), fixada quando você aprova o pareamento. A chave privada \
                                         nunca sai do aparelho. Revogar derruba a conexão na hora.",
                                    ),
                            ),
                    )
                    .child(
                        Badge::new(
                            count.to_string(),
                            if count > 0 {
                                BadgeVariant::Success
                            } else {
                                BadgeVariant::Outline
                            },
                        )
                        .height(28.0)
                        .padding_x(10.0)
                        .text_size(11.0)
                        .weight(FontWeight::NORMAL),
                    ),
            )
            .child(
                div().mt(px(12.0)).flex().flex_wrap().gap(px(8.0)).child(
                    Button::new("settings-connections-refresh")
                        .variant(ButtonVariant::Outline)
                        .size(ButtonSize::Sm)
                        .text_size(12.0)
                        .on_click(cx.listener(|settings, _, _, cx| settings.load_devices(cx)))
                        .child(icon_label(
                            refresh_icon("settings-connections-refreshing", connections.loading),
                            "Atualizar",
                        )),
                ),
            );
        let devices = if connections.devices.is_empty() && !connections.loading {
            div()
                .rounded(px(12.0))
                .border_1()
                .border_dashed()
                .border_color(colors.border.opacity(0.7))
                .p(px(16.0))
                .text_center()
                .text_size(px(12.0))
                .text_color(colors.muted_foreground)
                .child("Nenhum dispositivo pareado ainda.")
                .into_any_element()
        } else {
            self.devices_table(cx)
        };
        div()
            .debug_selector(|| "settings-connections".into())
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(header)
            .children(self.relay_card(cx))
            .child(devices)
            .child(self.machines_card(cx))
            .child(self.audit_section(cx))
            .into_any_element()
    }

    fn devices_table(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let now = chrono::Utc::now();
        let head = |label: &'static str| {
            div()
                .flex_1()
                .px(px(12.0))
                .py(px(8.0))
                .font_weight(FontWeight::MEDIUM)
                .child(label)
        };
        let rows = self
            .connections
            .devices
            .iter()
            .map(|device| self.device_row(device, now, cx))
            .collect::<Vec<_>>();
        card(cx)
            .overflow_hidden()
            .text_size(px(12.0))
            .child(
                div()
                    .flex()
                    .bg(colors.muted.opacity(0.3))
                    .text_size(px(11.0))
                    .text_color(colors.muted_foreground)
                    .child(head("Dispositivo").flex_grow(2.0))
                    .child(head("Último uso"))
                    .child(head("Pareado em"))
                    .child(div().w(px(96.0))),
            )
            .children(rows)
            .into_any_element()
    }

    fn device_row(
        &self,
        device: &Device,
        now: chrono::DateTime<chrono::Utc>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let revoking = self.connections.revoking.as_deref() == Some(device.device_id.as_str());
        let target = device.clone();
        let id = device.device_id.clone();
        div()
            .id(SharedString::from(format!("settings-device-{id}")))
            .debug_selector(move || format!("settings-device-{id}"))
            .flex()
            .items_center()
            .border_t_1()
            .border_color(colors.border.opacity(0.4))
            .hover(|row| row.bg(colors.muted.opacity(0.1)))
            .child(
                div()
                    .flex_grow(2.0)
                    .flex_basis(px(0.0))
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .px(px(12.0))
                    .py(px(10.0))
                    .child(
                        Icon::new("key-round")
                            .size(14.0)
                            .color(colors.muted_foreground),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(colors.foreground)
                                    .child(div().truncate().child(device.device_name.clone()))
                                    .when(device.kind == "relay", |name| {
                                        name.child(
                                            div()
                                                .rounded(px(4.0))
                                                .border_1()
                                                .border_color(colors.border.opacity(0.6))
                                                .px(px(4.0))
                                                .text_size(px(9.0))
                                                .font_weight(FontWeight::NORMAL)
                                                .text_color(colors.muted_foreground)
                                                .child(crate::ui::Tracked::new("RELAY", 9.0, 0.05)),
                                        )
                                    }),
                            )
                            .children(device.user_agent.clone().map(|agent| {
                                div()
                                    .truncate()
                                    .text_size(px(10.0))
                                    .text_color(colors.muted_foreground)
                                    .child(agent)
                            })),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .flex_basis(px(0.0))
                    .px(px(12.0))
                    .py(px(10.0))
                    .text_color(colors.muted_foreground)
                    .child(relative(device.last_used_at.as_deref(), now)),
            )
            .child(
                div()
                    .flex_1()
                    .flex_basis(px(0.0))
                    .px(px(12.0))
                    .py(px(10.0))
                    .text_color(colors.muted_foreground)
                    .child(relative(Some(&device.created_at), now)),
            )
            .child(
                div().w(px(96.0)).flex().justify_end().px(px(12.0)).child(
                    Button::new(SharedString::from(format!(
                        "settings-device-revoke-{}",
                        device.device_id
                    )))
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::Xs)
                    .text_size(12.0)
                    .text_color(colors.destructive)
                    .hover_text_color(colors.destructive)
                    .disabled(revoking)
                    .on_click(cx.listener(move |settings, _, _, cx| {
                        settings.revoke_device(target.clone(), cx)
                    }))
                    .child(icon_label(
                        if revoking {
                            spinner("settings-device-revoking", 14.0).into_any_element()
                        } else {
                            Icon::new("trash-2").size(14.0).into_any_element()
                        },
                        "Revogar",
                    )),
                ),
            )
            .into_any_element()
    }

    fn audit_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let Some((entries, loading)) = &self.connections.audit else {
            return div()
                .id("settings-connections-audit")
                .debug_selector(|| "settings-connections-audit".into())
                .w_full()
                .rounded(px(12.0))
                .border_1()
                .border_color(colors.border.opacity(0.6))
                .bg(colors.muted.opacity(0.1))
                .px(px(16.0))
                .py(px(10.0))
                .text_size(px(12.0))
                .text_color(colors.muted_foreground)
                .cursor_pointer()
                .hover(|row| row.bg(colors.muted.opacity(0.2)))
                .on_click(cx.listener(|settings, _, _, cx| settings.toggle_audit(cx)))
                .child("Ver log de auditoria de pareamentos →")
                .into_any_element();
        };
        let body = if entries.is_empty() {
            div()
                .px(px(16.0))
                .py(px(24.0))
                .text_center()
                .text_size(px(12.0))
                .text_color(colors.muted_foreground)
                .child(if *loading {
                    "Carregando…"
                } else {
                    "Nenhum evento registrado."
                })
                .into_any_element()
        } else {
            div()
                .id("settings-connections-audit-list")
                .max_h(px(288.0))
                .overflow_y_scroll()
                .text_size(px(11.0))
                .children(
                    entries
                        .iter()
                        .enumerate()
                        .map(|(index, entry)| audit_row(index, entry, cx)),
                )
                .into_any_element()
        };
        card(cx)
            .debug_selector(|| "settings-connections-audit-open".into())
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_b_1()
                    .border_color(colors.border.opacity(0.4))
                    .px(px(16.0))
                    .py(px(10.0))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(colors.foreground)
                            .child("Log de auditoria"),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(
                                Button::new("settings-connections-audit-refresh")
                                    .variant(ButtonVariant::Ghost)
                                    .size(ButtonSize::Xs)
                                    .on_click(
                                        cx.listener(|settings, _, _, cx| settings.load_audit(cx)),
                                    )
                                    .child(refresh_icon(
                                        "settings-connections-audit-loading",
                                        *loading,
                                    )),
                            )
                            .child(
                                Button::new("settings-connections-audit-close")
                                    .variant(ButtonVariant::Ghost)
                                    .size(ButtonSize::Xs)
                                    .text_size(12.0)
                                    .on_click(
                                        cx.listener(|settings, _, _, cx| settings.toggle_audit(cx)),
                                    )
                                    .child("Fechar"),
                            ),
                    ),
            )
            .child(body)
            .into_any_element()
    }
}

fn audit_row(index: usize, entry: &AuditEntry, cx: &gpui::App) -> AnyElement {
    let colors = &cx.theme().colors;
    let (label, icon, tone) = event_label(&entry.event);
    let tone = match tone {
        Tone::Success => colors.success,
        Tone::Warn => colors.warning,
        Tone::Danger => colors.destructive,
        Tone::Quiet => colors.muted_foreground,
    };
    let mono = cx.theme().fonts.mono.clone();
    let device = entry.device_id.as_ref().map(|id| {
        let short: String = id.chars().take(8).collect();
        div()
            .font_family(mono.clone())
            .child(format!("device {short}…"))
    });
    let ip = entry
        .ip
        .as_ref()
        .map(|ip| div().font_family(mono.clone()).child(format!("ip {ip}")));
    let agent = entry
        .user_agent
        .as_ref()
        .map(|agent| div().truncate().child(agent.clone()));
    div()
        .flex()
        .items_start()
        .gap(px(12.0))
        .px(px(16.0))
        .py(px(8.0))
        .when(index > 0, |row| {
            row.border_t_1().border_color(colors.border.opacity(0.4))
        })
        .child(
            div()
                .mt(px(2.0))
                .child(Icon::new(icon).size(14.0).color(tone)),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            div()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(colors.foreground)
                                .child(label.to_string()),
                        )
                        .child(
                            div()
                                .text_color(colors.muted_foreground.opacity(0.7))
                                .child(audit_time(&entry.created_at)),
                        ),
                )
                .child(
                    div()
                        .mt(px(2.0))
                        .flex()
                        .flex_wrap()
                        .gap_x(px(12.0))
                        .gap_y(px(2.0))
                        .text_size(px(10.0))
                        .text_color(colors.muted_foreground)
                        .children(device)
                        .children(ip)
                        .children(agent),
                ),
        )
        .into_any_element()
}

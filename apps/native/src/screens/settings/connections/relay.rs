//! The relay's card (`RelayAccessCard`): its state, the address with
//! "Salvar", "Parear pelo relay" and this desktop's fingerprint; the QR's
//! dialog (5 minutes, "Gerar outro" once it expires) and "Aprovar …?" for
//! each phone that scanned it.

use std::time::Instant;

use gpui::{AnyElement, Context, FontWeight, div, prelude::*, px};

use super::super::Settings;
use super::view::{card, icon_label};
use crate::data::remote_access::qr_modules;
use crate::theme::ActiveTheme;
use crate::ui::{
    Badge, BadgeVariant, Button, ButtonSize, ButtonVariant, Dialog, Icon, dialog_footer,
    dialog_header, spinner,
};

const QR_SIZE: f32 = 224.0;

/// `QRCode.toString(value, { margin: 1, width: 224 })`: dark `#111827` on
/// white, one module of margin; each row's dark runs drawn as one bar.
fn qr(value: &str) -> AnyElement {
    let Some(modules) = qr_modules(value) else {
        return div().size(px(QR_SIZE)).into_any_element();
    };
    let count = modules.len() + 2;
    let module = QR_SIZE / count as f32;
    let dark = crate::theme::color::hex(0x111827);
    let rows = modules.iter().map(|row| {
        let mut bars = Vec::new();
        let mut column = 0;
        while column < row.len() {
            let start = column;
            while column < row.len() && row[column] == row[start] {
                column += 1;
            }
            let width = (column - start) as f32 * module;
            bars.push(
                div()
                    .flex_none()
                    .w(px(width))
                    .h_full()
                    .when(row[start], |bar| bar.bg(dark)),
            );
        }
        div().flex().h(px(module)).children(bars)
    });
    div()
        .debug_selector(|| "settings-relay-qr".into())
        .size(px(QR_SIZE))
        .bg(gpui::white())
        .p(px(module))
        .children(rows)
        .into_any_element()
}

/// What approving means: a phone drives sessions; another DCC also runs
/// tasks and opens terminals here, as the person of this computer.
pub(super) fn approval_text(platform: Option<&str>) -> &'static str {
    match platform {
        Some("macos" | "linux" | "windows") => {
            "Outro computador com o DCC pediu acesso a este. Ele vai poder criar tarefas, rodar \
             agentes e abrir terminais aqui, com as suas contas e os seus projetos. Aprove só se \
             for alguém em quem você confia e o código abaixo for o mesmo que ele mostra."
        }
        _ => {
            "Um celular escaneou o QR deste desktop. Aprove só se o código abaixo for o mesmo \
             que ele mostra."
        }
    }
}

impl Settings {
    pub(super) fn relay_card(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let status = self.connections.relay.clone()?;
        let colors = cx.theme().colors.clone();
        let mono = cx.theme().fonts.mono.clone();
        let configured = status.relay_url.is_some();
        let draft = self
            .connections
            .relay_input
            .read(cx)
            .text()
            .trim()
            .to_string();
        let unchanged = draft == status.relay_url.clone().unwrap_or_default();
        let saving = self.connections.saving;
        let field = if status.relay_url_from_env {
            div()
                .flex_1()
                .min_w_0()
                .rounded(px(8.0))
                .border_1()
                .border_color(colors.border)
                .bg(colors.background)
                .p(px(8.0))
                .opacity(0.6)
                .font_family(mono.clone())
                .text_size(px(12.0))
                .truncate()
                .child(status.relay_url.clone().unwrap_or_default())
                .into_any_element()
        } else {
            div()
                .debug_selector(|| "settings-relay-url".into())
                .flex_1()
                .min_w_0()
                .rounded(px(8.0))
                .border_1()
                .border_color(colors.border)
                .bg(colors.background)
                .p(px(8.0))
                .font_family(mono.clone())
                .text_size(px(12.0))
                .child(self.connections.relay_input.clone())
                .into_any_element()
        };
        Some(
            card(cx)
                .debug_selector(|| "settings-relay".into())
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
                                            Icon::new("wifi")
                                                .size(16.0)
                                                .stroke(1.9)
                                                .color(colors.muted_foreground),
                                        )
                                        .child(
                                            div()
                                                .text_size(px(14.0))
                                                .font_weight(FontWeight::MEDIUM)
                                                .text_color(colors.foreground)
                                                .child("Celular de qualquer rede"),
                                        ),
                                )
                                .child(
                                    div()
                                        .mt(px(4.0))
                                        .text_size(px(12.0))
                                        .line_height(px(19.5))
                                        .text_color(colors.muted_foreground)
                                        .child(
                                            "O app do celular fala com este desktop por um relay, cifrado \
                                             de ponta a ponta: o relay só repassa bytes. Sem rede local, \
                                             Tailscale ou conta.",
                                        ),
                                ),
                        )
                        .child(
                            Badge::new(
                                if !configured {
                                    "Sem relay"
                                } else if status.connected {
                                    "Conectado"
                                } else {
                                    "Ocioso"
                                },
                                if status.connected {
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
                    div()
                        .mt(px(12.0))
                        .text_size(px(12.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(colors.foreground)
                        .child("Endereço do relay")
                        .child(
                            div()
                                .mt(px(8.0))
                                .flex()
                                .gap(px(8.0))
                                .child(field)
                                .when(!status.relay_url_from_env, |row| {
                                    row.child(
                                        Button::new("settings-relay-save")
                                            .variant(ButtonVariant::Outline)
                                            .size(ButtonSize::Sm)
                                            .text_size(12.0)
                                            .disabled(saving || unchanged)
                                            .on_click(
                                                cx.listener(|settings, _, _, cx| settings.save_relay(cx)),
                                            )
                                            .child(
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .gap(px(6.0))
                                                    .when(saving, |label| {
                                                        label.child(spinner("settings-relay-saving", 14.0))
                                                    })
                                                    .child("Salvar"),
                                            ),
                                    )
                                }),
                        )
                        .child(
                            div()
                                .mt(px(8.0))
                                .text_size(px(11.0))
                                .line_height(px(17.9))
                                .font_weight(FontWeight::NORMAL)
                                .text_color(colors.muted_foreground)
                                .child(if status.relay_url_from_env {
                                    "Definido pela variável DCC_RELAY_URL."
                                } else {
                                    "Padrão: o relay do projeto (wss://relay.devcommandcenter.com). Dá para \
                                     usar um seu (apps/relay); apagar e salvar desliga. Fica conectado só \
                                     enquanto houver celular pareado por aqui ou um QR aberto."
                                }),
                        ),
                )
                .child(
                    div()
                        .mt(px(12.0))
                        .flex()
                        .flex_wrap()
                        .gap(px(8.0))
                        .child(
                            Button::new("settings-relay-pair")
                                .size(ButtonSize::Sm)
                                .text_size(12.0)
                                .disabled(!configured)
                                .on_click(cx.listener(|settings, _, _, cx| settings.open_pairing(cx)))
                                .child(icon_label(
                                    Icon::new("qr-code").size(14.0).into_any_element(),
                                    "Parear pelo relay",
                                )),
                        )
                        .children(status.desktop_fingerprint.map(|fingerprint| {
                            div()
                                .self_center()
                                .font_family(mono.clone())
                                .text_size(px(11.0))
                                .text_color(colors.muted_foreground)
                                .child(format!("este desktop: {fingerprint}"))
                        })),
                )
                .into_any_element(),
        )
    }

    /// "Parear pelo relay" or "Aprovar …?", over Settings.
    pub(in super::super) fn connections_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.connections.approvals.is_empty() {
            return self.approval_dialog(cx);
        }
        let pairing = self.connections.pairing.as_ref()?;
        let colors = cx.theme().colors.clone();
        let mono = cx.theme().fonts.mono.clone();
        let body = match &pairing.offer {
            Some((offer, ends)) if !pairing.loading => {
                let left = ends.saturating_duration_since(Instant::now()).as_secs();
                if left == 0 {
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap(px(12.0))
                        .child(
                            div()
                                .text_size(px(13.0))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(colors.foreground)
                                .child("Código expirado"),
                        )
                        .child(
                            Button::new("settings-relay-regenerate")
                                .size(ButtonSize::Sm)
                                .text_size(12.0)
                                .on_click(
                                    cx.listener(|settings, _, _, cx| settings.generate_offer(cx)),
                                )
                                .child(icon_label(
                                    Icon::new("refresh-cw").size(14.0).into_any_element(),
                                    "Gerar outro",
                                )),
                        )
                        .into_any_element()
                } else {
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap(px(12.0))
                        .child(qr(&offer.url))
                        .child(
                            div()
                                .flex()
                                .gap(px(4.0))
                                .text_size(px(11.0))
                                .text_color(colors.muted_foreground)
                                .child("expira em")
                                .child(
                                    div()
                                        .font_family(mono.clone())
                                        .text_color(colors.foreground.opacity(0.8))
                                        .child(format!("{}:{:02}", left / 60, left % 60)),
                                ),
                        )
                        .child(
                            div()
                                .font_family(mono.clone())
                                .text_size(px(11.0))
                                .text_color(colors.muted_foreground)
                                .child(format!("este desktop: {}", offer.desktop_fingerprint)),
                        )
                        // Another DCC has no camera: it takes the link.
                        .child({
                            let link = offer.url.clone();
                            Button::new("settings-relay-copy-link")
                                .variant(ButtonVariant::Outline)
                                .size(ButtonSize::Sm)
                                .text_size(12.0)
                                .on_click(move |_, _, cx| {
                                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                        link.clone(),
                                    ));
                                    crate::ui::Toaster::success(
                                        "Link copiado: cole em Conexões › Máquinas do outro DCC",
                                        cx,
                                    );
                                })
                                .child(icon_label(
                                    Icon::new("copy").size(14.0).into_any_element(),
                                    "Copiar link para outro DCC",
                                ))
                        })
                        .into_any_element()
                }
            }
            _ => spinner("settings-relay-offer-loading", 20.0).into_any_element(),
        };
        let close = cx.entity().downgrade();
        let close_footer = close.clone();
        Some(
            Dialog::new("settings-relay-pairing", move |_, cx| {
                let _ = close.update(cx, |settings, cx| settings.close_pairing(cx));
            })
            .child(
                div()
                    .debug_selector(|| "settings-relay-pairing".into())
                    .flex()
                    .flex_col()
                    .gap(px(16.0))
                    .child(dialog_header(
                        "Parear pelo relay",
                        Some(
                            "Escaneie com o app DCC no celular. O código vale por 5 minutos e para um \
                             aparelho; depois você aprova o aparelho aqui."
                                .into(),
                        ),
                        cx,
                    ))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .min_h(px(260.0))
                            .items_center()
                            .justify_center()
                            .child(body),
                    )
                    .child(dialog_footer(
                        [Button::new("settings-relay-pairing-close")
                            .variant(ButtonVariant::Outline)
                            .size(ButtonSize::Default)
                            .text_size(14.0)
                            .on_click(move |_, _, cx| {
                                let _ = close_footer.update(cx, |settings, cx| settings.close_pairing(cx));
                            })
                            .child("Fechar")
                            .into_any_element()],
                        cx,
                    )),
            )
            .into_any_element(),
        )
    }

    fn approval_dialog(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let current = self.connections.approvals.first()?.clone();
        let colors = cx.theme().colors.clone();
        let answering = self.connections.answering;
        let deny = cx.entity().downgrade();
        Some(
            Dialog::new("settings-relay-approval", move |_, cx| {
                let _ = deny.update(cx, |settings, cx| settings.answer_pairing(false, cx));
            })
            .child(
                div()
                    .debug_selector(|| "settings-relay-approval".into())
                    .flex()
                    .flex_col()
                    .gap(px(16.0))
                    .child(dialog_header(
                        format!("Aprovar {}?", current.device_name),
                        Some(approval_text(current.platform.as_deref()).into()),
                        cx,
                    ))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap(px(8.0))
                            .py(px(12.0))
                            .child(
                                Icon::new("shield-check")
                                    .size(20.0)
                                    .stroke(1.8)
                                    .color(colors.muted_foreground),
                            )
                            .child(
                                div()
                                    .font_family(cx.theme().fonts.mono.clone())
                                    .text_size(px(22.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(colors.foreground)
                                    .child(crate::ui::Tracked::new(
                                        current.fingerprint.clone(),
                                        22.0,
                                        0.12,
                                    )),
                            )
                            .children(current.platform.clone().map(|platform| {
                                div()
                                    .text_size(px(11.0))
                                    .text_color(colors.muted_foreground)
                                    .child(platform)
                            })),
                    )
                    .child(dialog_footer(
                        [
                            Button::new("settings-relay-deny")
                                .variant(ButtonVariant::Outline)
                                .size(ButtonSize::Default)
                                .text_size(14.0)
                                .disabled(answering)
                                .on_click(cx.listener(|settings, _, _, cx| {
                                    settings.answer_pairing(false, cx)
                                }))
                                .child("Recusar")
                                .into_any_element(),
                            Button::new("settings-relay-approve")
                                .size(ButtonSize::Default)
                                .text_size(14.0)
                                .disabled(answering)
                                .on_click(cx.listener(|settings, _, _, cx| {
                                    settings.answer_pairing(true, cx)
                                }))
                                .child("Aprovar")
                                .into_any_element(),
                        ],
                        cx,
                    )),
            )
            .into_any_element(),
        )
    }
}

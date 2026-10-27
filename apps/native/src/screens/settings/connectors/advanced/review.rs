//! "Confiar e ativar?": where it connects or the exact command, where the
//! credentials go and the fingerprint, then "Manter desativada" or
//! "Confirmar e ativar" for that fingerprint.

use gpui::{AnyElement, Context, FontWeight, SharedString, div, prelude::*, px};

use super::super::super::Settings;
use super::parts::{badges, credential_badge, labelled, text, title, warning_card};
use crate::data::connectors::Integration;
use crate::data::mcp_advanced::transport_preview;
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::ui::{Badge, BadgeVariant, Button, ButtonSize, ButtonVariant, Icon, spinner};

/// `text-[11px] font-medium uppercase tracking-[0.08em]`.
fn heading(value: SharedString, cx: &gpui::App) -> AnyElement {
    div()
        .text_size(px(11.0))
        .font_weight(FontWeight::MEDIUM)
        .text_color(cx.theme().colors.muted_foreground)
        .child(crate::ui::Tracked::new(value.to_uppercase(), 11.0, 0.08))
        .into_any_element()
}

impl Settings {
    pub(super) fn advanced_review(
        &self,
        integration: &Integration,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let definition = &integration.definition;
        let http = definition.transport.kind == "http";
        let activating = self.connectors.advanced.busy.as_deref()
            == Some(format!("activate:{}", definition.id).as_str());
        let mono = cx.theme().fonts.mono.clone();
        let cwd = (!http)
            .then(|| definition.transport.cwd.clone())
            .flatten()
            .filter(|cwd| !cwd.is_empty());
        let activate = integration.clone();
        let back = || {
            Button::new("settings-mcp-review-back")
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Sm)
                .text_size(12.0)
                .on_click(cx.listener(|settings, _, _, cx| settings.advanced_back(cx)))
                .child(labelled(
                    Icon::new("arrow-left").size(14.0).into_any_element(),
                    cx.t("settings.integrations.back"),
                ))
        };
        div()
            .debug_selector(|| "settings-mcp-review".into())
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(div().ml(px(-8.0)).child(back()))
            .child(
                warning_card(cx).child(
                    div()
                        .flex()
                        .items_start()
                        .gap(px(12.0))
                        .child(
                            div()
                                .flex()
                                .flex_none()
                                .size(px(36.0))
                                .items_center()
                                .justify_center()
                                .rounded(px(8.0))
                                .bg(colors.warning.opacity(0.16))
                                .child(Icon::new("shield-check").size(16.0)),
                        )
                        .child(
                            div()
                                .child(title(
                                    14.0,
                                    cx.t(if integration.needs_trust() {
                                        "settings.integrations.reviewTrustTitle"
                                    } else {
                                        "settings.integrations.reviewEnableTitle"
                                    }),
                                    cx,
                                ))
                                .child(
                                    text(
                                        12.0,
                                        true,
                                        cx.t("settings.integrations.reviewTrustHint"),
                                        cx,
                                    )
                                    .mt(px(4.0)),
                                ),
                        ),
                ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(16.0))
                    .rounded(px(12.0))
                    .border_1()
                    .border_color(colors.border.opacity(0.6))
                    .p(px(16.0))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_start()
                            .justify_between()
                            .gap(px(12.0))
                            .child(
                                div()
                                    .child(title(14.0, definition.display_name.clone(), cx))
                                    .child(
                                        div()
                                            .mt(px(4.0))
                                            .flex()
                                            .flex_wrap()
                                            .gap(px(6.0))
                                            .children(badges(integration, cx))
                                            .child(Badge::new(
                                                cx.t("settings.integrations.awaitingConfirmation"),
                                                BadgeVariant::Warn,
                                            )),
                                    ),
                            )
                            .children(credential_badge(integration, cx)),
                    )
                    .child(
                        div()
                            .child(heading(
                                cx.t(if http {
                                    "settings.integrations.destination"
                                } else {
                                    "settings.integrations.exactCommand"
                                }),
                                cx,
                            ))
                            .child(
                                div()
                                    .debug_selector(|| "settings-mcp-review-preview".into())
                                    .mt(px(8.0))
                                    .rounded(px(8.0))
                                    .border_1()
                                    .border_color(colors.border.opacity(0.5))
                                    .bg(colors.muted.opacity(0.2))
                                    .p(px(12.0))
                                    .font_family(mono.clone())
                                    .text_size(px(12.0))
                                    .line_height(px(19.5))
                                    .text_color(colors.foreground)
                                    .child(transport_preview(integration)),
                            )
                            .children(cwd.map(|cwd| {
                                div()
                                    .mt(px(8.0))
                                    .font_family(mono.clone())
                                    .text_size(px(11.0))
                                    .text_color(colors.muted_foreground)
                                    .child(cx.t_with(
                                        "settings.integrations.cwdPreview",
                                        &Args::default().with("cwd", cwd),
                                    ))
                            })),
                    )
                    .when(!definition.secret_refs.is_empty(), |details| {
                        details.child(
                            div()
                                .child(heading(
                                    cx.t("settings.integrations.secretDestinations"),
                                    cx,
                                ))
                                .child(div().mt(px(8.0)).flex().flex_wrap().gap(px(6.0)).children(
                                    definition.secret_refs.iter().map(|secret| {
                                        div().font_family(mono.clone()).child(Badge::new(
                                            secret.target.name.clone(),
                                            BadgeVariant::Outline,
                                        ))
                                    }),
                                )),
                        )
                    })
                    .child(
                        div()
                            .child(heading(cx.t("settings.integrations.fingerprint"), cx))
                            .child(
                                div()
                                    .mt(px(8.0))
                                    .font_family(mono.clone())
                                    .text_size(px(10.0))
                                    .line_height(px(16.25))
                                    .text_color(colors.muted_foreground)
                                    .child(definition.trust.current_fingerprint.clone()),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(8.0))
                    .child(
                        Button::new("settings-mcp-keep-disabled")
                            .variant(ButtonVariant::Ghost)
                            .size(ButtonSize::Default)
                            .text_size(14.0)
                            .disabled(activating)
                            .on_click(cx.listener(|settings, _, _, cx| settings.advanced_back(cx)))
                            .child(cx.t("settings.integrations.keepDisabled")),
                    )
                    .child(
                        Button::new("settings-mcp-confirm-enable")
                            .size(ButtonSize::Default)
                            .text_size(14.0)
                            .disabled(activating)
                            .on_click(cx.listener(move |settings, _, _, cx| {
                                settings.activate_advanced(activate.clone(), cx)
                            }))
                            .child(labelled(
                                if activating {
                                    spinner("settings-mcp-activating", 14.0).into_any_element()
                                } else {
                                    Icon::new("power").size(14.0).into_any_element()
                                },
                                cx.t("settings.integrations.confirmEnable"),
                            )),
                    ),
            )
            .into_any_element()
    }
}

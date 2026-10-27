//! The saved resets of a plan (`ResetCreditList`) and the confirmation
//! before spending one, with the headroom warning.

use std::collections::HashSet;

use dcc_core::domain::provider::ProviderAccountUsageState;
use gpui::{AnyElement, Context, FontWeight, SharedString, div, prelude::*, px};

use super::super::Settings;
use super::accounts::AccountKey;
use super::usage::{UsageState, english};
use crate::data::provider_usage::{Redeemable, relative_expiry, short_date_time};
use crate::i18n::{Args, Translate};
use crate::theme::tailwind as tw;
use crate::theme::{ActiveTheme, Appearance};
use crate::ui::{
    Button, ButtonSize, ButtonVariant, Dialog, Icon, dialog_footer, dialog_header, spinner,
};

impl Settings {
    pub(super) fn reset_list(
        &self,
        key: &AccountKey,
        resets: &[(usize, Redeemable)],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let english = english(cx);
        let now = crate::data::time::now_ms();
        let named: Vec<String> = resets
            .iter()
            .map(|(position, reset)| {
                if !reset.title.is_empty() {
                    reset.title.clone()
                } else if *position == 0 {
                    cx.t("settings.model.usageResetNext").to_string()
                } else {
                    cx.t_with(
                        "settings.model.usageResetLabel",
                        &Args::default().with("index", position.to_string()),
                    )
                    .to_string()
                }
            })
            .collect();
        // Same-titled credits need a position to be told apart.
        let same = named.iter().collect::<HashSet<_>>().len() < named.len();
        let pending = self.providers.usage.resetting;
        let rows = resets
            .iter()
            .zip(named)
            .enumerate()
            .map(|(index, ((_, reset), title))| {
                let title = if same {
                    format!("{title} · {}", index + 1)
                } else {
                    title
                };
                let expiry = match (&reset.expires_at, reset.expires_ms) {
                    (Some(at), Some(ms)) => cx.t_with(
                        "settings.model.usageResetExpiresRelative",
                        &Args::default()
                            .with("date", short_date_time(at, english).unwrap_or_default())
                            .with("relative", relative_expiry(ms, now, english)),
                    ),
                    _ => cx.t("settings.model.usageResetNoExpiry"),
                };
                let chosen = (
                    key.clone(),
                    Redeemable {
                        title: title.clone(),
                        ..reset.clone()
                    },
                );
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .px(px(12.0))
                    .py(px(10.0))
                    .when(index > 0, |row| {
                        row.border_t_1().border_color(colors.border.opacity(0.5))
                    })
                    .child(
                        div()
                            .min_w_0()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .text_size(px(12.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(div().truncate().child(title))
                                    .when(index == 0 && resets.len() > 1, |line| {
                                        line.child(
                                            div()
                                                .flex_none()
                                                .rounded_full()
                                                .bg(tw::AMBER_500.alpha(0.12))
                                                .px(px(6.0))
                                                .py(px(1.0))
                                                .text_size(px(10.0))
                                                .text_color(match cx.theme().appearance {
                                                    Appearance::Light => tw::AMBER_700.hsla(),
                                                    Appearance::Dark => tw::AMBER_400.hsla(),
                                                })
                                                .child(cx.t("settings.model.usageResetSoonest")),
                                        )
                                    }),
                            )
                            .child(
                                div()
                                    .mt(px(2.0))
                                    .truncate()
                                    .text_size(px(11.0))
                                    .text_color(colors.muted_foreground)
                                    .child(expiry),
                            ),
                    )
                    .child(
                        Button::new(SharedString::from(format!("settings-usage-reset-{index}")))
                            .variant(if index == 0 {
                                ButtonVariant::Default
                            } else {
                                ButtonVariant::Outline
                            })
                            .size(ButtonSize::Sm)
                            .text_size(12.0)
                            .disabled(pending)
                            .on_click(cx.listener(move |settings, _, _, cx| {
                                settings.providers.usage.confirm = Some(chosen.clone());
                                cx.notify();
                            }))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .child(Icon::new("rotate-ccw").size(14.0))
                                    .child(cx.t("settings.model.usageResetAction")),
                            ),
                    )
            });
        div()
            .overflow_hidden()
            .rounded(px(14.0))
            .border_1()
            .border_color(colors.border.opacity(0.6))
            .bg(colors.background)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .border_b_1()
                    .border_color(colors.border.opacity(0.5))
                    .px(px(12.0))
                    .py(px(8.0))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .font_weight(FontWeight::MEDIUM)
                            .child(cx.t("settings.model.usageResetsTitle")),
                    )
                    .child(
                        div()
                            .font_features(crate::ui::tabular_nums())
                            .text_size(px(11.0))
                            .text_color(colors.muted_foreground)
                            .child(cx.t_with(
                                "settings.model.usageResetAvailable",
                                &Args::count(resets.len() as i64),
                            )),
                    ),
            )
            .children(rows)
            .into_any_element()
    }

    /// "Usar reset do limite de uso?", with the headroom warning.
    pub(in super::super) fn reset_dialog(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (key, reset) = self.providers.usage.confirm.as_ref()?;
        let english = english(cx);
        let body = match &reset.expires_at {
            Some(at) => cx.t_with(
                "settings.model.usageResetConfirmBodyCredit",
                &Args::default()
                    .with("title", reset.title.clone())
                    .with("date", short_date_time(at, english).unwrap_or_default()),
            ),
            None => cx.t("settings.model.usageResetConfirmBody"),
        };
        let lowest = match self.providers.usage.data.get(key) {
            Some(UsageState::Ready(Some(usage)))
                if matches!(usage.state, ProviderAccountUsageState::Available) =>
            {
                usage
                    .windows
                    .iter()
                    .map(|window| window.remaining_percent)
                    .reduce(f64::min)
            }
            _ => None,
        };
        let headroom = lowest.filter(|lowest| *lowest >= 10.0).map(|lowest| {
            div()
                .rounded(px(10.0))
                .bg(tw::AMBER_500.alpha(0.1))
                .px(px(12.0))
                .py(px(8.0))
                .text_size(px(12.0))
                .line_height(px(19.5))
                .text_color(match cx.theme().appearance {
                    Appearance::Light => tw::AMBER_800.hsla(),
                    Appearance::Dark => tw::AMBER_300.hsla(),
                })
                .child(cx.t_with(
                    "settings.model.usageResetHeadroom",
                    &Args::default().with("percent", lowest.round().to_string()),
                ))
        });
        let pending = self.providers.usage.resetting;
        let close = cx.entity().downgrade();
        Some(
            Dialog::new("settings-usage-reset-confirm", move |_, cx| {
                let _ = close.update(cx, |settings, cx| {
                    if !settings.providers.usage.resetting {
                        settings.providers.usage.confirm = None;
                        cx.notify();
                    }
                });
            })
            .child(dialog_header(
                cx.t("settings.model.usageResetConfirmTitle"),
                Some(body),
                cx,
            ))
            .children(headroom)
            .child(dialog_footer(
                [
                    Button::new("settings-usage-reset-cancel")
                        .variant(ButtonVariant::Outline)
                        .disabled(pending)
                        .on_click(cx.listener(|settings, _, _, cx| {
                            settings.providers.usage.confirm = None;
                            cx.notify();
                        }))
                        .child(cx.t("settings.model.usageResetCancel"))
                        .into_any_element(),
                    Button::new("settings-usage-reset-ok")
                        .disabled(pending)
                        .on_click(cx.listener(|settings, _, _, cx| settings.spend_reset(cx)))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .children(
                                    pending.then(|| spinner("settings-usage-resetting", 14.0)),
                                )
                                .child(cx.t("settings.model.usageResetConfirm")),
                        )
                        .into_any_element(),
                ],
                cx,
            ))
            .into_any_element(),
        )
    }
}

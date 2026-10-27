//! A sign-in's plan (`PlanMeter`, `ProviderUsageDetails`): the short bar of
//! its tightest window on the account's row, the windows with how much is
//! left and when they reset, "Atualizar uso da conta", and the saved resets
//! with the confirmation before spending one.

use std::collections::{HashMap, HashSet};

use dcc_core::domain::provider::{
    ProviderAccountUsage, ProviderAccountUsageState, ProviderDescriptor, ProviderResetOutcome,
    ProviderUsageWindow,
};
use dcc_core::ports::provider::ProviderRuntimeConfig;
use gpui::{AnyElement, Context, FontWeight, Hsla, SharedString, div, prelude::*, px, relative};

use super::super::Settings;
use super::accounts::{AccountKey, Status};
use crate::data::provider_usage::{
    Redeemable, Severity, redeemable, severity, short_date_time, short_time, tightest, window_label,
};
use crate::i18n::{Args, Translate};
use crate::theme::tailwind as tw;
use crate::theme::{ActiveTheme, Appearance};
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, Toaster, spinner};

#[derive(Clone, Debug)]
pub(in super::super) enum UsageState {
    Loading,
    Failed,
    Ready(Option<ProviderAccountUsage>),
}

#[derive(Default)]
pub(in super::super) struct Usage {
    pub(in super::super) data: HashMap<AccountKey, UsageState>,
    /// Accounts whose details are open under their row.
    pub(in super::super) open: HashSet<AccountKey>,
    /// A refresh running, so the button spins.
    refreshing: HashSet<AccountKey>,
    /// "Usar reset do limite de uso?" for this sign-in and credit.
    pub(in super::super) confirm: Option<(AccountKey, Redeemable)>,
    pub(super) resetting: bool,
}

pub(super) fn english(cx: &gpui::App) -> bool {
    cx.global::<crate::i18n::I18n>().locale() == crate::i18n::Locale::En
}

fn tone(severity: Option<Severity>, cx: &gpui::App) -> Option<Hsla> {
    let colors = &cx.theme().colors;
    severity.map(|severity| match severity {
        Severity::Critical => colors.destructive,
        Severity::Warning => match cx.theme().appearance {
            Appearance::Light => tw::AMBER_600.hsla(),
            Appearance::Dark => tw::AMBER_400.hsla(),
        },
    })
}

fn bar_fill(severity: Option<Severity>, cx: &gpui::App, rest: f32) -> Hsla {
    let colors = &cx.theme().colors;
    match severity {
        Some(Severity::Critical) => colors.destructive,
        Some(Severity::Warning) => tw::AMBER_500.hsla(),
        None => colors.foreground.opacity(rest),
    }
}

impl Settings {
    /// `withProviderAccount(draftToProviderRuntimeConfig(...), accountId)`
    fn usage_runtime(&self, key: &AccountKey) -> Option<ProviderRuntimeConfig> {
        let provider = self
            .providers
            .catalog
            .as_ref()?
            .iter()
            .find(|provider| provider.id.0 == key.0)?;
        let base = crate::data::provider_runtime::to_config(
            &self.providers.runtime.draft(&key.0),
            &provider.capabilities,
        );
        match &key.1 {
            Some(account) => Some(ProviderRuntimeConfig {
                account_id: Some(account.clone()),
                ..base.unwrap_or_default()
            }),
            None => base,
        }
    }

    /// The plan of each signed-in account (or the provider's only one) on
    /// screen, asked once.
    pub(super) fn ensure_usage(&mut self, cx: &mut Context<Self>) {
        let Some(provider) = self.providers.current().cloned() else {
            return;
        };
        if !provider.capabilities.supports_account_usage {
            return;
        }
        let keys: Vec<AccountKey> = if provider.capabilities.supports_accounts {
            self.providers
                .accounts
                .statuses
                .iter()
                .filter(|(key, status)| {
                    key.0 == provider.id.0
                        && matches!(status, Status::Ready(status) if status.signed_in())
                })
                .map(|(key, _)| key.clone())
                .collect()
        } else {
            vec![(provider.id.0.clone(), None)]
        };
        for key in keys {
            if !self.providers.usage.data.contains_key(&key) {
                self.load_usage(key, cx);
            }
        }
    }

    pub(super) fn load_usage(&mut self, key: AccountKey, cx: &mut Context<Self>) {
        let Some(api) = self.general.api() else {
            self.providers.usage.data.insert(key, UsageState::Failed);
            return;
        };
        let runtime = self.usage_runtime(&key);
        let usage = &mut self.providers.usage;
        usage.refreshing.insert(key.clone());
        usage.data.entry(key.clone()).or_insert(UsageState::Loading);
        cx.notify();
        cx.spawn(async move |this, cx| {
            let provider = key.0.clone();
            let read = cx
                .background_executor()
                .spawn(async move { api.provider_usage(&provider, runtime.as_ref()) })
                .await;
            let _ = this.update(cx, |settings, cx| {
                let usage = &mut settings.providers.usage;
                usage.refreshing.remove(&key);
                usage.data.insert(
                    key,
                    match read {
                        Ok(read) => UsageState::Ready(read),
                        Err(_) => UsageState::Failed,
                    },
                );
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn spend_reset(&mut self, cx: &mut Context<Self>) {
        let (Some(api), Some((key, reset))) =
            (self.general.api(), self.providers.usage.confirm.clone())
        else {
            return;
        };
        let runtime = self.usage_runtime(&key);
        self.providers.usage.resetting = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let provider = key.0.clone();
            let credit = reset.id.clone();
            let outcome =
                cx.background_executor()
                    .spawn(async move {
                        api.provider_reset(&provider, credit.as_deref(), runtime.as_ref())
                    })
                    .await;
            let _ = this.update(cx, |settings, cx| {
                let usage = &mut settings.providers.usage;
                usage.resetting = false;
                match outcome {
                    Ok(outcome) => {
                        usage.confirm = None;
                        match outcome {
                            ProviderResetOutcome::NoCredit => {
                                Toaster::error(cx.t("settings.model.usageResetNoCredit"), cx)
                            }
                            ProviderResetOutcome::NothingToReset => {
                                Toaster::info(cx.t("settings.model.usageResetNothingToReset"), cx)
                            }
                            _ => Toaster::success(cx.t("settings.model.usageResetSuccess"), cx),
                        }
                        settings.load_usage(key, cx);
                    }
                    Err(_) => Toaster::error(cx.t("settings.model.usageResetError"), cx),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// `PlanMeter`: nothing until the plan is known.
    pub(super) fn plan_meter(
        &self,
        key: &AccountKey,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let colors = cx.theme().colors.clone();
        let UsageState::Ready(usage) = self.providers.usage.data.get(key)? else {
            return None;
        };
        let window = tightest(usage.as_ref())?;
        let severity = severity(window);
        let text = if window.is_exhausted {
            cx.t("settings.model.accountsExhausted")
        } else {
            cx.t_with(
                "settings.model.accountsLeft",
                &Args::default().with("percent", window.remaining_percent.round().to_string()),
            )
        };
        let used = (window.used_percent.clamp(0.0, 100.0) / 100.0) as f32;
        Some(
            div()
                .flex()
                .flex_none()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .h(px(6.0))
                        .w(px(64.0))
                        .overflow_hidden()
                        .rounded_full()
                        .bg(colors.muted)
                        .child(
                            div()
                                .h_full()
                                .w(relative(used))
                                .rounded_full()
                                .bg(bar_fill(severity, cx, 0.5)),
                        ),
                )
                .child(
                    div()
                        .w(px(72.0))
                        .text_right()
                        .font_features(crate::ui::tabular_nums())
                        .text_size(px(11.0))
                        .text_color(tone(severity, cx).unwrap_or(colors.muted_foreground))
                        .child(text),
                )
                .into_any_element(),
        )
    }

    fn window_row(&self, window: &ProviderUsageWindow, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let severity = severity(window);
        let label: SharedString = match window_label(window) {
            Ok(key) => cx.t(&format!("settings.model.{key}")),
            Err(spelled) => spelled.into(),
        };
        let reset = window
            .resets_at
            .as_deref()
            .and_then(|at| short_date_time(at, english(cx)));
        let used = (window.used_percent.clamp(0.0, 100.0) / 100.0) as f32;
        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .text_size(px(12.0))
                    .child(
                        div()
                            .truncate()
                            .text_color(colors.muted_foreground)
                            .child(label),
                    )
                    .child(
                        div()
                            .flex_none()
                            .font_weight(FontWeight::MEDIUM)
                            .font_features(crate::ui::tabular_nums())
                            .text_color(tone(severity, cx).unwrap_or(colors.foreground))
                            .child(
                                cx.t_with(
                                    "settings.model.usageRemaining",
                                    &Args::default().with(
                                        "percent",
                                        window.remaining_percent.round().to_string(),
                                    ),
                                ),
                            ),
                    ),
            )
            .child(
                div()
                    .h(px(6.0))
                    .overflow_hidden()
                    .rounded_full()
                    .bg(colors.muted)
                    .child(
                        div()
                            .h_full()
                            .w(relative(used))
                            .rounded_full()
                            .bg(bar_fill(severity, cx, 0.45)),
                    ),
            )
            .children(reset.map(|date| {
                div()
                    .text_size(px(11.0))
                    .text_color(colors.muted_foreground.opacity(0.8))
                    .child(cx.t_with(
                        "settings.model.usageResetsAt",
                        &Args::default().with("date", date),
                    ))
            }))
            .into_any_element()
    }

    /// `ProviderUsageDetails` for one sign-in.
    pub(super) fn usage_details(
        &self,
        provider: &ProviderDescriptor,
        key: &AccountKey,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let state = self
            .providers
            .usage
            .data
            .get(key)
            .cloned()
            .unwrap_or(UsageState::Loading);
        let refreshing = self.providers.usage.refreshing.contains(key);
        let usage = match &state {
            UsageState::Ready(usage) => usage.clone(),
            _ => None,
        };
        let available = usage
            .as_ref()
            .filter(|usage| matches!(usage.state, ProviderAccountUsageState::Available));
        let updated = available.and_then(|usage| short_time(&usage.updated_at, english(cx)));
        let now = crate::data::time::now_ms();
        let resets = if provider.capabilities.supports_account_resets {
            redeemable(
                available.and_then(|usage| usage.reset_credits.as_ref()),
                now,
            )
        } else {
            Vec::new()
        };
        let body: Vec<AnyElement> = match &state {
            UsageState::Loading => vec![quiet(cx.t("settings.model.usageLoading"), cx)],
            UsageState::Failed => vec![
                div()
                    .text_size(px(12.0))
                    .text_color(colors.destructive)
                    .child(cx.t("settings.model.usageError"))
                    .into_any_element(),
            ],
            UsageState::Ready(Some(usage))
                if matches!(usage.state, ProviderAccountUsageState::AwaitingActivity) =>
            {
                vec![quiet(cx.t("settings.model.usageAwaitingActivity"), cx)]
            }
            UsageState::Ready(Some(usage)) if !usage.windows.is_empty() => usage
                .windows
                .iter()
                .map(|window| self.window_row(window, cx))
                .collect(),
            UsageState::Ready(_) => vec![quiet(cx.t("settings.model.usageUnavailable"), cx)],
        };
        let refresh_key = key.clone();
        let slug = key.1.clone().unwrap_or_else(|| "default".into());
        div()
            .debug_selector(move || format!("settings-usage-{slug}"))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(colors.muted_foreground)
                            .children(updated.map(|time| {
                                cx.t_with(
                                    "settings.model.usageUpdatedAt",
                                    &Args::default().with("time", time),
                                )
                            })),
                    )
                    .child(
                        Button::new(SharedString::from(format!(
                            "settings-usage-refresh-{}",
                            key.1.as_deref().unwrap_or("default")
                        )))
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Xs)
                        .padding_x(6.0)
                        .text_size(11.0)
                        .muted()
                        .disabled(refreshing)
                        .on_click(cx.listener(move |settings, _, _, cx| {
                            settings.load_usage(refresh_key.clone(), cx)
                        }))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(4.0))
                                .child(if refreshing {
                                    spinner("settings-usage-refreshing", 12.0).into_any_element()
                                } else {
                                    Icon::new("refresh-ccw").size(12.0).into_any_element()
                                })
                                .child(cx.t("settings.model.usageRefresh")),
                        ),
                    ),
            )
            .children((!resets.is_empty()).then(|| self.reset_list(key, &resets, cx)))
            .children(body)
            .into_any_element()
    }
}

fn quiet(text: SharedString, cx: &gpui::App) -> AnyElement {
    div()
        .text_size(px(12.0))
        .line_height(px(19.5))
        .text_color(cx.theme().colors.muted_foreground)
        .child(text)
        .into_any_element()
}

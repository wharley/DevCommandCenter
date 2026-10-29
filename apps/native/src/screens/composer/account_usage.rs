//! `AccountUsageStrip` under the model picker's effort: how much of the
//! selected provider's plan is left, a bar per window ("5 horas", "7 dias"),
//! and a refresh. Read when the picker opens and on the refresh; only for a
//! provider that reports its usage.

use dcc_core::domain::provider::{ProviderAccountUsage, ProviderAccountUsageState};
use gpui::{AnyElement, Context, FontWeight, SharedString, Task, div, prelude::*, px, relative};

use super::Composer;
use crate::data::desktop_api::DesktopApi;
use crate::data::provider_usage::{bar_fill, severity, short_date_time, tone};
use crate::i18n::{Args, I18n, Locale, Translate};
use crate::theme::ActiveTheme;
use crate::ui::{Icon, Tooltip, spinner};

#[derive(Default)]
pub(crate) struct AccountUsage {
    /// The provider the last read was for.
    provider: Option<String>,
    usage: Option<ProviderAccountUsage>,
    fetching: bool,
    failed: bool,
    _task: Option<Task<()>>,
}

impl Composer {
    /// `useProviderAccountUsage(...).refetch()` for the selected provider.
    pub(crate) fn refresh_account_usage(&mut self, cx: &mut Context<Self>) {
        let Some(provider) = self
            .provider()
            .filter(|provider| provider.capabilities.supports_account_usage)
            .cloned()
        else {
            return;
        };
        let Some(api) = DesktopApi::load() else {
            return;
        };
        let runtime = self.runtime_with_account(&provider);
        let state = &mut self.picker.usage;
        if state.provider.as_deref() != Some(provider.id.0.as_str()) {
            state.usage = None;
        }
        state.provider = Some(provider.id.0.clone());
        state.fetching = true;
        state.failed = false;
        state._task = Some(cx.spawn(async move |this, cx| {
            let read = cx
                .background_executor()
                .spawn(async move { api.provider_usage(&provider.id.0, Some(&runtime)) })
                .await;
            let _ = this.update(cx, |composer, cx| {
                let state = &mut composer.picker.usage;
                state.fetching = false;
                match read {
                    Ok(usage) => state.usage = usage,
                    Err(_) => state.failed = true,
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
}

/// The strip, or nothing for a provider that does not report usage.
pub(super) fn account_usage_strip(
    composer: &Composer,
    cx: &mut Context<Composer>,
) -> Option<AnyElement> {
    let provider = composer
        .provider()
        .filter(|provider| provider.capabilities.supports_account_usage)?;
    let colors = cx.theme().colors.clone();
    let state = &composer.picker.usage;
    // A read for another provider is not this one's.
    let usage = state
        .usage
        .as_ref()
        .filter(|_| state.provider.as_deref() == Some(provider.id.0.as_str()));
    let english = cx.global::<I18n>().locale() == Locale::En;
    let windows = usage
        .filter(|usage| matches!(usage.state, ProviderAccountUsageState::Available))
        .map(|usage| usage.windows.clone())
        .unwrap_or_default();
    let body = if windows.is_empty() {
        let (key, color) = if usage
            .is_some_and(|usage| matches!(usage.state, ProviderAccountUsageState::AwaitingActivity))
        {
            (
                "composer.accountUsage.awaitingActivity",
                colors.muted_foreground,
            )
        } else if state.failed {
            ("composer.accountUsage.error", colors.destructive)
        } else if state.fetching {
            ("composer.accountUsage.loading", colors.muted_foreground)
        } else {
            ("composer.accountUsage.title", colors.muted_foreground)
        };
        div()
            .truncate()
            .text_size(px(11.0))
            .text_color(color)
            .child(cx.t(key))
            .into_any_element()
    } else {
        let bars = windows.iter().enumerate().map(|(index, window)| {
            let level = severity(window);
            let name: SharedString = match window.window_duration_minutes {
                Some(300) => cx.t("composer.accountUsage.fiveHour"),
                Some(10_080) => cx.t("composer.accountUsage.sevenDay"),
                _ => window.id.replace('_', " ").into(),
            };
            let remaining = window.remaining_percent.round();
            let bar = div()
                .min_w_0()
                .child(
                    div()
                        .flex()
                        .items_baseline()
                        .justify_between()
                        .gap(px(8.0))
                        .text_size(px(10.0))
                        .child(
                            div()
                                .truncate()
                                .text_color(colors.muted_foreground)
                                .child(name),
                        )
                        .child(
                            div()
                                .flex_none()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(tone(level, cx).unwrap_or(colors.foreground))
                                .child(
                                    cx.t_with(
                                        "composer.accountUsage.remaining",
                                        &Args::default()
                                            .with("percent", (remaining as i64).to_string()),
                                    ),
                                ),
                        ),
                )
                .child(
                    div()
                        .mt(px(4.0))
                        .h(px(4.0))
                        .overflow_hidden()
                        .rounded_full()
                        .bg(colors.muted)
                        .child(
                            div()
                                .h_full()
                                .rounded_full()
                                .w(relative((remaining / 100.0).clamp(0.0, 1.0) as f32))
                                .bg(bar_fill(level, cx, 0.4)),
                        ),
                );
            let reset = window
                .resets_at
                .as_deref()
                .and_then(|at| short_date_time(at, english));
            let cell = div().flex_1().min_w_0();
            match reset {
                Some(date) => cell
                    .child(
                        Tooltip::text(
                            ("account-usage-window", index),
                            bar,
                            cx.t_with(
                                "composer.accountUsage.nextReset",
                                &Args::default().with("date", date),
                            ),
                        )
                        .fill(),
                    )
                    .into_any_element(),
                None => cell.child(bar).into_any_element(),
            }
        });
        div()
            .flex()
            .items_center()
            .gap(px(12.0))
            .children(bars)
            .into_any_element()
    };
    let fetching = state.fetching;
    let refresh = Tooltip::text(
        "account-usage-refresh-tip",
        div()
            .id("account-usage-refresh")
            .debug_selector(|| "account-usage-refresh".into())
            .flex()
            .flex_none()
            .size(px(24.0))
            .items_center()
            .justify_center()
            .rounded(px(6.0))
            .text_color(colors.muted_foreground)
            .when(!fetching, |button| {
                let hover = colors.accent;
                let foreground = colors.foreground;
                button
                    .cursor_pointer()
                    .hover(move |style| style.bg(hover).text_color(foreground))
                    .on_click(cx.listener(|composer, _, _, cx| {
                        cx.stop_propagation();
                        composer.refresh_account_usage(cx);
                    }))
            })
            .child(if fetching {
                spinner("account-usage-spinner", 12.0).into_any_element()
            } else {
                Icon::new("refresh-ccw").size(12.0).into_any_element()
            }),
        cx.t("composer.accountUsage.refresh"),
    );
    Some(
        div()
            .debug_selector(|| "account-usage-strip".into())
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(8.0))
            .border_t_1()
            .border_color(colors.border.opacity(0.6))
            .px(px(12.0))
            .py(px(8.0))
            .child(div().flex_1().min_w_0().child(body))
            .child(refresh)
            .into_any_element(),
    )
}
